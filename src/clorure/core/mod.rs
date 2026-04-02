use std::sync::Arc;

use crate::clorure::forms::Symbol;
use crate::clorure::runtime::builtin::{BuiltinError, BuiltinRegistry, CallContext};
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
    registry.register(
        Symbol::parse("clorure.core/+"),
        Arc::new(|args, _| match args {
            [Value::Int(a), Value::Int(b)] => Ok(Value::Int(a + b)),
            _ => Err(BuiltinError::new("+ expects two integers")),
        }),
    );
    registry.register(
        Symbol::parse("clorure.core/vector"),
        Arc::new(|args, _| Ok(Value::vector(args.to_vec()))),
    );
    registry.register(
        Symbol::parse("clorure.core/count"),
        Arc::new(|args, _| match args {
            [Value::List(list)] => Ok(Value::Int(list.items.len() as i64)),
            [Value::Vector(vector)] => Ok(Value::Int(vector.items.len() as i64)),
            [Value::Map(map)] => Ok(Value::Int(map.entries.len() as i64)),
            [Value::Set(set)] => Ok(Value::Int(set.entries.len() as i64)),
            _ => Err(BuiltinError::new("count expects one collection")),
        }),
    );
    registry.register(
        Symbol::parse("clorure.core/first"),
        Arc::new(|args, _| match args {
            [Value::List(list)] => Ok(list.items.first().cloned().unwrap_or(Value::Nil)),
            [Value::Vector(vector)] => Ok(vector.items.first().cloned().unwrap_or(Value::Nil)),
            _ => Err(BuiltinError::new("first expects a seqable collection")),
        }),
    );
    registry.register(
        Symbol::parse("clorure.core/rest"),
        Arc::new(|args, _| match args {
            [Value::List(list)] => Ok(Value::list(list.items.iter().skip(1).cloned().collect())),
            [Value::Vector(vector)] => Ok(Value::vector(
                vector.items.iter().skip(1).cloned().collect(),
            )),
            _ => Err(BuiltinError::new("rest expects a seqable collection")),
        }),
    );
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
