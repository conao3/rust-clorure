use crate::clorure::ast::{AstNode, AstOp};
use crate::clorure::env::{
    AnalysisContext, AnalysisEnv, CompilerState, CompilerWarning, NamespaceAnalysis,
};
use crate::clorure::forms::{Form, FormKind, Symbol};
use crate::clorure::macro_support::MacroExpander;
use crate::clorure::runtime::value::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzerError {
    pub message: String,
    pub form: Box<Form>,
}

type FnMethod = (Vec<Symbol>, Vec<Form>);
type ParsedFnMethods = (Option<Symbol>, Vec<FnMethod>);

impl AnalyzerError {
    pub(crate) fn new(message: impl Into<String>, form: Form) -> Self {
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
    let resolved = resolve_symbol(state, &env, &symbol);
    if !env.locals.contains_key(symbol.name())
        && !state.vars.contains_key(&resolved)
        && !state.vars.contains_key(&symbol)
        && !symbol.is_qualified()
    {
        state.add_warning(CompilerWarning::new(
            "unresolved-symbol",
            format!("symbol '{symbol}' is not defined in the current analysis env"),
            form.meta.clone(),
        ));
    }

    Ok(AstNode::new(AstOp::SymbolRef, env, form).with_attr("symbol", Value::symbol(resolved)))
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
            "defmacro" => analyze_defmacro(form, &items[1..], state, env),
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
    let analysis = parse_ns_metadata(state, &ns_symbol, &args[1..])?;
    state.register_ns_with_analysis(ns_symbol.clone(), analysis.clone());
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

fn analyze_defmacro(
    form: Form,
    args: &[Form],
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<AstNode, AnalyzerError> {
    let (symbol_form, fn_form) = match args {
        [symbol, fn_form] => (symbol.clone(), fn_form.clone()),
        [symbol, params_form, body @ ..] => (
            symbol.clone(),
            Form::new(
                FormKind::List(
                    std::iter::once(Form::symbol(None, "fn", form.meta.clone()))
                        .chain(std::iter::once(params_form.clone()))
                        .chain(body.iter().cloned())
                        .collect(),
                ),
                form.meta.clone(),
            ),
        ),
        _ => {
            return Err(AnalyzerError::new(
                "defmacro requires a symbol, params, and a body",
                form,
            ));
        }
    };
    let symbol = expect_symbol(symbol_form)?;
    let init = analyze(fn_form, state, env.clone())?;
    let qualified = symbol.qualify(env.ns.to_string());
    state.register_var(qualified.clone(), Value::Nil);
    Ok(AstNode::new(AstOp::DefMacro, env, form)
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
    let (name, methods) = parse_fn_methods(args, &form)?;
    let mut method_bodies = Vec::new();
    let mut method_params = Vec::new();
    for (params, body_forms) in methods {
        let mut fn_env = env.with_context(AnalysisContext::Return);
        for param in &params {
            fn_env = fn_env.with_local(param.clone());
        }
        method_bodies.push(analyze_do_body(&body_forms, state, fn_env)?);
        method_params.push(Value::vector(
            params.into_iter().map(Value::symbol).collect(),
        ));
    }
    let mut node = AstNode::new(AstOp::Fn, env, form)
        .with_children(method_bodies)
        .with_attr("methods", Value::vector(method_params.clone()));
    if let [single] = method_params.as_slice() {
        node = node.with_attr("params", single.clone());
    }
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

fn parse_fn_methods(args: &[Form], form: &Form) -> Result<ParsedFnMethods, AnalyzerError> {
    match args {
        [params, body @ ..] if matches!(params.value, FormKind::Vector(_)) => Ok((
            None,
            vec![(expect_vector_symbols(params.clone())?, body.to_vec())],
        )),
        [name_form, params, body @ ..] if matches!(params.value, FormKind::Vector(_)) => Ok((
            Some(expect_symbol(name_form.clone())?),
            vec![(expect_vector_symbols(params.clone())?, body.to_vec())],
        )),
        methods if methods.iter().all(is_fn_method_form) => Ok((
            None,
            methods
                .iter()
                .cloned()
                .map(parse_fn_method_form)
                .collect::<Result<_, _>>()?,
        )),
        [name_form, methods @ ..]
            if matches!(name_form.value, FormKind::Symbol(_))
                && methods.iter().all(is_fn_method_form) =>
        {
            Ok((
                Some(expect_symbol(name_form.clone())?),
                methods
                    .iter()
                    .cloned()
                    .map(parse_fn_method_form)
                    .collect::<Result<_, _>>()?,
            ))
        }
        _ => Err(AnalyzerError::new(
            "fn requires an optional name and one or more arity clauses",
            form.clone(),
        )),
    }
}

fn is_fn_method_form(form: &Form) -> bool {
    matches!(
        &form.value,
        FormKind::List(items)
            if items
                .first()
                .is_some_and(|first| matches!(first.value, FormKind::Vector(_)))
    )
}

fn parse_fn_method_form(form: Form) -> Result<FnMethod, AnalyzerError> {
    let FormKind::List(items) = form.value else {
        return Err(AnalyzerError::new("fn method must be a list", form));
    };
    let Some((params_form, body_forms)) = items.split_first() else {
        return Err(AnalyzerError::new(
            "fn method requires params and a body",
            Form::new(FormKind::List(items), default_meta()),
        ));
    };
    Ok((
        expect_vector_symbols(params_form.clone())?,
        body_forms.to_vec(),
    ))
}

fn parse_ns_metadata(
    state: &CompilerState,
    ns_symbol: &Symbol,
    clauses: &[Form],
) -> Result<NamespaceAnalysis, AnalyzerError> {
    let mut analysis = NamespaceAnalysis::default();
    for clause in clauses {
        let FormKind::List(items) = &clause.value else {
            return Err(AnalyzerError::new(
                "ns clauses must be lists",
                clause.clone(),
            ));
        };
        let Some(Form {
            value: FormKind::Keyword(kind),
            ..
        }) = items.first()
        else {
            return Err(AnalyzerError::new(
                "ns clauses must start with a keyword",
                clause.clone(),
            ));
        };

        match kind.name() {
            "require" => {
                for spec in &items[1..] {
                    let parsed = parse_require_spec(spec.clone(), false)?;
                    let alias = parsed.alias.unwrap_or_else(|| parsed.ns.name().to_string());
                    analysis.requires.insert(alias.clone(), parsed.ns.clone());
                    for referred in parsed.refer {
                        analysis.uses.insert(
                            referred.name().to_string(),
                            referred.qualify(parsed.ns.to_string()),
                        );
                    }
                    if parsed.refer_all {
                        for public in namespace_publics(state, &parsed.ns) {
                            analysis.uses.insert(public.name().to_string(), public);
                        }
                    }
                    for renamed in parsed.rename {
                        analysis.renames.insert(
                            renamed.1.name().to_string(),
                            renamed.0.qualify(parsed.ns.to_string()),
                        );
                    }
                    if parsed.include_macros
                        || !parsed.refer_macros.is_empty()
                        || !parsed.rename_macros.is_empty()
                    {
                        analysis.require_macros.insert(alias, macro_ns(&parsed.ns));
                    }
                    for referred in parsed.refer_macros {
                        analysis.use_macros.insert(
                            referred.name().to_string(),
                            referred.qualify(macro_ns(&parsed.ns).to_string()),
                        );
                    }
                    if parsed.include_macros || parsed.refer_macros_all {
                        for public in namespace_publics(state, &macro_ns(&parsed.ns)) {
                            analysis
                                .use_macros
                                .insert(public.name().to_string(), public);
                        }
                    }
                    for renamed in parsed.rename_macros {
                        analysis.rename_macros.insert(
                            renamed.1.name().to_string(),
                            renamed.0.qualify(macro_ns(&parsed.ns).to_string()),
                        );
                    }
                }
            }
            "require-macros" => {
                for spec in &items[1..] {
                    let parsed = parse_require_spec(spec.clone(), true)?;
                    let alias = parsed.alias.unwrap_or_else(|| parsed.ns.name().to_string());
                    analysis.require_macros.insert(alias, macro_ns(&parsed.ns));
                    for referred in parsed.refer {
                        analysis.use_macros.insert(
                            referred.name().to_string(),
                            referred.qualify(macro_ns(&parsed.ns).to_string()),
                        );
                    }
                    if parsed.refer_all {
                        for public in namespace_publics(state, &macro_ns(&parsed.ns)) {
                            analysis
                                .use_macros
                                .insert(public.name().to_string(), public);
                        }
                    }
                    for renamed in parsed.rename {
                        analysis.rename_macros.insert(
                            renamed.1.name().to_string(),
                            renamed.0.qualify(macro_ns(&parsed.ns).to_string()),
                        );
                    }
                }
            }
            _ => {}
        }
    }
    if ns_symbol.to_string() == crate::clorure::core::CORE_NS {
        analysis.require_macros.insert(
            crate::clorure::core::CORE_MACROS_NS.to_string(),
            Symbol::parse(crate::clorure::core::CORE_MACROS_NS),
        );
    }
    Ok(analysis)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RequireSpec {
    ns: Symbol,
    alias: Option<String>,
    refer: Vec<Symbol>,
    refer_all: bool,
    refer_macros: Vec<Symbol>,
    refer_macros_all: bool,
    rename: Vec<(Symbol, Symbol)>,
    rename_macros: Vec<(Symbol, Symbol)>,
    include_macros: bool,
}

impl RequireSpec {
    fn new(ns: Symbol) -> Self {
        Self {
            ns,
            alias: None,
            refer: Vec::new(),
            refer_all: false,
            refer_macros: Vec::new(),
            refer_macros_all: false,
            rename: Vec::new(),
            rename_macros: Vec::new(),
            include_macros: false,
        }
    }
}

fn parse_require_spec(form: Form, macro_only: bool) -> Result<RequireSpec, AnalyzerError> {
    let original = form.clone();
    let FormKind::Vector(items) = form.value else {
        return Err(AnalyzerError::new(
            "require specs must be vectors",
            original,
        ));
    };
    let Some(first) = items.first() else {
        return Err(AnalyzerError::new(
            "require specs must include a namespace symbol",
            original,
        ));
    };
    let ns = expect_symbol(first.clone())?;
    let mut spec = RequireSpec::new(ns);

    let mut idx = 1usize;
    while idx < items.len() {
        let FormKind::Keyword(keyword) = &items[idx].value else {
            return Err(AnalyzerError::new(
                "require options must be keywords",
                items[idx].clone(),
            ));
        };
        let option = keyword.name();
        let value = items.get(idx + 1).cloned().ok_or_else(|| {
            AnalyzerError::new("require option is missing a value", items[idx].clone())
        })?;
        match option {
            "as" => {
                spec.alias = Some(expect_symbol(value)?.to_string());
            }
            "refer" => {
                let refer = parse_refer_value(value)?;
                spec.refer = refer.symbols;
                spec.refer_all = refer.all;
            }
            "refer-macros" => {
                let refer = parse_refer_value(value)?;
                spec.refer_macros = refer.symbols;
                spec.refer_macros_all = refer.all;
            }
            "rename" => {
                spec.rename = expect_rename_map(value)?;
            }
            "rename-macros" => {
                spec.rename_macros = expect_rename_map(value)?;
            }
            "include-macros" => {
                spec.include_macros = matches!(value.value, FormKind::Bool(true));
            }
            _ => {}
        }
        idx += 2;
    }

    if macro_only {
        spec.refer.extend(spec.refer_macros.clone());
        spec.rename.extend(spec.rename_macros.clone());
    }

    Ok(spec)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ReferSpec {
    symbols: Vec<Symbol>,
    all: bool,
}

fn parse_refer_value(form: Form) -> Result<ReferSpec, AnalyzerError> {
    match form.value {
        FormKind::Keyword(keyword) if keyword.name() == "all" => Ok(ReferSpec {
            symbols: Vec::new(),
            all: true,
        }),
        FormKind::Vector(items) => Ok(ReferSpec {
            symbols: items
                .into_iter()
                .map(expect_symbol)
                .collect::<Result<_, _>>()?,
            all: false,
        }),
        _ => Err(AnalyzerError::new(
            "expected :all or a vector of symbols",
            form,
        )),
    }
}

fn expect_rename_map(form: Form) -> Result<Vec<(Symbol, Symbol)>, AnalyzerError> {
    let FormKind::Map(entries) = form.value else {
        return Err(AnalyzerError::new("expected a rename map", form));
    };
    entries
        .into_iter()
        .map(|(from, to)| Ok((expect_symbol(from)?, expect_symbol(to)?)))
        .collect()
}

fn macro_ns(ns: &Symbol) -> Symbol {
    if ns.to_string().ends_with("$macros") {
        ns.clone()
    } else {
        Symbol::parse(&format!("{ns}$macros"))
    }
}

fn namespace_publics(state: &CompilerState, ns: &Symbol) -> Vec<Symbol> {
    state
        .namespace_analysis(ns)
        .map(|analysis| analysis.publics.clone())
        .unwrap_or_default()
}

fn resolve_symbol(state: &CompilerState, env: &AnalysisEnv, symbol: &Symbol) -> Symbol {
    if symbol.is_qualified() {
        if let Some(analysis) = state.namespace_analysis(&env.ns) {
            if let Some(alias_target) = symbol
                .namespace()
                .and_then(|alias| analysis.requires.get(alias))
            {
                return Symbol::parse(&format!("{}/{}", alias_target, symbol.name()));
            }
            if let Some(rename_target) = analysis.renames.get(symbol.name()) {
                return rename_target.clone();
            }
        }
        symbol.clone()
    } else if let Some(analysis) = state.namespace_analysis(&env.ns) {
        let current_ns_symbol = symbol.qualify(env.ns.to_string());
        if state.resolve_var(&current_ns_symbol).is_some() {
            return current_ns_symbol;
        }
        if let Some(renamed) = analysis.renames.get(symbol.name()) {
            return renamed.clone();
        }
        analysis
            .uses
            .get(symbol.name())
            .cloned()
            .unwrap_or_else(|| symbol.clone())
    } else {
        symbol.clone()
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
