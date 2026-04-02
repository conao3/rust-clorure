use rust_clorure::clorure::core::{
    CompatibilityPhase, default_call_context, feature_matrix, install_phase1,
    is_compat_branch_supported, phase_for,
};
use rust_clorure::clorure::forms::Symbol;
use rust_clorure::clorure::reader::read_all;
use rust_clorure::clorure::runtime::builtin::BuiltinRegistry;
use rust_clorure::clorure::runtime::value::Value;

#[test]
fn compatibility_matrix_exposes_phase_progression() {
    let matrix = feature_matrix();
    assert!(matrix.iter().any(|row| row.area == "forms" && row.phase1));
    assert_eq!(phase_for("metadata"), Some(CompatibilityPhase::Phase2));
    assert!(is_compat_branch_supported(
        "multimethod",
        CompatibilityPhase::Phase3
    ));
}

#[test]
fn phase1_core_builtins_cover_basic_seq_workflows() {
    let mut registry = BuiltinRegistry::new();
    install_phase1(&mut registry);

    let vector = registry
        .invoke(
            &Symbol::parse("clojure.core/vector"),
            &[Value::Int(1), Value::Int(2), Value::Int(3)],
            &default_call_context(),
        )
        .unwrap();
    let count = registry
        .invoke(
            &Symbol::parse("clojure.core/count"),
            std::slice::from_ref(&vector),
            &default_call_context(),
        )
        .unwrap();
    let first = registry
        .invoke(
            &Symbol::parse("clojure.core/first"),
            std::slice::from_ref(&vector),
            &default_call_context(),
        )
        .unwrap();
    let list = registry
        .invoke(
            &Symbol::parse("clojure.core/list"),
            &[Value::Int(4), Value::Int(5)],
            &default_call_context(),
        )
        .unwrap();

    assert_eq!(count, Value::Int(3));
    assert_eq!(first, Value::Int(1));
    assert_eq!(list, Value::list(vec![Value::Int(4), Value::Int(5)]));
}

#[test]
fn reader_conditionals_keep_clorure_branch_first_class() {
    let forms = read_all("#?(:clorure 1 :cljs 2 :clj 3)").unwrap();
    assert_eq!(
        forms[0].value,
        rust_clorure::clorure::forms::FormKind::Int(1)
    );
}
