use rust_clorure::clorure::analyzer::analyze;
use rust_clorure::clorure::ast::AstOp;
use rust_clorure::clorure::env::{AnalysisContext, AnalysisEnv, CompilerState};
use rust_clorure::clorure::forms::Symbol;
use rust_clorure::clorure::reader::read_all;
use rust_clorure::clorure::runtime::value::Value;

#[test]
fn analyzer_builds_ns_and_def_nodes() {
    let forms = read_all("(ns demo.core) (def foo 42)").unwrap();
    let mut state = CompilerState::default();
    let ns_ast = analyze(forms[0].clone(), &mut state, AnalysisEnv::default()).unwrap();

    assert_eq!(ns_ast.op, AstOp::Ns);
    assert!(state.namespaces.contains_key(&Symbol::parse("demo.core")));

    let def_ast = analyze(
        forms[1].clone(),
        &mut state,
        AnalysisEnv::default().with_ns(Symbol::parse("demo.core")),
    )
    .unwrap();
    assert_eq!(def_ast.op, AstOp::Def);
    assert_eq!(
        def_ast.attrs.get("name"),
        Some(&Value::symbol(Symbol::parse("demo.core/foo")))
    );
}

#[test]
fn analyzer_tracks_let_locals_and_unresolved_warnings() {
    let forms = read_all("(let [x 1] (+ x y))").unwrap();
    let mut state = CompilerState::default();
    let ast = analyze(forms[0].clone(), &mut state, AnalysisEnv::default()).unwrap();

    assert_eq!(ast.op, AstOp::Let);
    assert_eq!(ast.children.len(), 2);
    assert_eq!(state.warnings.len(), 2);
    assert_eq!(state.warnings[0].kind, "unresolved-symbol");
}

#[test]
fn analyzer_marks_return_context_inside_fn_body() {
    let forms = read_all("(fn [x] (do x 42))").unwrap();
    let mut state = CompilerState::default();
    let ast = analyze(forms[0].clone(), &mut state, AnalysisEnv::default()).unwrap();

    assert_eq!(ast.op, AstOp::Fn);
    let body = &ast.children[0];
    assert_eq!(body.op, AstOp::Do);
    let nested_do = &body.children[0];
    assert_eq!(nested_do.op, AstOp::Do);
    assert_eq!(nested_do.children[1].env.context, AnalysisContext::Return);
}
