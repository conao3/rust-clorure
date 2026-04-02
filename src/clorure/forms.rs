use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol {
    namespace: Option<String>,
    name: String,
}

impl Symbol {
    pub fn new(namespace: Option<String>, name: impl Into<String>) -> Self {
        Self {
            namespace,
            name: name.into(),
        }
    }

    pub fn parse(input: &str) -> Self {
        if let Some((namespace, name)) = input.split_once('/') {
            Self::new(Some(namespace.to_string()), name.to_string())
        } else {
            Self::new(None, input.to_string())
        }
    }

    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_qualified(&self) -> bool {
        self.namespace.is_some()
    }

    pub fn qualify(&self, namespace: impl Into<String>) -> Self {
        if self.is_qualified() {
            self.clone()
        } else {
            Self::new(Some(namespace.into()), self.name.clone())
        }
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.namespace {
            Some(namespace) => write!(f, "{namespace}/{}", self.name),
            None => write!(f, "{}", self.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Keyword {
    namespace: Option<String>,
    name: String,
}

impl Keyword {
    pub fn new(namespace: Option<String>, name: impl Into<String>) -> Self {
        Self {
            namespace,
            name: name.into(),
        }
    }

    pub fn parse(input: &str) -> Self {
        let raw = input.strip_prefix(':').unwrap_or(input);
        if let Some((namespace, name)) = raw.split_once('/') {
            Self::new(Some(namespace.to_string()), name.to_string())
        } else {
            Self::new(None, raw.to_string())
        }
    }

    pub fn namespace(&self) -> Option<&str> {
        self.namespace.as_deref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.namespace {
            Some(namespace) => write!(f, ":{namespace}/{}", self.name),
            None => write!(f, ":{}", self.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderMeta {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub file: Option<String>,
}

impl ReaderMeta {
    pub fn new(
        line: usize,
        column: usize,
        end_line: usize,
        end_column: usize,
        file: Option<String>,
    ) -> Self {
        Self {
            line,
            column,
            end_line,
            end_column,
            file,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub value: FormKind,
    pub meta: ReaderMeta,
}

impl Form {
    pub fn new(value: FormKind, meta: ReaderMeta) -> Self {
        Self { value, meta }
    }

    pub fn symbol(namespace: Option<String>, name: impl Into<String>, meta: ReaderMeta) -> Self {
        Self::new(FormKind::Symbol(Symbol::new(namespace, name)), meta)
    }

    pub fn keyword(namespace: Option<String>, name: impl Into<String>, meta: ReaderMeta) -> Self {
        Self::new(FormKind::Keyword(Keyword::new(namespace, name)), meta)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormKind {
    Nil,
    Bool(bool),
    Int(i64),
    String(String),
    Keyword(Keyword),
    Symbol(Symbol),
    List(Vec<Form>),
    Vector(Vec<Form>),
    Map(Vec<(Form, Form)>),
    Quote(Box<Form>),
    SyntaxQuote(Box<Form>),
    Unquote(Box<Form>),
    UnquoteSplicing(Box<Form>),
    TaggedLiteral { tag: Symbol, value: Box<Form> },
}
