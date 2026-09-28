use std::path::Path;

use groundcontrol_common::config::ChunkingConfig;
use groundcontrol_common::types::{EdgeProvenance, ExternalRefKind, ResolutionConfidence};

use super::CodeGraphExtractor;
use crate::parser::code::chunker::CodeChunker;

#[test]
fn test_language_specific_ast_edges() {
    let config = ChunkingConfig::default();

    // 1. TypeScript: decorates, extends, implements
    let ts_code = r#"
@Injectable()
export class UserService extends BaseService implements IUserService {
    @Get('/users')
    getUsers() {}
}
"#;
    let ts_res = CodeChunker::parse_and_chunk(Path::new("src/user.ts"), ts_code, &config).unwrap();
    let ts_edges = CodeGraphExtractor::extract_edges_for_file(
        Path::new("src/user.ts"),
        ts_code,
        &ts_res.symbols,
        &ts_res.symbols,
    );
    assert!(
        ts_edges.iter().any(|e| e.edge_type == "extends"
            && e.source == "UserService"
            && e.target == "BaseService"),
        "Expected UserService -[:extends]-> BaseService, got: {:?}",
        ts_edges
    );
    assert!(
        ts_edges.iter().any(|e| e.edge_type == "implements"
            && e.source == "UserService"
            && e.target == "IUserService"),
        "Expected UserService -[:implements]-> IUserService, got: {:?}",
        ts_edges
    );
    assert!(
        ts_edges.iter().any(|e| e.edge_type == "decorates" && e.target == "Injectable"),
        "Expected decorates Injectable, got: {:?}",
        ts_edges
    );

    // 2. Python: decorates, inherits
    let py_code = r#"
@app.route("/items")
class ItemView(BaseView):
    @login_required
    def get(self):
        pass
"#;
    let py_res = CodeChunker::parse_and_chunk(Path::new("app/views.py"), py_code, &config).unwrap();
    let py_edges = CodeGraphExtractor::extract_edges_for_file(
        Path::new("app/views.py"),
        py_code,
        &py_res.symbols,
        &py_res.symbols,
    );
    assert!(
        py_edges
            .iter()
            .any(|e| e.edge_type == "inherits" && e.source == "ItemView" && e.target == "BaseView"),
        "Expected ItemView -[:inherits]-> BaseView, got: {:?}",
        py_edges
    );
    assert!(
        py_edges.iter().any(|e| e.edge_type == "decorates" && e.target == "app.route"),
        "Expected decorates app.route, got: {:?}",
        py_edges
    );

    // 3. Rust: macro_expands
    let rs_code = r#"
pub fn run() {
    println!("hello");
    tokio::select! {
        _ = a => {}
    }
}
"#;
    let rs_res = CodeChunker::parse_and_chunk(Path::new("src/main.rs"), rs_code, &config).unwrap();
    let rs_edges = CodeGraphExtractor::extract_edges_for_file(
        Path::new("src/main.rs"),
        rs_code,
        &rs_res.symbols,
        &rs_res.symbols,
    );
    assert!(
        rs_edges.iter().any(|e| e.edge_type == "macro_expands" && e.target == "println"),
        "Expected run -[:macro_expands]-> println, got: {:?}",
        rs_edges
    );
    assert!(
        rs_edges.iter().any(|e| e.edge_type == "macro_expands" && e.target == "tokio::select"),
        "Expected run -[:macro_expands]-> tokio::select, got: {:?}",
        rs_edges
    );

    // 4. Go: struct_embeds
    let go_code = r#"
package server

import "sync"

type Server struct {
    sync.Mutex
    Logger
    port int
}
"#;
    let go_res =
        CodeChunker::parse_and_chunk(Path::new("server/server.go"), go_code, &config).unwrap();
    let go_edges = CodeGraphExtractor::extract_edges_for_file(
        Path::new("server/server.go"),
        go_code,
        &go_res.symbols,
        &go_res.symbols,
    );
    assert!(
        go_edges.iter().any(|e| e.edge_type == "struct_embeds" && e.target == "sync.Mutex"),
        "Expected Server -[:struct_embeds]-> sync.Mutex, got: {:?}",
        go_edges
    );
    assert!(
        go_edges.iter().any(|e| e.edge_type == "struct_embeds" && e.target == "Logger"),
        "Expected Server -[:struct_embeds]-> Logger, got: {:?}",
        go_edges
    );

    // 5. SQL: foreign_key
    let sql_code = r#"
CREATE TABLE orders (
    id INT PRIMARY KEY,
    user_id INT REFERENCES users(id),
    FOREIGN KEY (account_id) REFERENCES accounts(id)
);
"#;
    let sql_res =
        CodeChunker::parse_and_chunk(Path::new("schema/orders.sql"), sql_code, &config).unwrap();
    let sql_edges = CodeGraphExtractor::extract_edges_for_file(
        Path::new("schema/orders.sql"),
        sql_code,
        &sql_res.symbols,
        &sql_res.symbols,
    );
    assert!(
        sql_edges.iter().any(|e| e.edge_type == "foreign_key" && e.target == "users"),
        "Expected orders -[:foreign_key]-> users, got: {:?}",
        sql_edges
    );
    assert!(
        sql_edges.iter().any(|e| e.edge_type == "foreign_key" && e.target == "accounts"),
        "Expected orders -[:foreign_key]-> accounts, got: {:?}",
        sql_edges
    );
}

