//! Cypher-Lite Linear Path Query DSL & Recursive CTE Execution Engine.
//!
//! Implements a constrained, deterministic ASCII path pattern grammar
//! for heterogeneous graph expansion across code symbols and documentation notes.
//!
//! Grammar:
//! ```text
//! PathPattern  := NodePattern ( EdgePattern NodePattern )*
//! NodePattern  := '(' ( Ident )? ( ':' Ident )? ( '{' Properties '}' )? ')'
//! EdgePattern  := ( '<-' | '-' ) '[' ( EdgeTypes )? ( '*' MinHops '..' MaxHops )? ']' ( '->' | '-' )
//!              | '-->' | '<--' | '--'
//! EdgeTypes    := ':' Identifier ( '|' Identifier )*
//! Properties   := Identifier ':' StringLiteral ( ',' Identifier ':' StringLiteral )*
//! ```

use std::collections::HashMap;

use rusqlite::params;

use ctxvault_common::types::{GraphMatchResult, MatchedEdge, MatchedNode, PathMatch};
use ctxvault_common::{Error, Result};

use crate::persistence::Store;

// ---------------------------------------------------------------------------
// AST Types
// ---------------------------------------------------------------------------

/// Direction of an edge in the query pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryDirection {
    /// Outbound: `-[...]->`
    Outgoing,
    /// Inbound: `<-[...]-`
    Incoming,
    /// Bidirectional / Undirected: `-[...]-`
    Undirected,
}

/// A parsed node pattern, e.g. `(c:CodeSymbol {name: 'SelectVictimsOnNode'})`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NodePattern {
    /// Optional variable name (e.g. `c`, `doc`).
    pub variable: Option<String>,
    /// Optional node label (e.g. `CodeSymbol`, `DocNode`, `Interface`).
    pub label: Option<String>,
    /// Exact property match filters (e.g. `name: '...'`).
    pub properties: HashMap<String, String>,
}

/// A parsed edge pattern, e.g. `<-[:calls|implements*1..2]-`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgePattern {
    /// Traversal direction relative to preceding node.
    pub direction: QueryDirection,
    /// Allowed edge types (empty means any edge type).
    pub edge_types: Vec<String>,
    /// Minimum hops (inclusive, default 1).
    pub min_hops: usize,
    /// Maximum hops (inclusive, default 1).
    pub max_hops: usize,
}

/// A full linear path pattern, e.g. `(A)-[:implements]->(B)<-[:calls*1..2]-(C)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathPattern {
    /// Starting node pattern (anchor).
    pub start_node: NodePattern,
    /// Chained edge and subsequent node steps.
    pub steps: Vec<(EdgePattern, NodePattern)>,
}

// ---------------------------------------------------------------------------
// Recursive Descent Parser
// ---------------------------------------------------------------------------

struct PatternParser<'a> {
    input: &'a str,
}

