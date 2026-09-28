//! Verilog language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Verilog language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Verilog,
    name: "verilog",
    extensions: &["v", "sv", "svh"],
    filenames: &[],
    grammar: || tree_sitter_verilog::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Verilog language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Verilog,
    function_node_kinds: &["task_declaration", "function_declaration"],
    method_node_kinds: &[],
    class_node_kinds: &["class_declaration"],
    struct_node_kinds: &[],
    interface_node_kinds: &["interface_declaration"],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &["module_declaration"],
    type_alias_node_kinds: &[],
    name_field: None,
    comment_prefix: "//",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["task_declaration", "function_declaration", "module_declaration"],
    import_node_kinds: &["include_compiler_directive"],
    call_node_kinds: &["system_tf_call", "tf_call"],
};