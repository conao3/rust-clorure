use std::collections::BTreeMap;

use rust_clorure::clorure::forms::{Keyword, Symbol};
use rust_clorure::clorure::runtime::collections::{Counted, ISeq, Seqable};
use rust_clorure::clorure::runtime::value::{FnValue, MetaMap, Value};
use rust_clorure::clorure::runtime::var::NamespaceRegistry;

#[test]
fn runtime_values_support_metadata_and_sequence_traits() {
    let mut meta = MetaMap::new();
    meta.insert(Keyword::parse(":doc"), Value::String("vector".to_string()));
    let value = Value::vector(vec![Value::Int(1), Value::Int(2)]).with_meta(meta.clone());

    match value {
        Value::Vector(vector) => {
            assert_eq!(vector.count(), 2);
            assert_eq!(vector.first(), Some(&Value::Int(1)));
            assert_eq!(vector.rest(), vec![Value::Int(2)]);
            assert_eq!(vector.seq(), Some(vec![Value::Int(1), Value::Int(2)]));
            assert_eq!(vector.meta.as_ref(), Some(&meta));
        }
        other => panic!("expected vector, got {other:?}"),
    }
}

#[test]
fn namespace_registry_tracks_aliases_and_dynamic_bindings() {
    let mut registry = NamespaceRegistry::new();
    let ns = Symbol::parse("demo.core");
    let var_id = registry.intern(
        ns.clone(),
        Symbol::parse("answer"),
        Value::Int(42),
        true,
        BTreeMap::new(),
    );

    registry.alias_namespace(&ns, Symbol::parse("str"), Symbol::parse("clojure.string"));
    registry.refer_var(&ns, Symbol::parse("the-answer"), var_id);
    registry.import_symbol(&ns, Symbol::parse("File"), "std::fs::File".to_string());
    registry.push_binding(var_id, Value::Int(7)).unwrap();

    let resolved = registry
        .resolve_public(&ns, &Symbol::parse("the-answer"))
        .unwrap();
    assert_eq!(resolved.current_value(), &Value::Int(7));
    assert_eq!(
        registry.resolve_alias(&ns, &Symbol::parse("str")),
        Some(&Symbol::parse("clojure.string"))
    );

    registry.pop_binding(var_id);
    assert_eq!(
        registry.get_var(var_id).unwrap().current_value(),
        &Value::Int(42)
    );
}

#[test]
fn runtime_functions_are_invokable() {
    let function = Value::Fn(FnValue {
        name: Some(Symbol::parse("demo/inc")),
        arity: 1,
        func: std::sync::Arc::new(|args| match args {
            [Value::Int(value)] => Ok(Value::Int(value + 1)),
            _ => Err("bad args".to_string()),
        }),
        meta: None,
    });

    assert_eq!(function.invoke(&[Value::Int(1)]).unwrap(), Value::Int(2));
}
