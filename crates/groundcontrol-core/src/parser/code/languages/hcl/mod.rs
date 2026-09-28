//! Hcl language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Hcl language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Hcl,
    name: "hcl",
    extensions: &["hcl", "tf", "tfvars"],
    filenames: &[],
    grammar: || tree_sitter_hcl::LANGUAGE.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Hcl language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Hcl,
    function_node_kinds: &["attribute"],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &["block"],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &[],
    type_alias_node_kinds: &[],
    name_field: None,
    comment_prefix: "#",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["block"],
    import_node_kinds: &[],
    call_node_kinds: &["function_call"],
};