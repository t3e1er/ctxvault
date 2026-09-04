//! Language detection and Tree-sitter grammar bindings for polyglot codebases.

use std::path::Path;
use tree_sitter::Language;

/// Supported source code languages for AST-aware parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SupportedLanguage {
    /// Rust (`.rs`)
    Rust,
    /// TypeScript (`.ts`, `.mts`, `.cts`)
    TypeScript,
    /// TSX (`.tsx`)
    Tsx,
    /// JavaScript (`.js`, `.jsx`, `.mjs`, `.cjs`)
    JavaScript,
    /// Python (`.py`, `.pyi`)
    Python,
    /// Go (`.go`)
    Go,
    /// C (`.c`, `.h`)
    C,
    /// C++ (`.cpp`, `.hpp`, `.cc`, `.cxx`, `.hh`)
    Cpp,
    /// Java (`.java`)
    Java,
    /// C# (`.cs`)
    CSharp,
    /// Ruby (`.rb`, `.rake`, `.gemspec`)
    Ruby,
    /// PHP (`.php`, `.phtml`)
    Php,
    /// Swift (`.swift`)
    Swift,
    /// Elixir (`.ex`, `.exs`)
    Elixir,
    /// Lua (`.lua`)
    Lua,
    /// Bash / Shell (`.sh`, `.bash`, `.zsh`)
    Bash,
}

impl SupportedLanguage {
    /// Canonical language identifier string (e.g., "rust", "typescript", "python").
    pub fn name(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::JavaScript => "javascript",
            Self::Python => "python",
            Self::Go => "go",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Java => "java",
            Self::CSharp => "csharp",
            Self::Ruby => "ruby",
            Self::Php => "php",
            Self::Swift => "swift",
            Self::Elixir => "elixir",
            Self::Lua => "lua",
            Self::Bash => "bash",
        }
    }

    /// Return the native `tree_sitter::Language` grammar for this language.
    pub fn tree_sitter_language(&self) -> Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::JavaScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::C => tree_sitter_c::LANGUAGE.into(),
            Self::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Self::Java => tree_sitter_java::LANGUAGE.into(),
            Self::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Self::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            Self::Php => tree_sitter_php::LANGUAGE_PHP.into(),
            Self::Swift => tree_sitter_swift::LANGUAGE.into(),
            Self::Elixir => tree_sitter_elixir::LANGUAGE.into(),
            Self::Lua => tree_sitter_lua::LANGUAGE.into(),
            Self::Bash => tree_sitter_bash::LANGUAGE.into(),
        }
    }

    /// Single line comment prefix for scope breadcrumbs.
    pub fn comment_prefix(&self) -> &'static str {
        match self {
            Self::Python | Self::Ruby | Self::Elixir | Self::Bash => "#",
            Self::Lua => "--",
            _ => "//",
        }
    }
}

/// Detect programming language from file path extension.
pub fn detect_language(path: &Path) -> Option<SupportedLanguage> {
    let filename = path.file_name()?.to_str()?.to_lowercase();
    if filename == "gemfile" || filename == "rakefile" {
        return Some(SupportedLanguage::Ruby);
    }

    let ext = path.extension()?.to_str()?.to_lowercase();
    match ext.as_str() {
        "rs" => Some(SupportedLanguage::Rust),
        "ts" | "mts" | "cts" => Some(SupportedLanguage::TypeScript),
        "tsx" => Some(SupportedLanguage::Tsx),
        "js" | "jsx" | "mjs" | "cjs" => Some(SupportedLanguage::JavaScript),
        "py" | "pyi" => Some(SupportedLanguage::Python),
        "go" => Some(SupportedLanguage::Go),
        "c" | "h" => Some(SupportedLanguage::C),
        "cpp" | "hpp" | "cc" | "cxx" | "hh" | "hxx" => Some(SupportedLanguage::Cpp),
        "java" => Some(SupportedLanguage::Java),
        "cs" => Some(SupportedLanguage::CSharp),
        "rb" | "rake" | "gemspec" => Some(SupportedLanguage::Ruby),
        "php" | "phtml" | "php3" | "php4" | "php5" | "phps" => Some(SupportedLanguage::Php),
        "swift" => Some(SupportedLanguage::Swift),
        "ex" | "exs" => Some(SupportedLanguage::Elixir),
        "lua" => Some(SupportedLanguage::Lua),
        "sh" | "bash" | "zsh" | "ksh" => Some(SupportedLanguage::Bash),
        _ => None,
    }
}

/// Determine whether a given file path is an indexable source code file.
pub fn is_code_file(path: &Path) -> bool {
    detect_language(path).is_some()
}
