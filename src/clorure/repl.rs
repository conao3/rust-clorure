use crate::clorure::analyzer::AnalyzerError;
use crate::clorure::analyzer::analyze;
use crate::clorure::compiler::{BackendTarget, CompiledArtifact, compile};
use crate::clorure::env::{AnalysisEnv, CompilerState};
use crate::clorure::forms::Form;

pub fn eval_form(
    form: Form,
    state: &mut CompilerState,
    env: AnalysisEnv,
) -> Result<CompiledArtifact, AnalyzerError> {
    let ast = analyze(form, state, env)?;
    Ok(compile(&ast, BackendTarget::Bytecode))
}
