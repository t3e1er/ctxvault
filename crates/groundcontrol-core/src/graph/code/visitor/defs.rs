//! Definition, inheritance, interface implementation, decorator, and macro extraction.

use groundcontrol_common::types::EdgeProvenance;
use tree_sitter::Node;

use crate::parser::code::languages::SupportedLanguage;

use super::state::CallAndImportVisitor;

impl<'a> CallAndImportVisitor<'a> {
    /// Declarative Tree-sitter query edge extraction: mints `inherits`, `implements`,
    /// and dynamic `@rel` captures directly from compiled query packs.
    pub(crate) fn extract_query_edges(&mut self, root_node: Node) {
        let Some(query) = crate::parser::code::query::get_language_query(self.language) else {
            return;
        };

        let matches = query.extract_matches(root_node, self.content.as_bytes());
        for m in matches {
            let symbol_name = &self.content[m.name_node.start_byte()..m.name_node.end_byte()];
            let unquoted = symbol_name.trim_matches(['"', '\'', '`']);
            let clean_name = unquoted.split('<').next().unwrap_or(unquoted).trim();
            let start_line = m.name_node.start_position().row + 1;

            let source_scope = self
                .file_symbols
                .iter()
                .find(|s| {
                    (s.name == clean_name || s.name.ends_with(clean_name))
                        && s.start_line <= start_line
                        && s.end_line >= start_line
                })
                .map(|s| s.scope_path.clone())
                .unwrap_or_else(|| clean_name.to_string());

            if m.is_test {
                self.test_callers.insert(source_scope.clone());
            }

            // 1. Declarative inherits/extends edges
            for inh_node in &m.inherits_nodes {
                let target_raw = self.node_text(*inh_node).trim().to_string();
                let clean_target = target_raw.split('<').next().unwrap_or(&target_raw).trim();
                if !clean_target.is_empty() {
                    let (target, conf) = self.resolve_target(clean_target);
                    let edge_type = if self.language == SupportedLanguage::TypeScript
                        || self.language == SupportedLanguage::Tsx
                        || self.language == SupportedLanguage::JavaScript
                    {
                        "extends"
                    } else {
                        "inherits"
                    };
                    self.add_rel_edge(
                        source_scope.clone(),
                        target,
                        edge_type,
                        0.9,
                        EdgeProvenance::CodeExtends,
                        conf,
                    );
                }
            }

            // 2. Declarative implements edges
            for imp_node in &m.implements_nodes {
                let target_raw = self.node_text(*imp_node).trim().to_string();
                let clean_target = target_raw.split('<').next().unwrap_or(&target_raw).trim();
                if !clean_target.is_empty() {
                    let (target, conf) = self.resolve_target(clean_target);
                    self.add_rel_edge(
                        source_scope.clone(),
                        target,
                        "implements",
                        0.9,
                        EdgeProvenance::CodeImplementsTrait,
                        conf,
                    );
                }
            }

            // 3. Dynamic grammar captures (@rel)
            for (rel_name, dyn_node) in &m.dynamic_captures {
                let target_raw = self.node_text(*dyn_node).trim().to_string();
                let clean_target =
                    target_raw.trim_matches(['"', '\'', '`']).trim_start_matches('*').trim();
                if !clean_target.is_empty() {
                    let (target, conf) = self.resolve_target(clean_target);
                    let prov = match rel_name.as_str() {
                        "decorates" => EdgeProvenance::CodeDecorates,
                        "macro_expands" => EdgeProvenance::CodeMacroExpands,
                        "embeds_struct" | "struct_embeds" => EdgeProvenance::CodeStructEmbeds,
                        "foreign_key" => EdgeProvenance::CodeForeignKey,
                        "handles" => EdgeProvenance::CodeHandlesRoute,
                        _ => EdgeProvenance::CodeExtends,
                    };
                    self.add_rel_edge(source_scope.clone(), target, rel_name, 0.85, prov, conf);
                }
            }
        }
    }

    pub(super) fn extract_container_edges(&mut self, node: Node) {
        let kind = node.kind();
        let Some(container) = self.current_container.clone() else {
            return;
        };

        match self.language {
            SupportedLanguage::Go => {
                self.extract_go_embedded_fields(node, &container);
            }
            SupportedLanguage::Sql => {
                if kind == "create_table" || kind == "create_table_statement" {
                    self.extract_sql_foreign_key_references(node, &container);
                }
            }
            _ => {}
        }
    }

