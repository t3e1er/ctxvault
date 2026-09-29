//! Pure-Rust In-Engine "Hybrid LSP": Lexical Scope & Static Type Resolver.
//!
//! Tracks lexical scope stacks, variable type bindings, and imported symbols
//! directly on Tree-sitter ASTs to disambiguate method calls and receiver invocations
//! (e.g., `client.query(...)` -> `SearchClient > query`) with [`ResolutionConfidence::High`](groundcontrol_common::types::ResolutionConfidence::High),
//! eliminating speculative cross-file call edges without external LSP daemons.
//!
//! # Cross-corpus scope
//!
//! `TypeEnvironment` is an *intra-file* resolver: its scope frames and type
//! bindings are built and consumed while walking a single file's AST during
//! extraction, and are not retained past that. It therefore has no cross-corpus
//! symbol table of its own. The cross-corpus resolver trust ladder
//! (`ResolverKind` in [`crate::corpus_manager`]) reserves a `HybridLsp` tier for
//! when in-engine LSP-grade data becomes queryable across corpora, but keeps the
//! *live* ladder at SCIP → qualified-name so no do-nothing tier is introduced.

use std::collections::HashMap;

use crate::parser::code::manifest::PackageManifest;

/// Lexical scope frame tracking variable types, imports, and monikers within a block.
#[derive(Debug, Clone, Default)]
pub struct ScopeFrame {
    /// Local variable name -> Inferred Type Name (e.g., "client" -> "SearchEngine").
    pub variables: HashMap<String, String>,
    /// Imported symbol name -> Module / Import path (e.g., "SearchEngine" -> "crate::search::engine").
    pub imports: HashMap<String, String>,
    /// Symbol/variable name -> SCIP moniker URI.
    pub monikers: HashMap<String, String>,
    /// Enclosing struct/class/impl type name if inside a container, method, or impl block.
    pub enclosing_type: Option<String>,
}

/// Lexical scope stack and type environment.
#[derive(Debug, Clone, Default)]
pub struct TypeEnvironment {
    frames: Vec<ScopeFrame>,
    manifest: Option<PackageManifest>,
}

impl TypeEnvironment {
    /// Create a new language-agnostic type environment for a file.
    pub fn new() -> Self {
        Self { frames: vec![ScopeFrame::default()], manifest: None }
    }

    /// Attach a package manifest to the type environment for moniker synthesis.
    pub fn with_manifest(mut self, manifest: Option<PackageManifest>) -> Self {
        self.manifest = manifest;
        self
    }

    /// Set or update the package manifest.
    pub fn set_manifest(&mut self, manifest: Option<PackageManifest>) {
        self.manifest = manifest;
    }

    /// Get the associated package manifest if present.
    pub fn manifest(&self) -> Option<&PackageManifest> {
        self.manifest.as_ref()
    }

    /// Push a new nested lexical scope frame (e.g. entering function, class, or block).
    pub fn push_scope(&mut self, enclosing_type: Option<String>) {
        let mut frame = ScopeFrame::default();
        if let Some(enc) = enclosing_type {
            frame.enclosing_type = Some(enc);
        } else {
            frame.enclosing_type = self.current_enclosing_type();
        }
        self.frames.push(frame);
    }

