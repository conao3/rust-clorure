use std::collections::HashMap;

use crate::clorure::forms::Symbol;
use crate::clorure::runtime::value::{MetaMap, Value};

pub type VarId = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Var {
    pub ns: Symbol,
    pub sym: Symbol,
    pub root: Value,
    pub dynamic: bool,
    pub meta: MetaMap,
    pub binding_stack: Vec<Value>,
}

impl Var {
    pub fn current_value(&self) -> &Value {
        self.binding_stack.last().unwrap_or(&self.root)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Namespace {
    pub name: Symbol,
    pub aliases: HashMap<Symbol, Symbol>,
    pub refers: HashMap<Symbol, VarId>,
    pub publics: HashMap<Symbol, VarId>,
    pub imports: HashMap<Symbol, String>,
}

impl Namespace {
    pub fn new(name: Symbol) -> Self {
        Self {
            name,
            aliases: HashMap::new(),
            refers: HashMap::new(),
            publics: HashMap::new(),
            imports: HashMap::new(),
        }
    }
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

    pub fn ensure_namespace(&mut self, ns: Symbol) -> &mut Namespace {
        self.namespaces
            .entry(ns.clone())
            .or_insert_with(|| Namespace::new(ns))
    }

    pub fn intern(
        &mut self,
        ns: Symbol,
        sym: Symbol,
        root: Value,
        dynamic: bool,
        meta: MetaMap,
    ) -> VarId {
        let var_id = self.next_var_id;
        self.next_var_id += 1;

        self.vars.insert(
            var_id,
            Var {
                ns: ns.clone(),
                sym: sym.clone(),
                root,
                dynamic,
                meta,
                binding_stack: Vec::new(),
            },
        );

        self.ensure_namespace(ns).publics.insert(sym, var_id);
        var_id
    }

    pub fn alias_namespace(&mut self, ns: &Symbol, alias: Symbol, target: Symbol) {
        self.ensure_namespace(ns.clone())
            .aliases
            .insert(alias, target);
    }

    pub fn refer_var(&mut self, ns: &Symbol, alias: Symbol, var_id: VarId) {
        self.ensure_namespace(ns.clone())
            .refers
            .insert(alias, var_id);
    }

    pub fn import_symbol(&mut self, ns: &Symbol, alias: Symbol, target: String) {
        self.ensure_namespace(ns.clone())
            .imports
            .insert(alias, target);
    }

    pub fn get_var(&self, var_id: VarId) -> Option<&Var> {
        self.vars.get(&var_id)
    }

    pub fn get_var_mut(&mut self, var_id: VarId) -> Option<&mut Var> {
        self.vars.get_mut(&var_id)
    }

    pub fn resolve_public(&self, ns: &Symbol, sym: &Symbol) -> Option<&Var> {
        let namespace = self.namespaces.get(ns)?;
        let var_id = namespace
            .publics
            .get(sym)
            .or_else(|| namespace.refers.get(sym))?;
        self.vars.get(var_id)
    }

    pub fn resolve_alias(&self, ns: &Symbol, alias: &Symbol) -> Option<&Symbol> {
        self.namespaces.get(ns)?.aliases.get(alias)
    }

    pub fn swap_root(&mut self, var_id: VarId, root: Value) -> Option<()> {
        let var = self.vars.get_mut(&var_id)?;
        var.root = root;
        Some(())
    }

    pub fn push_binding(&mut self, var_id: VarId, value: Value) -> Option<()> {
        let var = self.vars.get_mut(&var_id)?;
        if var.dynamic {
            var.binding_stack.push(value);
            Some(())
        } else {
            None
        }
    }

    pub fn pop_binding(&mut self, var_id: VarId) -> Option<Value> {
        self.vars.get_mut(&var_id)?.binding_stack.pop()
    }
}
