use crate::clorure::ast::{AstNode, AstOp};
use crate::clorure::env::{AnalysisContext, AnalysisEnv, CompilerState, CompilerWarning};
use crate::clorure::forms::{Form, FormKind, Symbol};
use crate::clorure::macro_support::MacroExpander;
use crate::clorure::runtime::value::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzerError {
    pub message: String,
    pub form: Box<Form>,
}

impl AnalyzerError {
    fn new(message: impl Into<String>, form: Form) -> Self {
        Self {
            message: message.into(),
            form: Box::new(form),
        }
    }
}

impl std::fmt::Display for AnalyzerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} at {}:{}",
            self.message, self.form.meta.line, self.form.meta.column
        )
    }
}

impl std::error::Error for AnalyzerError {}

pub fn analyze(
    form: Form,
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    match form.value.clone() {
        FormKind::Nil
        | FormKind::Bool(_)
        | FormKind::Int(_)
        | FormKind::String(_)
        | FormKind::Keyword(_)
        | FormKind::Vector(_)
        | FormKind::Map(_) => analyze_constant(form, state, env),
        FormKind::Symbol(symbol) => analyze_symbol(form, symbol, state, env),
        FormKind::Quote(inner) => analyze_quote(form, inner.as_ref().clone(), state, env),
        FormKind::List(items) => analyze_list(form, items, state, env),
        FormKind::SyntaxQuote(inner) => analyze_quote(form, inner.as_ref().clone(), state, env),
        FormKind::Unquote(_) | FormKind::UnquoteSplicing(_) => Err(AnalyzerError::new(
            "unquote forms are only valid inside syntax-quote",
            form,
        )),
        FormKind::TaggedLiteral { .. } => analyze_constant(form, state, env),
    }
}

fn analyze_constant(
    form: Form,
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    let value =
        Value::from_form(&form).map_err(|message| AnalyzerError::new(message, form.clone()))?;
    let idx = state.add_constant(value.clone());
    Ok(AstNode::new(AstOp::Constant, env, form)
        .with_tag(Symbol::parse("const"))
        .with_attr("constant_index", Value::Int(idx as i64))
        .with_attr("value", value))
}

fn analyze_symbol(
    form: Form,
    symbol: Symbol,
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    if !env.locals.contains_key(symbol.name())
        && !state.vars.contains_key(&symbol)
        && !symbol.is_qualified()
    {
        state.add_warning(CompilerWarning::new(
            "unresolved-symbol",
            format!("symbol '{symbol}' is not defined in the current analysis env"),
            form.meta.clone(),
        ));
    }

    Ok(AstNode::new(AstOp::SymbolRef, env, form).with_attr("symbol", Value::symbol(symbol)))
}

fn analyze_quote(
    form: Form,
    quoted: Form,
    _state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    Ok(AstNode::new(AstOp::Quote, env, form).with_attr("quoted", Value::Form(Box::new(quoted))))
}

fn analyze_list(
    form: Form,
    items: Vec<Form>,
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    if items.is_empty() {
        return analyze_constant(form, state, env);
    }

    let mut expander = MacroExpander::default();
    if let Some(expanded) = expander
        .macroexpand_1(&form, state, &env)
        .map_err(|error| AnalyzerError::new(error.message, Form::new(FormKind::Nil, error.meta)))?
    {
        return analyze(expanded, state, env.enter_macroexpansion());
    }

    let head = items[0].clone();
    if let FormKind::Symbol(symbol) = &head.value {
        return match symbol.to_string().as_str() {
            "ns" => analyze_ns(form, &items[1..], state, env),
            "def" => analyze_def(form, &items[1..], state, env),
            "fn" => analyze_fn(form, &items[1..], state, env),
            "let" => analyze_let(form, &items[1..], state, env),
            "if" => analyze_if(form, &items[1..], state, env),
            "do" => analyze_do(form, &items[1..], state, env),
            "loop" => analyze_loop(form, &items[1..], state, env),
            "recur" => analyze_recur(form, &items[1..], state, env),
            "quote" => analyze_quote_special(form, &items[1..], state, env),
            "var" => analyze_var(form, &items[1..], state, env),
            "try" => analyze_try(form, &items[1..], state, env),
            "throw" => analyze_throw(form, &items[1..], state, env),
            _ => analyze_invoke(form, items, state, env),
        };
    }

    analyze_invoke(form, items, state, env)
}