    pub(super) fn extract_go_embedded_fields(&mut self, node: Node, container: &str) {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "struct_type" {
                let mut fcursor = child.walk();
                for fchild in child.children(&mut fcursor) {
                    if fchild.kind() == "field_declaration_list" {
                        let mut dcursor = fchild.walk();
                        for field in fchild.children(&mut dcursor) {
                            if field.kind() == "field_declaration"
                                && field.child_by_field_name("name").is_none()
                            {
                                if let Some(type_node) = field.child_by_field_name("type") {
                                    let raw_type = self
                                        .node_text(type_node)
                                        .trim()
                                        .trim_start_matches('*')
                                        .trim()
                                        .to_string();
                                    if !raw_type.is_empty() {
                                        let (target, conf) = self.resolve_target(&raw_type);
                                        self.add_rel_edge(
                                            container.to_string(),
                                            target,
                                            "struct_embeds",
                                            0.85,
                                            EdgeProvenance::CodeStructEmbeds,
                                            conf,
                                        );
                                    }
                                } else {
                                    for c in field.children(&mut field.walk()) {
                                        let ckind = c.kind();
                                        if ckind == "type_identifier"
                                            || ckind == "qualified_type"
                                            || ckind == "pointer_type"
                                        {
                                            let raw_type = self
                                                .node_text(c)
                                                .trim()
                                                .trim_start_matches('*')
                                                .trim()
                                                .to_string();
                                            if !raw_type.is_empty() {
                                                let (target, conf) = self.resolve_target(&raw_type);
                                                self.add_rel_edge(
                                                    container.to_string(),
                                                    target,
                                                    "struct_embeds",
                                                    0.85,
                                                    EdgeProvenance::CodeStructEmbeds,
                                                    conf,
                                                );
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                self.extract_go_embedded_fields(child, container);
            }
        }
    }

    pub(super) fn extract_sql_foreign_key_references(&mut self, node: Node, container: &str) {
        let mut stack = vec![node];
        while let Some(curr) = stack.pop() {
            let kind = curr.kind();
            if kind == "keyword_references" || kind == "references_clause" || kind == "references" {
                let target_table = if let Some(next) = curr.next_named_sibling() {
                    if next.kind() == "object_reference" {
                        if let Some(n) = next.child_by_field_name("name") {
                            Some(self.node_text(n).trim().to_string())
                        } else {
                            Some(self.node_text(next).trim().to_string())
                        }
                    } else {
                        Some(self.node_text(next).trim().to_string())
                    }
                } else {
                    let mut found = None;
                    for c in curr.children(&mut curr.walk()) {
                        if c.kind() == "object_reference" {
                            found = Some(self.node_text(c).trim().to_string());
                            break;
                        }
                    }
                    found
                };

                if let Some(ref_table) = target_table {
                    let clean = ref_table.trim_matches('"').trim_matches('`').trim();
                    if !clean.is_empty() {
                        let (target, conf) = self.resolve_target(clean);
                        self.add_rel_edge(
                            container.to_string(),
                            target,
                            "foreign_key",
                            0.9,
                            EdgeProvenance::CodeForeignKey,
                            conf,
                        );
                    }
                }
            }

            let mut cursor = curr.walk();
            for child in curr.children(&mut cursor) {
                stack.push(child);
            }
        }
    }

    pub(super) fn extract_ts_decorator(&mut self, node: Node) {
        let Some(name) = self.extract_decorator_name(node) else {
            return;
        };

        let decorated_target = if let Some(sibling) = node.next_named_sibling() {
            let skind = sibling.kind();
            if skind == "method_definition"
                || skind == "property_definition"
                || skind == "class_declaration"
                || skind == "function_declaration"
            {
                sibling.child_by_field_name("name").map(|n| {
                    let text = self.node_text(n).trim().to_string();
                    if let Some(ref cont) = self.current_container {
                        format!("{} > {}", cont, text)
                    } else {
                        text
                    }
                })
            } else {
                None
            }
        } else if let Some(parent) = node.parent() {
            if parent.kind() == "export_statement" {
                if let Some(decl) = parent.child_by_field_name("declaration") {
                    decl.child_by_field_name("name").map(|n| self.node_text(n).trim().to_string())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        let target_sym = decorated_target
            .or_else(|| self.current_caller.clone())
            .or_else(|| self.current_container.clone());

        if let Some(sym) = target_sym {
            let (target, conf) = self.resolve_target(&name);
            self.add_rel_edge(sym, target, "decorates", 0.85, EdgeProvenance::CodeDecorates, conf);
        }
    }

    pub(super) fn extract_decorator_name(&self, node: Node) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let kind = child.kind();
            if kind == "call_expression" || kind == "call" {
                if let Some(func) = child.child_by_field_name("function").or_else(|| child.child(0))
                {
                    return Some(self.node_text(func).trim().to_string());
                }
            } else if kind == "identifier"
                || kind == "property_identifier"
                || kind == "attribute"
                || kind == "member_expression"
            {
                return Some(self.node_text(child).trim().to_string());
            }
        }
        None
    }

    pub(super) fn extract_python_decorated(&mut self, node: Node) {
        let definition = node.child_by_field_name("definition").or_else(|| {
            let mut cursor = node.walk();
            let mut found = None;
            for c in node.children(&mut cursor) {
                let k = c.kind();
                if k == "function_definition" || k == "class_definition" {
                    found = Some(c);
                    break;
                }
            }
            found
        });

        let Some(def_node) = definition else {
            return;
        };
        let def_name =
            def_node.child_by_field_name("name").map(|n| self.node_text(n).trim().to_string());
        let Some(raw_name) = def_name else {
            return;
        };

        let full_scope = if let Some(ref cont) = self.current_container {
            format!("{} > {}", cont, raw_name)
        } else {
            raw_name
        };

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "decorator" {
                if let Some(dec_name) = self.extract_decorator_name(child) {
                    let (target, conf) = self.resolve_target(&dec_name);
                    self.add_rel_edge(
                        full_scope.clone(),
                        target,
                        "decorates",
                        0.85,
                        EdgeProvenance::CodeDecorates,
                        conf,
                    );
                }
            }
        }
    }

    pub(super) fn extract_rust_macro(&mut self, node: Node) {
        if let Some(macro_node) = node.child_by_field_name("macro").or_else(|| node.child(0)) {
            let macro_text =
                self.node_text(macro_node).trim().trim_end_matches('!').trim().to_string();
            if !macro_text.is_empty() {
                let caller = self
                    .current_caller
                    .clone()
                    .or_else(|| self.current_container.clone())
                    .unwrap_or_else(|| self.file_path.clone());

                let (target, conf) = self.resolve_target(&macro_text);
                self.add_rel_edge(
                    caller,
                    target,
                    "macro_expands",
                    0.7,
                    EdgeProvenance::CodeMacroExpands,
                    conf,
                );
            }
        }
    }
}
