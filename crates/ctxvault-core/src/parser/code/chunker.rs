//! AST-aware structural code chunker (`cAST` pattern).
//!
//! Decomposes polyglot source code files into syntactically complete AST nodes
//! (functions, methods, classes, traits, structs) with bound docstrings and
//! prepended hierarchical scope breadcrumbs for optimal hybrid retrieval.

use std::path::Path;

use ctxvault_common::config::ChunkingConfig;
use ctxvault_common::types::{Chunk, CodeSymbol, CodeSymbolType};
use tree_sitter::{Node, Parser};

use super::languages::{detect_language, SupportedLanguage};

/// Result of parsing a code file: chunks for embedding/BM25 and extracted code symbols.
#[derive(Debug, Clone)]
pub struct CodeParseResult {
    /// Extracted syntactic code chunks.
    pub chunks: Vec<Chunk>,
    /// Extracted code symbol definitions.
    pub symbols: Vec<CodeSymbol>,
}

/// AST-aware code chunker.
pub struct CodeChunker;

impl CodeChunker {
    /// Parse and chunk a source code file.
    pub fn parse_and_chunk(
        file_path: &Path,
        content: &str,
        _config: &ChunkingConfig,
    ) -> Option<CodeParseResult> {
        let lang = detect_language(file_path)?;
        let mut parser = Parser::new();
        if let Err(e) = parser.set_language(&lang.tree_sitter_language()) {
            tracing::warn!("Failed to set language for {}: {:?}", file_path.display(), e);
            return None;
        }

        let Some(tree) = parser.parse(content, None) else {
            tracing::warn!("Failed to parse content for {}", file_path.display());
            return None;
        };
        let mut extractor =
            AstExtractor::new(file_path.to_string_lossy().to_string(), content, lang);
        extractor.traverse(tree.root_node());

        Some(CodeParseResult { chunks: extractor.chunks, symbols: extractor.symbols })
    }
}

struct AstExtractor<'a> {
    file_path: String,
    content: &'a str,
    language: SupportedLanguage,
    scope_stack: Vec<String>,
    chunks: Vec<Chunk>,
    symbols: Vec<CodeSymbol>,
    chunk_index: usize,
}

impl<'a> AstExtractor<'a> {
    fn new(file_path: String, content: &'a str, language: SupportedLanguage) -> Self {
        Self {
            file_path,
            content,
            language,
            scope_stack: Vec::new(),
            chunks: Vec::new(),
            symbols: Vec::new(),
            chunk_index: 0,
        }
    }

    fn current_scope(&self) -> String {
        if self.scope_stack.is_empty() {
            self.file_path.clone()
        } else {
            self.scope_stack.join(" > ")
        }
    }

    fn traverse(&mut self, node: Node) {
        let lang = self.language;

        let symbol_info = match lang {
            SupportedLanguage::Rust => self.classify_rust_node(node),
            SupportedLanguage::TypeScript
            | SupportedLanguage::Tsx
            | SupportedLanguage::JavaScript => self.classify_js_ts_node(node),
            SupportedLanguage::Python => self.classify_python_node(node),
            SupportedLanguage::Go => self.classify_go_node(node),
            SupportedLanguage::C | SupportedLanguage::Cpp => self.classify_c_cpp_node(node),
            SupportedLanguage::Java => self.classify_java_node(node),
            SupportedLanguage::CSharp => self.classify_csharp_node(node),
            SupportedLanguage::Ruby => self.classify_ruby_node(node),
            SupportedLanguage::Php => self.classify_php_node(node),
            SupportedLanguage::Swift => self.classify_swift_node(node),
            SupportedLanguage::Elixir => self.classify_elixir_node(node),
            SupportedLanguage::Lua => self.classify_lua_node(node),
            SupportedLanguage::Bash => self.classify_bash_node(node),
        };

        if let Some((sym_type, name, signature)) = symbol_info {
            let start_byte = node.start_byte();
            let end_byte = node.end_byte();
            let start_line = node.start_position().row + 1;
            let end_line = node.end_position().row + 1;

            let parent_scope = self.current_scope();
            let full_scope = if parent_scope == self.file_path {
                name.clone()
            } else {
                format!("{parent_scope} > {name}")
            };

            let docstring = self.extract_docstring(node);
            let raw_node_text = &self.content[start_byte..end_byte];

            // Build enriched chunk text with AST scope breadcrumb
            let comment = lang.comment_prefix();
            let breadcrumb = format!(
                "{comment} Scope: {full_scope}\n{comment} Language: {}\n{comment} File: {}\n",
                lang.name(),
                self.file_path
            );

            let chunk_text = if let Some(ref doc) = docstring {
                format!("{breadcrumb}{doc}\n{raw_node_text}")
            } else {
                format!("{breadcrumb}{raw_node_text}")
            };

            // Register symbol definition
            self.symbols.push(CodeSymbol {
                file_path: self.file_path.clone(),
                name: name.clone(),
                scope_path: full_scope.clone(),
                symbol_type: sym_type,
                language: lang.name().to_string(),
                signature,
                docstring: docstring.clone(),
                start_line,
                end_line,
            });

            // Register AST chunk
            let chunk =
                Chunk::new(&self.file_path, self.chunk_index, chunk_text, start_byte, end_byte)
                    .with_code_metadata(lang.name(), &full_scope, start_line, end_line);
            self.chunks.push(chunk);
            self.chunk_index += 1;

            // If it's a container type (class, struct, trait, impl), push to scope stack and traverse children
            let is_container = matches!(
                sym_type,
                CodeSymbolType::Class
                    | CodeSymbolType::Struct
                    | CodeSymbolType::Trait
                    | CodeSymbolType::Interface
                    | CodeSymbolType::Module
                    | CodeSymbolType::Enum
            );

            if is_container {
                self.scope_stack.push(name);
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.traverse(child);
                }
                self.scope_stack.pop();
                return;
            }
        }

