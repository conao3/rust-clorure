use std::collections::HashMap;
use std::sync::Arc;

use crate::clorure::env::CompilerState;
use crate::clorure::forms::{Form, FormKind, Symbol};
use crate::clorure::runtime::builtin::{BuiltinError, BuiltinFn, BuiltinRegistry, CallContext};
use crate::clorure::runtime::value::{FnValue, NativeFn, Value};

pub const CORE_NS: &str = "clrs.core";
pub const CORE_MACROS_NS: &str = "clrs.core$macros";
pub const BOOTSTRAP_NS: &str = "clorure.bootstrap.primitives";
pub const CORE_SOURCE_PATH: &str = "clrs/core.cljs";
pub const CORE_MACROS_SOURCE_PATH: &str = "clrs/core.cljc";

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
    for namespace in [CORE_NS, "clorure.core"] {
        for name in [
            "+", "vector", "list", "count", "first", "rest", "=", "nil?", "conj", "nth", "boolean",
        ] {
            register_core_symbol(registry, namespace, name, core_builtin(name));
        }
    }
}

pub fn install_bootstrap_primitives(
    state: &mut CompilerState,
    runtime_state: &mut HashMap<Symbol, Value>,
) {
    state.register_ns(Symbol::parse(BOOTSTRAP_NS));
    for (symbol, value) in bootstrap_primitives() {
        let short = Symbol::parse(symbol.name());
        state.register_var(symbol.clone(), value.clone());
        state.register_var(short.clone(), value.clone());
        runtime_state.insert(symbol, value.clone());
        runtime_state.insert(short, value);
    }
}

