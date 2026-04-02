use std::sync::Arc;

use rust_clorure::clorure::analyzer::analyze;
use rust_clorure::clorure::ast::AstOp;
use rust_clorure::clorure::env::{AnalysisEnv, CompilerState};
use rust_clorure::clorure::forms::{Form, FormKind, ReaderMeta, Symbol};
use rust_clorure::clorure::macro_support::{
    CompileTimeEffect, EvalPolicy, ExpansionBudget, MacroExpander, defmacro, gensym,
};
use rust_clorure::clorure::reader::read_all;
use rust_clorure::clorure::runtime::value::{FnValue, Value};

#[test]
fn macroexpand_invokes_registered_macro_once() {
    let mut state = CompilerState::default();
    defmacro(
        &mut state,
        Symbol::parse("demo.core/when"),
        FnValue {
            name: Some(Symbol::parse("demo.core/when")),
            arity: 2,
            func: Arc::new(|args| {
                let [Value::Form(test), Value::Form(body)] = args else {
                    return Err("when expects two forms".to_string());
                };
                Ok(Value::Form(Box::new(Form::new(
                    FormKind::List(vec![
                        Form::symbol(None, "if", test.meta.clone()),
                        *test.clone(),
                        *body.clone(),
                        Form::new(FormKind::Nil, body.meta.clone()),
                    ]),
                    test.meta.clone(),
                ))))
            }),
            meta: None,
        },
    );

    let forms = read_all("(when true 42)").unwrap();
    let mut expander = MacroExpander::default();
    let expanded = expander
        .macroexpand(
            &forms[0],
            &state,
            &AnalysisEnv::default().with_ns(Symbol::parse("demo.core")),
        )
        .unwrap();

    match expanded.value {
        FormKind::List(items) => {
            assert_eq!(items[0].value, FormKind::Symbol(Symbol::parse("if")));
            assert_eq!(items[2].value, FormKind::Int(42));
        }
        other => panic!("expected expanded list, got {other:?}"),
    }
}

#[test]
fn analyzer_expands_macros_before_building_ast() {
    let mut state = CompilerState::default();
    defmacro(
        &mut state,
        Symbol::parse("demo.core/emit-do"),
        FnValue {
            name: Some(Symbol::parse("demo.core/emit-do")),
            arity: 0,
            func: Arc::new(|_| {
                Ok(Value::Form(Box::new(Form::new(
                    FormKind::List(vec![
                        Form::symbol(None, "do", ReaderMeta::new(1, 1, 1, 1, None)),
                        Form::new(FormKind::Int(1), ReaderMeta::new(1, 1, 1, 1, None)),
                        Form::new(FormKind::Int(2), ReaderMeta::new(1, 1, 1, 1, None)),
                    ]),
                    ReaderMeta::new(1, 1, 1, 1, None),
                ))))
            }),
            meta: None,
        },
    );

    let form = read_all("(emit-do)").unwrap().remove(0);
    let ast = analyze(
        form,
        &mut state,
        AnalysisEnv::default().with_ns(Symbol::parse("demo.core")),
    )
    .unwrap();
    assert_eq!(ast.op, AstOp::Do);
}

#[test]
fn analyzer_builds_defmacro_nodes() {
    let form = read_all("(defmacro emit-one [] 1)").unwrap().remove(0);
    let mut state = CompilerState::default();
    let ast = analyze(
        form,
        &mut state,
        AnalysisEnv::default().with_ns(Symbol::parse("demo.macros$macros")),
    )
    .unwrap();
    assert_eq!(ast.op, AstOp::DefMacro);
    assert_eq!(
        ast.attrs.get("name"),
        Some(&Value::symbol(Symbol::parse("demo.macros$macros/emit-one")))
    );
}

#[test]
fn expansion_budget_and_policy_are_enforced() {
    let mut expander = MacroExpander::new(
        ExpansionBudget {
            max_depth: 1,
            max_expansions: 1,
        },
        EvalPolicy::default(),
    );

    let form = read_all("(unknown-macro)").unwrap().remove(0);
    let state = CompilerState::default();
    let expanded = expander
        .macroexpand(&form, &state, &AnalysisEnv::default())
        .unwrap();
    assert_eq!(expanded, form);

    let err = MacroExpander::new(ExpansionBudget::default(), EvalPolicy::default())
        .ensure_effect_allowed(
            CompileTimeEffect::HostInterop,
            ReaderMeta::new(1, 1, 1, 1, None),
        )
        .unwrap_err();
    assert!(err.message.contains("not allowed"));

    let generated = gensym(Some("tmp"));
    assert!(generated.name().starts_with("tmp__gensym__"));
}
