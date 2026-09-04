//! Polyglot source code parsing, Tree-sitter AST extraction, and structural chunking (`cAST`).

pub mod chunker;
pub mod languages;

pub use chunker::{CodeChunker, CodeParseResult};
pub use languages::{detect_language, is_code_file, SupportedLanguage};
