use std::sync::Arc;

use crate::clorure::env::CompilerState;
use crate::clorure::forms::Symbol;
use crate::clorure::runtime::builtin::{BuiltinError, BuiltinFn, BuiltinRegistry, CallContext};
use crate::clorure::runtime::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityPhase {
    Phase1,
    Phase2,
    Phase3,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureStatus {
    pub area: &'static str,
    pub phase1: bool,
    pub phase2: bool,
    pub phase3: bool,
}

pub fn feature_matrix() -> Vec<FeatureStatus> {
    vec![
        FeatureStatus {
            area: "forms",
            phase1: true,
            phase2: true,
            phase3: true,
        },
        FeatureStatus {
            area: "basic-seq",
            phase1: true,
            phase2: true,
            phase3: true,
        },
        FeatureStatus {
            area: "metadata",
            phase1: false,
            phase2: true,
            phase3: true,
        },
        FeatureStatus {
            area: "protocols",
            phase1: false,
            phase2: true,
            phase3: true,
        },
        FeatureStatus {
            area: "multimethod",
            phase1: false,
            phase2: false,
            phase3: true,
        },
        FeatureStatus {
            area: "aot-export",
            phase1: false,
            phase2: false,
            phase3: true,
        },
    ]
}

pub fn install_phase1(registry: &mut BuiltinRegistry) {
    for namespace in ["clojure.core", "clorure.core"] {
        register_core_symbol(
            registry,
            namespace,
            "+",
            Arc::new(|args, _| match args {
                [Value::Int(a), Value::Int(b)] => Ok(Value::Int(a + b)),
                _ => Err(BuiltinError::new("+ expects two integers")),
            }),
        );
        register_core_symbol(
            registry,
            namespace,
            "vector",
            Arc::new(|args, _| Ok(Value::vector(args.to_vec()))),
        );
        register_core_symbol(
            registry,
            namespace,
            "list",
            Arc::new(|args, _| Ok(Value::list(args.to_vec()))),
        );
        register_core_symbol(
            registry,
            namespace,
            "count",
            Arc::new(|args, _| match args {
                [Value::List(list)] => Ok(Value::Int(list.items.len() as i64)),
                [Value::Vector(vector)] => Ok(Value::Int(vector.items.len() as i64)),
                [Value::Map(map)] => Ok(Value::Int(map.entries.len() as i64)),
                [Value::Set(set)] => Ok(Value::Int(set.entries.len() as i64)),
                _ => Err(BuiltinError::new("count expects one collection")),
            }),
        );
        register_core_symbol(
            registry,
            namespace,
            "first",
            Arc::new(|args, _| match args {
                [Value::List(list)] => Ok(list.items.first().cloned().unwrap_or(Value::Nil)),
                [Value::Vector(vector)] => Ok(vector.items.first().cloned().unwrap_or(Value::Nil)),
                _ => Err(BuiltinError::new("first expects a seqable collection")),
            }),
        );
        register_core_symbol(
            registry,
            namespace,
            "rest",
            Arc::new(|args, _| match args {
                [Value::List(list)] => {
                    Ok(Value::list(list.items.iter().skip(1).cloned().collect()))
                }
                [Value::Vector(vector)] => Ok(Value::vector(
                    vector.items.iter().skip(1).cloned().collect(),
                )),
                _ => Err(BuiltinError::new("rest expects a seqable collection")),
            }),
        );
        register_core_symbol(
            registry,
            namespace,
            "=",
            Arc::new(|args, _| match args {
                [left, right] => Ok(Value::Bool(left == right)),
                _ => Err(BuiltinError::new("= expects two values")),
            }),
        );
        register_core_symbol(
            registry,
            namespace,
            "nil?",
            Arc::new(|args, _| match args {
                [Value::Nil] => Ok(Value::Bool(true)),
                [_] => Ok(Value::Bool(false)),
                _ => Err(BuiltinError::new("nil? expects one value")),
            }),
        );
    }
}

pub fn install_into_state(
    state: &mut CompilerState,
    runtime_state: &mut std::collections::HashMap<Symbol, Value>,
    registry: &BuiltinRegistry,
) {
    state.register_ns(Symbol::parse("clojure.core"));
    state.register_ns(Symbol::parse("clorure.core"));
    for (symbol, value) in registry.exported_values() {
        state.register_var(symbol.clone(), value.clone());
        runtime_state.insert(symbol.clone(), value.clone());
        if symbol.is_qualified() {
            let short = Symbol::parse(symbol.name());
            state.register_var(short.clone(), value.clone());
            runtime_state.insert(short, value);
        }
    }
}

pub fn phase_for(feature: &str) -> Option<CompatibilityPhase> {
    feature_matrix().into_iter().find_map(|status| {
        if status.area == feature {
            if status.phase1 {
                Some(CompatibilityPhase::Phase1)
            } else if status.phase2 {
                Some(CompatibilityPhase::Phase2)
            } else if status.phase3 {
                Some(CompatibilityPhase::Phase3)
            } else {
                None
            }
        } else {
            None
        }
    })
}

pub fn is_compat_branch_supported(feature: &str, phase: CompatibilityPhase) -> bool {
    feature_matrix()
        .into_iter()
        .find(|status| status.area == feature)
        .is_some_and(|status| match phase {
            CompatibilityPhase::Phase1 => status.phase1,
            CompatibilityPhase::Phase2 => status.phase2,
            CompatibilityPhase::Phase3 => status.phase3,
        })
}

pub fn default_call_context() -> CallContext {
    CallContext::default()
}

fn register_core_symbol(
    registry: &mut BuiltinRegistry,
    namespace: &str,
    name: &str,
    function: BuiltinFn,
) {
    registry.register(
        Symbol::parse(&format!("{namespace}/{name}")),
        Arc::clone(&function),
    );
}
