//! Ocaml language support.

use super::spec::{LanguageDefinition, LanguageSpec};
use super::SupportedLanguage;

/// Declarative definition for Ocaml language support.
pub static DEFINITION: LanguageDefinition = LanguageDefinition {
    language: SupportedLanguage::Ocaml,
    name: "ocaml",
    extensions: &["ml", "mli"],
    filenames: &[],
    grammar: || tree_sitter_ocaml::LANGUAGE_OCAML.into(),
    tags_query: include_str!("tags.scm"),
    locals_query: include_str!("locals.scm"),
    route_overlay: include_str!("routes.scm"),
    spec: &SPEC,
};

/// Declarative AST specification for Ocaml language support.
pub static SPEC: LanguageSpec = LanguageSpec {
    language: SupportedLanguage::Ocaml,
    function_node_kinds: &["let_binding", "value_definition"],
    method_node_kinds: &[],
    class_node_kinds: &[],
    struct_node_kinds: &[],
    interface_node_kinds: &[],
    trait_node_kinds: &[],
    enum_node_kinds: &[],
    module_node_kinds: &["module_definition", "module_binding"],
    type_alias_node_kinds: &["type_definition"],
    name_field: Some("name"),
    comment_prefix: "(*",
    doc_comment_kinds: &["comment"],
    callable_node_kinds: &["let_binding", "value_definition"],
    import_node_kinds: &["open_statement"],
    call_node_kinds: &["application_expression"],
};