#[test]
fn test_code_graph_extraction_calls_and_defines() {
    let code_a = r#"
pub struct SearchEngine;

impl SearchEngine {
    pub fn search(&self, q: &str) -> Vec<String> {
        let results = rrf_fuse(q);
        results
    }
}

pub fn rrf_fuse(q: &str) -> Vec<String> {
    vec![q.to_string()]
}
"#;
    let config = ChunkingConfig::default();
    let parse_res =
        CodeChunker::parse_and_chunk(Path::new("src/search.rs"), code_a, &config).unwrap();
    let edges = CodeGraphExtractor::extract_edges_for_file(
        Path::new("src/search.rs"),
        code_a,
        &parse_res.symbols,
        &parse_res.symbols,
    );

    // Check defines edges
    assert!(edges.iter().any(|e| e.edge_type == "defines" && e.target == "SearchEngine"));
    assert!(edges.iter().any(|e| e.edge_type == "defines" && e.target == "SearchEngine > search"));
    assert!(edges.iter().any(|e| e.edge_type == "defines" && e.target == "rrf_fuse"));

    // Check calls edges: SearchEngine > search calls rrf_fuse
    assert!(edges.iter().any(|e| e.edge_type == "calls"
        && e.source == "SearchEngine > search"
        && e.target == "rrf_fuse"));
}

#[test]
fn test_call_edge_confidence_unique_is_high() {
    // rrf_fuse resolves uniquely within the current file -> High confidence.
    let code = r#"
pub fn search(q: &str) -> Vec<String> {
    rrf_fuse(q)
}

pub fn rrf_fuse(q: &str) -> Vec<String> {
    vec![q.to_string()]
}
"#;
    let config = ChunkingConfig::default();
    let parse_res =
        CodeChunker::parse_and_chunk(Path::new("src/search.rs"), code, &config).unwrap();
    let edges = CodeGraphExtractor::extract_edges_for_file(
        Path::new("src/search.rs"),
        code,
        &parse_res.symbols,
        &parse_res.symbols,
    );

    let call_edge = edges
        .iter()
        .find(|e| e.edge_type == "calls" && e.target == "rrf_fuse")
        .expect("expected a call edge to rrf_fuse");
    assert_eq!(call_edge.confidence, Some(ResolutionConfidence::High));

    // defines edges are exact -> High.
    let define_edge =
        edges.iter().find(|e| e.edge_type == "defines").expect("expected a defines edge");
    assert_eq!(define_edge.confidence, Some(ResolutionConfidence::High));
}

#[test]
fn test_call_edge_confidence_ambiguous_is_medium_or_speculative() {
    // The caller file has no local `helper`; two workspace candidates named
    // `helper` exist in different files. One shares the caller's directory,
    // so the same-directory heuristic (case 3) applies -> Medium.
    let caller_code = r#"
pub fn run() {
    helper();
}
"#;
    let config = ChunkingConfig::default();
    let caller_res =
        CodeChunker::parse_and_chunk(Path::new("src/a/caller.rs"), caller_code, &config).unwrap();

    // Two distinct `helper` symbols in different files.
    let same_dir = r#"pub fn helper() {}"#;
    let other_dir = r#"pub fn helper() {}"#;
    let same_dir_res =
        CodeChunker::parse_and_chunk(Path::new("src/a/other.rs"), same_dir, &config).unwrap();
    let other_dir_res =
        CodeChunker::parse_and_chunk(Path::new("src/b/other.rs"), other_dir, &config).unwrap();

    let mut all_symbols = caller_res.symbols.clone();
    all_symbols.extend(same_dir_res.symbols.clone());
    all_symbols.extend(other_dir_res.symbols.clone());

    let edges = CodeGraphExtractor::extract_edges_for_file(
        Path::new("src/a/caller.rs"),
        caller_code,
        &caller_res.symbols,
        &all_symbols,
    );

    let call_edge = edges
        .iter()
        .find(|e| e.edge_type == "calls" && e.target == "helper")
        .expect("expected a call edge to helper");
    assert_eq!(
        call_edge.confidence,
        Some(ResolutionConfidence::Medium),
        "same-directory disambiguation should yield Medium confidence"
    );
    assert_ne!(call_edge.confidence, Some(ResolutionConfidence::High));
}

