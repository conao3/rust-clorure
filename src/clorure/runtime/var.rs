use std::collections::HashMap;

use crate::clorure::forms::Symbol;
use crate::clorure::runtime::value::{MetaMap, Value};

pub type VarId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Var {
    pub ns: Symbol,
    pub sym: Symbol,
    pub value: Value,
    pub meta: MetaMap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Namespace {
    pub name: Symbol,
    pub publics: HashMap<Symbol, VarId>,
}

#[derive(Debug, Default)]
pub struct NamespaceRegistry {
    next_var_id: VarId,
    namespaces: HashMap<Symbol, Namespace>,
    vars: HashMap<VarId, Var>,
}

impl NamespaceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, ns: Symbol, sym: Symbol, value: Value, meta: MetaMap) -> VarId {
        let var_id = self.next_var_id;
        self.next_var_id += 1;

        self.vars.insert(
            var_id,
            Var {
                ns: ns.clone(),
                sym: sym.clone(),
                value,
                meta,
            },
        );

        self.namespaces
            .entry(ns.clone())
            .or_insert_with(|| Namespace {
                name: ns,
                publics: HashMap::new(),
            })
            .publics
            .insert(sym, var_id);

        var_id
    }

    pub fn get_var(&self, var_id: VarId) -> Option<&Var> {
        self.vars.get(&var_id)
    }

    pub fn resolve(&self, ns: &Symbol, sym: &Symbol) -> Option<&Var> {
        let namespace = self.namespaces.get(ns)?;
        let var_id = namespace.publics.get(sym)?;
        self.vars.get(var_id)
    }
}
