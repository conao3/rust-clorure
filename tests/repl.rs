use std::fs;
use std::sync::Arc;

use rust_clorure::clorure::analyzer::analyze;
use rust_clorure::clorure::aot::{AotManifest, emit_with_manifest};
use rust_clorure::clorure::env::{AnalysisEnv, CompilerState};
use rust_clorure::clorure::forms::Symbol;
use rust_clorure::clorure::repl;
use rust_clorure::clorure::runtime::value::{FnValue, Value};
use rust_clorure::clorure::session::Session;

#[test]
fn repl_eval_and_analyze_return_incremental_results() {
    let mut session = Session::default();
    let analysis =
        repl::analyze_str(&mut session, "(def answer 42)", AnalysisEnv::default()).unwrap();
    assert_eq!(analysis.asts.len(), 1);

    let outcome = repl::eval_str(&mut session, "(def answer 42)", AnalysisEnv::default()).unwrap();
    assert_eq!(outcome.produced_vars, vec![Symbol::parse("user/answer")]);
    assert!(
        session
            .runtime_state
            .contains_key(&Symbol::parse("user/answer"))
    );

    let plus = repl::eval_str(&mut session, "(+ 1 2)", AnalysisEnv::default()).unwrap();
    assert_eq!(
        plus.return_value,
        Some(rust_clorure::clorure::runtime::value::Value::Int(3))
    );

    let qualified =
        repl::eval_str(&mut session, "(clrs.core/+ 3 4)", AnalysisEnv::default()).unwrap();
    assert_eq!(
        qualified.return_value,
        Some(rust_clorure::clorure::runtime::value::Value::Int(7))
    );

    let when = repl::eval_str(&mut session, "(when true 9)", AnalysisEnv::default()).unwrap();
    assert_eq!(
        when.return_value,
        Some(rust_clorure::clorure::runtime::value::Value::Int(9))
    );

    let inc = repl::eval_str(&mut session, "(inc 9)", AnalysisEnv::default()).unwrap();
    assert_eq!(
        inc.return_value,
        Some(rust_clorure::clorure::runtime::value::Value::Int(10))
    );

    let dec = repl::eval_str(&mut session, "(dec 9)", AnalysisEnv::default()).unwrap();
    assert_eq!(
        dec.return_value,
        Some(rust_clorure::clorure::runtime::value::Value::Int(8))
    );

    let empty = repl::eval_str(&mut session, "(empty? (vector))", AnalysisEnv::default()).unwrap();
    assert_eq!(
        empty.return_value,
        Some(rust_clorure::clorure::runtime::value::Value::Bool(true))
    );

    let second = repl::eval_str(
        &mut session,
        "(second (vector 1 2 3))",
        AnalysisEnv::default(),
    )
    .unwrap();
    assert_eq!(
        second.return_value,
        Some(rust_clorure::clorure::runtime::value::Value::Int(2))
    );

    let zero = repl::eval_str(&mut session, "(zero? 0)", AnalysisEnv::default()).unwrap();
    assert_eq!(
        zero.return_value,
        Some(rust_clorure::clorure::runtime::value::Value::Bool(true))
    );

    let when_not =
        repl::eval_str(&mut session, "(when-not false 12)", AnalysisEnv::default()).unwrap();
    assert_eq!(
        when_not.return_value,
        Some(rust_clorure::clorure::runtime::value::Value::Int(12))
    );

    let if_not =
        repl::eval_str(&mut session, "(if-not false 1 2)", AnalysisEnv::default()).unwrap();
    assert_eq!(
        if_not.return_value,
        Some(rust_clorure::clorure::runtime::value::Value::Int(1))
    );

    let defn_call = repl::eval_str(
        &mut session,
        "(defn add-one [x] (+ x 1)) (add-one 4)",
        AnalysisEnv::default(),
    )
    .unwrap();
    assert_eq!(
        defn_call.return_value,
        Some(rust_clorure::clorure::runtime::value::Value::Int(5))
    );

    session.bind_global(
        Symbol::parse("clrs.core/+"),
        Value::Fn(FnValue {
            name: Some(Symbol::parse("clrs.core/+")),
            arity: 2,
            func: Arc::new(|_| Ok(Value::Int(99))),
            meta: None,
        }),
    );
    let rebound = repl::eval_str(&mut session, "(inc 1)", AnalysisEnv::default()).unwrap();
    assert_eq!(rebound.return_value, Some(Value::Int(99)));
}