#[test]
fn test_hybrid_lsp_receiver_method_disambiguation_rust() {
    let config = ChunkingConfig::default();

    let caller_code = r#"
use crate::search::SearchClient;

pub fn execute() {
    let client = SearchClient::new();
    client.query("rust");
}
"#;
    let target_search_code = r#"
pub struct SearchClient;

impl SearchClient {
    pub fn new() -> Self { SearchClient }
    pub fn query(&self, q: &str) -> Vec<String> { vec![] }
}
"#;
    let legacy_search_code = r#"
pub struct SearchClient;

impl SearchClient {
    pub fn new() -> Self { SearchClient }
    pub fn query(&self, q: &str) -> Vec<String> { vec![] }
}
"#;

    let caller_res =
        CodeChunker::parse_and_chunk(Path::new("src/caller.rs"), caller_code, &config).unwrap();
    let target_res =
        CodeChunker::parse_and_chunk(Path::new("src/search.rs"), target_search_code, &config)
            .unwrap();
    let legacy_res =
        CodeChunker::parse_and_chunk(Path::new("legacy/search.rs"), legacy_search_code, &config)
            .unwrap();

    let mut all_symbols = caller_res.symbols.clone();
    all_symbols.extend(target_res.symbols.clone());
    all_symbols.extend(legacy_res.symbols.clone());

    let symbol_index = CodeGraphExtractor::build_symbol_index(&all_symbols);
    let extraction = CodeGraphExtractor::extract_edges_for_file_with_index(
        Path::new("src/caller.rs"),
        caller_code,
        &caller_res.symbols,
        &symbol_index,
    );

    // Verify import edge exists
    assert!(extraction
        .edges
        .iter()
        .any(|e| e.edge_type == "imports" && e.target == "crate::search::SearchClient"));

    // Verify call edge resolves to SearchClient > query with High confidence
    let call_edge = extraction
        .edges
        .iter()
        .find(|e| e.edge_type == "calls" && e.target == "SearchClient > query")
        .expect("expected call edge to SearchClient > query");
    assert_eq!(call_edge.confidence, Some(ResolutionConfidence::High));
}

#[test]
fn test_hybrid_lsp_receiver_method_disambiguation_typescript() {
    let config = ChunkingConfig::default();

    let caller_code = r#"
import { ApiClient } from "./api";

export class Controller {
    handleRequest() {
        const client = new ApiClient();
        client.fetchData();
    }
}
"#;
    let target_api_code = r#"
export class ApiClient {
    fetchData() {
        return "live";
    }
}
"#;
    let mock_api_code = r#"
export class ApiClient {
    fetchData() {
        return "mock";
    }
}
"#;

    let caller_res =
        CodeChunker::parse_and_chunk(Path::new("src/controller.ts"), caller_code, &config).unwrap();
    let target_res =
        CodeChunker::parse_and_chunk(Path::new("src/api.ts"), target_api_code, &config).unwrap();
    let mock_res =
        CodeChunker::parse_and_chunk(Path::new("mock/api.ts"), mock_api_code, &config).unwrap();

    let mut all_symbols = caller_res.symbols.clone();
    all_symbols.extend(target_res.symbols.clone());
    all_symbols.extend(mock_res.symbols.clone());

    let symbol_index = CodeGraphExtractor::build_symbol_index(&all_symbols);
    let extraction = CodeGraphExtractor::extract_edges_for_file_with_index(
        Path::new("src/controller.ts"),
        caller_code,
        &caller_res.symbols,
        &symbol_index,
    );

    let call_edge = extraction
        .edges
        .iter()
        .find(|e| e.edge_type == "calls" && e.target == "ApiClient > fetchData")
        .expect("expected call edge to ApiClient > fetchData");
    assert_eq!(call_edge.confidence, Some(ResolutionConfidence::High));
}

