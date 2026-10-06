//! Universal Deterministic Import-Path Resolution Engine across polyglot codebases.
//!
//! Parses import statements, normalizes relative and module paths against repository roots,
//! maps imported symbols to their canonical target files/modules, and populates the file's
//! [`ImportTable`] to provide deterministic Tier 2 resolution confidence for cross-file calls.

use std::collections::HashMap;
use std::path::Path;

use groundcontrol_common::types::{Edge, EdgeProvenance, ResolutionConfidence};
use tree_sitter::Node;

use crate::parser::code::languages::{get_language_spec, SupportedLanguage};

use super::state::CallAndImportVisitor;

/// Resolved target metadata for an imported symbol or module.
#[derive(Debug, Clone)]
pub struct ImportResolution {
    /// The raw source path or module string from the import statement.
    pub raw_source: String,
    /// Normalized file path prefix or canonical module path (e.g. "src/api", "src/search", "app/models").
    pub target_path_prefix: Option<String>,
}

impl ImportResolution {
    /// Create a new import resolution entry.
    pub fn new(raw_source: String, target_path_prefix: Option<String>) -> Self {
        Self { raw_source, target_path_prefix }
    }
}

/// File-level import table mapping imported symbols and module namespaces to resolved paths.
#[derive(Debug, Clone, Default)]
pub struct ImportTable {
    /// Specific imported symbol names (e.g., "SearchClient" -> ImportResolution).
    pub symbols: HashMap<String, ImportResolution>,
    /// Module or namespace aliases (e.g., "api" -> ImportResolution).
    pub modules: HashMap<String, ImportResolution>,
    /// Wildcard / star imports (e.g., `use crate::search::*;`, `from .models import *`).
    pub wildcards: Vec<ImportResolution>,
}

impl ImportTable {
    /// Check whether a candidate symbol's file path matches the normalized import target.
    pub fn matches_target_path(&self, symbol_or_type: &str, file_path: &str) -> bool {
        let norm_file = file_path.replace('\\', "/");
        if let Some(res) = self.symbols.get(symbol_or_type) {
            if !res.raw_source.is_empty() && norm_file.contains(&res.raw_source) {
                return true;
            }
            if let Some(ref prefix) = res.target_path_prefix {
                let norm_prefix = prefix.replace('\\', "/");
                if norm_file.starts_with(&norm_prefix) || norm_file.contains(&norm_prefix) {
                    return true;
                }
                let file_without_ext =
                    norm_file.rsplit_once('.').map(|(base, _)| base).unwrap_or(&norm_file);
                if file_without_ext.ends_with(&norm_prefix)
                    || norm_prefix.ends_with(file_without_ext)
                {
                    return true;
                }
            }
        }
        for w in &self.wildcards {
            if !w.raw_source.is_empty() && norm_file.contains(&w.raw_source) {
                return true;
            }
            if let Some(ref prefix) = w.target_path_prefix {
                let norm_prefix = prefix.replace('\\', "/");
                if norm_file.starts_with(&norm_prefix) || norm_file.contains(&norm_prefix) {
                    return true;
                }
            }
        }
        for m in self.modules.values() {
            if !m.raw_source.is_empty() && norm_file.contains(&m.raw_source) {
                return true;
            }
            if let Some(ref prefix) = m.target_path_prefix {
                let norm_prefix = prefix.replace('\\', "/");
                if norm_file.starts_with(&norm_prefix) || norm_file.contains(&norm_prefix) {
                    return true;
                }
            }
        }
        false
    }
}

