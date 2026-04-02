use rust_clorure::clorure::bootstrap::{CoreDump, dump_core, empty_state, load_analysis_cache};
use rust_clorure::clorure::env::{CompilerState, NamespaceAnalysis};
use rust_clorure::clorure::forms::Symbol;
use rust_clorure::clorure::runtime::value::Value;

#[test]
fn empty_state_preloads_core_like_cljs_js() {
    let state = empty_state();
    assert!(state.namespaces.contains_key(&Symbol::parse("clrs.core")));
    assert!(
        state
            .namespaces
            .contains_key(&Symbol::parse("clrs.core$macros"))
    );
    assert!(state.vars.contains_key(&Symbol::parse("clrs.core/+")));
    assert_eq!(
        state.source_maps.get(&Symbol::parse("clrs.core")),
        Some(&"bootstrap://clrs/core.cljs".to_string())
    );
    assert!(state.namespaces[&Symbol::parse("clrs.core")].bootstrap);
}

#[test]
fn dump_core_extracts_bootstrap_cache() {
    let dump = dump_core();
    assert!(dump.namespaces.contains_key(&Symbol::parse("clrs.core")));
    assert!(dump.vars.contains_key(&Symbol::parse("clrs.core/vector")));
    assert_eq!(
        dump.source_maps.get(&Symbol::parse("clrs.core$macros")),
        Some(&"bootstrap://clrs/core.cljc".to_string())
    );
}

#[test]
fn load_analysis_cache_merges_precomputed_namespaces() {
    let mut state = CompilerState::default();
    let mut dump = CoreDump::default();
    dump.namespaces.insert(
        Symbol::parse("demo.core"),
        NamespaceAnalysis {
            publics: vec![Symbol::parse("demo.core/x")],
            bootstrap: false,
            ..NamespaceAnalysis::default()
        },
    );
    dump.vars
        .insert(Symbol::parse("demo.core/x"), Value::Int(1));
    dump.source_maps
        .insert(Symbol::parse("demo.core"), "memory://demo/core".to_string());

    load_analysis_cache(&mut state, dump);

    assert!(state.namespaces.contains_key(&Symbol::parse("demo.core")));
    assert!(state.vars.contains_key(&Symbol::parse("demo.core/x")));
    assert_eq!(
        state.source_maps.get(&Symbol::parse("demo.core")),
        Some(&"memory://demo/core".to_string())
    );
}
