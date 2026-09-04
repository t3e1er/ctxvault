//! Polyglot source code parsing, Tree-sitter AST extraction, and structural chunking (`cAST`).

pub mod chunker;
pub mod languages;
pub mod scope;

pub use chunker::{CodeChunker, CodeParseResult};
pub use languages::{detect_language, is_code_file, SupportedLanguage};
pub use scope::normalize_scope_path;
