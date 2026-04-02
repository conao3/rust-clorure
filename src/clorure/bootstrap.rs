use std::collections::HashMap;

use crate::clorure::backend::bytecode::Vm;
use crate::clorure::env::{AnalysisEnv, CompilerState, NamespaceAnalysis};
use crate::clorure::forms::Symbol;
use crate::clorure::runtime::value::Value;
use crate::clorure::{analyzer, compiler, core, reader};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CoreDump {
    pub namespaces: HashMap<Symbol, NamespaceAnalysis>,
    pub vars: HashMap<Symbol, Value>,
    pub source_maps: HashMap<Symbol, String>,
}

pub fn dump_core() -> CoreDump {
    let mut state = CompilerState::default();
    let mut runtime_state = HashMap::new();
    core::install_bootstrap_primitives(&mut state, &mut runtime_state);
    evaluate_bootstrap_source(
        &mut state,
        &mut runtime_state,
        &core::core_source(),
        core::CORE_NS,
    )
    .expect("core bootstrap source should evaluate");
    evaluate_bootstrap_source(
        &mut state,
        &mut runtime_state,
        &core::core_macros_source(),
        core::CORE_MACROS_NS,
    )
    .expect("core macros bootstrap source should evaluate");
    promote_core_short_names(&mut state, &mut runtime_state);
    dump_core_from_state(&state)
}

pub fn dump_core_from_state(state: &CompilerState) -> CoreDump {
    let mut dump = CoreDump::default();
    for ns in [
        Symbol::parse(core::CORE_NS),
        Symbol::parse(core::CORE_MACROS_NS),
    ] {
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
            .is_none_or(|ns| ns == core::CORE_NS || ns == core::CORE_MACROS_NS);
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

fn evaluate_bootstrap_source(
    state: &mut CompilerState,
    runtime_state: &mut HashMap<Symbol, Value>,
    source: &str,
    ns: &str,
) -> Result<(), String> {
    let forms = reader::read_all(source).map_err(|error| error.message)?;
    for form in forms {
        let ast = analyzer::analyze(
            form,
            state,
            AnalysisEnv::default().with_ns(Symbol::parse(ns)),
        )
        .map_err(|error| error.to_string())?;
        let compiler::CompiledArtifact::Bytecode(program) =
            compiler::compile(&ast, compiler::BackendTarget::Bytecode)
        else {
            unreachable!();
        };
        let mut vm = Vm::with_globals(runtime_state.clone());
        let value = vm.run(&program).map_err(|error| error.message)?;
        *runtime_state = vm.globals;
        for symbol in &program.produced_vars {
            state.register_var(
                symbol.clone(),
                runtime_state
                    .get(symbol)
                    .cloned()
                    .unwrap_or_else(|| value.clone()),
            );
        }
    }
    Ok(())
}

fn promote_core_short_names(state: &mut CompilerState, runtime_state: &mut HashMap<Symbol, Value>) {
    let exported = state
        .vars
        .iter()
        .filter(|(symbol, _)| symbol.namespace() == Some(core::CORE_NS))
        .map(|(symbol, value)| (Symbol::parse(symbol.name()), value.clone()))
        .collect::<Vec<_>>();

    for (short, value) in exported {
        state.register_var(short.clone(), value.clone());
        runtime_state.insert(short, value);
    }
}