        // Default: traverse all children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.traverse(child);
        }
    }

    fn extract_docstring(&self, node: Node) -> Option<String> {
        // 1. Python body docstring
        if self.language == SupportedLanguage::Python {
            if let Some(body) = node.child_by_field_name("body") {
                if let Some(first_stmt) = body.child(0) {
                    if first_stmt.kind() == "expression_statement" {
                        let text = self.node_text(first_stmt).trim();
                        if (text.starts_with("\"\"\"") && text.ends_with("\"\"\""))
                            || (text.starts_with("'''") && text.ends_with("'''"))
                        {
                            return Some(text.to_string());
                        }
                    }
                }
            }
        }

        // 2. Check preceding siblings of this node or its parent (e.g. export_statement)
        let mut target_node = node;
        if let Some(parent) = node.parent() {
            if parent.kind() == "export_statement" {
                target_node = parent;
            }
        }

        let mut prev = target_node.prev_sibling();
        let mut comments = Vec::new();

        while let Some(sibling) = prev {
            let kind = sibling.kind();
            if kind.contains("comment") || kind == "line_comment" || kind == "block_comment" {
                let text = self.node_text(sibling);
                comments.push(text.trim().to_string());
                prev = sibling.prev_sibling();
            } else {
                break;
            }
        }

        // 3. Also check child comments if grammar places comments inside node
        if comments.is_empty() {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                let kind = child.kind();
                if kind.contains("comment") || kind == "line_comment" || kind == "block_comment" {
                    comments.push(self.node_text(child).trim().to_string());
                } else if !kind.is_empty() && kind != "decorator" && kind != "attribute_item" {
                    break;
                }
            }
        }

        if comments.is_empty() {
            None
        } else {
            comments.reverse();
            Some(comments.join("\n"))
        }
    }

    fn node_text(&self, node: Node) -> &str {
        &self.content[node.start_byte()..node.end_byte()]
    }

    fn find_child_identifier(&self, node: Node) -> Option<String> {
        if let Some(n) = node.child_by_field_name("name") {
            return Some(self.node_text(n).to_string());
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let kind = child.kind();
            if kind == "identifier"
                || kind == "type_identifier"
                || kind == "property_identifier"
                || kind == "field_identifier"
            {
                return Some(self.node_text(child).to_string());
            }
        }
        None
    }

    fn classify_rust_node(&self, node: Node) -> Option<(CodeSymbolType, String, String)> {
        match node.kind() {
            "function_item" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Function, name, sig))
            }
            "struct_item" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Struct, name, sig))
            }
            "enum_item" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Enum, name, sig))
            }
            "trait_item" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Trait, name, sig))
            }
            "impl_item" => {
                let type_name = if let Some(n) = node.child_by_field_name("type") {
                    self.node_text(n).to_string()
                } else {
                    let mut found = None;
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        let kind = child.kind();
                        if kind == "type_identifier"
                            || kind == "generic_type"
                            || kind == "primitive_type"
                            || kind == "scoped_type_identifier"
                        {
                            found = Some(self.node_text(child).to_string());
                        }
                    }
                    found.unwrap_or_else(|| "impl".to_string())
                };

                let trait_name = node
                    .child_by_field_name("trait")
                    .map(|n| format!("{} for ", self.node_text(n)));
                let full_name = format!("{}{}", trait_name.unwrap_or_default(), type_name);
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Module, full_name, sig))
            }
            "mod_item" => {
                let name = self.find_child_identifier(node)?;
                let sig = format!("mod {name}");
                Some((CodeSymbolType::Module, name, sig))
            }
            "type_item" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::TypeAlias, name, sig))
            }
            _ => None,
        }
    }

    fn classify_js_ts_node(&self, node: Node) -> Option<(CodeSymbolType, String, String)> {
        match node.kind() {
            "function_declaration" | "function" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Function, name, sig))
            }
            "method_definition" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Method, name, sig))
            }
            "class_declaration" | "class" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Class, name, sig))
            }
            "interface_declaration" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Interface, name, sig))
            }
            "type_alias_declaration" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::TypeAlias, name, sig))
            }
            "enum_declaration" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Enum, name, sig))
            }
            _ => None,
        }
    }

    fn classify_python_node(&self, node: Node) -> Option<(CodeSymbolType, String, String)> {
        match node.kind() {
            "function_definition" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                let sym_type = if self.scope_stack.is_empty() {
                    CodeSymbolType::Function
                } else {
                    CodeSymbolType::Method
                };
                Some((sym_type, name, sig))
            }
            "class_definition" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Class, name, sig))
            }
            _ => None,
        }
    }

    fn classify_go_node(&self, node: Node) -> Option<(CodeSymbolType, String, String)> {
        match node.kind() {
            "function_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Function, name, sig))
            }
            "method_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Method, name, sig))
            }
            "type_declaration" => {
                let sig = self.extract_first_line(node);
                let name = node
                    .child(0)
                    .and_then(|c| c.child_by_field_name("name"))
                    .map(|n| self.node_text(n).to_string())
                    .unwrap_or_else(|| sig.clone());
                Some((CodeSymbolType::Struct, name, sig))
            }
            _ => None,
        }
    }

    fn classify_c_cpp_node(&self, node: Node) -> Option<(CodeSymbolType, String, String)> {
        match node.kind() {
            "function_definition" => {
                let declarator = node.child_by_field_name("declarator");
                let name = declarator
                    .map(|d| self.node_text(d).to_string())
                    .unwrap_or_else(|| "function".to_string());
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Function, name, sig))
            }
            "class_specifier" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Class, name, sig))
            }
            "struct_specifier" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Struct, name, sig))
            }
            "enum_specifier" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Enum, name, sig))
            }
            _ => None,
        }
    }

    fn classify_java_node(&self, node: Node) -> Option<(CodeSymbolType, String, String)> {
        match node.kind() {
            "method_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Method, name, sig))
            }
            "class_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Class, name, sig))
            }
            "interface_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Interface, name, sig))
            }
            "enum_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Enum, name, sig))
            }
            _ => None,
        }
    }

    fn classify_csharp_node(&self, node: Node) -> Option<(CodeSymbolType, String, String)> {
        match node.kind() {
            "method_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Method, name, sig))
            }
            "class_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Class, name, sig))
            }
            "interface_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Interface, name, sig))
            }
            "enum_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Enum, name, sig))
            }
            "struct_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Struct, name, sig))
            }
            _ => None,
        }
    }

    fn classify_ruby_node(&self, node: Node) -> Option<(CodeSymbolType, String, String)> {
        match node.kind() {
            "class" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Class, name, sig))
            }
            "module" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Module, name, sig))
            }
            "method" | "singleton_method" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Method, name, sig))
            }
            _ => None,
        }
    }

    fn classify_php_node(&self, node: Node) -> Option<(CodeSymbolType, String, String)> {
        match node.kind() {
            "class_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Class, name, sig))
            }
            "interface_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Interface, name, sig))
            }
            "trait_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Trait, name, sig))
            }
            "enum_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Enum, name, sig))
            }
            "method_declaration" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Method, name, sig))
            }
            "function_definition" => {
                let name =
                    node.child_by_field_name("name").map(|n| self.node_text(n).to_string())?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Function, name, sig))
            }
            _ => None,
        }
    }

    fn classify_swift_node(&self, node: Node) -> Option<(CodeSymbolType, String, String)> {
        match node.kind() {
            "class_declaration" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Class, name, sig))
            }
            "struct_declaration" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Struct, name, sig))
            }
            "protocol_declaration" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Interface, name, sig))
            }
            "enum_declaration" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Enum, name, sig))
            }
            "function_declaration" => {
                let name = self.find_child_identifier(node)?;
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Function, name, sig))
            }
            "init_declaration" => {
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Method, "init".to_string(), sig))
            }
            _ => None,
        }
    }

    fn classify_elixir_node(&self, node: Node) -> Option<(CodeSymbolType, String, String)> {
        if node.kind() == "call" {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            if let Some(target) = children.first() {
                let target_name = self.node_text(*target).trim();
                if target_name == "defmodule" {
                    let name = children
                        .get(1)
                        .map(|n| {
                            let text = self.node_text(*n).trim();
                            text.split_whitespace().next().unwrap_or(text).to_string()
                        })
                        .unwrap_or_else(|| "Module".to_string());
                    let sig = self.extract_first_line(node);
                    return Some((CodeSymbolType::Module, name, sig));
                } else if target_name == "def" || target_name == "defp" || target_name == "defmacro"
                {
                    let name = children
                        .get(1)
                        .map(|n| {
                            let t = self.node_text(*n).trim();
                            t.split('(').next().unwrap_or(t).trim().to_string()
                        })
                        .unwrap_or_else(|| "function".to_string());
                    let sig = self.extract_first_line(node);
                    return Some((CodeSymbolType::Function, name, sig));
                } else if target_name == "defprotocol" || target_name == "defimpl" {
                    let name = children
                        .get(1)
                        .map(|n| self.node_text(*n).trim().to_string())
                        .unwrap_or_else(|| "Protocol".to_string());
                    let sig = self.extract_first_line(node);
                    return Some((CodeSymbolType::Trait, name, sig));
                }
            }
        }
        None
    }

    fn classify_lua_node(&self, node: Node) -> Option<(CodeSymbolType, String, String)> {
        match node.kind() {
            "function_declaration" | "local_function" => {
                let name =
                    self.find_child_identifier(node).unwrap_or_else(|| "function".to_string());
                let sig = self.extract_first_line(node);
                Some((CodeSymbolType::Function, name, sig))
            }
            _ => None,
        }
    }

    fn classify_bash_node(&self, node: Node) -> Option<(CodeSymbolType, String, String)> {
        if node.kind() == "function_definition" {
            let name = node
                .child_by_field_name("name")
                .map(|n| self.node_text(n).to_string())
                .unwrap_or_else(|| {
                    self.find_child_identifier(node).unwrap_or_else(|| "func".to_string())
                });
            let sig = self.extract_first_line(node);
            Some((CodeSymbolType::Function, name, sig))
        } else {
            None
        }
    }

    fn extract_first_line(&self, node: Node) -> String {
        let text = self.node_text(node);
        text.lines().next().unwrap_or(text).trim().trim_end_matches('{').trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_chunking_and_symbols() {
        let code = r#"
/// High performance search engine.
pub struct SearchEngine {
    pub name: String,
}

impl SearchEngine {
    /// Execute hybrid search across all modalities.
    pub fn search_hybrid(&self, query: &str) -> Vec<String> {
        let mut results = Vec::new();
        results.push(query.to_string());
        results
    }
}
"#;
        let config = ChunkingConfig::default();
        let res = CodeChunker::parse_and_chunk(Path::new("src/search/engine.rs"), code, &config)
            .expect("should parse rust");

        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "SearchEngine" && s.symbol_type == CodeSymbolType::Struct));
        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "search_hybrid" && s.symbol_type == CodeSymbolType::Function));

        // Verify scope breadcrumb in chunk text
        let search_chunk = res
            .chunks
            .iter()
            .find(|c| c.scope_path.as_deref() == Some("SearchEngine > search_hybrid"))
            .expect("should find search_hybrid chunk");
        assert!(search_chunk.text.contains("// Scope: SearchEngine > search_hybrid"));
        assert!(search_chunk.text.contains("// Language: rust"));
        assert!(search_chunk.text.contains("Execute hybrid search"));
    }

    #[test]
    fn test_python_chunking_and_symbols() {
        let code = r#"
class DataProcessor:
    """Processes large datasets."""
    
    def process_batch(self, items):
        # Process items
        return [x * 2 for x in items]
"#;
        let config = ChunkingConfig::default();
        let res = CodeChunker::parse_and_chunk(Path::new("processor.py"), code, &config)
            .expect("should parse python");

        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "DataProcessor" && s.symbol_type == CodeSymbolType::Class));
        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "process_batch" && s.symbol_type == CodeSymbolType::Method));

        let batch_chunk = res
            .chunks
            .iter()
            .find(|c| c.scope_path.as_deref() == Some("DataProcessor > process_batch"))
            .expect("should find process_batch chunk");
        assert!(batch_chunk.text.contains("# Scope: DataProcessor > process_batch"));
        assert!(batch_chunk.text.contains("# Language: python"));
    }

    #[test]
    fn test_typescript_chunking_and_symbols() {
        let code = r#"
export interface UserRecord {
    id: string;
    username: string;
}

export class UserService {
    /** Fetch user profile by ID */
    async getUser(id: string): Promise<UserRecord> {
        return { id, username: "admin" };
    }
}
"#;
        let config = ChunkingConfig::default();
        let res = CodeChunker::parse_and_chunk(Path::new("src/user.ts"), code, &config)
            .expect("should parse typescript");

        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "UserRecord" && s.symbol_type == CodeSymbolType::Interface));
        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "UserService" && s.symbol_type == CodeSymbolType::Class));
        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "getUser" && s.symbol_type == CodeSymbolType::Method));

        let get_user_chunk = res
            .chunks
            .iter()
            .find(|c| c.scope_path.as_deref() == Some("UserService > getUser"))
            .expect("should find getUser chunk");
        assert!(get_user_chunk.text.contains("// Scope: UserService > getUser"));
        assert!(get_user_chunk.text.contains("Fetch user profile by ID"));
    }

    #[test]
    fn test_extended_languages_chunking_and_symbols() {
        let config = ChunkingConfig::default();

        // 1. Ruby
        let ruby_code = r#"
class AuthManager
  def authenticate(user, password)
    true
  end
end
"#;
        let res = CodeChunker::parse_and_chunk(Path::new("auth.rb"), ruby_code, &config).unwrap();
        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "AuthManager" && s.symbol_type == CodeSymbolType::Class));
        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "authenticate" && s.symbol_type == CodeSymbolType::Method));

        // 2. PHP
        let php_code = r#"<?php
