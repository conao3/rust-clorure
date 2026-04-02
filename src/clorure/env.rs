use std::collections::HashMap;

use crate::clorure::forms::Symbol;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerEnv {
    pub current_ns: Symbol,
    pub locals: HashMap<String, Symbol>,
    pub features: Vec<String>,
}

impl Default for CompilerEnv {
    fn default() -> Self {
        Self {
            current_ns: Symbol::parse("user"),
            locals: HashMap::new(),
            features: vec!["clorure".to_string(), "clj".to_string()],
        }
    }
}