fn analyze_ns(
    form: Form,
    args: &[Form],
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    let ns_form = args
        .first()
        .cloned()
        .ok_or_else(|| AnalyzerError::new("ns requires a namespace symbol", form.clone()))?;
    let ns_symbol = expect_symbol(ns_form)?;
    state.register_ns(ns_symbol.clone());
    Ok(
        AstNode::new(AstOp::Ns, env.with_ns(ns_symbol.clone()), form)
            .with_tag(Symbol::parse("ns"))
            .with_attr("ns", Value::symbol(ns_symbol)),
    )
}

fn analyze_def(
    form: Form,
    args: &[Form],
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    if args.len() != 2 {
        return Err(AnalyzerError::new(
            "def requires a symbol and a value",
            form,
        ));
    }
    let symbol = expect_symbol(args[0].clone())?;
    let init = analyze(args[1].clone(), state, env.clone())?;
    let qualified = symbol.qualify(env.ns.to_string());
    state.register_var(qualified.clone(), Value::Nil);
    Ok(AstNode::new(AstOp::Def, env, form)
        .with_tag(Symbol::parse("var"))
        .with_children(vec![init])
        .with_attr("name", Value::symbol(qualified)))
}

fn analyze_fn(
    form: Form,
    args: &[Form],
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    let (name, params_form, body_forms) = match args {
        [params, body @ ..] if matches!(params.value, FormKind::Vector(_)) => {
            (None, params.clone(), body)
        }
        [name_form, params, body @ ..] if matches!(params.value, FormKind::Vector(_)) => (
            Some(expect_symbol(name_form.clone())?),
            params.clone(),
            body,
        ),
        _ => {
            return Err(AnalyzerError::new(
                "fn requires an optional name, a vector of params, and a body",
                form,
            ));
        }
    };

    let params = expect_vector_symbols(params_form)?;
    let mut fn_env = env.with_context(AnalysisContext::Return);
    for param in &params {
        fn_env = fn_env.with_local(param.clone());
    }
    let body = analyze_do_body(body_forms, state, fn_env)?;
    let mut node = AstNode::new(AstOp::Fn, env, form)
        .with_children(vec![body])
        .with_attr(
            "params",
            Value::vector(params.into_iter().map(Value::symbol).collect()),
        );
    if let Some(name) = name {
        node = node.with_attr("name", Value::symbol(name));
    }
    Ok(node)
}

fn analyze_let(
    form: Form,
    args: &[Form],
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    let [bindings_form, body @ ..] = args else {
        return Err(AnalyzerError::new("let requires bindings and a body", form));
    };

    let bindings = expect_binding_vector(bindings_form.clone())?;
    let mut let_env = env.clone();
    let mut children = Vec::new();
    for (symbol, init_form) in &bindings {
        let init = analyze(init_form.clone(), state, let_env.clone())?;
        children.push(init);
        let_env = let_env.with_local(symbol.clone());
    }
    children.push(analyze_do_body(body, state, let_env)?);

    Ok(AstNode::new(AstOp::Let, env, form)
        .with_children(children)
        .with_attr(
            "bindings",
            Value::vector(
                bindings
                    .into_iter()
                    .map(|(symbol, _)| Value::symbol(symbol))
                    .collect(),
            ),
        ))
}

fn analyze_if(
    form: Form,
    args: &[Form],
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    if !(2..=3).contains(&args.len()) {
        return Err(AnalyzerError::new(
            "if requires condition, then, and optional else",
            form,
        ));
    }
    let cond = analyze(args[0].clone(), state, env.clone())?;
    let then_branch = analyze(args[1].clone(), state, env.clone())?;
    let else_branch = if args.len() == 3 {
        analyze(args[2].clone(), state, env.clone())?
    } else {
        analyze(
            Form::new(FormKind::Nil, args[1].meta.clone()),
            state,
            env.clone(),
        )?
    };

    Ok(AstNode::new(AstOp::If, env, form).with_children(vec![cond, then_branch, else_branch]))
}

fn analyze_do(
    form: Form,
    args: &[Form],
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    let body = analyze_do_body(args, state, env.clone())?;
    Ok(AstNode::new(AstOp::Do, env, form).with_children(body.children))
}

fn analyze_do_body(
    forms: &[Form],
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    let mut children = Vec::new();
    for (idx, form) in forms.iter().enumerate() {
        let context = if idx + 1 == forms.len() {
            AnalysisContext::Return
        } else {
            AnalysisContext::Statement
        };
        children.push(analyze(form.clone(), state, env.with_context(context))?);
    }
    Ok(AstNode::new(
        AstOp::Do,
        env,
        Form::new(
            FormKind::Nil,
            forms.first().map_or_else(default_meta, |f| f.meta.clone()),
        ),
    )
    .with_children(children))
}