#[test]
fn test_external_ref_capture_local_vs_unresolved_call() {
    // The caller defines `run`, calls the locally-defined `helper` (resolves to a
    // real in-corpus symbol) and `missing_external` (resolves to nothing).
    let code = r#"
pub fn helper() {}

pub fn run() {
    helper();
    missing_external();
}
"#;
    let config = ChunkingConfig::default();
    let res = CodeChunker::parse_and_chunk(Path::new("src/lib.rs"), code, &config).unwrap();

    let symbol_index = CodeGraphExtractor::build_symbol_index(&res.symbols);
    let extraction = CodeGraphExtractor::extract_edges_for_file_with_index(
        Path::new("src/lib.rs"),
        code,
        &res.symbols,
        &symbol_index,
    );

    // (a) The fully-local call yields a normal `calls` edge...
    let local_edge = extraction
        .edges
        .iter()
        .find(|e| e.edge_type == "calls" && e.target == "helper")
        .expect("expected a resolved `calls` edge to the local helper");
    assert_eq!(local_edge.confidence, Some(ResolutionConfidence::High));

    // ...and NO external ref for the resolved local call.
    assert!(
        !extraction.external_refs.iter().any(|r| r.raw_target == "helper"),
        "a fully-local call must not produce an ExternalRef, got: {:?}",
        extraction.external_refs
    );

    // (b) The unresolved external call is captured as an ExternalRef.
    let ext = extraction
        .external_refs
        .iter()
        .find(|r| r.raw_target == "missing_external" && r.kind == ExternalRefKind::Call)
        .expect("expected an ExternalRef for the unresolved external call");
    assert_eq!(ext.caller_scope_path, "run");
    assert_eq!(ext.confidence, ResolutionConfidence::Speculative);
}

#[test]
fn test_polyglot_pure_ast_declarative_inheritance() {
    let config = ChunkingConfig::default();

    // 1. Java: class extends base implements interface
    let java_code = r#"
package com.example;

public class AdService extends BaseService implements IAdService {
    public void serveAd() {}
}
"#;
    let java_res =
        CodeChunker::parse_and_chunk(Path::new("src/AdService.java"), java_code, &config).unwrap();
    let java_edges = CodeGraphExtractor::extract_edges_for_file(
        Path::new("src/AdService.java"),
        java_code,
        &java_res.symbols,
        &java_res.symbols,
    );
    assert!(
        java_edges.iter().any(|e| e.edge_type == "inherits"
            && e.source == "AdService"
            && e.target == "BaseService"),
        "Expected Java AdService -[:inherits]-> BaseService, got: {:?}",
        java_edges
    );
    assert!(
        java_edges.iter().any(|e| e.edge_type == "implements"
            && e.source == "AdService"
            && e.target == "IAdService"),
        "Expected Java AdService -[:implements]-> IAdService, got: {:?}",
        java_edges
    );

    // 2. C#: class inherits base
    let cs_code = r#"
namespace Cart;

public class CartService : BaseCartService {
    public void Checkout() {}
}
"#;
    let cs_res =
        CodeChunker::parse_and_chunk(Path::new("src/CartService.cs"), cs_code, &config).unwrap();
    let cs_edges = CodeGraphExtractor::extract_edges_for_file(
        Path::new("src/CartService.cs"),
        cs_code,
        &cs_res.symbols,
        &cs_res.symbols,
    );
    assert!(
        cs_edges.iter().any(|e| e.edge_type == "inherits"
            && e.source == "CartService"
            && e.target == "BaseCartService"),
        "Expected C# CartService -[:inherits]-> BaseCartService, got: {:?}",
        cs_edges
    );

    // 3. C++: class inherits base
    let cpp_code = r#"
class Derived : public BaseClass {
    void process() {}
};
"#;
    let cpp_res =
        CodeChunker::parse_and_chunk(Path::new("src/derived.cpp"), cpp_code, &config).unwrap();
    let cpp_edges = CodeGraphExtractor::extract_edges_for_file(
        Path::new("src/derived.cpp"),
        cpp_code,
        &cpp_res.symbols,
        &cpp_res.symbols,
    );
    assert!(
        cpp_edges
            .iter()
            .any(|e| e.edge_type == "inherits" && e.source == "Derived" && e.target == "BaseClass"),
        "Expected C++ Derived -[:inherits]-> BaseClass, got: {:?}",
        cpp_edges
    );

    // 4. Go: struct embeds
    let go_code = r#"
package service

type OrderService struct {
    CommonService
}
"#;
    let go_res =
        CodeChunker::parse_and_chunk(Path::new("service/order.go"), go_code, &config).unwrap();
    let go_edges = CodeGraphExtractor::extract_edges_for_file(
        Path::new("service/order.go"),
        go_code,
        &go_res.symbols,
        &go_res.symbols,
    );
    assert!(
        go_edges.iter().any(|e| e.edge_type == "struct_embeds"
            && e.source == "OrderService"
            && e.target == "CommonService"),
        "Expected Go OrderService -[:struct_embeds]-> CommonService, got: {:?}",
        go_edges
    );
}

