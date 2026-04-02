use crate::clorure::analyzer::analyze;
use crate::clorure::compiler::{BackendTarget, CompiledArtifact, compile};
use crate::clorure::env::CompilerEnv;
use crate::clorure::forms::Form;

pub fn eval_form(form: Form, env: CompilerEnv) -> CompiledArtifact {
    let ast = analyze(form, env);
    compile(&ast, BackendTarget::Bytecode)
}