fn analyze_loop(
    form: Form,
    args: &[Form],
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    let [bindings_form, body @ ..] = args else {
        return Err(AnalyzerError::new(
            "loop requires bindings and a body",
            form,
        ));
    };
    let bindings = expect_binding_vector(bindings_form.clone())?;
    let mut loop_env = env.enter_loop();
    let mut children = Vec::new();
    for (symbol, init_form) in &bindings {
        children.push(analyze(init_form.clone(), state, loop_env.clone())?);
        loop_env = loop_env.with_local(symbol.clone());
    }
    children.push(analyze_do_body(body, state, loop_env)?);
    Ok(AstNode::new(AstOp::Loop, env, form).with_children(children))
}

fn analyze_recur(
    form: Form,
    args: &[Form],
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    if env.loop_depth == 0 {
        return Err(AnalyzerError::new(
            "recur is only valid in loop or fn tail position",
            form,
        ));
    }
    let mut children = Vec::new();
    for arg in args {
        children.push(analyze(arg.clone(), state, env.clone())?);
    }
    Ok(AstNode::new(AstOp::Recur, env, form).with_children(children))
}

fn analyze_quote_special(
    form: Form,
    args: &[Form],
    _state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    if args.len() != 1 {
        return Err(AnalyzerError::new("quote requires one form", form));
    }
    Ok(AstNode::new(AstOp::Quote, env, form)
        .with_attr("quoted", Value::Form(Box::new(args[0].clone()))))
}

fn analyze_var(
    form: Form,
    args: &[Form],
    _state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    if args.len() != 1 {
        return Err(AnalyzerError::new("var requires a single symbol", form));
    }
    let symbol = expect_symbol(args[0].clone())?;
    Ok(AstNode::new(AstOp::Var, env, form).with_attr("symbol", Value::symbol(symbol)))
}

fn analyze_try(
    form: Form,
    args: &[Form],
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    if args.is_empty() {
        return Err(AnalyzerError::new("try requires a body", form));
    }
    let mut children = Vec::new();
    for arg in args {
        children.push(analyze(arg.clone(), state, env.clone())?);
    }
    Ok(AstNode::new(AstOp::Try, env, form).with_children(children))
}

fn analyze_throw(
    form: Form,
    args: &[Form],
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    if args.len() != 1 {
        return Err(AnalyzerError::new("throw requires one argument", form));
    }
    let value = analyze(args[0].clone(), state, env.clone())?;
    Ok(AstNode::new(AstOp::Throw, env, form).with_children(vec![value]))
}

fn analyze_invoke(
    form: Form,
    items: Vec<Form>,
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    let mut children = Vec::with_capacity(items.len());
    for item in items {
        children.push(analyze(item, state, env.clone())?);
    }
    Ok(AstNode::new(AstOp::Invoke, env, form).with_children(children))
}

fn expect_symbol(form: Form) -> Result<Symbol, AnalyzerError> {
    match form.value {
        FormKind::Symbol(symbol) => Ok(symbol),
        _ => Err(AnalyzerError::new("expected symbol", form)),
    }
}

fn expect_vector_symbols(form: Form) -> Result<Vec<Symbol>, AnalyzerError> {
    let meta = form.meta.clone();
    match form.value {
        FormKind::Vector(items) => items.into_iter().map(expect_symbol).collect(),
        _ => Err(AnalyzerError::new(
            "expected vector of symbols",
            Form::new(FormKind::Nil, meta),
        )),
    }
}

fn expect_binding_vector(form: Form) -> Result<Vec<(Symbol, Form)>, AnalyzerError> {
    let meta = form.meta.clone();
    match form.value {
        FormKind::Vector(items) => {
            let chunks = items.chunks_exact(2);
            if !chunks.remainder().is_empty() {
                return Err(AnalyzerError::new(
                    "binding vector requires an even number of forms",
                    Form::new(FormKind::Nil, meta),
                ));
            }
            let mut bindings = Vec::new();
            for pair in chunks {
                bindings.push((expect_symbol(pair[0].clone())?, pair[1].clone()));
            }
            Ok(bindings)
        }
        _ => Err(AnalyzerError::new(
            "expected binding vector",
            Form::new(FormKind::Nil, meta),
        )),
    }
}

fn default_meta() -> crate::clorure::forms::ReaderMeta {
    crate::clorure::forms::ReaderMeta::new(1, 1, 1, 1, None)
}