#[test]
fn test_test_to_target_linking_and_assertion_sink_pruning() {
    let config = ChunkingConfig::default();

    let code = r#"
pub fn calculate_discount(price: f64) -> f64 {
    price * 0.9
}

#[test]
fn test_calculate_discount() {
    let d = calculate_discount(100.0);
    assert_eq!(d, 90.0);
}
"#;
    let res =
        CodeChunker::parse_and_chunk(Path::new("tests/discount_test.rs"), code, &config).unwrap();
    let symbol_index = CodeGraphExtractor::build_symbol_index(&res.symbols);
    let extraction = CodeGraphExtractor::extract_edges_for_file_with_index(
        Path::new("tests/discount_test.rs"),
        code,
        &res.symbols,
        &symbol_index,
    );

    // (a) Normal calls edge exists
    assert!(
        extraction.edges.iter().any(|e| e.edge_type == "calls"
            && e.source == "test_calculate_discount"
            && e.target == "calculate_discount"),
        "Expected test_calculate_discount -[:calls]-> calculate_discount, got: {:?}",
        extraction.edges
    );

    // (b) Semantic tests edge exists with EdgeProvenance::CodeTests
    let test_edge = extraction
        .edges
        .iter()
        .find(|e| {
            e.edge_type == "tests"
                && e.source == "test_calculate_discount"
                && e.target == "calculate_discount"
        })
        .expect("Expected test_calculate_discount -[:tests]-> calculate_discount");
    assert_eq!(test_edge.provenance, EdgeProvenance::CodeTests);

    // (c) Assertion sinks (assert_eq) must be pruned from tests edges
    assert!(
        !extraction.edges.iter().any(|e| e.edge_type == "tests" && e.target.contains("assert")),
        "Assertion sinks must be pruned from `tests` edges, got: {:?}",
        extraction.edges
    );
}

