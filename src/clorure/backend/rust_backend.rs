use crate::clorure::ast::AstNode;
use crate::clorure::backend::bytecode::emit as emit_bytecode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AotManifest {
    pub ns: String,
    pub entrypoints: Vec<String>,
}

impl Default for AotManifest {
    fn default() -> Self {
        Self {
            ns: "user".to_string(),
            entrypoints: Vec::new(),
        }
    }
}

pub fn emit(node: &AstNode) -> String {
    emit_with_manifest(node, &AotManifest::default())
}

pub fn emit_with_manifest(node: &AstNode, manifest: &AotManifest) -> String {
    let program = emit_bytecode(node);
    let consts = program
        .instructions
        .iter()
        .filter_map(|instruction| match instruction {
            crate::clorure::backend::bytecode::Instruction::Const(value) => {
                Some(format!("        {:?},", value))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "pub struct NamespaceSpec {{ pub name: &'static str }}\n\
         pub struct VarSpec {{ pub name: &'static str }}\n\
         pub struct ConstTable(pub &'static [&'static str]);\n\n\
         pub fn register_generated_ns() -> NamespaceSpec {{\n\
             let _consts = ConstTable(&[\n{consts}\n             ]);\n\
             let _entrypoints = {:?};\n\
             NamespaceSpec {{ name: {:?} }}\n\
         }}\n",
        manifest.entrypoints, manifest.ns
    )
}