pub fn install_into_state(
    state: &mut CompilerState,
    runtime_state: &mut HashMap<Symbol, Value>,
    registry: &BuiltinRegistry,
) {
    state.register_ns(Symbol::parse(CORE_NS));
    state.register_ns(Symbol::parse(CORE_MACROS_NS));
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

pub fn core_source() -> String {
    include_str!("../../../clrs/core.cljs").to_string()
}

pub fn core_macros_source() -> String {
    include_str!("../../../clrs/core.cljc").to_string()
}

pub fn source_map_for(ns: &Symbol) -> Option<String> {
    match ns.to_string().as_str() {
        CORE_NS => Some(format!("bootstrap://{CORE_SOURCE_PATH}")),
        CORE_MACROS_NS => Some(format!("bootstrap://{CORE_MACROS_SOURCE_PATH}")),
        _ => None,
    }
}

pub fn embedded_namespace_source(ns: &Symbol) -> Option<&'static str> {
    match ns.to_string().as_str() {
        CORE_NS | "clorure.core" => Some(include_str!("../../../clrs/core.cljs")),
        CORE_MACROS_NS => Some(include_str!("../../../clrs/core.cljc")),
        _ => None,
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
    registry.register(Symbol::parse(&format!("{namespace}/{name}")), function);
}

fn bootstrap_primitives() -> Vec<(Symbol, Value)> {
    let mut values = Vec::new();
    for name in [
        "+", "vector", "list", "count", "first", "rest", "=", "nil?", "conj", "nth", "boolean",
    ] {
        let symbol = Symbol::parse(&format!("{BOOTSTRAP_NS}/{name}"));
        values.push((
            symbol.clone(),
            Value::Fn(FnValue {
                name: Some(symbol),
                arity: usize::MAX,
                func: runtime_builtin(name),
                meta: None,
            }),
        ));
    }

    let when_symbol = Symbol::parse(&format!("{BOOTSTRAP_NS}/when"));
    values.push((
        when_symbol.clone(),
        Value::Macro(FnValue {
            name: Some(when_symbol),
            arity: 2,
            func: Arc::new(|args| match args {
                [Value::Form(test), Value::Form(body)] => Ok(Value::Form(Box::new(Form::new(
                    FormKind::List(vec![
                        Form::symbol(None, "if", test.meta.clone()),
                        *test.clone(),
                        *body.clone(),
                        Form::new(FormKind::Nil, body.meta.clone()),
                    ]),
                    test.meta.clone(),
                )))),
                _ => Err("when expects two forms".to_string()),
            }),
            meta: None,
        }),
    ));

    let when_not_symbol = Symbol::parse(&format!("{BOOTSTRAP_NS}/when-not"));
    values.push((
        when_not_symbol.clone(),
        Value::Macro(FnValue {
            name: Some(when_not_symbol),
            arity: 2,
            func: Arc::new(|args| match args {
                [Value::Form(test), Value::Form(body)] => Ok(Value::Form(Box::new(Form::new(
                    FormKind::List(vec![
                        Form::symbol(None, "if", test.meta.clone()),
                        Form::new(
                            FormKind::List(vec![
                                Form::symbol(None, "not", test.meta.clone()),
                                *test.clone(),
                            ]),
                            test.meta.clone(),
                        ),
                        *body.clone(),
                        Form::new(FormKind::Nil, body.meta.clone()),
                    ]),
                    test.meta.clone(),
                )))),
                _ => Err("when-not expects two forms".to_string()),
            }),
            meta: None,
        }),
    ));

    let if_not_symbol = Symbol::parse(&format!("{BOOTSTRAP_NS}/if-not"));
    values.push((
        if_not_symbol.clone(),
        Value::Macro(FnValue {
            name: Some(if_not_symbol),
            arity: 3,
            func: Arc::new(|args| match args {
                [
                    Value::Form(test),
                    Value::Form(then_branch),
                    Value::Form(else_branch),
                ] => Ok(Value::Form(Box::new(Form::new(
                    FormKind::List(vec![
                        Form::symbol(None, "if", test.meta.clone()),
                        Form::new(
                            FormKind::List(vec![
                                Form::symbol(None, "not", test.meta.clone()),
                                *test.clone(),
                            ]),
                            test.meta.clone(),
                        ),
                        *then_branch.clone(),
                        *else_branch.clone(),
                    ]),
                    test.meta.clone(),
                )))),
                _ => Err("if-not expects three forms".to_string()),
            }),
            meta: None,
        }),
    ));

    let thread_first_symbol = Symbol::parse(&format!("{BOOTSTRAP_NS}/->"));
    values.push((
        thread_first_symbol.clone(),
        Value::Macro(FnValue {
            name: Some(thread_first_symbol),
            arity: usize::MAX,
            func: Arc::new(|args| {
                let Some((first, rest)) = args.split_first() else {
                    return Err("-> expects at least one form".to_string());
                };
                let mut current = expect_macro_form(first)?;
                for step in rest {
                    let step_form = expect_macro_form(step)?;
                    current = match step_form.value {
                        FormKind::Symbol(symbol) => Form::new(
                            FormKind::List(vec![
                                Form::new(FormKind::Symbol(symbol), step_form.meta.clone()),
                                current,
                            ]),
                            step_form.meta.clone(),
                        ),
                        FormKind::List(mut items) => {
                            if items.is_empty() {
                                return Err("-> steps cannot be empty lists".to_string());
                            }
                            let mut threaded = Vec::with_capacity(items.len() + 1);
                            threaded.push(items.remove(0));
                            threaded.push(current);
                            threaded.extend(items);
                            Form::new(FormKind::List(threaded), step_form.meta.clone())
                        }
                        _ => return Err("-> steps must be symbols or lists".to_string()),
                    };
                }
                Ok(Value::Form(Box::new(current)))
            }),
            meta: None,
        }),
    ));

    let when_let_symbol = Symbol::parse(&format!("{BOOTSTRAP_NS}/when-let"));
    values.push((
        when_let_symbol.clone(),
        Value::Macro(FnValue {
            name: Some(when_let_symbol),
            arity: 2,
            func: Arc::new(|args| match args {
                [Value::Form(binding_form), Value::Form(body)] => {
                    let FormKind::Vector(bindings) = &binding_form.value else {
                        return Err("when-let expects a binding vector".to_string());
                    };
                    if bindings.len() != 2 {
                        return Err("when-let expects exactly one binding pair".to_string());
                    }
                    let test_symbol = bindings[0].clone();
                    let init_expr = bindings[1].clone();
                    Ok(Value::Form(Box::new(Form::new(
                        FormKind::List(vec![
                            Form::symbol(None, "let", binding_form.meta.clone()),
                            binding_form.as_ref().clone(),
                            Form::new(
                                FormKind::List(vec![
                                    Form::symbol(None, "if", body.meta.clone()),
                                    test_symbol,
                                    *body.clone(),
                                    Form::new(FormKind::Nil, body.meta.clone()),
                                ]),
                                body.meta.clone(),
                            ),
                        ]),
                        init_expr.meta.clone(),
                    ))))
                }
                _ => Err("when-let expects a binding vector and a body".to_string()),
            }),
            meta: None,
        }),
    ));

    values
}

fn expect_macro_form(value: &Value) -> Result<Form, String> {
    match value {
        Value::Form(form) => Ok((**form).clone()),
        _ => Err("macro expects reader forms".to_string()),
    }
}

fn core_builtin(name: &str) -> BuiltinFn {
    match name {
        "+" => Arc::new(|args, _| match args {
            [Value::Int(a), Value::Int(b)] => Ok(Value::Int(a + b)),
            _ => Err(BuiltinError::new("+ expects two integers")),
        }),
        "vector" => Arc::new(|args, _| Ok(Value::vector(args.to_vec()))),
        "list" => Arc::new(|args, _| Ok(Value::list(args.to_vec()))),
        "count" => Arc::new(|args, _| match args {
            [Value::List(list)] => Ok(Value::Int(list.items.len() as i64)),
            [Value::Vector(vector)] => Ok(Value::Int(vector.items.len() as i64)),
            [Value::Map(map)] => Ok(Value::Int(map.entries.len() as i64)),
            [Value::Set(set)] => Ok(Value::Int(set.entries.len() as i64)),
            _ => Err(BuiltinError::new("count expects one collection")),
        }),
        "first" => Arc::new(|args, _| match args {
            [Value::List(list)] => Ok(list.items.first().cloned().unwrap_or(Value::Nil)),
            [Value::Vector(vector)] => Ok(vector.items.first().cloned().unwrap_or(Value::Nil)),
            _ => Err(BuiltinError::new("first expects a seqable collection")),
        }),
        "rest" => Arc::new(|args, _| match args {
            [Value::List(list)] => Ok(Value::list(list.items.iter().skip(1).cloned().collect())),
            [Value::Vector(vector)] => Ok(Value::vector(
                vector.items.iter().skip(1).cloned().collect(),
            )),
            _ => Err(BuiltinError::new("rest expects a seqable collection")),
        }),
        "=" => Arc::new(|args, _| match args {
            [left, right] => Ok(Value::Bool(left == right)),
            _ => Err(BuiltinError::new("= expects two values")),
        }),
        "nil?" => Arc::new(|args, _| match args {
            [Value::Nil] => Ok(Value::Bool(true)),
            [_] => Ok(Value::Bool(false)),
            _ => Err(BuiltinError::new("nil? expects one value")),
        }),
        "conj" => Arc::new(|args, _| match args {
            [Value::Vector(vector), value] => {
                let mut items = (*vector.items).clone();
                items.push(value.clone());
                Ok(Value::vector(items))
            }
            [Value::List(list), value] => {
                let mut items = vec![value.clone()];
                items.extend(list.items.iter().cloned());
                Ok(Value::list(items))
            }
            _ => Err(BuiltinError::new("conj expects a list/vector and a value")),
        }),
        "nth" => Arc::new(|args, _| match args {
            [Value::Vector(vector), Value::Int(index)] => vector
                .items
                .get(*index as usize)
                .cloned()
                .ok_or_else(|| BuiltinError::new("nth index out of bounds")),
            [Value::List(list), Value::Int(index)] => list
                .items
                .get(*index as usize)
                .cloned()
                .ok_or_else(|| BuiltinError::new("nth index out of bounds")),
            _ => Err(BuiltinError::new("nth expects a list/vector and an index")),
        }),
        "boolean" => Arc::new(|args, _| match args {
            [Value::Nil] => Ok(Value::Bool(false)),
            [Value::Bool(false)] => Ok(Value::Bool(false)),
            [_] => Ok(Value::Bool(true)),
            _ => Err(BuiltinError::new("boolean expects one value")),
        }),
        _ => Arc::new(|_, _| Err(BuiltinError::new("unknown core builtin"))),
    }
}

fn runtime_builtin(name: &str) -> NativeFn {
    let builtin = core_builtin(name);
    Arc::new(move |args| builtin(args, &CallContext::default()).map_err(|error| error.message))
}