impl<'a> PatternParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input }
    }

    fn skip_ws(&mut self) {
        self.input = self.input.trim_start();
    }

    fn peek(&self) -> Option<char> {
        self.input.chars().next()
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.input.starts_with(prefix)
    }

    fn consume_str(&mut self, prefix: &str) -> bool {
        self.skip_ws();
        if self.input.starts_with(prefix) {
            self.input = &self.input[prefix.len()..];
            true
        } else {
            false
        }
    }

    fn expect_str(&mut self, prefix: &str) -> Result<()> {
        self.skip_ws();
        if self.consume_str(prefix) {
            Ok(())
        } else {
            Err(Error::Config(format!("expected '{}' at: '{}'", prefix, self.input)))
        }
    }

    fn parse_ident(&mut self) -> Option<String> {
        self.skip_ws();
        let mut len = 0;
        for c in self.input.chars() {
            if c.is_alphanumeric() || c == '_' || c == '.' || c == '-' {
                len += c.len_utf8();
            } else {
                break;
            }
        }
        if len == 0 {
            None
        } else {
            let s = &self.input[..len];
            self.input = &self.input[len..];
            Some(s.to_string())
        }
    }

    fn parse_string_literal(&mut self) -> Result<String> {
        self.skip_ws();
        let quote = match self.peek() {
            Some('\'') => '\'',
            Some('"') => '"',
            _ => {
                return Err(Error::Config(format!("expected string literal at: '{}'", self.input)))
            }
        };
        self.input = &self.input[quote.len_utf8()..];
        let mut len = 0;
        let mut found_close = false;
        for c in self.input.chars() {
            if c == quote {
                found_close = true;
                break;
            }
            len += c.len_utf8();
        }
        if !found_close {
            return Err(Error::Config("unterminated string literal".to_string()));
        }
        let s = self.input[..len].to_string();
        self.input = &self.input[len + quote.len_utf8()..];
        Ok(s)
    }

    fn parse_node(&mut self) -> Result<NodePattern> {
        self.expect_str("(")?;
        self.skip_ws();

        let mut variable = None;
        let mut label = None;

        if let Some(id1) = self.parse_ident() {
            if self.consume_str(":") {
                variable = Some(id1);
                if let Some(lbl) = self.parse_ident() {
                    label = Some(lbl);
                }
            } else {
                variable = Some(id1);
            }
        } else if self.consume_str(":") {
            if let Some(lbl) = self.parse_ident() {
                label = Some(lbl);
            }
        }

        let mut properties = HashMap::new();
        if self.consume_str("{") {
            loop {
                self.skip_ws();
                if self.consume_str("}") {
                    break;
                }
                let key = self.parse_ident().ok_or_else(|| {
                    Error::Config(format!("expected property key at: '{}'", self.input))
                })?;
                self.expect_str(":")?;
                self.skip_ws();
                let val = if self.peek() == Some('\'') || self.peek() == Some('"') {
                    self.parse_string_literal()?
                } else if let Some(val_id) = self.parse_ident() {
                    val_id
                } else {
                    return Err(Error::Config(format!(
                        "expected property value at: '{}'",
                        self.input
                    )));
                };
                properties.insert(key, val);
                self.skip_ws();
                if self.consume_str(",") {
                    continue;
                } else if self.consume_str("}") {
                    break;
                } else {
                    return Err(Error::Config(format!(
                        "expected ',' or '}}' in properties at: '{}'",
                        self.input
                    )));
                }
            }
        }

        self.expect_str(")")?;

        Ok(NodePattern { variable, label, properties })
    }

    fn parse_edge(&mut self) -> Result<EdgePattern> {
        self.skip_ws();

        // 1. Shorthand checks: -->, <--, --
        if self.consume_str("-->") {
            return Ok(EdgePattern {
                direction: QueryDirection::Outgoing,
                edge_types: Vec::new(),
                min_hops: 1,
                max_hops: 1,
            });
        }
        if self.consume_str("<--") {
            return Ok(EdgePattern {
                direction: QueryDirection::Incoming,
                edge_types: Vec::new(),
                min_hops: 1,
                max_hops: 1,
            });
        }
        if self.consume_str("--") {
            return Ok(EdgePattern {
                direction: QueryDirection::Undirected,
                edge_types: Vec::new(),
                min_hops: 1,
                max_hops: 1,
            });
        }

        // 2. Full edge syntax: (<- | -) [ (:types)? (*range)? ] (-> | -)
        let inbound = self.consume_str("<-");
        if !inbound {
            self.expect_str("-")?;
        }
        self.expect_str("[")?;

        let mut edge_types = Vec::new();
        if self.consume_str(":") {
            loop {
                if let Some(et) = self.parse_ident() {
                    edge_types.push(et);
                }
                self.skip_ws();
                if self.consume_str("|") {
                    continue;
                } else {
                    break;
                }
            }
        }

        let mut min_hops = 1;
        let mut max_hops = 1;

        if self.consume_str("*") {
            self.skip_ws();
            let mut num1_str = String::new();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    num1_str.push(c);
                    self.input = &self.input[1..];
                } else {
                    break;
                }
            }
            if !num1_str.is_empty() {
                min_hops = num1_str.parse::<usize>().unwrap_or(1);
            }
            if self.consume_str("..") {
                let mut num2_str = String::new();
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        num2_str.push(c);
                        self.input = &self.input[1..];
                    } else {
                        break;
                    }
                }
                max_hops =
                    if !num2_str.is_empty() { num2_str.parse::<usize>().unwrap_or(5) } else { 5 };
            } else {
                max_hops = min_hops;
            }
        }

        self.expect_str("]")?;

        let outbound = self.consume_str("->");
        if !outbound {
            self.expect_str("-")?;
        }

        let direction = match (inbound, outbound) {
            (true, false) => QueryDirection::Incoming,
            (false, true) => QueryDirection::Outgoing,
            _ => QueryDirection::Undirected,
        };

        Ok(EdgePattern { direction, edge_types, min_hops, max_hops: max_hops.max(min_hops) })
    }

    fn parse(&mut self) -> Result<PathPattern> {
        let start_node = self.parse_node()?;
        let mut steps = Vec::new();

        loop {
            self.skip_ws();
            if self.input.is_empty() {
                break;
            }
            if self.starts_with("-") || self.starts_with("<-") {
                let edge = self.parse_edge()?;
                let node = self.parse_node()?;
                steps.push((edge, node));
            } else {
                break;
            }
        }

        Ok(PathPattern { start_node, steps })
    }
}

