use std::sync::Arc;

use rust_clorure::clorure::forms::Symbol;
use rust_clorure::clorure::interop::rust::{Capability, HostAdapter, InteropRegistry};
use rust_clorure::clorure::runtime::builtin::{BuiltinError, BuiltinRegistry, CallContext};
use rust_clorure::clorure::runtime::value::Value;

struct EchoAdapter;

impl HostAdapter for EchoAdapter {
    fn type_name(&self) -> &'static str {
        "EchoAdapter"
    }

    fn invoke_method(
        &self,
        method: &Symbol,
        args: &[Value],
        _context: &CallContext,
    ) -> Result<Value, BuiltinError> {
        match method.to_string().as_str() {
            "echo" => Ok(Value::vector(args.to_vec())),
            _ => Err(BuiltinError::new("unknown method")),
        }
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::Clock]
    }
}

#[test]
fn builtin_registry_invokes_registered_function() {
    let mut registry = BuiltinRegistry::new();
    registry.register(
        Symbol::parse("clorure.core/inc"),
        Arc::new(|args, _| match args {
            [Value::Int(value)] => Ok(Value::Int(value + 1)),
            _ => Err(BuiltinError::new("inc expects one int")),
        }),
    );

    let value = registry
        .invoke(
            &Symbol::parse("clorure.core/inc"),
            &[Value::Int(41)],
            &CallContext::default(),
        )
        .unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn interop_registry_requires_capabilities() {
    let mut registry = InteropRegistry::new();
    let handle = registry.register_adapter(Arc::new(EchoAdapter));
    let Value::OpaqueHandle(handle) = handle else {
        panic!("expected opaque handle");
    };

    let disabled = registry.invoke(
        handle,
        &Symbol::parse("echo"),
        &[Value::String("hello".to_string())],
        &CallContext::default(),
    );
    assert!(disabled.is_err());

    let enabled = registry
        .invoke(
            handle,
            &Symbol::parse("echo"),
            &[Value::String("hello".to_string())],
            &CallContext {
                capabilities_enabled: true,
                ..CallContext::default()
            },
        )
        .unwrap();
    assert_eq!(
        enabled,
        Value::vector(vec![Value::String("hello".to_string())])
    );
    assert_eq!(
        registry.adapter_capabilities(handle),
        Some(vec![Capability::Clock])
    );
}
