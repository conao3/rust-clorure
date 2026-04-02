use std::collections::BTreeMap;

use crate::clorure::env::CompilerEnv;
use crate::clorure::forms::{Form, Symbol};
use crate::clorure::runtime::value::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AstOp {
    Constant,
    Invoke,
    Let,
    Def,
    Fn,
    If,
    Do,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstNode {
    pub op: AstOp,
    pub env: CompilerEnv,
    pub form: Form,
    pub tag: Option<Symbol>,
    pub attrs: BTreeMap<String, Value>,
}
