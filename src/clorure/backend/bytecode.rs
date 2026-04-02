use crate::clorure::ast::AstNode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    Constant,
    Return,
}

pub fn emit(_node: &AstNode) -> Vec<Instruction> {
    vec![Instruction::Constant, Instruction::Return]
}
