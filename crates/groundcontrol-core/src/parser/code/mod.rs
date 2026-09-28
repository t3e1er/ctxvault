//! Polyglot source code parsing, Tree-sitter AST extraction, and structural chunking (`cAST`).

pub mod chunker;
pub mod grammar;
pub mod languages;
pub mod manifest;
pub mod patterns;
pub mod query;
pub mod scope;
pub mod spec;

pub use chunker::{CodeChunker, CodeParseResult};
pub use grammar::{
    AstGrammarExtractor, DataFlowPath, DataFlowSink, ExtractedGrammarSemantics,
    GenericAstGrammarExtractor, GrammarTransition, WeightedToken,
};
pub use languages::{detect_language, is_code_file, SupportedLanguage};
pub use manifest::{detect_manifest_in_dir, find_enclosing_manifest, PackageManifest};
pub use patterns::{extract_semantic_tokens, split_identifier};
pub use query::{get_language_query, ExtractedQueryDefinition, LanguageQuery};
pub use scope::{normalize_scope_path, scope_matches};
pub use spec::{get_language_spec, LanguageSpec};
