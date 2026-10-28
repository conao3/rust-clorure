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
fn analyzer_tracks_requires_and_alias_resolution() {
    let forms = read_all(
        "(ns demo.app (:require [demo.lib :as lib :refer [ping]] [demo.macros :as m :refer-macros [when-demo]])) (lib/value) ping",
    )
    .unwrap();
    let mut state = CompilerState::default();
    analyze(forms[0].clone(), &mut state, AnalysisEnv::default()).unwrap();

    let ns = state
        .namespace_analysis(&Symbol::parse("demo.app"))
        .expect("ns analysis should exist");
    assert_eq!(ns.requires.get("lib"), Some(&Symbol::parse("demo.lib")));
    assert_eq!(
        ns.require_macros.get("m"),
        Some(&Symbol::parse("demo.macros$macros"))
    );
    assert_eq!(ns.uses.get("ping"), Some(&Symbol::parse("demo.lib/ping")));
    assert_eq!(
        ns.use_macros.get("when-demo"),
        Some(&Symbol::parse("demo.macros$macros/when-demo"))
    );

    let aliased = analyze(
        forms[1].clone(),
        &mut state,
        AnalysisEnv::default().with_ns(Symbol::parse("demo.app")),
    )
    .unwrap();
    assert_eq!(
        aliased.children[0].attrs.get("symbol"),
        Some(&Value::symbol(Symbol::parse("demo.lib/value")))
    );

    let referred = analyze(
        forms[2].clone(),
        &mut state,
        AnalysisEnv::default().with_ns(Symbol::parse("demo.app")),
    )
    .unwrap();
    assert_eq!(
        referred.attrs.get("symbol"),
        Some(&Value::symbol(Symbol::parse("demo.lib/ping")))
    );
}

#[test]
fn analyzer_resolves_renamed_refers() {
    let forms = read_all(
        "(ns demo.app (:require [demo.lib :refer [ping] :rename {ping pong}] [demo.macros :refer-macros [when-demo] :rename-macros {when-demo unless-demo}])) pong",
    )
    .unwrap();
    let mut state = CompilerState::default();
    analyze(forms[0].clone(), &mut state, AnalysisEnv::default()).unwrap();

    let ns = state
        .namespace_analysis(&Symbol::parse("demo.app"))
        .expect("ns analysis should exist");
    assert_eq!(
        ns.renames.get("pong"),
        Some(&Symbol::parse("demo.lib/ping"))
    );
    assert_eq!(
        ns.rename_macros.get("unless-demo"),
        Some(&Symbol::parse("demo.macros$macros/when-demo"))
    );

    let renamed = analyze(
        forms[1].clone(),
        &mut state,
        AnalysisEnv::default().with_ns(Symbol::parse("demo.app")),
    )
    .unwrap();
    assert_eq!(
        renamed.attrs.get("symbol"),
        Some(&Value::symbol(Symbol::parse("demo.lib/ping")))
    );
}

#[test]
fn analyzer_expands_refer_all_when_publics_are_known() {
    let forms = read_all("(ns demo.app (:require [demo.lib :refer :all])) value").unwrap();
    let mut state = CompilerState::default();
    state.register_ns(Symbol::parse("demo.lib"));
    state.register_var(Symbol::parse("demo.lib/value"), Value::Int(1));
    state.register_var(Symbol::parse("demo.lib/ping"), Value::Int(2));

    analyze(forms[0].clone(), &mut state, AnalysisEnv::default()).unwrap();
    let ns = state
        .namespace_analysis(&Symbol::parse("demo.app"))
        .expect("ns analysis should exist");
    assert_eq!(ns.uses.get("value"), Some(&Symbol::parse("demo.lib/value")));
    assert_eq!(ns.uses.get("ping"), Some(&Symbol::parse("demo.lib/ping")));

    let referred = analyze(
        forms[1].clone(),
        &mut state,
        AnalysisEnv::default().with_ns(Symbol::parse("demo.app")),
    )
    .unwrap();
    assert_eq!(
        referred.attrs.get("symbol"),
        Some(&Value::symbol(Symbol::parse("demo.lib/value")))
    );
}

