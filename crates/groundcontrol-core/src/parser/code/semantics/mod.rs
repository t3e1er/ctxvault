//! AST semantics, grammar extraction, identifier normalization, and lexical scoping.

pub mod grammar;
pub mod patterns;
pub mod scope;

pub use grammar::{
    AstGrammarExtractor, DataFlowPath, DataFlowSink, ExtractedGrammarSemantics,
    GenericAstGrammarExtractor, GrammarTransition, WeightedToken,
};
pub use patterns::{expand_abbreviation, extract_semantic_tokens, split_identifier};
pub use scope::{normalize_scope_path, scope_matches};
