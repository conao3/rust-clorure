use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::clorure::analyzer::{AnalyzerError, analyze};
use crate::clorure::ast::{AstNode, AstOp};
use crate::clorure::backend::bytecode::{BytecodeProgram, SharedGlobals, Vm};
use crate::clorure::bootstrap;
use crate::clorure::compiler::{BackendTarget, CompiledArtifact, compile};
use crate::clorure::env::{AnalysisEnv, CompilerState, CompilerWarning};
use crate::clorure::forms::{Form, Symbol};
use crate::clorure::runtime::builtin::BuiltinRegistry;
use crate::clorure::runtime::value::{FnValue, Value};

pub trait Loader: Send + Sync {
    fn load_file(&self, path: &Path) -> Result<String, String>;
    fn load_ns(&self, ns: &Symbol) -> Result<String, String>;
}

#[derive(Default)]
pub struct FileSystemLoader;

impl Loader for FileSystemLoader {
    fn load_file(&self, path: &Path) -> Result<String, String> {
        fs::read_to_string(path).map_err(|error| error.to_string())
    }

    fn load_ns(&self, ns: &Symbol) -> Result<String, String> {
        if let Some(source) = crate::clorure::core::embedded_namespace_source(ns) {
            return Ok(source.to_string());
        }
        let (stem, extensions): (String, &[&str]) = if ns.to_string().ends_with("$macros") {
            (
                ns.to_string().trim_end_matches("$macros").replace('.', "/"),
                &["cljc", "clj"],
            )
        } else {
            (ns.to_string().replace('.', "/"), &["cljs", "cljc", "clj"])
        };
        for ext in extensions {
            let relative = format!("{stem}.{ext}");
            if let Ok(source) = fs::read_to_string(&relative) {
                return Ok(source);
            }
        }
        Err(format!("namespace source for '{ns}' was not found"))
    }
}

pub trait Evaluator: Send + Sync {
    fn eval(&self, globals: &SharedGlobals, program: &BytecodeProgram) -> Result<Value, String>;
}

#[derive(Default)]
pub struct VmEvaluator;