#[test]
fn analyzer_prefers_current_namespace_for_unqualified_vars() {
    let forms = read_all("(def local-value 1) local-value").unwrap();
    let mut state = CompilerState::default();
    analyze(
        forms[0].clone(),
        &mut state,
        AnalysisEnv::default().with_ns(Symbol::parse("demo.core")),
    )
    .unwrap();
    let resolved = analyze(
        forms[1].clone(),
        &mut state,
        AnalysisEnv::default().with_ns(Symbol::parse("demo.core")),
    )
    .unwrap();
    assert_eq!(
        resolved.attrs.get("symbol"),
        Some(&Value::symbol(Symbol::parse("demo.core/local-value")))
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

#[test]
fn analyzer_rejects_ambiguous_variadic_method_sets() {
    let duplicate = read_all("(fn ([x] x) ([y] y))").unwrap().remove(0);
    let mut state = CompilerState::default();
    let err = analyze(duplicate, &mut state, AnalysisEnv::default()).unwrap_err();
    assert!(err.message.contains("duplicate fn arity"));

    let non_terminal_variadic = read_all("(fn ([x & more] x) ([x y] y))").unwrap().remove(0);
    let err = analyze(non_terminal_variadic, &mut state, AnalysisEnv::default()).unwrap_err();
    assert!(
        err.message
            .contains("variadic fn arity must be the last method")
    );
}

#[test]
fn analyzer_builds_try_with_catch_binding() {
    let form = read_all("(try (throw 1) (catch err err))")
        .unwrap()
        .remove(0);
    let mut state = CompilerState::default();
    let ast = analyze(form, &mut state, AnalysisEnv::default()).unwrap();

    assert_eq!(ast.op, AstOp::Try);
    assert_eq!(
        ast.attrs.get("catch_binding"),
        Some(&Value::symbol(Symbol::parse("err")))
    );
    assert_eq!(ast.children.len(), 2);
}

#[test]
fn analyzer_builds_try_with_finally_clause() {
    let form = read_all("(try 1 (catch err err) (finally 2 3))")
        .unwrap()
        .remove(0);
    let mut state = CompilerState::default();
    let ast = analyze(form, &mut state, AnalysisEnv::default()).unwrap();

    assert_eq!(ast.op, AstOp::Try);
    assert_eq!(ast.children.len(), 3);
    assert_eq!(
        ast.attrs.get("catch_binding"),
        Some(&Value::symbol(Symbol::parse("err")))
    );
}

#[test]
fn analyzer_registers_protocol_definitions() {
    let form = read_all("(defprotocol Greeter (greet [this]) (farewell [this name]))")
        .unwrap()
        .remove(0);
    let mut state = CompilerState::default();
    let ast = analyze(
        form,
        &mut state,
        AnalysisEnv::default().with_ns(Symbol::parse("demo.core")),
    )
    .unwrap();

    assert_eq!(ast.op, AstOp::DefProtocol);
    assert_eq!(
        ast.attrs.get("name"),
        Some(&Value::symbol(Symbol::parse("demo.core/Greeter")))
    );
    assert_eq!(state.protocol_definitions.len(), 1);
    assert_eq!(
        state.protocol_definitions[0].name,
        Symbol::parse("demo.core/Greeter")
    );
    assert_eq!(
        state.protocol_definitions[0].methods,
        vec![
            Symbol::parse("demo.core/greet"),
            Symbol::parse("demo.core/farewell")
        ]
    );
    assert!(state.vars.contains_key(&Symbol::parse("demo.core/Greeter")));
    assert!(state.vars.contains_key(&Symbol::parse("demo.core/greet")));
    assert!(
        state
            .vars
            .contains_key(&Symbol::parse("demo.core/farewell"))
    );
}

#[test]
fn analyzer_builds_extend_type_nodes() {
    let form = read_all("(extend-type Vector Greeter (greet [this] (count this)))")
        .unwrap()
        .remove(0);
    let mut state = CompilerState::default();
    let ast = analyze(
        form,
        &mut state,
        AnalysisEnv::default().with_ns(Symbol::parse("demo.core")),
    )
    .unwrap();

    assert_eq!(ast.op, AstOp::ExtendType);
    assert_eq!(
        ast.attrs.get("target_type"),
        Some(&Value::symbol(Symbol::parse("Vector")))
    );
    assert_eq!(
        ast.attrs.get("protocol"),
        Some(&Value::symbol(Symbol::parse("demo.core/Greeter")))
    );
    assert_eq!(ast.children.len(), 1);
}
