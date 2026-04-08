use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::clorure::ast::{AstNode, AstOp};
use crate::clorure::forms::{ReaderMeta, Symbol};
use crate::clorure::runtime::value::{FnValue, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosureMethod {
    pub params: Vec<String>,
    pub rest: Option<String>,
    pub body: Box<BytecodeProgram>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    Const(Value),
    Dup,
    LookupVar(Symbol),
    LookupLocal(String),
    SetLocal(String),
    SetGlobal(Symbol),
    Call(usize),
    TailCall(usize),
    Jump(usize),
    BranchNil(usize),
    MakeClosure {
        params: Vec<String>,
        rest: Option<String>,
        body: Box<BytecodeProgram>,
        captured: Vec<String>,
    },
    MakeMultiClosure {
        methods: Vec<ClosureMethod>,
        captured: Vec<String>,
    },
    InvokeRecur(Vec<String>),
    PushHandler(usize),
    PopHandler,
    Throw,
    Pop,
    Return,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BytecodeProgram {
    pub instructions: Vec<Instruction>,
    pub source_map: Vec<ReaderMeta>,
    pub produced_vars: Vec<Symbol>,
    pub produced_macros: Vec<Symbol>,
}

pub fn emit(node: &AstNode) -> BytecodeProgram {
    let mut compiler = BytecodeCompiler::default();
    compiler.compile_node(node);
    compiler.push(Instruction::Return, node.form.meta.clone());
    BytecodeProgram {
        instructions: compiler.instructions,
        source_map: compiler.source_map,
        produced_vars: compiler.produced_vars,
        produced_macros: compiler.produced_macros,
    }
}

#[derive(Default)]
struct BytecodeCompiler {
    instructions: Vec<Instruction>,
    source_map: Vec<ReaderMeta>,
    produced_vars: Vec<Symbol>,
    produced_macros: Vec<Symbol>,
}

impl BytecodeCompiler {
    fn push(&mut self, instruction: Instruction, meta: ReaderMeta) {
        self.instructions.push(instruction);
        self.source_map.push(meta);
    }

    fn patch(&mut self, index: usize, instruction: Instruction) {
        self.instructions[index] = instruction;
    }

    fn compile_node(&mut self, node: &AstNode) {
        match node.op {
            AstOp::Constant | AstOp::Quote => {
                let value = node
                    .attrs
                    .get("value")
                    .cloned()
                    .or_else(|| node.attrs.get("quoted").cloned())
                    .unwrap_or(Value::Nil);
                self.push(Instruction::Const(value), node.form.meta.clone());
            }
            AstOp::SymbolRef => {
                if let Some(Value::Symbol(symbol)) = node.attrs.get("symbol") {
                    if node.env.locals.contains_key(symbol.symbol.name()) {
                        self.push(
                            Instruction::LookupLocal(symbol.symbol.name().to_string()),
                            node.form.meta.clone(),
                        );
                    } else {
                        self.push(
                            Instruction::LookupVar(symbol.symbol.clone()),
                            node.form.meta.clone(),
                        );
                    }
                }
            }
            AstOp::Ns | AstOp::DefProtocol | AstOp::ExtendType => {
                self.push(Instruction::Const(Value::Nil), node.form.meta.clone());
            }
            AstOp::Def | AstOp::DefMacro => {
                if let Some(child) = node.children.first() {
                    self.compile_node(child);
                } else {
                    self.push(Instruction::Const(Value::Nil), node.form.meta.clone());
                }
                if let Some(Value::Symbol(symbol)) = node.attrs.get("name") {
                    if matches!(node.op, AstOp::DefMacro) {
                        self.produced_macros.push(symbol.symbol.clone());
                    } else {
                        self.produced_vars.push(symbol.symbol.clone());
                    }
                    self.push(
                        Instruction::SetGlobal(symbol.symbol.clone()),
                        node.form.meta.clone(),
                    );
                }
            }
            AstOp::Fn => {
                let captures = node.env.locals.keys().cloned().collect::<Vec<_>>();
                let methods = closure_methods(node);
                if methods.len() <= 1 {
                    let method = methods.into_iter().next().unwrap_or_else(|| ClosureMethod {
                        params: Vec::new(),
                        rest: None,
                        body: Box::new(emit(&node.children.first().cloned().unwrap_or_else(
                            || {
                                AstNode::new(AstOp::Constant, node.env.clone(), node.form.clone())
                                    .with_attr("value", Value::Nil)
                            },
                        ))),
                    });
                    self.push(
                        Instruction::MakeClosure {
                            params: method.params,
                            rest: method.rest,
                            body: method.body,
                            captured: captures,
                        },
                        node.form.meta.clone(),
                    );
                } else {
                    self.push(
                        Instruction::MakeMultiClosure {
                            methods,
                            captured: captures,
                        },
                        node.form.meta.clone(),
                    );
                }
            }
            AstOp::Let | AstOp::Loop => {
                let binding_names = match node.attrs.get("bindings") {
                    Some(Value::Vector(vector)) => vector
                        .items
                        .iter()
                        .filter_map(|value| match value {
                            Value::Symbol(symbol) => Some(symbol.symbol.name().to_string()),
                            _ => None,
                        })
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                let loop_start = self.instructions.len();
                let init_count = binding_names
                    .len()
                    .min(node.children.len().saturating_sub(1));
                for (binding, child) in binding_names
                    .iter()
                    .zip(node.children.iter().take(init_count))
                {
                    self.compile_node(child);
                    self.push(
                        Instruction::SetLocal(binding.clone()),
                        child.form.meta.clone(),
                    );
                }
                if let Some(body) = node.children.get(init_count) {
                    self.compile_node(body);
                } else {
                    self.push(Instruction::Const(Value::Nil), node.form.meta.clone());
                }
                if matches!(node.op, AstOp::Loop) {
                    let recur_targets = find_recur_targets(&self.instructions[loop_start..]);
                    for idx in recur_targets {
                        if let Instruction::InvokeRecur(_) = &self.instructions[loop_start + idx] {
                            self.patch(loop_start + idx, Instruction::Jump(loop_start));
                        }
                    }
                }
            }
            AstOp::If => {
                self.compile_node(&node.children[0]);
                let branch_idx = self.instructions.len();
                self.push(Instruction::BranchNil(usize::MAX), node.form.meta.clone());
                self.compile_node(&node.children[1]);
                let jump_idx = self.instructions.len();
                self.push(Instruction::Jump(usize::MAX), node.form.meta.clone());
                let else_idx = self.instructions.len();
                self.compile_node(&node.children[2]);
                let end_idx = self.instructions.len();
                self.patch(branch_idx, Instruction::BranchNil(else_idx));
                self.patch(jump_idx, Instruction::Jump(end_idx));
            }
            AstOp::Do => {
                if node.children.is_empty() {
                    self.push(Instruction::Const(Value::Nil), node.form.meta.clone());
                }
                for (idx, child) in node.children.iter().enumerate() {
                    self.compile_node(child);
                    if idx + 1 != node.children.len() {
                        self.push(Instruction::Pop, child.form.meta.clone());
                    }
                }
            }
            AstOp::Recur => {
                let binding_names = node.env.locals.keys().cloned().collect::<Vec<_>>();
                for child in &node.children {
                    self.compile_node(child);
                }
                self.push(
                    Instruction::InvokeRecur(binding_names),
                    node.form.meta.clone(),
                );
            }
            AstOp::Var => {
                if let Some(Value::Symbol(symbol)) = node.attrs.get("symbol") {
                    self.push(
                        Instruction::LookupVar(symbol.symbol.clone()),
                        node.form.meta.clone(),
                    );
                }
            }
            AstOp::Try => {
                let has_catch = node.attrs.contains_key("catch_binding");
                let has_finally = node.children.len() > if has_catch { 2 } else { 1 };
                if !has_finally && has_catch {
                    let binding = match node.attrs.get("catch_binding") {
                        Some(Value::Symbol(binding)) => binding,
                        _ => unreachable!(),
                    };
                    let handler_idx = self.instructions.len();
                    self.push(Instruction::PushHandler(usize::MAX), node.form.meta.clone());
                    self.compile_node(&node.children[0]);
                    self.push(Instruction::PopHandler, node.form.meta.clone());
                    let jump_idx = self.instructions.len();
                    self.push(Instruction::Jump(usize::MAX), node.form.meta.clone());
                    let catch_start = self.instructions.len();
                    self.push(
                        Instruction::SetLocal(binding.symbol.name().to_string()),
                        node.form.meta.clone(),
                    );
                    self.push(Instruction::Pop, node.form.meta.clone());
                    self.compile_node(&node.children[1]);
                    let end_idx = self.instructions.len();
                    self.patch(handler_idx, Instruction::PushHandler(catch_start));
                    self.patch(jump_idx, Instruction::Jump(end_idx));
                } else if !has_finally {
                    let handler_idx = self.instructions.len();
                    self.push(Instruction::PushHandler(usize::MAX), node.form.meta.clone());
                    for (idx, child) in node.children.iter().enumerate() {
                        self.compile_node(child);
                        if idx + 1 != node.children.len() {
                            self.push(Instruction::Pop, child.form.meta.clone());
                        }
                    }
                    self.push(Instruction::PopHandler, node.form.meta.clone());
                    let end_idx = self.instructions.len();
                    self.patch(handler_idx, Instruction::PushHandler(end_idx));
                } else {
                    let result_slot = format!("__try_result_{}", self.instructions.len());
                    let error_slot = format!("__try_error_{}", self.instructions.len());
                    let finally_idx = if has_catch { 2 } else { 1 };
                    let finally_node = &node.children[finally_idx];
                    let handler_idx = self.instructions.len();
                    self.push(Instruction::PushHandler(usize::MAX), node.form.meta.clone());
                    self.compile_node(&node.children[0]);
                    self.push(Instruction::PopHandler, node.form.meta.clone());
                    self.push(Instruction::SetLocal(result_slot.clone()), node.form.meta.clone());
                    self.push(Instruction::Pop, node.form.meta.clone());
                    self.compile_node(finally_node);
                    self.push(Instruction::Pop, node.form.meta.clone());
                    self.push(Instruction::LookupLocal(result_slot.clone()), node.form.meta.clone());
                    let end_jump_idx = self.instructions.len();
                    self.push(Instruction::Jump(usize::MAX), node.form.meta.clone());

                    let catch_start = self.instructions.len();
                    if has_catch {
                        let binding = match node.attrs.get("catch_binding") {
                            Some(Value::Symbol(binding)) => binding,
                            _ => unreachable!(),
                        };
                        self.push(
                            Instruction::SetLocal(binding.symbol.name().to_string()),
                            node.form.meta.clone(),
                        );
                        self.push(Instruction::Pop, node.form.meta.clone());
                        let catch_throw_idx = self.instructions.len();
                        self.push(Instruction::PushHandler(usize::MAX), node.form.meta.clone());
                        self.compile_node(&node.children[1]);
                        self.push(Instruction::PopHandler, node.form.meta.clone());
                        self.push(Instruction::SetLocal(result_slot.clone()), node.form.meta.clone());
                        self.push(Instruction::Pop, node.form.meta.clone());
                        self.compile_node(finally_node);
                        self.push(Instruction::Pop, node.form.meta.clone());
                        self.push(Instruction::LookupLocal(result_slot.clone()), node.form.meta.clone());
                        let catch_end_jump_idx = self.instructions.len();
                        self.push(Instruction::Jump(usize::MAX), node.form.meta.clone());

                        let finally_rethrow_start = self.instructions.len();
                        self.push(Instruction::SetLocal(error_slot.clone()), node.form.meta.clone());
                        self.push(Instruction::Pop, node.form.meta.clone());
                        self.compile_node(finally_node);
                        self.push(Instruction::Pop, node.form.meta.clone());
                        self.push(Instruction::LookupLocal(error_slot.clone()), node.form.meta.clone());
                        self.push(Instruction::Throw, node.form.meta.clone());
                        let end_idx = self.instructions.len();

                        self.patch(handler_idx, Instruction::PushHandler(catch_start));
                        self.patch(catch_throw_idx, Instruction::PushHandler(finally_rethrow_start));
                        self.patch(end_jump_idx, Instruction::Jump(end_idx));
                        self.patch(catch_end_jump_idx, Instruction::Jump(end_idx));
                    } else {
                        self.push(Instruction::SetLocal(error_slot.clone()), node.form.meta.clone());
                        self.push(Instruction::Pop, node.form.meta.clone());
                        self.compile_node(finally_node);
                        self.push(Instruction::Pop, node.form.meta.clone());
                        self.push(Instruction::LookupLocal(error_slot.clone()), node.form.meta.clone());
                        self.push(Instruction::Throw, node.form.meta.clone());
                        let end_idx = self.instructions.len();
                        self.patch(handler_idx, Instruction::PushHandler(catch_start));
                        self.patch(end_jump_idx, Instruction::Jump(end_idx));
                    }
                }
            }
            AstOp::Throw => {
                if let Some(child) = node.children.first() {
                    self.compile_node(child);
                } else {
                    self.push(Instruction::Const(Value::Nil), node.form.meta.clone());
                }
                self.push(Instruction::Throw, node.form.meta.clone());
            }
            AstOp::Invoke => {
                for child in &node.children {
                    self.compile_node(child);
                }
                let argc = node.children.len().saturating_sub(1);
                self.push(Instruction::Call(argc), node.form.meta.clone());
            }
        }
    }
}

fn find_recur_targets(instructions: &[Instruction]) -> Vec<usize> {
    let mut indices = Vec::new();
    for (idx, instruction) in instructions.iter().enumerate() {
        if matches!(instruction, Instruction::InvokeRecur(_)) {
            indices.push(idx);
        }
    }
    indices
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackTraceFrame {
    pub ip: usize,
    pub span: ReaderMeta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmError {
    pub message: String,
    pub stacktrace: Vec<StackTraceFrame>,
}

pub type SharedGlobals = Arc<Mutex<HashMap<Symbol, Value>>>;

#[derive(Debug, Clone, Default)]
pub struct Vm {
    globals: SharedGlobals,
    pub debug_mode: bool,
}

impl Vm {
    pub fn with_globals(globals: HashMap<Symbol, Value>) -> Self {
        Self {
            globals: Arc::new(Mutex::new(globals)),
            debug_mode: false,
        }
    }

    pub fn with_shared_globals(globals: SharedGlobals) -> Self {
        Self {
            globals,
            debug_mode: false,
        }
    }

    pub fn snapshot_globals(&self) -> HashMap<Symbol, Value> {
        self.globals
            .lock()
            .expect("globals mutex should not be poisoned")
            .clone()
    }

    pub fn run(&mut self, program: &BytecodeProgram) -> Result<Value, VmError> {
        self.run_with_locals(program, HashMap::new())
    }

    pub fn run_with_locals(
        &mut self,
        program: &BytecodeProgram,
        mut locals: HashMap<String, Value>,
    ) -> Result<Value, VmError> {
        let mut stack = Vec::new();
        let mut ip = 0usize;
        let mut handlers = Vec::new();

        loop {
            let Some(instruction) = program.instructions.get(ip).cloned() else {
                return Ok(Value::Nil);
            };
            if self.debug_mode {
                let _ = &instruction;
            }
            match instruction {
                Instruction::Const(value) => stack.push(value),
                Instruction::Dup => {
                    let value = stack
                        .last()
                        .cloned()
                        .ok_or_else(|| self.error(program, ip, "stack underflow".to_string()))?;
                    stack.push(value);
                }
                Instruction::LookupVar(symbol) => {
                    let value = self
                        .globals
                        .lock()
                        .expect("globals mutex should not be poisoned")
                        .get(&symbol)
                        .cloned()
                        .ok_or_else(|| {
                            self.error(program, ip, format!("unknown global '{symbol}'"))
                        })?;
                    stack.push(value);
                }
                Instruction::LookupLocal(name) => {
                    let value = locals.get(&name).cloned().ok_or_else(|| {
                        self.error(program, ip, format!("unknown local '{name}'"))
                    })?;
                    stack.push(value);
                }
                Instruction::SetLocal(name) => {
                    let value = stack
                        .pop()
                        .ok_or_else(|| self.error(program, ip, "stack underflow".to_string()))?;
                    locals.insert(name, value.clone());
                    stack.push(value);
                }
                Instruction::SetGlobal(symbol) => {
                    let value = stack
                        .pop()
                        .ok_or_else(|| self.error(program, ip, "stack underflow".to_string()))?;
                    self.globals
                        .lock()
                        .expect("globals mutex should not be poisoned")
                        .insert(symbol, value.clone());
                    stack.push(value);
                }
                Instruction::Call(argc) | Instruction::TailCall(argc) => {
                    let mut args = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(stack.pop().ok_or_else(|| {
                            self.error(program, ip, "stack underflow".to_string())
                        })?);
                    }
                    args.reverse();
                    let callee = stack
                        .pop()
                        .ok_or_else(|| self.error(program, ip, "missing callee".to_string()))?;
                    let result = match callee {
                        Value::Fn(function) | Value::Macro(function) => (function.func)(&args)
                            .map_err(|message| self.error(program, ip, message))?,
                        other => {
                            return Err(self.error(
                                program,
                                ip,
                                format!("cannot call non-function value '{other:?}'"),
                            ));
                        }
                    };
                    stack.push(result);
                }
                Instruction::Jump(target) => {
                    ip = target;
                    continue;
                }
                Instruction::BranchNil(target) => {
                    let value = stack
                        .pop()
                        .ok_or_else(|| self.error(program, ip, "stack underflow".to_string()))?;
                    if matches!(value, Value::Nil | Value::Bool(false)) {
                        ip = target;
                        continue;
                    }
                }
                Instruction::MakeClosure {
                    params,
                    rest,
                    body,
                    captured,
                } => {
                    let mut captured_locals = HashMap::new();
                    for name in captured {
                        if let Some(value) = locals.get(&name).cloned() {
                            captured_locals.insert(name, value);
                        }
                    }
                    let body = (*body).clone();
                    let shared_globals = Arc::clone(&self.globals);
                    let closure = FnValue {
                        name: None,
                        arity: if rest.is_some() {
                            usize::MAX
                        } else {
                            params.len()
                        },
                        func: Arc::new(move |args| {
                            if rest.is_none() && args.len() != params.len() {
                                return Err(format!(
                                    "no matching arity for {} argument(s)",
                                    args.len()
                                ));
                            }
                            if rest.is_some() && args.len() < params.len() {
                                return Err(format!(
                                    "no matching arity for {} argument(s)",
                                    args.len()
                                ));
                            }
                            let mut vm = Vm::with_shared_globals(Arc::clone(&shared_globals));
                            let mut frame = captured_locals.clone();
                            for (param, value) in params.iter().zip(args.iter()) {
                                frame.insert(param.clone(), value.clone());
                            }
                            if let Some(rest) = &rest {
                                frame.insert(
                                    rest.clone(),
                                    Value::list(args[params.len()..].to_vec()),
                                );
                            }
                            vm.run_with_locals(&body, frame)
                                .map_err(|error| error.message)
                        }),
                        meta: None,
                    };
                    stack.push(Value::Fn(closure));
                }
                Instruction::MakeMultiClosure { methods, captured } => {
                    let mut captured_locals = HashMap::new();
                    for name in captured {
                        if let Some(value) = locals.get(&name).cloned() {
                            captured_locals.insert(name, value);
                        }
                    }
                    let shared_globals = Arc::clone(&self.globals);
                    let closure = FnValue {
                        name: None,
                        arity: usize::MAX,
                        func: Arc::new(move |args| {
                            let Some(method) = select_method(&methods, args.len()) else {
                                return Err(format!(
                                    "no matching arity for {} argument(s)",
                                    args.len()
                                ));
                            };
                            let mut vm = Vm::with_shared_globals(Arc::clone(&shared_globals));
                            let mut frame = captured_locals.clone();
                            for (param, value) in method.params.iter().zip(args.iter()) {
                                frame.insert(param.clone(), value.clone());
                            }
                            if let Some(rest) = &method.rest {
                                frame.insert(
                                    rest.clone(),
                                    Value::list(args[method.params.len()..].to_vec()),
                                );
                            }
                            vm.run_with_locals(&method.body, frame)
                                .map_err(|error| error.message)
                        }),
                        meta: None,
                    };
                    stack.push(Value::Fn(closure));
                }
                Instruction::InvokeRecur(bindings) => {
                    let mut values = Vec::with_capacity(bindings.len());
                    for _ in 0..bindings.len() {
                        values.push(stack.pop().ok_or_else(|| {
                            self.error(program, ip, "stack underflow".to_string())
                        })?);
                    }
                    values.reverse();
                    for (binding, value) in bindings.into_iter().zip(values) {
                        locals.insert(binding, value);
                    }
                    ip = 0;
                    continue;
                }
                Instruction::PushHandler(target) => handlers.push(target),
                Instruction::PopHandler => {
                    let _ = handlers.pop();
                }
                Instruction::Throw => {
                    let value = stack
                        .pop()
                        .ok_or_else(|| self.error(program, ip, "stack underflow".to_string()))?;
                    if let Some(target) = handlers.pop() {
                        stack.push(value);
                        ip = target;
                        continue;
                    }
                    return Err(self.error(program, ip, format!("uncaught throw: {value:?}")));
                }
                Instruction::Pop => {
                    let _ = stack.pop();
                }
                Instruction::Return => return Ok(stack.pop().unwrap_or(Value::Nil)),
            }
            ip += 1;
        }
    }

    fn error(&self, program: &BytecodeProgram, ip: usize, message: String) -> VmError {
        let span = program
            .source_map
            .get(ip)
            .cloned()
            .unwrap_or_else(|| ReaderMeta::new(1, 1, 1, 1, None));
        VmError {
            message,
            stacktrace: vec![StackTraceFrame { ip, span }],
        }
    }

    pub fn step(&mut self, program: &BytecodeProgram) -> Vec<Instruction> {
        let mut seen = HashSet::new();
        let mut trace = Vec::new();
        for instruction in &program.instructions {
            if seen.insert(trace.len()) {
                trace.push(instruction.clone());
            }
        }
        trace
    }
}

fn closure_methods(node: &AstNode) -> Vec<ClosureMethod> {
    let method_params = match node.attrs.get("methods") {
        Some(Value::Vector(vector)) => vector
            .items
            .iter()
            .filter_map(|method| match method {
                Value::Vector(params) => Some(
                    params
                        .items
                        .iter()
                        .filter_map(|value| match value {
                            Value::Symbol(symbol) => Some(symbol.symbol.name().to_string()),
                            _ => None,
                        })
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let method_rests = match node.attrs.get("rests") {
        Some(Value::Vector(vector)) => vector
            .items
            .iter()
            .map(|value| match value {
                Value::Symbol(symbol) => Some(symbol.symbol.name().to_string()),
                Value::Nil => None,
                _ => None,
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };

    node.children
        .iter()
        .enumerate()
        .map(|(idx, body)| ClosureMethod {
            params: method_params.get(idx).cloned().unwrap_or_default(),
            rest: method_rests.get(idx).cloned().unwrap_or(None),
            body: Box::new(emit(body)),
        })
        .collect()
}

fn select_method(methods: &[ClosureMethod], argc: usize) -> Option<&ClosureMethod> {
    methods
        .iter()
        .find(|method| method.rest.is_none() && method.params.len() == argc)
        .or_else(|| {
            methods
                .iter()
                .find(|method| method.rest.is_some() && argc >= method.params.len())
        })
}
