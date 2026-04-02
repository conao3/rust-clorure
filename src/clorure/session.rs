use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use crate::clorure::analyzer::{AnalyzerError, analyze};
use crate::clorure::ast::AstNode;
use crate::clorure::backend::bytecode::{BytecodeProgram, Vm};
use crate::clorure::bootstrap;
use crate::clorure::compiler::{BackendTarget, CompiledArtifact, compile};
use crate::clorure::env::{AnalysisEnv, CompilerState, CompilerWarning};
use crate::clorure::forms::{Form, Symbol};
use crate::clorure::runtime::builtin::BuiltinRegistry;
use crate::clorure::runtime::value::Value;

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
        let relative = format!("{}.clj", ns.to_string().replace('.', "/"));
        fs::read_to_string(relative).map_err(|error| error.to_string())
    }
}

pub trait Evaluator: Send + Sync {
    fn eval(
        &self,
        globals: &mut HashMap<Symbol, Value>,
        program: &BytecodeProgram,
    ) -> Result<Value, String>;
}

#[derive(Default)]
pub struct VmEvaluator;

impl Evaluator for VmEvaluator {
    fn eval(
        &self,
        globals: &mut HashMap<Symbol, Value>,
        program: &BytecodeProgram,
    ) -> Result<Value, String> {
        let mut vm = Vm::with_globals(globals.clone());
        let value = vm.run(program).map_err(|error| error.message)?;
        *globals = vm.globals;
        Ok(value)
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
    loader: Arc<dyn Loader>,
    evaluator: Arc<dyn Evaluator>,
}

impl Default for Session {
    fn default() -> Self {
        let mut builtin_registry = BuiltinRegistry::new();
        crate::clorure::core::install_phase1(&mut builtin_registry);
        let compiler_state = bootstrap::empty_state();
        Self {
            runtime_state: bootstrap::bootstrap_runtime_state(&compiler_state),
            compiler_state,
            source_cache: HashMap::new(),
            bytecode_cache: HashMap::new(),
            builtin_registry,
            loader: Arc::new(FileSystemLoader),
            evaluator: Arc::new(VmEvaluator),
        }
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
            return_value = Some(self.evaluator.eval(&mut self.runtime_state, &program)?);
            if let Some(value) = return_value.clone() {
                for symbol in &program.produced_vars {
                    self.runtime_state.insert(symbol.clone(), value.clone());
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
        self.eval_str(&source, env.with_ns(ns.clone()))
    }
}

fn cache_key(env: &AnalysisEnv, ast: &AstNode) -> String {
    format!("{}:{}:{}", env.ns, ast.form.meta.line, ast.form.meta.column)
}
