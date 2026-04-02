use std::collections::BTreeMap;

use crate::clorure::env::AnalysisEnv;
use crate::clorure::forms::{Form, Symbol};
use crate::clorure::runtime::value::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AstOp {
    Constant,
    SymbolRef,
    Invoke,
    Ns,
    Def,
    Fn,
    Let,
    If,
    Do,
    Loop,
    Recur,
    Quote,
    Var,
    Try,
    Throw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstNode {
    pub op: AstOp,
    pub env: AnalysisEnv,
    pub form: Form,
    pub tag: Option<Symbol>,
    pub children: Vec<AstNode>,
    pub attrs: BTreeMap<String, Value>,
}

impl AstNode {
    pub fn new(op: AstOp, env: AnalysisEnv, form: Form) -> Self {
        Self {
            op,
            env,
            form,
            tag: None,
            children: Vec::new(),
            attrs: BTreeMap::new(),
        }
    }

    pub fn with_tag(mut self, tag: Symbol) -> Self {
        self.tag = Some(tag);
        self
    }

    pub fn with_children(mut self, children: Vec<AstNode>) -> Self {
        self.children = children;
        self
    }

    pub fn with_attr(mut self, key: impl Into<String>, value: Value) -> Self {
        self.attrs.insert(key.into(), value);
        self
    }
}
