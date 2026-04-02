use std::collections::BTreeMap;
use std::sync::Arc;

use crate::clorure::forms::{Form, Keyword, Symbol};

pub type MetaMap = BTreeMap<Keyword, Value>;

pub type NativeFn = Arc<dyn Fn(&[Value]) -> Result<Value, String> + Send + Sync>;

#[derive(Clone)]
pub struct FnValue {
    pub name: Option<Symbol>,
    pub arity: usize,
    pub func: NativeFn,
}

impl std::fmt::Debug for FnValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FnValue")
            .field("name", &self.name)
            .field("arity", &self.arity)
            .finish()
    }
}

impl PartialEq for FnValue {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.arity == other.arity
    }
}

impl Eq for FnValue {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    String(String),
    Keyword(Keyword),
    Symbol(Symbol),
    List(Vec<Value>),
    Vector(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Form(Box<Form>),
    Fn(FnValue),
    Macro(FnValue),
}