/// Normalize an import statement path relative to the current file and project root.
pub fn normalize_import_path(current_file: &str, raw_import: &str) -> Option<String> {
    let current_norm = current_file.replace('\\', "/");
    let current_dir = Path::new(&current_norm)
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let raw = raw_import.trim().trim_matches(['"', '\'', '<', '>']).trim();

    if raw.is_empty() {
        return None;
    }

    // 1. Relative paths: `./foo`, `../foo`
    if raw.starts_with("./") || raw.starts_with("../") {
        let mut base_parts: Vec<&str> = if current_dir.is_empty() {
            Vec::new()
        } else {
            current_dir.split('/').filter(|s| !s.is_empty()).collect()
        };
        for segment in raw.split('/') {
            if segment == "." || segment.is_empty() {
                continue;
            } else if segment == ".." {
                base_parts.pop();
            } else {
                base_parts.push(segment);
            }
        }
        let joined = base_parts.join("/");
        let clean = joined
            .strip_suffix(".ts")
            .or_else(|| joined.strip_suffix(".js"))
            .or_else(|| joined.strip_suffix(".tsx"))
            .or_else(|| joined.strip_suffix(".jsx"))
            .or_else(|| joined.strip_suffix(".py"))
            .or_else(|| joined.strip_suffix(".rs"))
            .unwrap_or(&joined);
        return Some(clean.to_string());
    }

    // 2. Rust crate paths: `crate::foo::bar`
    if let Some(sub) = raw.strip_prefix("crate::") {
        let sub_path = sub.replace("::", "/");
        if let Some(idx) = current_norm.find("/src/") {
            let crate_src = &current_norm[..idx + 5];
            let joined = format!("{}{}", crate_src, sub_path);
            let clean = joined.strip_suffix(".rs").unwrap_or(&joined);
            return Some(clean.to_string());
        } else if current_norm.starts_with("src/") {
            let joined = format!("src/{}", sub_path);
            let clean = joined.strip_suffix(".rs").unwrap_or(&joined);
            return Some(clean.to_string());
        } else {
            return Some(sub_path);
        }
    }

    // 3. Rust super paths: `super::foo`
    if let Some(sub) = raw.strip_prefix("super::") {
        let parent_dir = Path::new(&current_dir)
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let sub_path = sub.replace("::", "/");
        let joined =
            if parent_dir.is_empty() { sub_path } else { format!("{}/{}", parent_dir, sub_path) };
        return Some(joined);
    }

    // 4. Python relative imports: `.foo`, `..foo`
    if raw.starts_with('.') {
        let dots = raw.chars().take_while(|c| *c == '.').count();
        let mod_rest = &raw[dots..];
        let mut base_parts: Vec<&str> = if current_dir.is_empty() {
            Vec::new()
        } else {
            current_dir.split('/').filter(|s| !s.is_empty()).collect()
        };
        for _ in 1..dots {
            base_parts.pop();
        }
        if !mod_rest.is_empty() {
            for part in mod_rest.split('.') {
                if !part.is_empty() {
                    base_parts.push(part);
                }
            }
        }
        return Some(base_parts.join("/"));
    }

    // 5. C/C++ header include: `"foo.h"`
    if raw.ends_with(".h") || raw.ends_with(".hpp") {
        let stem = Path::new(raw).file_stem()?.to_string_lossy();
        if !current_dir.is_empty() {
            return Some(format!("{}/{}", current_dir, stem));
        } else {
            return Some(stem.to_string());
        }
    }

    // 6. Generic/Go/Java/Python module path
    let clean = raw.replace("::", "/").replace('.', "/");
    Some(clean)
}

impl<'a> CallAndImportVisitor<'a> {
    /// Pre-scan and extract all top-level imports in the file before visiting call sites.
    pub(crate) fn extract_all_imports(&mut self, root_node: Node) {
        let spec = get_language_spec(self.language);
        let mut cursor = root_node.walk();
        for child in root_node.children(&mut cursor) {
            if spec.is_import(child.kind()) {
                self.extract_import(child);
            }
        }
    }