/// Parse a linear Cypher-Lite path pattern from string.
pub fn parse_path_pattern(input: &str) -> Result<PathPattern> {
    let mut parser = PatternParser::new(input);
    let pattern = parser.parse()?;
    parser.skip_ws();
    if !parser.input.is_empty() {
        return Err(Error::Config(format!(
            "unexpected trailing characters in graph_match pattern: '{}'",
            parser.input
        )));
    }
    Ok(pattern)
}

// ---------------------------------------------------------------------------
// Query Compilation & Execution (Parameterized Recursive CTE)
// ---------------------------------------------------------------------------

/// Execution engine that compiles a `PathPattern` into SQLite queries.
pub struct QueryEngine<'a> {
    store: &'a Store,
}

impl<'a> QueryEngine<'a> {
    /// Create a new query engine over the SQLite store.
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }

    /// Execute a pattern match with optional class, where filter, and limit constraints.
    pub fn execute_match(
        &self,
        pattern: &PathPattern,
        edge_class_filter: Option<&str>,
        where_filter: Option<&str>,
        limit: usize,
        max_depth_cap: usize,
    ) -> Result<GraphMatchResult> {
        let limit = limit.clamp(1, 100);
        let max_depth_cap = max_depth_cap.clamp(1, 5);

        // 1. Resolve starting anchor candidate nodes
        let anchor_candidates = self.resolve_node_candidates(&pattern.start_node)?;
        if anchor_candidates.is_empty() {
            return Ok(GraphMatchResult::default());
        }

        let mut all_matches = Vec::new();
        let mut matched_nodes_map: HashMap<String, MatchedNode> = HashMap::new();
        let mut matched_edges_set: std::collections::HashSet<(String, String, String, String)> =
            std::collections::HashSet::new();

        // 2. Perform path expansion from each candidate anchor
        for anchor in &anchor_candidates {
            let mut current_paths: Vec<(String, usize, String)> =
                vec![(anchor.clone(), 0, anchor.clone())];

            for (edge_pat, next_node) in &pattern.steps {
                let mut next_paths = Vec::new();
                let step_max_depth = edge_pat.max_hops.min(max_depth_cap);
                let step_min_depth = edge_pat.min_hops.max(1);

                for (curr_node, curr_depth, curr_path_str) in current_paths {
                    let step_results = self.traverse_step(
                        &curr_node,
                        edge_pat.direction,
                        &edge_pat.edge_types,
                        edge_class_filter,
                        step_min_depth,
                        step_max_depth,
                        limit,
                    )?;

                    for (target_node, depth_delta, _path_segment, edges_used) in step_results {
                        // Filter by next node constraints if any
                        if !self.matches_node_constraints(&target_node, next_node)? {
                            continue;
                        }

                        let dir_symbol = match edge_pat.direction {
                            QueryDirection::Outgoing => "->",
                            QueryDirection::Incoming => "<-",
                            QueryDirection::Undirected => "--",
                        };
                        let new_path_str =
                            format!("{} {} {}", curr_path_str, dir_symbol, target_node);
                        let total_depth = curr_depth + depth_delta;

                        for (s, t, et, dir) in edges_used {
                            matched_edges_set.insert((s, t, et, dir));
                        }

                        next_paths.push((target_node, total_depth, new_path_str));
                    }
                }

                current_paths = next_paths;
                if current_paths.is_empty() {
                    break;
                }
            }

            for (terminal_node, depth, path_str) in current_paths {
                // Apply where clause filter
                if let Some(wf) = where_filter {
                    if !self.eval_where_filter(&terminal_node, wf) {
                        continue;
                    }
                }

                let (file_path, symbol_type, line) = self.lookup_node_metadata(&terminal_node);

                all_matches.push(PathMatch {
                    node: terminal_node.clone(),
                    depth,
                    path: path_str,
                    symbol_type: symbol_type.clone(),
                    file_path: file_path.clone(),
                    line,
                });

                // Add to nodes map
                if !matched_nodes_map.contains_key(&terminal_node) {
                    let mut props = HashMap::new();
                    if let Some(fp) = &file_path {
                        props.insert("file_path".to_string(), fp.clone());
                    }
                    if let Some(st) = &symbol_type {
                        props.insert("symbol_type".to_string(), st.clone());
                    }
                    if let Some(l) = line {
                        props.insert("line".to_string(), l.to_string());
                    }
                    props.insert("name".to_string(), terminal_node.clone());

                    matched_nodes_map.insert(
                        terminal_node.clone(),
                        MatchedNode {
                            id: terminal_node.clone(),
                            label: symbol_type.unwrap_or_else(|| "Node".to_string()),
                            properties: props,
                        },
                    );
                }

                // Add anchor to nodes map
                if !matched_nodes_map.contains_key(anchor) {
                    let (a_file, a_type, a_line) = self.lookup_node_metadata(anchor);
                    let mut a_props = HashMap::new();
                    if let Some(fp) = &a_file {
                        a_props.insert("file_path".to_string(), fp.clone());
                    }
                    if let Some(l) = a_line {
                        a_props.insert("line".to_string(), l.to_string());
                    }
                    a_props.insert("name".to_string(), anchor.clone());
                    matched_nodes_map.insert(
                        anchor.clone(),
                        MatchedNode {
                            id: anchor.clone(),
                            label: a_type.unwrap_or_else(|| "Node".to_string()),
                            properties: a_props,
                        },
                    );
                }

                if all_matches.len() >= limit {
                    break;
                }
            }

            if all_matches.len() >= limit {
                break;
            }
        }

        let total_matches = all_matches.len();
        let nodes = matched_nodes_map.into_values().collect();
        let edges = matched_edges_set
            .into_iter()
            .map(|(source, target, edge_type, direction)| MatchedEdge {
                source,
                target,
                edge_type,
                direction,
            })
            .collect();

        Ok(GraphMatchResult { matches: all_matches, total_matches, nodes, edges })
    }

    /// Resolve candidate node identifiers from node pattern properties.
    fn resolve_node_candidates(&self, node: &NodePattern) -> Result<Vec<String>> {
        // Direct path property
        if let Some(path) = node.properties.get("path") {
            return Ok(vec![path.clone()]);
        }

        // Name property: query code_symbols and exact edge matches
        if let Some(name) = node.properties.get("name") {
            let mut candidates = Vec::new();
            if let Ok(syms) = self.store.find_symbols_by_name(name) {
                for s in syms {
                    candidates.push(s.scope_path);
                }
            }
            if candidates.is_empty() {
                candidates.push(name.clone());
            }
            candidates.sort();
            candidates.dedup();
            return Ok(candidates);
        }

        // Title property for doc nodes
        if let Some(title) = node.properties.get("title") {
            if let Ok(files) = self.store.list_files() {
                let matches: Vec<String> = files
                    .into_iter()
                    .filter(|f| f.title.as_deref() == Some(title))
                    .map(|f| f.path)
                    .collect();
                if !matches.is_empty() {
                    return Ok(matches);
                }
            }
        }

        // If variable or label without properties, fall back to empty or symbol types
        if let Some(label) = &node.label {
            if label == "CodeSymbol" {
                if let Ok(syms) = self.store.get_all_code_symbols() {
                    return Ok(syms.into_iter().take(25).map(|s| s.scope_path).collect());
                }
            }
        }

        // If no properties or unbound variable, resolve distinct sources from edges table
        let conn = self.store.conn();
        if let Ok(mut stmt) = conn.prepare("SELECT DISTINCT source FROM edges LIMIT 100") {
            if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                let endpoints: Vec<String> = rows.flatten().collect();
                if !endpoints.is_empty() {
                    return Ok(endpoints);
                }
            }
        }

        Ok(Vec::new())
    }

    /// Execute a single traversal step using SQLite.
    fn traverse_step(
        &self,
        anchor: &str,
        direction: QueryDirection,
        edge_types: &[String],
        edge_class_filter: Option<&str>,
        min_hops: usize,
        max_hops: usize,
        limit: usize,
    ) -> Result<Vec<(String, usize, String, Vec<(String, String, String, String)>)>> {
        let type_filter = edge_types.join(",");
        let class_filter = edge_class_filter.unwrap_or("").to_string();

        let mut results = Vec::new();

        match direction {
            QueryDirection::Outgoing => {
                let sql =
                    "WITH RECURSIVE traversal(current_node, depth, path, original_anchor) AS (
                    SELECT e.target, 1, ?1 || '->' || e.target, ?1
                    FROM edges e
                    WHERE e.source = ?1
                      AND (?2 = '' OR instr(?2, e.edge_type) > 0)
                      AND (?3 = '' OR e.edge_class = ?3)

                    UNION ALL

                    SELECT e.target, t.depth + 1, t.path || '->' || e.target, t.original_anchor
                    FROM edges e
                    JOIN traversal t ON e.source = t.current_node
                    WHERE t.depth < ?4
                      AND (?2 = '' OR instr(?2, e.edge_type) > 0)
                      AND (?3 = '' OR e.edge_class = ?3)
                      AND instr(t.path, e.target) = 0
                )
                SELECT DISTINCT current_node, depth, path
                FROM traversal
                WHERE depth >= ?5
                LIMIT ?6;";

                let conn = self.store.conn();
                let mut stmt = conn.prepare(sql).map_err(|e| Error::Database(e.to_string()))?;
                let rows = stmt
                    .query_map(
                        params![
                            anchor,
                            type_filter,
                            class_filter,
                            max_hops as i64,
                            min_hops as i64,
                            limit as i64
                        ],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, i64>(1)? as usize,
                                row.get::<_, String>(2)?,
                            ))
                        },
                    )
                    .map_err(|e| Error::Database(e.to_string()))?;

                for r in rows {
                    let (node, depth, path_str) = r.map_err(|e| Error::Database(e.to_string()))?;
                    let edges_used = vec![(
                        anchor.to_string(),
                        node.clone(),
                        type_filter.clone(),
                        "outgoing".to_string(),
                    )];
                    results.push((node, depth, path_str, edges_used));
                }
            }
            QueryDirection::Incoming => {
                let sql =
                    "WITH RECURSIVE traversal(current_node, depth, path, original_anchor) AS (
                    SELECT e.source, 1, ?1 || '<-' || e.source, ?1
                    FROM edges e
                    WHERE e.target = ?1
                      AND (?2 = '' OR instr(?2, e.edge_type) > 0)
                      AND (?3 = '' OR e.edge_class = ?3)

                    UNION ALL

                    SELECT e.source, t.depth + 1, t.path || '<-' || e.source, t.original_anchor
                    FROM edges e
                    JOIN traversal t ON e.target = t.current_node
                    WHERE t.depth < ?4
                      AND (?2 = '' OR instr(?2, e.edge_type) > 0)
                      AND (?3 = '' OR e.edge_class = ?3)
                      AND instr(t.path, e.source) = 0
                )
                SELECT DISTINCT current_node, depth, path
                FROM traversal
                WHERE depth >= ?5
                LIMIT ?6;";

                let conn = self.store.conn();
                let mut stmt = conn.prepare(sql).map_err(|e| Error::Database(e.to_string()))?;
                let rows = stmt
                    .query_map(
                        params![
                            anchor,
                            type_filter,
                            class_filter,
                            max_hops as i64,
                            min_hops as i64,
                            limit as i64
                        ],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, i64>(1)? as usize,
                                row.get::<_, String>(2)?,
                            ))
                        },
                    )
                    .map_err(|e| Error::Database(e.to_string()))?;

                for r in rows {
                    let (node, depth, path_str) = r.map_err(|e| Error::Database(e.to_string()))?;
                    let edges_used = vec![(
                        node.clone(),
                        anchor.to_string(),
                        type_filter.clone(),
                        "incoming".to_string(),
                    )];
                    results.push((node, depth, path_str, edges_used));
                }
            }
            QueryDirection::Undirected => {
                let out = self.traverse_step(
                    anchor,
                    QueryDirection::Outgoing,
                    edge_types,
                    edge_class_filter,
                    min_hops,
                    max_hops,
                    limit,
                )?;
                let inc = self.traverse_step(
                    anchor,
                    QueryDirection::Incoming,
                    edge_types,
                    edge_class_filter,
                    min_hops,
                    max_hops,
                    limit,
                )?;
                results.extend(out);
                results.extend(inc);
            }
        }

        Ok(results)
    }

    /// Check if target node conforms to subsequent pattern constraints.
    fn matches_node_constraints(&self, node_id: &str, pattern: &NodePattern) -> Result<bool> {
        if let Some(prop_name) = pattern.properties.get("name") {
            if !node_id.ends_with(prop_name) && node_id != prop_name {
                return Ok(false);
            }
        }
        if let Some(prop_path) = pattern.properties.get("path") {
            if node_id != prop_path {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Evaluate simple `where` clause predicates on candidate path.
    fn eval_where_filter(&self, node_id: &str, where_filter: &str) -> bool {
        let lower_filter = where_filter.to_lowercase();
        let lower_node = node_id.to_lowercase();

        if lower_filter.contains("not") && lower_filter.contains("contains") {
            for part in lower_filter.split("and") {
                if let Some(idx) = part.find("contains") {
                    let target = part[idx + "contains".len()..]
                        .trim()
                        .trim_matches(|c| c == '\'' || c == '"' || c == ' ');
                    if lower_node.contains(target) {
                        return false;
                    }
                }
            }
            return true;
        }

        if lower_filter.contains("contains") {
            for part in lower_filter.split("and") {
                if let Some(idx) = part.find("contains") {
                    let target = part[idx + "contains".len()..]
                        .trim()
                        .trim_matches(|c| c == '\'' || c == '"' || c == ' ');
                    if !lower_node.contains(target) {
                        return false;
                    }
                }
            }
            return true;
        }

        true
    }

    /// Look up metadata (file path, symbol type, start line) for a node.
    fn lookup_node_metadata(
        &self,
        node_id: &str,
    ) -> (Option<String>, Option<String>, Option<usize>) {
        if let Ok(syms) = self.store.find_symbols_by_qualified_name(node_id) {
            if let Some(s) = syms.first() {
                return (
                    Some(s.file_path.clone()),
                    Some(format!("{:?}", s.symbol_type)),
                    Some(s.start_line),
                );
            }
        }
        if let Ok(syms) = self.store.find_symbols_by_name(node_id) {
            if let Some(s) = syms.first() {
                return (
                    Some(s.file_path.clone()),
                    Some(format!("{:?}", s.symbol_type)),
                    Some(s.start_line),
                );
            }
        }
        if let Ok(Some(file)) = self.store.get_file(node_id) {
            return (Some(file.path), Some("DocNode".to_string()), Some(1));
        }
        (None, None, None)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_pattern() {
        let pattern = parse_path_pattern("(:CodeSymbol {name: 'SelectVictimsOnNode'})-[:implements]->(:Interface)<-[:calls*1..2]-(c:CodeSymbol)").unwrap();
        assert_eq!(pattern.start_node.label.as_deref(), Some("CodeSymbol"));
        assert_eq!(
            pattern.start_node.properties.get("name").map(|s| s.as_str()),
            Some("SelectVictimsOnNode")
        );
        assert_eq!(pattern.steps.len(), 2);

        let (ref e1, ref n1) = pattern.steps[0];
        assert_eq!(e1.direction, QueryDirection::Outgoing);
        assert_eq!(e1.edge_types, vec!["implements"]);
        assert_eq!(n1.label.as_deref(), Some("Interface"));

        let (ref e2, ref n2) = pattern.steps[1];
        assert_eq!(e2.direction, QueryDirection::Incoming);
        assert_eq!(e2.edge_types, vec!["calls"]);
        assert_eq!(e2.min_hops, 1);
        assert_eq!(e2.max_hops, 2);
        assert_eq!(n2.variable.as_deref(), Some("c"));
        assert_eq!(n2.label.as_deref(), Some("CodeSymbol"));
    }

    #[test]
    fn test_parse_doc_pattern() {
        let pattern = parse_path_pattern(
            "(:DocNode {title: 'Manifest Admission'})-[:wikilink*1..2]->(related:DocNode)",
        )
        .unwrap();
        assert_eq!(pattern.start_node.label.as_deref(), Some("DocNode"));
        assert_eq!(
            pattern.start_node.properties.get("title").map(|s| s.as_str()),
            Some("Manifest Admission")
        );
        assert_eq!(pattern.steps.len(), 1);
        let (ref e, ref n) = pattern.steps[0];
        assert_eq!(e.edge_types, vec!["wikilink"]);
        assert_eq!(n.variable.as_deref(), Some("related"));
        assert_eq!(n.label.as_deref(), Some("DocNode"));
    }

    #[test]
    fn test_parse_shorthand_arrows() {
        let pattern = parse_path_pattern("(a)-->(b)<--(c)--(d)").unwrap();
        assert_eq!(pattern.steps.len(), 3);
        assert_eq!(pattern.steps[0].0.direction, QueryDirection::Outgoing);
        assert_eq!(pattern.steps[1].0.direction, QueryDirection::Incoming);
        assert_eq!(pattern.steps[2].0.direction, QueryDirection::Undirected);
    }
}