class ApiClient {
    public function sendRequest($url) {
        return true;
    }
}
"#;
        let res = CodeChunker::parse_and_chunk(Path::new("client.php"), php_code, &config).unwrap();
        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "ApiClient" && s.symbol_type == CodeSymbolType::Class));
        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "sendRequest" && s.symbol_type == CodeSymbolType::Method));

        // 3. Swift
        let swift_code = r#"
class NetworkService {
    func fetchData() -> String {
        return "data"
    }
}
"#;
        let res =
            CodeChunker::parse_and_chunk(Path::new("Network.swift"), swift_code, &config).unwrap();
        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "NetworkService" && s.symbol_type == CodeSymbolType::Class));
        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "fetchData" && s.symbol_type == CodeSymbolType::Function));

        // 4. Elixir
        let elixir_code = r#"
defmodule MathEngine do
  def add(a, b) do
    a + b
  end
end
"#;
        let res = CodeChunker::parse_and_chunk(Path::new("math.ex"), elixir_code, &config).unwrap();
        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "MathEngine" && s.symbol_type == CodeSymbolType::Module));

        // 7. Lua
        let lua_code = r#"
local function calculate_total(price, tax)
    return price + tax
end
"#;
        let res = CodeChunker::parse_and_chunk(Path::new("math.lua"), lua_code, &config).unwrap();
        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "calculate_total" && s.symbol_type == CodeSymbolType::Function));

        // 8. Bash
        let bash_code = r#"
deploy_app() {
    echo "Deploying..."
}
"#;
        let res = CodeChunker::parse_and_chunk(Path::new("deploy.sh"), bash_code, &config).unwrap();
        assert!(res
            .symbols
            .iter()
            .any(|s| s.name == "deploy_app" && s.symbol_type == CodeSymbolType::Function));
    }
}