#[test]
fn test_declarative_route_query_packs_and_handles_edges() {
    use groundcontrol_common::types::CodeSymbolType;
    let config = ChunkingConfig::default();

    // 1. Rust: Axum route
    let rs_code = r#"
async fn list_users() {}
pub fn app() {
    Router::new().route("/api/users", get(list_users));
}
"#;
    let rs_res = CodeChunker::parse_and_chunk(Path::new("src/api.rs"), rs_code, &config).unwrap();
    assert!(
        rs_res
            .symbols
            .iter()
            .any(|s| s.symbol_type == CodeSymbolType::Route && s.name == "/api/users"),
        "Expected Axum route symbol '/api/users', got symbols: {:?}",
        rs_res.symbols
    );
    let rs_symbol_index = CodeGraphExtractor::build_symbol_index(&rs_res.symbols);
    let rs_extraction = CodeGraphExtractor::extract_edges_for_file_with_index(
        Path::new("src/api.rs"),
        rs_code,
        &rs_res.symbols,
        &rs_symbol_index,
    );
    let rs_route_edge = rs_extraction
        .edges
        .iter()
        .find(|e| e.edge_type == "handles" && e.source == "/api/users" && e.target == "list_users");
    assert!(
        rs_route_edge.is_some(),
        "Expected Axum /api/users -[:handles]-> list_users, got edges: {:?}",
        rs_extraction.edges
    );
    assert_eq!(rs_route_edge.unwrap().provenance, EdgeProvenance::CodeHandlesRoute);

    // 2. TypeScript: Express route
    let ts_code = r#"
function getUser() {}
app.get("/users/:id", getUser);
"#;
    let ts_res =
        CodeChunker::parse_and_chunk(Path::new("src/routes.ts"), ts_code, &config).unwrap();
    assert!(
        ts_res
            .symbols
            .iter()
            .any(|s| s.symbol_type == CodeSymbolType::Route && s.name == "/users/:id"),
        "Expected Express route symbol '/users/:id', got: {:?}",
        ts_res.symbols
    );
    let ts_symbol_index = CodeGraphExtractor::build_symbol_index(&ts_res.symbols);
    let ts_extraction = CodeGraphExtractor::extract_edges_for_file_with_index(
        Path::new("src/routes.ts"),
        ts_code,
        &ts_res.symbols,
        &ts_symbol_index,
    );
    let ts_route_edge = ts_extraction
        .edges
        .iter()
        .find(|e| e.edge_type == "handles" && e.source == "/users/:id" && e.target == "getUser");
    assert!(
        ts_route_edge.is_some(),
        "Expected Express /users/:id -[:handles]-> getUser, got edges: {:?}",
        ts_extraction.edges
    );
    assert_eq!(ts_route_edge.unwrap().provenance, EdgeProvenance::CodeHandlesRoute);

    // 3. Python: FastAPI route
    let py_code = r#"
@app.get("/items")
def get_items():
    pass
"#;
    let py_res = CodeChunker::parse_and_chunk(Path::new("src/main.py"), py_code, &config).unwrap();
    assert!(
        py_res.symbols.iter().any(|s| s.symbol_type == CodeSymbolType::Route && s.name == "/items"),
        "Expected FastAPI route symbol '/items', got: {:?}",
        py_res.symbols
    );
    let py_symbol_index = CodeGraphExtractor::build_symbol_index(&py_res.symbols);
    let py_extraction = CodeGraphExtractor::extract_edges_for_file_with_index(
        Path::new("src/main.py"),
        py_code,
        &py_res.symbols,
        &py_symbol_index,
    );
    let py_route_edge = py_extraction
        .edges
        .iter()
        .find(|e| e.edge_type == "handles" && e.source == "/items" && e.target == "get_items");
    assert!(
        py_route_edge.is_some(),
        "Expected FastAPI /items -[:handles]-> get_items, got edges: {:?}",
        py_extraction.edges
    );
    assert_eq!(py_route_edge.unwrap().provenance, EdgeProvenance::CodeHandlesRoute);
}

#[test]
fn test_expanded_language_query_packs() {
    use groundcontrol_common::types::CodeSymbolType;
    let config = ChunkingConfig::default();

    // 1. Kotlin
    let kt_code = r#"
class UserService : BaseService {
    @Test
    fun testLogin() {}
}
"#;
    let kt_res =
        CodeChunker::parse_and_chunk(Path::new("src/UserService.kt"), kt_code, &config).unwrap();
    assert!(kt_res
        .symbols
        .iter()
        .any(|s| s.name == "UserService" && s.symbol_type == CodeSymbolType::Class));
    let kt_edges = CodeGraphExtractor::extract_edges_for_file(
        Path::new("src/UserService.kt"),
        kt_code,
        &kt_res.symbols,
        &kt_res.symbols,
    );
    assert!(
        kt_edges.iter().any(|e| e.edge_type == "inherits"
            && e.source == "UserService"
            && e.target == "BaseService"),
        "Expected Kotlin UserService -[:inherits]-> BaseService, got: {:?}",
        kt_edges
    );

    // 2. Scala
    let scala_code = r#"
class PaymentProcessor extends BaseProcessor {
    def process(): Unit = ()
}
"#;
    let scala_res =
        CodeChunker::parse_and_chunk(Path::new("src/PaymentProcessor.scala"), scala_code, &config)
            .unwrap();
    assert!(scala_res
        .symbols
        .iter()
        .any(|s| s.name == "PaymentProcessor" && s.symbol_type == CodeSymbolType::Class));
    let scala_edges = CodeGraphExtractor::extract_edges_for_file(
        Path::new("src/PaymentProcessor.scala"),
        scala_code,
        &scala_res.symbols,
        &scala_res.symbols,
    );
    assert!(
        scala_edges.iter().any(|e| e.edge_type == "inherits"
            && e.source == "PaymentProcessor"
            && e.target == "BaseProcessor"),
        "Expected Scala PaymentProcessor -[:inherits]-> BaseProcessor, got: {:?}",
        scala_edges
    );

    // 3. Swift
    let swift_code = r#"
class AuthManager: BaseManager {
    func authenticate() {}
}
"#;
    let swift_res =
        CodeChunker::parse_and_chunk(Path::new("src/AuthManager.swift"), swift_code, &config)
            .unwrap();
    assert!(swift_res
        .symbols
        .iter()
        .any(|s| s.name == "AuthManager" && s.symbol_type == CodeSymbolType::Class));
    let swift_edges = CodeGraphExtractor::extract_edges_for_file(
        Path::new("src/AuthManager.swift"),
        swift_code,
        &swift_res.symbols,
        &swift_res.symbols,
    );
    assert!(
        swift_edges.iter().any(|e| e.edge_type == "inherits"
            && e.source == "AuthManager"
            && e.target == "BaseManager"),
        "Expected Swift AuthManager -[:inherits]-> BaseManager, got: {:?}",
        swift_edges
    );

    // 4. Elixir
    let ex_code = r#"
defmodule AccountService do
    def get_account(id) do
        :ok
    end
end
"#;
    let ex_res =
        CodeChunker::parse_and_chunk(Path::new("lib/account_service.ex"), ex_code, &config)
            .unwrap();
    assert!(ex_res
        .symbols
        .iter()
        .any(|s| s.name == "AccountService" && s.symbol_type == CodeSymbolType::Module));
    assert!(ex_res
        .symbols
        .iter()
        .any(|s| s.name == "get_account" && s.symbol_type == CodeSymbolType::Function));

    // 5. Erlang
    let erl_code = "-module(order_server).\nprocess_order(Id) -> ok.\n";
    let erl_res =
        CodeChunker::parse_and_chunk(Path::new("src/order_server.erl"), erl_code, &config).unwrap();
    assert!(erl_res
        .symbols
        .iter()
        .any(|s| s.name == "order_server" && s.symbol_type == CodeSymbolType::Module));
    assert!(erl_res
        .symbols
        .iter()
        .any(|s| s.name == "process_order" && s.symbol_type == CodeSymbolType::Function));
}

