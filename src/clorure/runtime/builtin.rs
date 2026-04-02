use std::collections::HashMap;
use std::sync::Arc;

use crate::clorure::forms::Symbol;
use crate::clorure::runtime::value::{FnValue, Value};

pub type BuiltinFn =
    Arc<dyn Fn(&[Value], &CallContext) -> Result<Value, BuiltinError> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinError {
    pub message: String,
}

impl BuiltinError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for BuiltinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for BuiltinError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallContext {
    pub current_ns: Symbol,
    pub file: Option<String>,
    pub capabilities_enabled: bool,
}

impl Default for CallContext {
    fn default() -> Self {
        Self {
            current_ns: Symbol::parse("user"),
            file: None,
            capabilities_enabled: false,
        }
    }
}

#[derive(Default)]
pub struct BuiltinRegistry {
    functions: HashMap<Symbol, BuiltinFn>,
}

impl BuiltinRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, symbol: Symbol, function: BuiltinFn) {
        self.functions.insert(symbol, function);
    }

    pub fn invoke(
        &self,
        symbol: &Symbol,
        args: &[Value],
        context: &CallContext,
    ) -> Result<Value, BuiltinError> {
        let function = self
            .functions
            .get(symbol)
            .ok_or_else(|| BuiltinError::new(format!("unknown builtin '{symbol}'")))?;
        function(args, context)
    }

    pub fn exported_values(&self) -> Vec<(Symbol, Value)> {
        self.functions
            .iter()
            .map(|(symbol, function)| {
                (
                    symbol.clone(),
                    Value::Fn(FnValue {
                        name: Some(symbol.clone()),
                        arity: usize::MAX,
                        func: {
                            let symbol = symbol.clone();
                            let function = Arc::clone(function);
                            Arc::new(move |args| {
                                function(
                                    args,
                                    &CallContext {
                                        current_ns: symbol
                                            .namespace()
                                            .map(Symbol::parse)
                                            .unwrap_or_else(|| Symbol::parse("user")),
                                        ..CallContext::default()
                                    },
                                )
                                .map_err(|error| error.message)
                            })
                        },
                        meta: None,
                    }),
                )
            })
            .collect()
    }
}
