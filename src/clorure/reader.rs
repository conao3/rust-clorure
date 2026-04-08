use std::collections::HashMap;
use std::sync::Arc;

use crate::clorure::forms::{Form, FormKind, Keyword, ReaderMeta, Symbol};

type ReaderResult<T> = Result<T, ReaderError>;
type TaggedLiteralHandler = Arc<dyn Fn(Form, ReaderMeta) -> ReaderResult<Form> + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderError {
    pub message: String,
    pub meta: ReaderMeta,
}

impl ReaderError {
    fn new(message: impl Into<String>, meta: ReaderMeta) -> Self {
        Self {
            message: message.into(),
            meta,
        }
    }
}

impl std::fmt::Display for ReaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at {}:{}",
            self.message, self.meta.line, self.meta.column
        )
    }
}

impl std::error::Error for ReaderError {}

#[derive(Clone)]
pub struct ReaderOptions {
    pub file: Option<String>,
    pub current_ns: String,
    pub tagged_literals: HashMap<String, TaggedLiteralHandler>,
}

impl std::fmt::Debug for ReaderOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReaderOptions")
            .field("file", &self.file)
            .field("current_ns", &self.current_ns)
            .field("tagged_literals", &self.tagged_literals.len())
            .finish()
    }
}

impl Default for ReaderOptions {
    fn default() -> Self {
        Self {
            file: None,
            current_ns: "user".to_string(),
            tagged_literals: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Cursor {
    offset: usize,
    line: usize,
    column: usize,
}

#[derive(Debug)]
enum ParsedItem {
    Form(Form),
    Splice(Vec<Form>),
}

pub struct Reader {
    chars: Vec<char>,
    index: usize,
    cursor: Cursor,
    last_consumed: Option<Cursor>,
    options: ReaderOptions,
}

impl Reader {
    pub fn new(input: &str) -> Self {
        Self::with_options(input, ReaderOptions::default())
    }

    pub fn with_options(input: &str, options: ReaderOptions) -> Self {
        Self {
            chars: input.chars().collect(),
            index: 0,
            cursor: Cursor {
                offset: 0,
                line: 1,
                column: 1,
            },
            last_consumed: None,
            options,
        }
    }

    pub fn read_form(&mut self) -> ReaderResult<Option<Form>> {
        self.skip_ignored();
        if self.eof() {
            return Ok(None);
        }

        match self.read_item(false)? {
            ParsedItem::Form(form) => Ok(Some(form)),
            ParsedItem::Splice(_) => {
                Err(self.error_here("reader conditional splicing is only valid inside collections"))
            }
        }
    }

    fn read_item(&mut self, in_collection: bool) -> ReaderResult<ParsedItem> {
        self.skip_ignored();
        if self.eof() {
            return Err(self.error_here("unexpected end of input"));
        }

        let start = self.cursor;
        let ch = self
            .peek()
            .ok_or_else(|| self.error_here("unexpected end of input"))?;
        match ch {
            '^' => self.read_metadata(),
            '\'' => {
                self.bump();
                let item = self.read_item(false)?;
                let inner = self.expect_form(item)?;
                Ok(ParsedItem::Form(
                    self.wrap_form(FormKind::Quote(Box::new(inner)), start),
                ))
            }
            '`' => {
                self.bump();
                let item = self.read_item(false)?;
                let inner = self.expect_form(item)?;
                let expanded = self.expand_syntax_quote(inner)?;
                Ok(ParsedItem::Form(self.wrap_form(
                    FormKind::SyntaxQuote(Box::new(expanded)),
                    start,
                )))
            }
            '~' => {
                self.bump();
                let value = if self.peek() == Some('@') {
                    self.bump();
                    let item = self.read_item(false)?;
                    let inner = self.expect_form(item)?;
                    FormKind::UnquoteSplicing(Box::new(inner))
                } else {
                    let item = self.read_item(false)?;
                    let inner = self.expect_form(item)?;
                    FormKind::Unquote(Box::new(inner))
                };
                Ok(ParsedItem::Form(self.wrap_form(value, start)))
            }
            '(' => self.read_list(),
            '[' => self.read_vector(),
            '{' => self.read_map(),
            '"' => self.read_string(),
            '#' => self.read_dispatch(in_collection),
            _ => self.read_atom(),
        }
    }

    fn read_metadata(&mut self) -> ReaderResult<ParsedItem> {
        let start = self.cursor;
        self.expect_char('^')?;
        let metadata_item = self.read_item(false)?;
        let metadata_form = self.expect_form(metadata_item)?;
        let target_item = self.read_item(false)?;
        let target_form = self.expect_form(target_item)?;
        let normalized_meta = self.normalize_metadata_form(metadata_form)?;

        Ok(ParsedItem::Form(Form::new(
            FormKind::List(vec![
                Form::symbol(None, "with-meta", self.meta_from(start)),
                target_form,
                normalized_meta,
            ]),
            self.meta_from(start),
        )))
    }

    fn read_list(&mut self) -> ReaderResult<ParsedItem> {
        let start = self.cursor;
        self.expect_char('(')?;
        let items = self.read_collection_items(')')?;
        Ok(ParsedItem::Form(
            self.wrap_form(FormKind::List(items), start),
        ))
    }

    fn read_vector(&mut self) -> ReaderResult<ParsedItem> {
        let start = self.cursor;
        self.expect_char('[')?;
        let items = self.read_collection_items(']')?;
        Ok(ParsedItem::Form(
            self.wrap_form(FormKind::Vector(items), start),
        ))
    }

    fn read_map(&mut self) -> ReaderResult<ParsedItem> {
        let start = self.cursor;
        self.expect_char('{')?;
        let mut entries = Vec::new();

        loop {
            self.skip_ignored();
            if self.peek() == Some('}') {
                self.bump();
                break;
            }
            if self.eof() {
                return Err(self.error_from(start, "unterminated map"));
            }

            let key_item = self.read_item(false)?;
            let key = self.expect_form(key_item)?;
            self.skip_ignored();
            if self.peek() == Some('}') {
                return Err(self.error_here("map literal requires an even number of forms"));
            }
            let value_item = self.read_item(false)?;
            let value = self.expect_form(value_item)?;
            entries.push((key, value));
        }

        Ok(ParsedItem::Form(
            self.wrap_form(FormKind::Map(entries), start),
        ))
    }

    fn read_collection_items(&mut self, end: char) -> ReaderResult<Vec<Form>> {
        let mut items = Vec::new();
        loop {
            self.skip_ignored();
            match self.peek() {
                Some(ch) if ch == end => {
                    self.bump();
                    break;
                }
                None => {
                    return Err(
                        self.error_here(format!("unterminated collection, expected '{end}'"))
                    );
                }
                _ => match self.read_item(true)? {
                    ParsedItem::Form(form) => items.push(form),
                    ParsedItem::Splice(mut forms) => items.append(&mut forms),
                },
            }
        }
        Ok(items)
    }

    fn read_string(&mut self) -> ReaderResult<ParsedItem> {
        let start = self.cursor;
        self.expect_char('"')?;
        let mut value = String::new();

        while let Some(ch) = self.bump() {
            match ch {
                '"' => {
                    return Ok(ParsedItem::Form(
                        self.wrap_form(FormKind::String(value), start),
                    ));
                }
                '\\' => {
                    let escaped = self
                        .bump()
                        .ok_or_else(|| self.error_here("unterminated escape sequence"))?;
                    let resolved = match escaped {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '\\' => '\\',
                        '"' => '"',
                        other => other,
                    };
                    value.push(resolved);
                }
                other => value.push(other),
            }
        }

        Err(self.error_from(start, "unterminated string"))
    }

    fn read_dispatch(&mut self, in_collection: bool) -> ReaderResult<ParsedItem> {
        let start = self.cursor;
        self.expect_char('#')?;

        match self.peek() {
            Some('?') => {
                self.bump();
                let splicing = if self.peek() == Some('@') {
                    self.bump();
                    true
                } else {
                    false
                };
                let selected = self.read_reader_conditional(start, splicing)?;
                if splicing {
                    if !in_collection {
                        return Err(self.error_from(
                            start,
                            "reader conditional splicing is only valid inside collections",
                        ));
                    }
                    let forms = match selected.value {
                        FormKind::List(items) | FormKind::Vector(items) => items,
                        _ => {
                            return Err(ReaderError::new(
                                "reader conditional splicing requires a list or vector result",
                                selected.meta,
                            ));
                        }
                    };
                    Ok(ParsedItem::Splice(forms))
                } else {
                    Ok(ParsedItem::Form(selected))
                }
            }
            Some(_) => {
                let tag = self.read_tag_symbol()?;
                let item = self.read_item(false)?;
                let value = self.expect_form(item)?;
                let meta = self.meta_from(start);
                if let Some(handler) = self.options.tagged_literals.get(&tag.to_string()) {
                    let mut transformed = handler(value, meta.clone())?;
                    transformed.meta = meta;
                    Ok(ParsedItem::Form(transformed))
                } else {
                    Ok(ParsedItem::Form(Form::new(
                        FormKind::TaggedLiteral {
                            tag,
                            value: Box::new(value),
                        },
                        meta,
                    )))
                }
            }
            None => Err(self.error_from(start, "unterminated dispatch macro")),
        }
    }

    fn read_reader_conditional(&mut self, start: Cursor, _splicing: bool) -> ReaderResult<Form> {
        self.skip_ignored();
        self.expect_char('(')?;
        let mut branches = Vec::new();

        loop {
            self.skip_ignored();
            if self.peek() == Some(')') {
                self.bump();
                break;
            }
            if self.eof() {
                return Err(self.error_from(start, "unterminated reader conditional"));
            }

            let feature_item = self.read_item(false)?;
            let feature_form = self.expect_form(feature_item)?;
            let feature = self.expect_keyword(feature_form)?;
            let form_item = self.read_item(false)?;
            let form = self.expect_form(form_item)?;
            branches.push((feature, form));
        }

        let selected = branches
            .iter()
            .find(|(keyword, _)| keyword.name() == "clorure")
            .or_else(|| branches.iter().find(|(keyword, _)| keyword.name() == "clj"))
            .map(|(_, form)| form.clone())
            .ok_or_else(|| {
                self.error_from(start, "reader conditional requires :clorure or :clj branch")
            })?;

        Ok(selected)
    }

    fn read_tag_symbol(&mut self) -> ReaderResult<Symbol> {
        let token = self.read_token();
        if token.is_empty() {
            return Err(self.error_here("expected tagged literal name"));
        }
        Ok(Symbol::parse(&token))
    }

    fn read_atom(&mut self) -> ReaderResult<ParsedItem> {
        let start = self.cursor;
        let token = self.read_token();
        if token.is_empty() {
            return Err(self.error_here("expected form"));
        }

        let value = match token.as_str() {
            "nil" => FormKind::Nil,
            "true" => FormKind::Bool(true),
            "false" => FormKind::Bool(false),
            _ if token.starts_with(':') => FormKind::Keyword(Keyword::parse(&token)),
            _ if is_integer_literal(&token) => {
                let value = token.parse::<i64>().map_err(|_| {
                    self.error_from(start, format!("invalid integer literal '{token}'"))
                })?;
                FormKind::Int(value)
            }
            _ => FormKind::Symbol(Symbol::parse(&token)),
        };

        Ok(ParsedItem::Form(self.wrap_form(value, start)))
    }

    fn expand_syntax_quote(&self, form: Form) -> ReaderResult<Form> {
        let mut gensyms = HashMap::new();
        let mut next_gensym_id = 0usize;
        self.expand_syntax_quote_inner(form, &mut gensyms, &mut next_gensym_id)
    }

    fn expand_syntax_quote_inner(
        &self,
        form: Form,
        gensyms: &mut HashMap<String, Symbol>,
        next_gensym_id: &mut usize,
    ) -> ReaderResult<Form> {
        let meta = form.meta.clone();
        let value = match form.value {
            FormKind::Symbol(symbol) => {
                if !symbol.is_qualified() && symbol.name().ends_with('#') {
                    let gensym = gensyms
                        .entry(symbol.name().to_string())
                        .or_insert_with(|| {
                            *next_gensym_id += 1;
                            let base = symbol.name().trim_end_matches('#');
                            Symbol::parse(&format!("{base}__gensym__{}", *next_gensym_id))
                        })
                        .clone();
                    FormKind::Symbol(gensym)
                } else if !symbol.is_qualified() {
                    FormKind::Symbol(symbol.qualify(self.options.current_ns.clone()))
                } else {
                    FormKind::Symbol(symbol)
                }
            }
            FormKind::List(items) => {
                let mut expanded = Vec::with_capacity(items.len());
                for item in items {
                    expanded.push(self.expand_syntax_quote_inner(item, gensyms, next_gensym_id)?);
                }
                FormKind::List(expanded)
            }
            FormKind::Vector(items) => {
                let mut expanded = Vec::with_capacity(items.len());
                for item in items {
                    expanded.push(self.expand_syntax_quote_inner(item, gensyms, next_gensym_id)?);
                }
                FormKind::Vector(expanded)
            }
            FormKind::Map(entries) => {
                let mut expanded = Vec::with_capacity(entries.len());
                for (key, value) in entries {
                    expanded.push((
                        self.expand_syntax_quote_inner(key, gensyms, next_gensym_id)?,
                        self.expand_syntax_quote_inner(value, gensyms, next_gensym_id)?,
                    ));
                }
                FormKind::Map(expanded)
            }
            FormKind::Quote(inner) => FormKind::Quote(Box::new(self.expand_syntax_quote_inner(
                *inner,
                gensyms,
                next_gensym_id,
            )?)),
            FormKind::SyntaxQuote(inner) => FormKind::SyntaxQuote(Box::new(
                self.expand_syntax_quote_inner(*inner, gensyms, next_gensym_id)?,
            )),
            FormKind::Unquote(inner) => FormKind::Unquote(inner),
            FormKind::UnquoteSplicing(inner) => FormKind::UnquoteSplicing(inner),
            FormKind::TaggedLiteral { tag, value } => FormKind::TaggedLiteral {
                tag,
                value: Box::new(self.expand_syntax_quote_inner(*value, gensyms, next_gensym_id)?),
            },
            other => other,
        };
        Ok(Form::new(value, meta))
    }

    fn expect_form(&self, item: ParsedItem) -> ReaderResult<Form> {
        match item {
            ParsedItem::Form(form) => Ok(form),
            ParsedItem::Splice(_) => Err(ReaderError::new(
                "reader conditional splicing is only valid inside collections",
                self.meta_here(),
            )),
        }
    }

    fn expect_keyword(&self, form: Form) -> ReaderResult<Keyword> {
        match form.value {
            FormKind::Keyword(keyword) => Ok(keyword),
            _ => Err(ReaderError::new(
                "reader conditional keys must be keywords",
                form.meta,
            )),
        }
    }

    fn normalize_metadata_form(&self, form: Form) -> ReaderResult<Form> {
        let meta = form.meta.clone();
        match form.value {
            FormKind::Map(_) => Ok(form),
            FormKind::Keyword(keyword) => Ok(Form::new(
                FormKind::Map(vec![(
                    Form::new(FormKind::Keyword(keyword), meta.clone()),
                    Form::new(FormKind::Bool(true), meta.clone()),
                )]),
                meta,
            )),
            FormKind::Symbol(symbol) => Ok(Form::new(
                FormKind::Map(vec![(
                    Form::keyword(None, "tag", meta.clone()),
                    Form::new(FormKind::Symbol(symbol), meta.clone()),
                )]),
                meta,
            )),
            FormKind::String(value) => Ok(Form::new(
                FormKind::Map(vec![(
                    Form::keyword(None, "tag", meta.clone()),
                    Form::new(FormKind::String(value), meta.clone()),
                )]),
                meta,
            )),
            _ => Err(ReaderError::new(
                "metadata must be a map, keyword, symbol, or string",
                meta,
            )),
        }
    }

    fn expect_char(&mut self, expected: char) -> ReaderResult<()> {
        match self.bump() {
            Some(ch) if ch == expected => Ok(()),
            _ => Err(self.error_here(format!("expected '{expected}'"))),
        }
    }

    fn read_token(&mut self) -> String {
        let mut token = String::new();
        while let Some(ch) = self.peek() {
            if is_delimiter(ch) || ch.is_whitespace() || ch == ',' {
                break;
            }
            token.push(ch);
            self.bump();
        }
        token
    }

    fn skip_ignored(&mut self) {
        loop {
            while let Some(ch) = self.peek() {
                if ch.is_whitespace() || ch == ',' {
                    self.bump();
                } else {
                    break;
                }
            }

            if self.peek() == Some(';') {
                while let Some(ch) = self.bump() {
                    if ch == '\n' {
                        break;
                    }
                }
                continue;
            }

            break;
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.index).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        let current = self.cursor;
        self.last_consumed = Some(current);
        self.index += 1;
        self.cursor.offset += 1;
        if ch == '\n' {
            self.cursor.line += 1;
            self.cursor.column = 1;
        } else {
            self.cursor.column += 1;
        }
        Some(ch)
    }

    fn eof(&self) -> bool {
        self.index >= self.chars.len()
    }

    fn wrap_form(&self, value: FormKind, start: Cursor) -> Form {
        Form::new(value, self.meta_from(start))
    }

    fn meta_from(&self, start: Cursor) -> ReaderMeta {
        let end = self.last_consumed.unwrap_or(start);
        ReaderMeta::new(
            start.line,
            start.column,
            end.line,
            end.column,
            self.options.file.clone(),
        )
    }

    fn meta_here(&self) -> ReaderMeta {
        ReaderMeta::new(
            self.cursor.line,
            self.cursor.column,
            self.cursor.line,
            self.cursor.column,
            self.options.file.clone(),
        )
    }

    fn error_here(&self, message: impl Into<String>) -> ReaderError {
        ReaderError::new(message, self.meta_here())
    }

    fn error_from(&self, start: Cursor, message: impl Into<String>) -> ReaderError {
        ReaderError::new(message, self.meta_from(start))
    }
}

pub fn read_all(input: &str) -> ReaderResult<Vec<Form>> {
    read_all_with_options(input, ReaderOptions::default())
}

pub fn read_all_with_options(input: &str, options: ReaderOptions) -> ReaderResult<Vec<Form>> {
    let mut reader = Reader::with_options(input, options);
    let mut forms = Vec::new();

    while let Some(form) = reader.read_form()? {
        forms.push(form);
    }

    Ok(forms)
}

fn is_delimiter(ch: char) -> bool {
    matches!(
        ch,
        '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\'' | '`' | '~' | ';'
    )
}

fn is_integer_literal(token: &str) -> bool {
    let digits = token.strip_prefix('-').unwrap_or(token);
    !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit())
}
