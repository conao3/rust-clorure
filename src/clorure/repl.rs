use crate::clorure::env::AnalysisEnv;
use crate::clorure::session::{AnalysisOutcome, EvalOutcome, Session};

pub fn eval_str(
    session: &mut Session,
    source: &str,
    env: AnalysisEnv,
) -> Result<EvalOutcome, String> {
    session.eval_str(source, env)
}

pub fn analyze_str(
    session: &mut Session,
    source: &str,
    env: AnalysisEnv,
) -> Result<AnalysisOutcome, String> {
    session
        .analyze_str(source, env)
        .map_err(|error| error.to_string())
}