    /// Pop the topmost scope frame.
    pub fn pop_scope(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    /// Get the current active enclosing container/type name (if any).
    pub fn current_enclosing_type(&self) -> Option<String> {
        self.frames.iter().rev().find_map(|f| f.enclosing_type.clone())
    }

    /// Register an imported symbol name and its source import path into file scope.
    pub fn register_import(&mut self, symbol_name: String, source_path: String) {
        if let Some(file_frame) = self.frames.first_mut() {
            file_frame.imports.insert(symbol_name, source_path);
        }
    }

    /// Register a variable binding in the current innermost scope frame.
    pub fn register_variable(&mut self, var_name: String, type_name: String) {
        if let Some(frame) = self.frames.last_mut() {
            frame.variables.insert(var_name, type_name);
        }
    }

    /// Look up the inferred type of a variable or receiver by walking up the scope stack.
    pub fn resolve_variable_type(&self, var_name: &str) -> Option<String> {
        // Special case: `self` or `this` maps to current enclosing type
        if var_name == "self" || var_name == "this" {
            return self.current_enclosing_type();
        }

        for frame in self.frames.iter().rev() {
            if let Some(t) = frame.variables.get(var_name) {
                return Some(t.clone());
            }
        }
        None
    }

    /// Look up the import source of a symbol name.
    pub fn resolve_import_source(&self, symbol_name: &str) -> Option<String> {
        self.frames.first().and_then(|f| f.imports.get(symbol_name).cloned())
    }

    /// Register a SCIP moniker in the current innermost scope frame.
    pub fn register_moniker(&mut self, symbol_name: String, moniker: String) {
        if let Some(frame) = self.frames.last_mut() {
            frame.monikers.insert(symbol_name, moniker);
        }
    }

    /// Look up a SCIP moniker by walking up the scope stack.
    pub fn resolve_moniker(&self, symbol_name: &str) -> Option<String> {
        for frame in self.frames.iter().rev() {
            if let Some(m) = frame.monikers.get(symbol_name) {
                return Some(m.clone());
            }
        }
        None
    }

    /// Synthesize a method call moniker for a receiver with known type bindings.
    pub fn resolve_receiver_moniker(
        &self,
        receiver_name: &str,
        method_name: &str,
        file_path: &str,
    ) -> Option<String> {
        let type_name = self.resolve_variable_type(receiver_name)?;
        Some(crate::graph::scip::synthesize_moniker(
            self.manifest.as_ref(),
            file_path,
            Some(&type_name),
            method_name,
            groundcontrol_common::types::CodeSymbolType::Method,
        ))
    }
}

/// Clean a raw type string by stripping references, pointers, modifiers, unwrapping
/// container wrappers (e.g. `Arc<Mutex<T>>` -> `T`), and stripping generic parameters.
pub fn clean_type_name(raw: &str) -> String {
    let mut cleaned = raw.trim();

    // Loop to peel references, pointers, and container wrappers
    loop {
        cleaned = cleaned.trim();
        cleaned = cleaned.trim_start_matches(['&', '*', ':']).trim();

        if let Some(rest) = cleaned.strip_prefix("const ") {
            cleaned = rest.trim();
            continue;
        }
        if let Some(rest) = cleaned.strip_prefix("mut ") {
            cleaned = rest.trim();
            continue;
        }

        // If ends with struct literal or call args, strip them: e.g. "SearchClient { ... }" or "SearchClient()"
        if let Some(idx) = cleaned.find(['{', '(']) {
            cleaned = cleaned[..idx].trim();
        }

        // Extract the base identifier to check for container wrappers like Arc<T>, Mutex<T>, Box<T>
        let base_ident = cleaned
            .rsplit("::")
            .next()
            .unwrap_or(cleaned)
            .rsplit('.')
            .next()
            .unwrap_or(cleaned)
            .trim();

        if let Some(open_idx) = base_ident.find('<') {
            let wrapper = base_ident[..open_idx].trim();
            if matches!(
                wrapper,
                "Arc"
                    | "Rc"
                    | "Box"
                    | "Mutex"
                    | "RwLock"
                    | "RefCell"
                    | "Cell"
                    | "Pin"
                    | "Option"
                    | "Result"
                    | "Weak"
            ) {
                if let Some(close_idx) = cleaned.rfind('>') {
                    let inner_start = cleaned.len() - base_ident.len() + open_idx + 1;
                    if inner_start < close_idx {
                        let inner = &cleaned[inner_start..close_idx];
                        // Take the primary type argument (e.g. T in Result<T, E>)
                        let first_arg = inner.split(',').next().unwrap_or(inner).trim();
                        cleaned = first_arg;
                        continue;
                    }
                }
            } else {
                // Not a container wrapper: drop generic parameters (e.g. SearchClient<T> -> SearchClient)
                let prefix_len = cleaned.len() - base_ident.len();
                cleaned = cleaned[..prefix_len + open_idx].trim();
            }
        }
        break;
    }

    cleaned
        .rsplit("::")
        .next()
        .unwrap_or(cleaned)
        .rsplit('.')
        .next()
        .unwrap_or(cleaned)
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_type_name() {
        assert_eq!(clean_type_name("&mut SearchClient"), "SearchClient");
        assert_eq!(clean_type_name("*const u8"), "u8");
        assert_eq!(clean_type_name(": SearchEngine"), "SearchEngine");
        assert_eq!(clean_type_name("crate::search::SearchEngine"), "SearchEngine");
        assert_eq!(clean_type_name("Option<SearchEngine>"), "SearchEngine");
        assert_eq!(clean_type_name("Arc<Mutex<SearchEngine>>"), "SearchEngine");
        assert_eq!(clean_type_name("SearchClient<T>"), "SearchClient");
    }

    #[test]
    fn test_lexical_scope_stack() {
        let mut env = TypeEnvironment::new();
        env.push_scope(Some("ParentClass".to_string()));
        assert_eq!(env.current_enclosing_type(), Some("ParentClass".to_string()));
        assert_eq!(env.resolve_variable_type("self"), Some("ParentClass".to_string()));

        env.register_variable("x".to_string(), "i32".to_string());
        assert_eq!(env.resolve_variable_type("x"), Some("i32".to_string()));

        env.push_scope(None);
        assert_eq!(env.resolve_variable_type("x"), Some("i32".to_string()));
        env.register_variable("x".to_string(), "String".to_string());
        assert_eq!(env.resolve_variable_type("x"), Some("String".to_string()));

        env.pop_scope();
        assert_eq!(env.resolve_variable_type("x"), Some("i32".to_string()));
    }
}
