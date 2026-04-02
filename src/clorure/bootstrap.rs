use std::collections::HashMap;

use crate::clorure::core;
use crate::clorure::env::{CompilerState, NamespaceAnalysis};
use crate::clorure::forms::Symbol;
use crate::clorure::runtime::builtin::BuiltinRegistry;
use crate::clorure::runtime::value::Value;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CoreDump {
    pub namespaces: HashMap<Symbol, NamespaceAnalysis>,
    pub vars: HashMap<Symbol, Value>,
    pub source_maps: HashMap<Symbol, String>,
}

pub fn dump_core() -> CoreDump {
    let mut state = CompilerState::default();
    let mut runtime_state = HashMap::new();
    let mut registry = BuiltinRegistry::new();
    core::install_phase1(&mut registry);
    core::install_into_state(&mut state, &mut runtime_state, &registry);
    dump_core_from_state(&state)
}

pub fn dump_core_from_state(state: &CompilerState) -> CoreDump {
    let mut dump = CoreDump::default();
    for ns in [Symbol::parse("clojure.core"), Symbol::parse("clorure.core")] {
        if let Some(analysis) = state.namespaces.get(&ns) {
            let mut analysis = analysis.clone();
            analysis.bootstrap = true;
            dump.namespaces.insert(ns.clone(), analysis);
            dump.source_maps.insert(
                ns.clone(),
                format!("bootstrap://{}", ns.to_string().replace('.', "/")),
            );
        }
    }
    for (symbol, value) in &state.vars {
        let include = symbol
            .namespace()
            .is_none_or(|ns| ns == "clojure.core" || ns == "clorure.core");
        if include {
            dump.vars.insert(symbol.clone(), value.clone());
        }
    }
    dump
}

pub fn empty_state() -> CompilerState {
    let mut state = CompilerState::default();
    load_analysis_cache(&mut state, dump_core());
    state
}

pub fn load_analysis_cache(state: &mut CompilerState, dump: CoreDump) {
    for (ns, analysis) in dump.namespaces {
        state.register_ns_with_analysis(ns, analysis);
    }
    for (symbol, value) in dump.vars {
        state.register_var(symbol, value);
    }
    for (ns, source_map) in dump.source_maps {
        state.load_source_map(ns, source_map);
    }
}

pub fn bootstrap_runtime_state(state: &CompilerState) -> HashMap<Symbol, Value> {
    state.vars.clone()
}