impl Evaluator for VmEvaluator {
    fn eval(&self, globals: &SharedGlobals, program: &BytecodeProgram) -> Result<Value, String> {
        Vm::with_shared_globals(Arc::clone(globals))
            .run(program)
            .map_err(|error| error.message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalOutcome {
    pub warnings: Vec<CompilerWarning>,
    pub errors: Vec<String>,
    pub produced_vars: Vec<Symbol>,
    pub return_value: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisOutcome {
    pub warnings: Vec<CompilerWarning>,
    pub asts: Vec<AstNode>,
}

pub struct Session {
    pub compiler_state: CompilerState,
    pub runtime_state: HashMap<Symbol, Value>,
    pub source_cache: HashMap<String, String>,
    pub bytecode_cache: HashMap<String, BytecodeProgram>,
    pub builtin_registry: BuiltinRegistry,
    shared_globals: SharedGlobals,
    protocol_impls: Arc<Mutex<HashMap<(Symbol, String), Value>>>,
    loader: Arc<dyn Loader>,
    evaluator: Arc<dyn Evaluator>,
}

impl Default for Session {
    fn default() -> Self {
        let mut builtin_registry = BuiltinRegistry::new();
        crate::clorure::core::install_phase1(&mut builtin_registry);
        let compiler_state = bootstrap::empty_state();
        let runtime_state = bootstrap::bootstrap_runtime_state(&compiler_state);
        let mut session = Self {
            shared_globals: Arc::new(Mutex::new(runtime_state.clone())),
            protocol_impls: Arc::new(Mutex::new(HashMap::new())),
            runtime_state,
            compiler_state,
            source_cache: HashMap::new(),
            bytecode_cache: HashMap::new(),
            builtin_registry,
            loader: Arc::new(FileSystemLoader),
            evaluator: Arc::new(VmEvaluator),
        };
        session
            .load_ns(
                &Symbol::parse(crate::clorure::core::CORE_NS),
                AnalysisEnv::default(),
            )
            .expect("clrs.core should bootstrap into the default session");
        session
            .load_ns(
                &Symbol::parse(crate::clorure::core::CORE_MACROS_NS),
                AnalysisEnv::default(),
            )
            .expect("clrs.core$macros should bootstrap into the default session");
        session
    }
}

impl Session {
    pub fn new(loader: Arc<dyn Loader>, evaluator: Arc<dyn Evaluator>) -> Self {
        Self {
            loader,
            evaluator,
            ..Self::default()
        }
    }

    pub fn bind_global(&mut self, symbol: Symbol, value: Value) {
        self.runtime_state.insert(symbol.clone(), value.clone());
        self.compiler_state
            .register_var(symbol.clone(), value.clone());
        self.shared_globals
            .lock()
            .expect("globals mutex should not be poisoned")
            .insert(symbol, value);
    }

    pub fn analyze_str(
        &mut self,
        source: &str,
        env: AnalysisEnv,
    ) -> Result<AnalysisOutcome, AnalyzerError> {
        let forms = crate::clorure::reader::read_all(source).map_err(|error| {
            AnalyzerError::new(
                error.message,
                Form::new(crate::clorure::forms::FormKind::Nil, error.meta),
            )
        })?;
        let mut asts = Vec::new();
        let base_warning_len = self.compiler_state.warnings.len();
        for form in forms {
            asts.push(analyze(form, &mut self.compiler_state, env.clone())?);
        }
        let warnings = self.compiler_state.warnings[base_warning_len..].to_vec();
        Ok(AnalysisOutcome { warnings, asts })
    }

    pub fn eval_str(&mut self, source: &str, env: AnalysisEnv) -> Result<EvalOutcome, String> {
        let forms = crate::clorure::reader::read_all(source).map_err(|error| error.message)?;
        let mut produced_vars = Vec::new();
        let mut return_value = None;
        let base_warning_len = self.compiler_state.warnings.len();

        for form in forms {
            let ast = analyze(form, &mut self.compiler_state, env.clone())
                .map_err(|error| error.to_string())?;
            let CompiledArtifact::Bytecode(program) = compile(&ast, BackendTarget::Bytecode) else {
                unreachable!();
            };
            produced_vars.extend(program.produced_vars.clone());
            produced_vars.extend(program.produced_macros.clone());
            return_value = Some(self.evaluator.eval(&self.shared_globals, &program)?);
            self.apply_special_runtime_effects(&ast)?;
            self.runtime_state = self.snapshot_globals();
            if let Some(value) = return_value.clone() {
                for symbol in &program.produced_vars {
                    let var_value = self
                        .runtime_state
                        .get(symbol)
                        .cloned()
                        .unwrap_or_else(|| value.clone());
                    self.bind_global(symbol.clone(), var_value.clone());
                    if symbol.namespace() == Some(crate::clorure::core::CORE_NS) {
                        self.bind_global(Symbol::parse(symbol.name()), var_value);
                    }
                }
                for symbol in &program.produced_macros {
                    let current = self
                        .runtime_state
                        .get(symbol)
                        .cloned()
                        .unwrap_or_else(|| value.clone());
                    let macro_value = match current {
                        Value::Fn(function) => Value::Macro(function),
                        Value::Macro(function) => Value::Macro(function),
                        other => other,
                    };
                    self.bind_global(symbol.clone(), macro_value);
                }
            }
            self.bytecode_cache.insert(cache_key(&env, &ast), program);
        }

        Ok(EvalOutcome {
            warnings: self.compiler_state.warnings[base_warning_len..].to_vec(),
            errors: Vec::new(),
            produced_vars,
            return_value,
        })
    }

    pub fn load_file(&mut self, path: &Path, env: AnalysisEnv) -> Result<EvalOutcome, String> {
        let source = self.loader.load_file(path)?;
        self.source_cache
            .insert(path.display().to_string(), source.clone());
        self.eval_str(&source, env)
    }

    pub fn load_ns(&mut self, ns: &Symbol, env: AnalysisEnv) -> Result<EvalOutcome, String> {
        let source = self.loader.load_ns(ns)?;
        self.source_cache.insert(ns.to_string(), source.clone());
        self.preload_required_namespaces(&source, ns)?;
        self.eval_str(&source, env.with_ns(ns.clone()))
    }

    fn snapshot_globals(&self) -> HashMap<Symbol, Value> {
        self.shared_globals
            .lock()
            .expect("globals mutex should not be poisoned")
            .clone()
    }

    fn preload_required_namespaces(&mut self, source: &str, ns: &Symbol) -> Result<(), String> {
        let forms = crate::clorure::reader::read_all(source).map_err(|error| error.message)?;
        let Some(first) = forms.first() else {
            return Ok(());
        };
        if !matches!(first.value, crate::clorure::forms::FormKind::List(_)) {
            return Ok(());
        }
        let _ = analyze(
            first.clone(),
            &mut self.compiler_state,
            AnalysisEnv::default(),
        )
        .map_err(|error| error.to_string())?;
        let Some(analysis) = self.compiler_state.namespace_analysis(ns).cloned() else {
            return Ok(());
        };

        for required in analysis.requires.into_values() {
            if required != *ns && !self.source_cache.contains_key(&required.to_string()) {
                let _ = self.load_ns(&required, AnalysisEnv::default())?;
            }
        }
        for required in analysis.require_macros.into_values() {
            if required != *ns && !self.source_cache.contains_key(&required.to_string()) {
                let _ = self.load_ns(&required, AnalysisEnv::default())?;
            }
        }
        Ok(())
    }

    fn apply_special_runtime_effects(&mut self, ast: &AstNode) -> Result<(), String> {
        match ast.op {
            AstOp::DefProtocol => self.install_protocol_dispatch(ast),
            AstOp::ExtendType => self.install_protocol_extension(ast),
            _ => Ok(()),
        }
    }

    fn install_protocol_dispatch(&mut self, ast: &AstNode) -> Result<(), String> {
        let methods = match ast.attrs.get("methods") {
            Some(Value::Vector(vector)) => vector.items.as_ref().clone(),
            _ => return Ok(()),
        };

        for value in methods {
            let Value::Symbol(symbol) = value else {
                continue;
            };
            let method_symbol = symbol.symbol.clone();
            let registry = Arc::clone(&self.protocol_impls);
            let dispatch_symbol = method_symbol.clone();
            let dispatch = Value::Fn(FnValue {
                name: Some(method_symbol.clone()),
                arity: usize::MAX,
                func: Arc::new(move |args| {
                    let Some(target) = args.first() else {
                        return Err(format!(
                            "protocol method '{}' expects at least one argument",
                            dispatch_symbol
                        ));
                    };
                    let type_tag = protocol_type_tag(target);
                    let implementation = registry
                        .lock()
                        .expect("protocol registry mutex should not be poisoned")
                        .get(&(dispatch_symbol.clone(), type_tag))
                        .cloned()
                        .ok_or_else(|| {
                            format!(
                                "no protocol implementation for '{}'",
                                dispatch_symbol
                            )
                        })?;
                    implementation.invoke(args)
                }),
                meta: None,
            });
            self.bind_global(method_symbol, dispatch);
        }

        Ok(())
    }

    fn install_protocol_extension(&mut self, ast: &AstNode) -> Result<(), String> {
        let target_type = match ast.attrs.get("target_type") {
            Some(Value::Symbol(symbol)) => symbol.symbol.name().to_ascii_lowercase(),
            _ => return Ok(()),
        };
        let methods = match ast.attrs.get("methods") {
            Some(Value::Vector(vector)) => vector.items.as_ref().clone(),
            _ => return Ok(()),
        };

        for (method_value, method_ast) in methods.into_iter().zip(ast.children.iter()) {
            let Value::Symbol(method_symbol) = method_value else {
                continue;
            };
            let CompiledArtifact::Bytecode(program) = compile(method_ast, BackendTarget::Bytecode) else {
                unreachable!();
            };
            let implementation = self.evaluator.eval(&self.shared_globals, &program)?;
            self.protocol_impls
                .lock()
                .expect("protocol registry mutex should not be poisoned")
                .insert((method_symbol.symbol.clone(), target_type.clone()), implementation);
        }

        Ok(())
    }
}

fn cache_key(env: &AnalysisEnv, ast: &AstNode) -> String {
    format!("{}:{}:{}", env.ns, ast.form.meta.line, ast.form.meta.column)
}

fn protocol_type_tag(value: &Value) -> String {
    match value {
        Value::Nil => "nil".to_string(),
        Value::Bool(_) => "bool".to_string(),
        Value::Int(_) => "int".to_string(),
        Value::String(_) => "string".to_string(),
        Value::Keyword(_) => "keyword".to_string(),
        Value::Symbol(_) => "symbol".to_string(),
        Value::List(_) => "list".to_string(),
        Value::Vector(_) => "vector".to_string(),
        Value::Map(_) => "map".to_string(),
        Value::Set(_) => "set".to_string(),
        Value::Form(_) => "form".to_string(),
        Value::Fn(_) => "fn".to_string(),
        Value::Macro(_) => "macro".to_string(),
        Value::OpaqueHandle(_) => "opaquehandle".to_string(),
    }
}
