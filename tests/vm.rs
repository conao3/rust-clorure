use std::sync::Arc;

use rust_clorure::clorure::analyzer::analyze;
use rust_clorure::clorure::backend::bytecode::{Instruction, Vm, emit};
use rust_clorure::clorure::env::{AnalysisEnv, CompilerState};
use rust_clorure::clorure::forms::Symbol;
use rust_clorure::clorure::runtime::value::{FnValue, Value};

fn compile(
    source: &str,
    state: &mut CompilerState,
) -> rust_clorure::clorure::backend::bytecode::BytecodeProgram {
    let form = rust_clorure::clorure::reader::read_all(source)
        .unwrap()
        .remove(0);
    let ast = analyze(form, state, AnalysisEnv::default()).unwrap();
    emit(&ast)
}

#[test]
fn bytecode_compiles_and_executes_let_if_and_call() {
    let mut state = CompilerState::default();
    state.register_var(
        Symbol::parse("+"),
        Value::Fn(FnValue {
            name: Some(Symbol::parse("+")),
            arity: 2,
            func: Arc::new(|args| match args {
                [Value::Int(a), Value::Int(b)] => Ok(Value::Int(a + b)),
                _ => Err("bad args".to_string()),
            }),
            meta: None,
        }),
    );
    let program = compile("(let [x 1 y 2] (if x (+ x y) 0))", &mut state);

    assert!(
        program
            .instructions
            .iter()
            .any(|ins| matches!(ins, Instruction::BranchNil(_)))
    );
    let mut vm = Vm::with_globals(state.vars.clone());
    let result = vm.run(&program).unwrap();
    assert_eq!(result, Value::Int(3));
}

#[test]
fn vm_debug_step_exposes_instruction_trace() {
    let mut state = CompilerState::default();
    let program = compile("42", &mut state);
    let mut vm = Vm::default();
    let trace = vm.step(&program);
    assert!(matches!(trace[0], Instruction::Const(Value::Int(42))));
}
