use std::fs;

use rust_clorure::clorure::analyzer::analyze;
use rust_clorure::clorure::aot::{AotManifest, emit_with_manifest};
use rust_clorure::clorure::env::{AnalysisEnv, CompilerState};
use rust_clorure::clorure::forms::Symbol;
use rust_clorure::clorure::repl;
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
