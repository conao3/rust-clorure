use std::collections::HashMap;
use std::sync::Arc;

use rust_clorure::clorure::forms::{FormKind, Symbol};
use rust_clorure::clorure::reader::{Reader, ReaderOptions, read_all, read_all_with_options};

#[test]
fn reads_scalars_with_metadata() {
    let forms = read_all("(foo 42 :kw \"bar\")").unwrap();
    let list = &forms[0];
    assert_eq!(list.meta.line, 1);
    assert_eq!(list.meta.column, 1);
    assert_eq!(list.meta.end_line, 1);
    assert_eq!(list.meta.end_column, 18);

    let items = match &list.value {
        FormKind::List(items) => items,
        other => panic!("expected list, got {other:?}"),
    };
    assert_eq!(items.len(), 4);
    assert!(matches!(items[1].value, FormKind::Int(42)));
    assert!(matches!(items[2].value, FormKind::Keyword(_)));
    assert!(matches!(items[3].value, FormKind::String(_)));
}

#[test]
fn incremental_reader_returns_one_form_at_a_time() {
    let mut reader = Reader::new("foo bar");
    let first = reader.read_form().unwrap().unwrap();
    let second = reader.read_form().unwrap().unwrap();
    let end = reader.read_form().unwrap();

    assert!(matches!(first.value, FormKind::Symbol(_)));
    assert!(matches!(second.value, FormKind::Symbol(_)));
    assert!(end.is_none());
}

#[test]
fn reader_conditional_prefers_clorure_then_clj() {
    let forms = read_all("#?(:clj 1 :clorure 2)").unwrap();
    assert_eq!(forms.len(), 1);
    assert!(matches!(forms[0].value, FormKind::Int(2)));

    let fallback = read_all("#?(:clj 7)").unwrap();
    assert!(matches!(fallback[0].value, FormKind::Int(7)));
}

#[test]
fn reader_conditional_splicing_works_inside_collections() {
    let forms = read_all("[1 #?@(:clorure [2 3]) 4]").unwrap();
    let vector = match &forms[0].value {
        FormKind::Vector(items) => items,
        other => panic!("expected vector, got {other:?}"),
    };

    let ints: Vec<i64> = vector
        .iter()
        .map(|form| match form.value {
            FormKind::Int(value) => value,
            ref other => panic!("expected int, got {other:?}"),
        })
        .collect();
    assert_eq!(ints, vec![1, 2, 3, 4]);
}

#[test]
fn syntax_quote_qualifies_symbols_and_expands_gensyms() {
    let options = ReaderOptions {
        current_ns: "demo.core".to_string(),
        ..ReaderOptions::default()
    };
    let forms = read_all_with_options("`(foo bar# ~baz)", options).unwrap();

    let syntax = match &forms[0].value {
        FormKind::SyntaxQuote(inner) => inner,
        other => panic!("expected syntax quote, got {other:?}"),
    };

    let items = match &syntax.value {
        FormKind::List(items) => items,
        other => panic!("expected list inside syntax quote, got {other:?}"),
    };

    match &items[0].value {
        FormKind::Symbol(symbol) => assert_eq!(symbol.to_string(), "demo.core/foo"),
        other => panic!("expected qualified symbol, got {other:?}"),
    }

    match &items[1].value {
        FormKind::Symbol(symbol) => {
            assert_eq!(symbol.namespace(), None);
            assert!(symbol.name().starts_with("bar__gensym__"));
        }
        other => panic!("expected gensym symbol, got {other:?}"),
    }

    match &items[2].value {
        FormKind::Unquote(inner) => match &inner.value {
            FormKind::Symbol(symbol) => assert_eq!(symbol.to_string(), "baz"),
            other => panic!("expected symbol inside unquote, got {other:?}"),
        },
        other => panic!("expected unquote, got {other:?}"),
    }
}

#[test]
fn tagged_literal_registry_can_transform_values() {
    let mut registry = HashMap::new();
    registry.insert(
        "inst".to_string(),
        Arc::new(|value, meta| {
            Ok(rust_clorure::clorure::forms::Form::new(
                FormKind::TaggedLiteral {
                    tag: Symbol::parse("tagged/inst"),
                    value: Box::new(value),
                },
                meta,
            ))
        }) as _,
    );
    let options = ReaderOptions {
        tagged_literals: registry,
        ..ReaderOptions::default()
    };

    let forms = read_all_with_options("#inst \"2024-01-01T00:00:00Z\"", options).unwrap();
    match &forms[0].value {
        FormKind::TaggedLiteral { tag, value } => {
            assert_eq!(tag.to_string(), "tagged/inst");
            assert!(matches!(value.value, FormKind::String(_)));
        }
        other => panic!("expected tagged literal, got {other:?}"),
    }
}
