use std::collections::HashMap;

use crate::clorure::forms::{ReaderMeta, Symbol};
use crate::clorure::runtime::value::{FnValue, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerWarning {
    pub kind: String,
    pub message: String,
    pub span: ReaderMeta,
}

impl CompilerWarning {
    pub fn new(kind: impl Into<String>, message: impl Into<String>, span: ReaderMeta) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolDefinition {
    pub name: Symbol,
    pub methods: Vec<Symbol>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NamespaceAnalysis {
    pub publics: Vec<Symbol>,
    pub bootstrap: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompilerState {
    pub namespaces: HashMap<Symbol, NamespaceAnalysis>,
    pub vars: HashMap<Symbol, Value>,
    pub protocol_definitions: Vec<ProtocolDefinition>,
    pub constant_table: Vec<Value>,
    pub source_maps: HashMap<Symbol, String>,
    pub warnings: Vec<CompilerWarning>,
}

impl CompilerState {
    pub fn add_warning(&mut self, warning: CompilerWarning) {
        self.warnings.push(warning);
    }

    pub fn add_constant(&mut self, value: Value) -> usize {
        self.constant_table.push(value);
        self.constant_table.len() - 1
    }

    pub fn register_var(&mut self, symbol: Symbol, value: Value) {
        if symbol.is_qualified() {
            let ns = Symbol::parse(symbol.namespace().unwrap_or("user"));
            self.namespaces
                .entry(ns)
                .or_default()
                .publics
                .push(symbol.clone());
        }
        self.vars.insert(symbol, value);
    }

    pub fn register_macro(&mut self, symbol: Symbol, func: FnValue) {
        self.vars.insert(symbol, Value::Macro(func));
    }

    pub fn resolve_var(&self, symbol: &Symbol) -> Option<&Value> {
        self.vars.get(symbol)
    }

    pub fn register_ns(&mut self, ns: Symbol) {
        self.namespaces.entry(ns).or_default();
    }

    pub fn register_ns_with_analysis(&mut self, ns: Symbol, analysis: NamespaceAnalysis) {
        self.namespaces.insert(ns, analysis);
    }

    pub fn load_source_map(&mut self, ns: Symbol, source_map: String) {
        self.source_maps.insert(ns, source_map);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisContext {
    Expr,
    Statement,
    Return,
    MacroExpansion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalBinding {
    pub symbol: Symbol,
    pub slot: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisEnv {
    pub file: Option<String>,
    pub ns: Symbol,
    pub locals: HashMap<String, LocalBinding>,
    pub context: AnalysisContext,
    pub loop_depth: usize,
    pub macroexpansion_depth: usize,
}

impl Default for AnalysisEnv {
    fn default() -> Self {
        Self {
            file: None,
            ns: Symbol::parse("user"),
            locals: HashMap::new(),
            context: AnalysisContext::Expr,
            loop_depth: 0,
            macroexpansion_depth: 0,
        }
    }
}

impl AnalysisEnv {
    pub fn with_context(&self, context: AnalysisContext) -> Self {
        let mut env = self.clone();
        env.context = context;
        env
    }

    pub fn with_ns(&self, ns: Symbol) -> Self {
        let mut env = self.clone();
        env.ns = ns;
        env
    }

    pub fn with_local(&self, symbol: Symbol) -> Self {
        let mut env = self.clone();
        let slot = env.locals.len();
        env.locals
            .insert(symbol.name().to_string(), LocalBinding { symbol, slot });
        env
    }

    pub fn enter_loop(&self) -> Self {
        let mut env = self.clone();
        env.loop_depth += 1;
        env
    }

    pub fn enter_macroexpansion(&self) -> Self {
        let mut env = self.clone();
        env.context = AnalysisContext::MacroExpansion;
        env.macroexpansion_depth += 1;
        env
    }
}
