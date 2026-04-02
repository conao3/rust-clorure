use std::sync::atomic::{AtomicUsize, Ordering};

use crate::clorure::env::{AnalysisEnv, CompilerState};
use crate::clorure::forms::{Form, FormKind, ReaderMeta, Symbol};
use crate::clorure::runtime::value::{FnValue, Value};

static GENSYM_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpansionBudget {
    pub max_depth: usize,
    pub max_expansions: usize,
}

impl Default for ExpansionBudget {
    fn default() -> Self {
        Self {
            max_depth: 32,
            max_expansions: 128,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalPolicy {
    pub allow_namespace_definitions: bool,
    pub allow_macro_definitions: bool,
    pub allow_constant_generation: bool,
    pub allow_host_interop: bool,
}

impl Default for EvalPolicy {
    fn default() -> Self {
        Self {
            allow_namespace_definitions: true,
            allow_macro_definitions: true,
            allow_constant_generation: true,
            allow_host_interop: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileTimeEffect {
    DefineNamespace,
    DefineMacro,
    ConstantGeneration,
    HostInterop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroError {
    pub message: String,
    pub meta: ReaderMeta,
}

impl MacroError {
    fn new(message: impl Into<String>, meta: ReaderMeta) -> Self {
        Self {
            message: message.into(),
            meta,
        }
    }
}

impl std::fmt::Display for MacroError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at {}:{}",
            self.message, self.meta.line, self.meta.column
        )
    }
}

impl std::error::Error for MacroError {}

#[derive(Default)]
pub struct MacroExpander {
    pub budget: ExpansionBudget,
    pub policy: EvalPolicy,
    expansions: usize,
}

impl MacroExpander {
    pub fn new(budget: ExpansionBudget, policy: EvalPolicy) -> Self {
        Self {
            budget,
            policy,
            expansions: 0,
        }
    }

    pub fn macroexpand_1(
        &mut self,
        form: &Form,
        state: &CompilerState,
        env: &AnalysisEnv,
    ) -> Result<Option<Form>, MacroError> {
        let FormKind::List(items) = &form.value else {
            return Ok(None);
        };
        let Some(Form {
            value: FormKind::Symbol(symbol),
            ..
        }) = items.first()
        else {
            return Ok(None);
        };

        let resolved = resolve_macro(state, env, symbol);
        let Some(Value::Macro(function)) = resolved else {
            return Ok(None);
        };

        if env.macroexpansion_depth >= self.budget.max_depth {
            return Err(MacroError::new(
                "macro expansion depth exceeded",
                form.meta.clone(),
            ));
        }
        if self.expansions >= self.budget.max_expansions {
            return Err(MacroError::new(
                "macro expansion count exceeded",
                form.meta.clone(),
            ));
        }
        self.expansions += 1;

        let args = items[1..]
            .iter()
            .cloned()
            .map(|item| Value::Form(Box::new(item)))
            .collect::<Vec<_>>();
        let result = (function.func)(&args)
            .map_err(|message| MacroError::new(message, form.meta.clone()))?;
        let expanded = result
            .into_form(form.meta.clone())
            .map_err(|message| MacroError::new(message, form.meta.clone()))?;
        Ok(Some(expanded))
    }

    pub fn macroexpand(
        &mut self,
        form: &Form,
        state: &CompilerState,
        env: &AnalysisEnv,
    ) -> Result<Form, MacroError> {
        let mut current = form.clone();
        let mut current_env = env.clone();
        while let Some(next) = self.macroexpand_1(&current, state, &current_env)? {
            current = next;
            current_env = current_env.enter_macroexpansion();
        }
        Ok(current)
    }

    pub fn ensure_effect_allowed(
        &self,
        effect: CompileTimeEffect,
        meta: ReaderMeta,
    ) -> Result<(), MacroError> {
        let allowed = match effect {
            CompileTimeEffect::DefineNamespace => self.policy.allow_namespace_definitions,
            CompileTimeEffect::DefineMacro => self.policy.allow_macro_definitions,
            CompileTimeEffect::ConstantGeneration => self.policy.allow_constant_generation,
            CompileTimeEffect::HostInterop => self.policy.allow_host_interop,
        };
        if allowed {
            Ok(())
        } else {
            Err(MacroError::new("compile-time effect is not allowed", meta))
        }
    }
}

pub fn gensym(prefix: Option<&str>) -> Symbol {
    let id = GENSYM_COUNTER.fetch_add(1, Ordering::SeqCst) + 1;
    Symbol::parse(&format!("{}__gensym__{id}", prefix.unwrap_or("G__")))
}

pub fn defmacro(state: &mut CompilerState, symbol: Symbol, func: FnValue) {
    state.register_macro(symbol, func);
}

fn resolve_macro<'a>(
    state: &'a CompilerState,
    env: &AnalysisEnv,
    symbol: &Symbol,
) -> Option<&'a Value> {
    if symbol.is_qualified() {
        resolve_qualified_macro(state, symbol)
    } else {
        let qualified = symbol.qualify(env.ns.to_string());
        let current_macro = macro_symbol_for_ns(&env.ns, symbol.name());
        state
            .resolve_var(&current_macro)
            .or_else(|| state.resolve_var(&qualified))
            .or_else(|| state.resolve_var(symbol))
            .or_else(|| {
                state.resolve_var(&Symbol::parse(&format!(
                    "{}{}{}",
                    crate::clorure::core::CORE_MACROS_NS,
                    "/",
                    symbol.name()
                )))
            })
    }
}

fn resolve_qualified_macro<'a>(state: &'a CompilerState, symbol: &Symbol) -> Option<&'a Value> {
    let namespace = symbol.namespace()?;
    let macro_symbol = if namespace.ends_with("$macros") {
        symbol.clone()
    } else {
        macro_symbol_for_ns(&Symbol::parse(namespace), symbol.name())
    };
    state
        .resolve_var(&macro_symbol)
        .or_else(|| state.resolve_var(symbol))
}

fn macro_symbol_for_ns(ns: &Symbol, name: &str) -> Symbol {
    let macro_ns = if ns.to_string().ends_with("$macros") {
        ns.to_string()
    } else {
        format!("{ns}$macros")
    };
    Symbol::parse(&format!("{macro_ns}/{name}"))
}
