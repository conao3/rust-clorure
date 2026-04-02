use crate::clorure::ast::{AstNode, AstOp};
use crate::clorure::env::CompilerEnv;
use crate::clorure::forms::Form;

pub fn analyze(form: Form, env: CompilerEnv) -> AstNode {
    AstNode {
        op: AstOp::Constant,
        env,
        form,
        tag: None,
        attrs: Default::default(),
    }
}