    /// Extract import statements, register symbols in [`ImportTable`], and emit graph edges.
    pub(super) fn extract_import(&mut self, node: Node) {
        let kind = node.kind();
        match self.language {
            SupportedLanguage::Rust => {
                if kind == "use_declaration" {
                    let text = self.node_text(node).trim().trim_end_matches(';').trim();
                    if let Some(target) = text.strip_prefix("use ") {
                        let target_str = target.trim().to_string();
                        self.edges.push(Edge {
                            source: self.file_path.clone(),
                            target: target_str.clone(),
                            edge_type: "imports".to_string(),
                            weight: 0.6,
                            provenance: EdgeProvenance::CodeImports,
                            target_corpus: None,
                            confidence: Some(ResolutionConfidence::Speculative),
                            target_path: None,
                            target_symbol: None,
                            target_kind: None,
                        });

                        // Parse use target (e.g., `crate::foo::Bar`, `crate::foo::{Bar, Baz}`, `crate::foo::*`)
                        if let Some((prefix, list)) = target_str.split_once('{') {
                            let prefix = prefix.trim_end_matches("::").trim();
                            let norm_prefix = normalize_import_path(&self.file_path, prefix);
                            let list_clean = list.trim_end_matches('}').trim();
                            for item in list_clean.split(',') {
                                let item = item.trim();
                                if item.is_empty() {
                                    continue;
                                }
                                let sym = item.rsplit(" as ").next().unwrap_or(item).trim();
                                self.import_table.symbols.insert(
                                    sym.to_string(),
                                    ImportResolution::new(target_str.clone(), norm_prefix.clone()),
                                );
                                self.type_env.register_import(sym.to_string(), target_str.clone());
                            }
                        } else if target_str.ends_with("::*") {
                            let prefix = target_str.trim_end_matches("::*").trim();
                            let norm_prefix = normalize_import_path(&self.file_path, prefix);
                            self.import_table
                                .wildcards
                                .push(ImportResolution::new(target_str.clone(), norm_prefix));
                        } else {
                            let sym =
                                target_str.rsplit("::").next().unwrap_or(&target_str).to_string();
                            let prefix =
                                target_str.rsplit_once("::").map(|(p, _)| p).unwrap_or(&target_str);
                            let norm_prefix = normalize_import_path(&self.file_path, prefix);
                            self.import_table.symbols.insert(
                                sym.clone(),
                                ImportResolution::new(target_str.clone(), norm_prefix),
                            );
                            self.type_env.register_import(sym, target_str);
                        }
                    }
                }
            }
            SupportedLanguage::TypeScript
            | SupportedLanguage::Tsx
            | SupportedLanguage::JavaScript => {
                if kind == "import_statement" {
                    let mut raw_source = String::new();
                    if let Some(source_node) = node.child_by_field_name("source") {
                        raw_source = self
                            .node_text(source_node)
                            .trim()
                            .trim_matches(['"', '\''])
                            .to_string();
                    } else {
                        // Fallback: search for quoted string literal in children
                        let mut cursor = node.walk();
                        for child in node.children(&mut cursor) {
                            if child.kind() == "string" {
                                raw_source = self
                                    .node_text(child)
                                    .trim()
                                    .trim_matches(['"', '\''])
                                    .to_string();
                                break;
                            }
                        }
                    }

                    if !raw_source.is_empty() {
                        let norm_prefix = normalize_import_path(&self.file_path, &raw_source);
                        self.edges.push(Edge {
                            source: self.file_path.clone(),
                            target: raw_source.clone(),
                            edge_type: "imports".to_string(),
                            weight: 0.6,
                            provenance: EdgeProvenance::CodeImports,
                            target_corpus: None,
                            confidence: Some(ResolutionConfidence::Speculative),
                            target_path: None,
                            target_symbol: None,
                            target_kind: None,
                        });

                        // Extract imported identifiers from import_clause / named_imports
                        let mut cursor = node.walk();
                        for child in node.children(&mut cursor) {
                            let ckind = child.kind();
                            if ckind == "import_clause" {
                                let mut icursor = child.walk();
                                for clause_child in child.children(&mut icursor) {
                                    let clause_kind = clause_child.kind();
                                    if clause_kind == "identifier" {
                                        // Default import: `import ApiClient from "./api"`
                                        let name = self.node_text(clause_child).trim().to_string();
                                        self.import_table.symbols.insert(
                                            name.clone(),
                                            ImportResolution::new(
                                                raw_source.clone(),
                                                norm_prefix.clone(),
                                            ),
                                        );
                                        self.type_env.register_import(name, raw_source.clone());
                                    } else if clause_kind == "namespace_import" {
                                        // `import * as api from "./api"`
                                        if let Some(id_node) =
                                            clause_child.child_by_field_name("name").or_else(|| {
                                                clause_child
                                                    .children(&mut clause_child.walk())
                                                    .find(|c| c.kind() == "identifier")
                                            })
                                        {
                                            let name = self.node_text(id_node).trim().to_string();
                                            self.import_table.modules.insert(
                                                name.clone(),
                                                ImportResolution::new(
                                                    raw_source.clone(),
                                                    norm_prefix.clone(),
                                                ),
                                            );
                                            self.import_table.wildcards.push(
                                                ImportResolution::new(
                                                    raw_source.clone(),
                                                    norm_prefix.clone(),
                                                ),
                                            );
                                            self.type_env.register_import(name, raw_source.clone());
                                        }
                                    } else if clause_kind == "named_imports" {
                                        // `import { A, B as C } from "./api"`
                                        let mut ncursor = clause_child.walk();
                                        for spec in clause_child.children(&mut ncursor) {
                                            if spec.kind() == "import_specifier" {
                                                let sym_node = spec
                                                    .child_by_field_name("alias")
                                                    .or_else(|| spec.child_by_field_name("name"))
                                                    .unwrap_or(spec);
                                                let name =
                                                    self.node_text(sym_node).trim().to_string();
                                                self.import_table.symbols.insert(
                                                    name.clone(),
                                                    ImportResolution::new(
                                                        raw_source.clone(),
                                                        norm_prefix.clone(),
                                                    ),
                                                );
                                                self.type_env
                                                    .register_import(name, raw_source.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            SupportedLanguage::Python => {
                if kind == "import_statement" || kind == "import_from_statement" {
                    let text = self.node_text(node).trim().to_string();
                    self.edges.push(Edge {
                        source: self.file_path.clone(),
                        target: text.clone(),
                        edge_type: "imports".to_string(),
                        weight: 0.6,
                        provenance: EdgeProvenance::CodeImports,
                        target_corpus: None,
                        confidence: Some(ResolutionConfidence::Speculative),
                        target_path: None,
                        target_symbol: None,
                        target_kind: None,
                    });

                    if let Some(rest) = text.strip_prefix("from ") {
                        if let Some((mod_part, names_part)) = rest.split_once(" import ") {
                            let raw_mod = mod_part.trim();
                            let norm_prefix = normalize_import_path(&self.file_path, raw_mod);
                            for name in names_part.split(',') {
                                let name = name.trim();
                                if name == "*" {
                                    self.import_table.wildcards.push(ImportResolution::new(
                                        raw_mod.to_string(),
                                        norm_prefix.clone(),
                                    ));
                                } else {
                                    let sym = name.rsplit(" as ").next().unwrap_or(name).trim();
                                    self.import_table.symbols.insert(
                                        sym.to_string(),
                                        ImportResolution::new(
                                            raw_mod.to_string(),
                                            norm_prefix.clone(),
                                        ),
                                    );
                                    self.type_env
                                        .register_import(sym.to_string(), raw_mod.to_string());
                                }
                            }
                        }
                    } else if let Some(rest) = text.strip_prefix("import ") {
                        for item in rest.split(',') {
                            let item = item.trim();
                            let (mod_name, alias) = if let Some((m, a)) = item.split_once(" as ") {
                                (m.trim(), a.trim())
                            } else {
                                (item, item.rsplit('.').next().unwrap_or(item))
                            };
                            let norm_prefix = normalize_import_path(&self.file_path, mod_name);
                            self.import_table.modules.insert(
                                alias.to_string(),
                                ImportResolution::new(mod_name.to_string(), norm_prefix),
                            );
                            self.type_env.register_import(alias.to_string(), mod_name.to_string());
                        }
                    }
                }
            }
            SupportedLanguage::Go => {
                if kind == "import_spec" {
                    let path = self.node_text(node).trim().trim_matches('"').to_string();
                    let pkg = if let Some(alias) = node.child_by_field_name("name") {
                        self.node_text(alias).trim().to_string()
                    } else {
                        path.rsplit('/').next().unwrap_or(&path).to_string()
                    };
                    let norm_prefix = normalize_import_path(&self.file_path, &path);

                    self.edges.push(Edge {
                        source: self.file_path.clone(),
                        target: path.clone(),
                        edge_type: "imports".to_string(),
                        weight: 0.6,
                        provenance: EdgeProvenance::CodeImports,
                        target_corpus: None,
                        confidence: Some(ResolutionConfidence::Speculative),
                        target_path: None,
                        target_symbol: None,
                        target_kind: None,
                    });
                    self.import_table
                        .modules
                        .insert(pkg.clone(), ImportResolution::new(path.clone(), norm_prefix));
                    self.type_env.register_import(pkg, path);
                }
            }
            _ => {
                let text = self.node_text(node).trim().trim_end_matches(';').trim().to_string();
                let clean = text
                    .strip_prefix("import ")
                    .or_else(|| text.strip_prefix("#include "))
                    .or_else(|| text.strip_prefix("include "))
                    .or_else(|| text.strip_prefix("using "))
                    .unwrap_or(&text)
                    .trim()
                    .trim_matches(['"', '\'', '<', '>'])
                    .to_string();
                if !clean.is_empty() && clean.len() < 200 {
                    let sym = clean.rsplit('.').next().unwrap_or(&clean).to_string();
                    let norm_prefix = normalize_import_path(&self.file_path, &clean);

                    self.edges.push(Edge {
                        source: self.file_path.clone(),
                        target: clean.to_string(),
                        edge_type: "imports".to_string(),
                        weight: 0.6,
                        provenance: EdgeProvenance::CodeImports,
                        target_corpus: None,
                        confidence: Some(ResolutionConfidence::Speculative),
                        target_path: None,
                        target_symbol: None,
                        target_kind: None,
                    });

                    self.import_table.symbols.insert(
                        sym.to_string(),
                        ImportResolution::new(clean.to_string(), norm_prefix),
                    );
                    self.type_env.register_import(sym.to_string(), clean);
                }
            }
        }
    }
}
