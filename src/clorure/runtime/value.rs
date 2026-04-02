use std::collections::BTreeMap;
use std::sync::Arc;

use crate::clorure::forms::{Form, FormKind, Keyword, Symbol};
use crate::clorure::runtime::collections::{Counted, ISeq, Seqable};

pub type MetaMap = BTreeMap<Keyword, Value>;
pub type NativeFn = Arc<dyn Fn(&[Value]) -> Result<Value, String> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolValue {
    pub symbol: Symbol,
    pub meta: Option<MetaMap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentList {
    pub items: Arc<Vec<Value>>,
    pub meta: Option<MetaMap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentVector {
    pub items: Arc<Vec<Value>>,
    pub meta: Option<MetaMap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentMap {
    pub entries: Arc<Vec<(Value, Value)>>,
    pub meta: Option<MetaMap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentSet {
    pub entries: Arc<Vec<Value>>,
    pub meta: Option<MetaMap>,
}

#[derive(Clone)]
pub struct FnValue {
    pub name: Option<Symbol>,
    pub arity: usize,
    pub func: NativeFn,
    pub meta: Option<MetaMap>,
}

impl std::fmt::Debug for FnValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FnValue")
            .field("name", &self.name)
            .field("arity", &self.arity)
            .field("meta", &self.meta)
            .finish()
    }
}

impl PartialEq for FnValue {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.arity == other.arity && self.meta == other.meta
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
    Symbol(SymbolValue),
    List(PersistentList),
    Vector(PersistentVector),
    Map(PersistentMap),
    Set(PersistentSet),
    Form(Box<Form>),
    Fn(FnValue),
    Macro(FnValue),
    OpaqueHandle(u64),
}

impl Value {
    pub fn symbol(symbol: Symbol) -> Self {
        Self::Symbol(SymbolValue { symbol, meta: None })
    }

    pub fn list(items: Vec<Value>) -> Self {
        Self::List(PersistentList {
            items: Arc::new(items),
            meta: None,
        })
    }

    pub fn vector(items: Vec<Value>) -> Self {
        Self::Vector(PersistentVector {
            items: Arc::new(items),
            meta: None,
        })
    }

    pub fn map(entries: Vec<(Value, Value)>) -> Self {
        Self::Map(PersistentMap {
            entries: Arc::new(entries),
            meta: None,
        })
    }

    pub fn set(entries: Vec<Value>) -> Self {
        Self::Set(PersistentSet {
            entries: Arc::new(entries),
            meta: None,
        })
    }

    pub fn with_meta(self, meta: MetaMap) -> Self {
        match self {
            Self::Symbol(symbol) => Self::Symbol(SymbolValue {
                symbol: symbol.symbol,
                meta: Some(meta),
            }),
            Self::List(list) => Self::List(PersistentList {
                items: list.items,
                meta: Some(meta),
            }),
            Self::Vector(vector) => Self::Vector(PersistentVector {
                items: vector.items,
                meta: Some(meta),
            }),
            Self::Map(map) => Self::Map(PersistentMap {
                entries: map.entries,
                meta: Some(meta),
            }),
            Self::Set(set) => Self::Set(PersistentSet {
                entries: set.entries,
                meta: Some(meta),
            }),
            Self::Fn(function) => Self::Fn(FnValue {
                meta: Some(meta),
                ..function
            }),
            Self::Macro(function) => Self::Macro(FnValue {
                meta: Some(meta),
                ..function
            }),
            other => other,
        }
    }

    pub fn meta(&self) -> Option<&MetaMap> {
        match self {
            Self::Symbol(symbol) => symbol.meta.as_ref(),
            Self::List(list) => list.meta.as_ref(),
            Self::Vector(vector) => vector.meta.as_ref(),
            Self::Map(map) => map.meta.as_ref(),
            Self::Set(set) => set.meta.as_ref(),
            Self::Fn(function) | Self::Macro(function) => function.meta.as_ref(),
            _ => None,
        }
    }

    pub fn invoke(&self, args: &[Value]) -> Result<Value, String> {
        match self {
            Self::Fn(function) | Self::Macro(function) => (function.func)(args),
            _ => Err("value is not invokable".to_string()),
        }
    }

    pub fn from_form(form: &Form) -> Result<Self, String> {
        match &form.value {
            FormKind::Nil => Ok(Self::Nil),
            FormKind::Bool(value) => Ok(Self::Bool(*value)),
            FormKind::Int(value) => Ok(Self::Int(*value)),
            FormKind::String(value) => Ok(Self::String(value.clone())),
            FormKind::Keyword(value) => Ok(Self::Keyword(value.clone())),
            FormKind::Symbol(value) => Ok(Self::symbol(value.clone())),
            FormKind::List(items) => Ok(Self::list(
                items
                    .iter()
                    .map(Self::from_form)
                    .collect::<Result<_, _>>()?,
            )),
            FormKind::Vector(items) => Ok(Self::vector(
                items
                    .iter()
                    .map(Self::from_form)
                    .collect::<Result<_, _>>()?,
            )),
            FormKind::Map(entries) => Ok(Self::map(
                entries
                    .iter()
                    .map(|(key, value)| Ok((Self::from_form(key)?, Self::from_form(value)?)))
                    .collect::<Result<_, String>>()?,
            )),
            FormKind::Quote(inner)
            | FormKind::SyntaxQuote(inner)
            | FormKind::Unquote(inner)
            | FormKind::UnquoteSplicing(inner) => Ok(Self::Form(inner.clone())),
            FormKind::TaggedLiteral { .. } => Ok(Self::Form(Box::new(form.clone()))),
        }
    }

    pub fn into_form(self, meta: crate::clorure::forms::ReaderMeta) -> Result<Form, String> {
        match self {
            Self::Nil => Ok(Form::new(FormKind::Nil, meta)),
            Self::Bool(value) => Ok(Form::new(FormKind::Bool(value), meta)),
            Self::Int(value) => Ok(Form::new(FormKind::Int(value), meta)),
            Self::String(value) => Ok(Form::new(FormKind::String(value), meta)),
            Self::Keyword(value) => Ok(Form::new(FormKind::Keyword(value), meta)),
            Self::Symbol(value) => Ok(Form::new(FormKind::Symbol(value.symbol), meta)),
            Self::List(value) => Ok(Form::new(
                FormKind::List(
                    value
                        .items
                        .iter()
                        .cloned()
                        .map(|item| item.into_form(meta.clone()))
                        .collect::<Result<_, _>>()?,
                ),
                meta,
            )),
            Self::Vector(value) => Ok(Form::new(
                FormKind::Vector(
                    value
                        .items
                        .iter()
                        .cloned()
                        .map(|item| item.into_form(meta.clone()))
                        .collect::<Result<_, _>>()?,
                ),
                meta,
            )),
            Self::Map(value) => Ok(Form::new(
                FormKind::Map(
                    value
                        .entries
                        .iter()
                        .cloned()
                        .map(|(key, value)| {
                            Ok((key.into_form(meta.clone())?, value.into_form(meta.clone())?))
                        })
                        .collect::<Result<_, String>>()?,
                ),
                meta,
            )),
            Self::Form(form) => Ok(*form),
            Self::Set(_) | Self::Fn(_) | Self::Macro(_) | Self::OpaqueHandle(_) => {
                Err("value cannot be converted into a reader form".to_string())
            }
        }
    }
}

impl ISeq for PersistentList {
    fn first(&self) -> Option<&Value> {
        self.items.first()
    }

    fn rest(&self) -> Vec<Value> {
        self.items.iter().skip(1).cloned().collect()
    }
}

impl ISeq for PersistentVector {
    fn first(&self) -> Option<&Value> {
        self.items.first()
    }

    fn rest(&self) -> Vec<Value> {
        self.items.iter().skip(1).cloned().collect()
    }
}

impl Seqable for PersistentList {
    fn seq(&self) -> Option<Vec<Value>> {
        if self.items.is_empty() {
            None
        } else {
            Some(self.items.as_ref().clone())
        }
    }
}

impl Seqable for PersistentVector {
    fn seq(&self) -> Option<Vec<Value>> {
        if self.items.is_empty() {
            None
        } else {
            Some(self.items.as_ref().clone())
        }
    }
}

impl Seqable for PersistentSet {
    fn seq(&self) -> Option<Vec<Value>> {
        if self.entries.is_empty() {
            None
        } else {
            Some(self.entries.as_ref().clone())
        }
    }
}

impl Counted for PersistentList {
    fn count(&self) -> usize {
        self.items.len()
    }
}

impl Counted for PersistentVector {
    fn count(&self) -> usize {
        self.items.len()
    }
}

impl Counted for PersistentMap {
    fn count(&self) -> usize {
        self.entries.len()
    }
}

impl Counted for PersistentSet {
    fn count(&self) -> usize {
        self.entries.len()
    }
}