#[test]
fn test_universal_import_resolution_rust_cross_file_disambiguation() {
    test_hybrid_lsp_receiver_method_disambiguation_rust();
}

#[test]
fn test_universal_import_resolution_typescript_disambiguation() {
    test_hybrid_lsp_receiver_method_disambiguation_typescript();
}

#[test]
fn test_universal_import_resolution_direct_imported_function() {
    let config = ChunkingConfig::default();

    let caller_code = r#"
import { calculateTax } from "./tax";

export function processOrder(order: any) {
    calculateTax(order);
}
"#;
    let target_tax_code = r#"
export function calculateTax(order: any) {
    return 0.1;
}
"#;
    let other_tax_code = r#"
export function calculateTax(order: any) {
    return 0.2;
}
"#;

    let caller_res =
        CodeChunker::parse_and_chunk(Path::new("src/order.ts"), caller_code, &config).unwrap();
    let target_res =
        CodeChunker::parse_and_chunk(Path::new("src/tax.ts"), target_tax_code, &config).unwrap();
    let other_res =
        CodeChunker::parse_and_chunk(Path::new("legacy/tax.ts"), other_tax_code, &config).unwrap();

    let mut all_symbols = caller_res.symbols.clone();
    all_symbols.extend(target_res.symbols.clone());
    all_symbols.extend(other_res.symbols.clone());

    let symbol_index = CodeGraphExtractor::build_symbol_index(&all_symbols);
    let extraction = CodeGraphExtractor::extract_edges_for_file_with_index(
        Path::new("src/order.ts"),
        caller_code,
        &caller_res.symbols,
        &symbol_index,
    );

    let call_edge = extraction
        .edges
        .iter()
        .find(|e| e.edge_type == "calls" && e.target == "calculateTax")
        .expect("expected call edge to calculateTax");
    assert_eq!(call_edge.confidence, Some(ResolutionConfidence::High));
}

