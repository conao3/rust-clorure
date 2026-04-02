use crate::clorure::ast::AstNode;
use crate::clorure::backend::{bytecode, rust_backend};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendTarget {
    Bytecode,
    Rust,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompiledArtifact {
    Bytecode(Vec<bytecode::Instruction>),
    Rust(String),
}

pub fn compile(node: &AstNode, target: BackendTarget) -> CompiledArtifact {
    match target {
        BackendTarget::Bytecode => CompiledArtifact::Bytecode(bytecode::emit(node)),
        BackendTarget::Rust => CompiledArtifact::Rust(rust_backend::emit(node)),
    }
}