#[test]
fn session_loads_files_and_namespaces() {
    let mut session = Session::default();
    let temp_root = std::env::temp_dir().join("rust-clorure-tests");
    fs::create_dir_all(temp_root.join("demo")).unwrap();
    let file_path = temp_root.join("example.clj");
    fs::write(&file_path, "(def file-value 7)").unwrap();

    let old_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(&temp_root).unwrap();
    fs::write(temp_root.join("demo/core.clj"), "(def ns-value 8)").unwrap();
    fs::write(
        temp_root.join("demo/lib.cljs"),
        "(ns demo.lib) (def value 13) (def ping 21)",
    )
    .unwrap();
    fs::write(
        temp_root.join("demo/macros.cljc"),
        "(ns demo.macros$macros) (def when-demo clorure.bootstrap.primitives/when) (defmacro emit-constant [] 55)",
    )
    .unwrap();
    fs::write(
        temp_root.join("demo/app.cljs"),
        "(ns demo.app (:require [demo.lib :as lib :refer [ping]] [demo.macros :as m :refer-macros [when-demo emit-constant]])) (def via-alias lib/value) (def via-refer ping) (def via-macro (when-demo true 34)) (def via-source-macro (emit-constant))",
    )
    .unwrap();
    fs::write(
        temp_root.join("demo/rename-app.cljs"),
        "(ns demo.rename-app (:require [demo.lib :refer [ping] :rename {ping pong}] [demo.macros :refer-macros [when-demo] :rename-macros {when-demo unless-demo}])) (def via-rename pong) (def via-rename-macro (unless-demo true 89))",
    )
    .unwrap();
    fs::write(
        temp_root.join("demo/all.cljs"),
        "(ns demo.all (:require [demo.lib :refer :all] [demo.macros :include-macros true :refer-macros :all])) (def via-all value) (def via-all-macro (emit-constant))",
    )
    .unwrap();
    fs::write(
        temp_root.join("demo/fnapp.cljs"),
        "(ns demo.fnapp) (defn plus-two [x] (+ x 2)) (def via-defn (plus-two 5))",
    )
    .unwrap();

    let file_outcome = session
        .load_file(&file_path, AnalysisEnv::default())
        .unwrap();
    assert_eq!(
        file_outcome.produced_vars,
        vec![Symbol::parse("user/file-value")]
    );

    let ns_outcome = session
        .load_ns(&Symbol::parse("demo.core"), AnalysisEnv::default())
        .unwrap();
    assert_eq!(
        ns_outcome.produced_vars,
        vec![Symbol::parse("demo.core/ns-value")]
    );

    let core_outcome = session
        .load_ns(&Symbol::parse("clrs.core"), AnalysisEnv::default())
        .unwrap();
    assert!(
        core_outcome
            .produced_vars
            .contains(&Symbol::parse("clrs.core/+"))
    );

    let macros_outcome = session
        .load_ns(&Symbol::parse("clrs.core$macros"), AnalysisEnv::default())
        .unwrap();
    assert!(
        macros_outcome
            .produced_vars
            .contains(&Symbol::parse("clrs.core$macros/when"))
    );

    fs::write(
        temp_root.join("demo/helpers.cljc"),
        "(ns demo.helpers$macros)\n(def helper-when clorure.bootstrap.primitives/when)",
    )
    .unwrap();
    let helper_macros = session
        .load_ns(
            &Symbol::parse("demo.helpers$macros"),
            AnalysisEnv::default(),
        )
        .unwrap();
    assert!(
        helper_macros
            .produced_vars
            .contains(&Symbol::parse("demo.helpers$macros/helper-when"))
    );
    let helper_eval = repl::eval_str(
        &mut session,
        "(demo.helpers/helper-when true 11)",
        AnalysisEnv::default(),
    )
    .unwrap();
    assert_eq!(
        helper_eval.return_value,
        Some(rust_clorure::clorure::runtime::value::Value::Int(11))
    );
    let app_outcome = session
        .load_ns(&Symbol::parse("demo.app"), AnalysisEnv::default())
        .unwrap();
    assert!(
        app_outcome
            .produced_vars
            .contains(&Symbol::parse("demo.app/via-alias"))
    );
    assert_eq!(
        session
            .runtime_state
            .get(&Symbol::parse("demo.app/via-alias")),
        Some(&rust_clorure::clorure::runtime::value::Value::Int(13))
    );
    assert_eq!(
        session
            .runtime_state
            .get(&Symbol::parse("demo.app/via-refer")),
        Some(&rust_clorure::clorure::runtime::value::Value::Int(21))
    );
    assert_eq!(
        session
            .runtime_state
            .get(&Symbol::parse("demo.app/via-macro")),
        Some(&rust_clorure::clorure::runtime::value::Value::Int(34))
    );
    assert_eq!(
        session
            .runtime_state
            .get(&Symbol::parse("demo.app/via-source-macro")),
        Some(&rust_clorure::clorure::runtime::value::Value::Int(55))
    );
    let helper_alias_eval = repl::eval_str(
        &mut session,
        "(m/when-demo true 15)",
        AnalysisEnv::default().with_ns(Symbol::parse("demo.app")),
    )
    .unwrap();
    assert_eq!(
        helper_alias_eval.return_value,
        Some(rust_clorure::clorure::runtime::value::Value::Int(15))
    );
    let rename_outcome = session
        .load_ns(&Symbol::parse("demo.rename-app"), AnalysisEnv::default())
        .unwrap();
    assert!(
        rename_outcome
            .produced_vars
            .contains(&Symbol::parse("demo.rename-app/via-rename"))
    );
    assert_eq!(
        session
            .runtime_state
            .get(&Symbol::parse("demo.rename-app/via-rename")),
        Some(&rust_clorure::clorure::runtime::value::Value::Int(21))
    );
    assert_eq!(
        session
            .runtime_state
            .get(&Symbol::parse("demo.rename-app/via-rename-macro")),
        Some(&rust_clorure::clorure::runtime::value::Value::Int(89))
    );
    let all_outcome = session
        .load_ns(&Symbol::parse("demo.all"), AnalysisEnv::default())
        .unwrap();
    assert!(
        all_outcome
            .produced_vars
            .contains(&Symbol::parse("demo.all/via-all"))
    );
    assert_eq!(
        session
            .runtime_state
            .get(&Symbol::parse("demo.all/via-all")),
        Some(&rust_clorure::clorure::runtime::value::Value::Int(13))
    );
    assert_eq!(
        session
            .runtime_state
            .get(&Symbol::parse("demo.all/via-all-macro")),
        Some(&rust_clorure::clorure::runtime::value::Value::Int(55))
    );
    let fnapp_outcome = session
        .load_ns(&Symbol::parse("demo.fnapp"), AnalysisEnv::default())
        .unwrap();
    assert!(
        fnapp_outcome
            .produced_vars
            .contains(&Symbol::parse("demo.fnapp/via-defn"))
    );
    assert_eq!(
        session
            .runtime_state
            .get(&Symbol::parse("demo.fnapp/via-defn")),
        Some(&rust_clorure::clorure::runtime::value::Value::Int(7))
    );

    std::env::set_current_dir(old_dir).unwrap();
}

#[test]
fn aot_backend_emits_registration_scaffold() {
    let mut state = CompilerState::default();
    let form = rust_clorure::clorure::reader::read_all("(def answer 42)")
        .unwrap()
        .remove(0);
    let ast = analyze(form, &mut state, AnalysisEnv::default()).unwrap();
    let source = emit_with_manifest(
        &ast,
        &AotManifest {
            ns: "demo.core".to_string(),
            entrypoints: vec!["demo.core/answer".to_string()],
        },
    );

    assert!(source.contains("register_generated_ns"));
    assert!(source.contains("ConstTable"));
    assert!(source.contains("demo.core"));
}