#[test]
fn test_hybrid_lsp_receiver_method_disambiguation_python() {
    let config = ChunkingConfig::default();

    let caller_code = r#"
from .service import DataService

def run():
    client = DataService()
    client.fetch()
"#;
    let service_code = r#"
class DataService:
    def fetch(self):
        return []
"#;
    let legacy_code = r#"
class DataService:
    def fetch(self):
        return []
"#;

    let caller_res =
        CodeChunker::parse_and_chunk(Path::new("app/views.py"), caller_code, &config).unwrap();
    let service_res =
        CodeChunker::parse_and_chunk(Path::new("app/service.py"), service_code, &config).unwrap();
    let legacy_res =
        CodeChunker::parse_and_chunk(Path::new("legacy/service.py"), legacy_code, &config).unwrap();

    let mut all_symbols = caller_res.symbols.clone();
    all_symbols.extend(service_res.symbols.clone());
    all_symbols.extend(legacy_res.symbols.clone());

    let symbol_index = CodeGraphExtractor::build_symbol_index(&all_symbols);
    let extraction = CodeGraphExtractor::extract_edges_for_file_with_index(
        Path::new("app/views.py"),
        caller_code,
        &caller_res.symbols,
        &symbol_index,
    );

    let call_edge = extraction
        .edges
        .iter()
        .find(|e| e.edge_type == "calls" && e.target == "DataService > fetch")
        .expect("expected call edge to DataService > fetch");
    assert_eq!(call_edge.confidence, Some(ResolutionConfidence::High));
}

#[test]
fn test_universal_import_resolution_python_relative() {
    test_hybrid_lsp_receiver_method_disambiguation_python();
}

#[test]
fn test_hybrid_lsp_receiver_method_disambiguation_go() {
    let config = ChunkingConfig::default();

    let caller_code = r#"
package main

import "./search"

func Execute(client *search.SearchClient) {
    client.Query()
}
"#;
    let target_search_code = r#"
package search

type SearchClient struct{}

func (s *SearchClient) Query() {}
"#;
    let legacy_search_code = r#"
package search

type SearchClient struct{}

func (s *SearchClient) Query() {}
"#;

    let caller_res =
        CodeChunker::parse_and_chunk(Path::new("src/caller.go"), caller_code, &config).unwrap();
    let target_res = CodeChunker::parse_and_chunk(
        Path::new("src/search/client.go"),
        target_search_code,
        &config,
    )
    .unwrap();
    let legacy_res = CodeChunker::parse_and_chunk(
        Path::new("legacy/search/client.go"),
        legacy_search_code,
        &config,
    )
    .unwrap();

    let mut all_symbols = caller_res.symbols.clone();
    all_symbols.extend(target_res.symbols.clone());
    all_symbols.extend(legacy_res.symbols.clone());

    let symbol_index = CodeGraphExtractor::build_symbol_index(&all_symbols);
    let extraction = CodeGraphExtractor::extract_edges_for_file_with_index(
        Path::new("src/caller.go"),
        caller_code,
        &caller_res.symbols,
        &symbol_index,
    );

    let call_edge = extraction
        .edges
        .iter()
        .find(|e| e.edge_type == "calls" && e.target == "SearchClient > Query")
        .expect("expected call edge to SearchClient > Query");
    assert_eq!(call_edge.confidence, Some(ResolutionConfidence::High));
}

#[test]
fn test_hybrid_lsp_receiver_method_disambiguation_java() {
    let config = ChunkingConfig::default();

    let caller_code = r#"
package com.example;

import com.example.service.SearchService;

public class Controller {
    public void handleRequest() {
        SearchService service = new SearchService();
        service.execute();
    }
}
"#;
    let target_service_code = r#"
package com.example.service;

public class SearchService {
    public void execute() {}
}
"#;
    let legacy_service_code = r#"
package legacy.service;

public class SearchService {
    public void execute() {}
}
"#;

    let caller_res = CodeChunker::parse_and_chunk(
        Path::new("src/com/example/Controller.java"),
        caller_code,
        &config,
    )
    .unwrap();
    let target_res = CodeChunker::parse_and_chunk(
        Path::new("src/com/example/service/SearchService.java"),
        target_service_code,
        &config,
    )
    .unwrap();
    let legacy_res = CodeChunker::parse_and_chunk(
        Path::new("legacy/service/SearchService.java"),
        legacy_service_code,
        &config,
    )
    .unwrap();

    let mut all_symbols = caller_res.symbols.clone();
    all_symbols.extend(target_res.symbols.clone());
    all_symbols.extend(legacy_res.symbols.clone());

    let symbol_index = CodeGraphExtractor::build_symbol_index(&all_symbols);
    let extraction = CodeGraphExtractor::extract_edges_for_file_with_index(
        Path::new("src/com/example/Controller.java"),
        caller_code,
        &caller_res.symbols,
        &symbol_index,
    );

    let call_edge = extraction
        .edges
        .iter()
        .find(|e| e.edge_type == "calls" && e.target == "SearchService > execute")
        .expect("expected call edge to SearchService > execute");
    assert_eq!(call_edge.confidence, Some(ResolutionConfidence::High));
}
