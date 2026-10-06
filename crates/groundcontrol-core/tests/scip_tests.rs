//! Integration tests for In-Process SCIP Synthesis, Manifest Scoper, and Passive Auto-Detection.

use std::fs;
use std::path::Path;

use groundcontrol_common::config::CorpusConfig;
use groundcontrol_common::traits::GraphStore;
use groundcontrol_common::types::CodeSymbolType;
use groundcontrol_core::corpus_manager::CorpusManager;
use groundcontrol_core::graph::hybrid_lsp::TypeEnvironment;
use groundcontrol_core::graph::scip::{looks_like_moniker, moniker_leaf, synthesize_moniker};
use groundcontrol_core::parser::code::manifest::{
    find_enclosing_manifest, parse_cargo_toml, parse_go_mod, parse_package_json, parse_pom_xml,
    parse_pyproject_toml, PackageManifest,
};
use groundcontrol_core::parser::code::CodeChunker;
use protobuf::{Enum, Message};
use tempfile::tempdir;

#[test]
fn test_manifest_parsers_scip_schemes() {
    let dir = Path::new("/test/repo");

    // Cargo.toml
    let cargo_toml = r#"
[package]
name = "orderservice"
version = "1.2.3"
"#;
    let m = parse_cargo_toml(cargo_toml, dir).unwrap();
    assert_eq!(m.scheme, "scip-rust");
    assert_eq!(m.manager, "cargo");
    assert_eq!(m.package_name, "orderservice");
    assert_eq!(m.version, "1.2.3");

    // go.mod
    let go_mod = r#"
module github.com/open-telemetry/opentelemetry-demo/src/checkoutservice

go 1.22
"#;
    let m = parse_go_mod(go_mod, dir).unwrap();
    assert_eq!(m.scheme, "scip-go");
    assert_eq!(m.manager, "gomod");
    assert_eq!(m.package_name, "github.com/open-telemetry/opentelemetry-demo/src/checkoutservice");

    // package.json
    let pkg_json = r#"
{
  "name": "@opentelemetry/frontend",
  "version": "2.0.0"
}
"#;
    let m = parse_package_json(pkg_json, dir).unwrap();
    assert_eq!(m.scheme, "scip-typescript");
    assert_eq!(m.manager, "npm");
    assert_eq!(m.package_name, "@opentelemetry/frontend");
    assert_eq!(m.version, "2.0.0");

    // pom.xml
    let pom_xml = r#"
<project>
    <groupId>com.oteldemo</groupId>
    <artifactId>adservice</artifactId>
    <version>3.1.4</version>
</project>
"#;
    let m = parse_pom_xml(pom_xml, dir).unwrap();
    assert_eq!(m.scheme, "scip-java");
    assert_eq!(m.manager, "maven");
    assert_eq!(m.package_name, "com.oteldemo.adservice");
    assert_eq!(m.version, "3.1.4");

    // pyproject.toml
    let pyproject = r#"
[project]
name = "paymentservice"
version = "0.5.0"
"#;
    let m = parse_pyproject_toml(pyproject, dir).unwrap();
    assert_eq!(m.scheme, "scip-python");
    assert_eq!(m.manager, "pip");
    assert_eq!(m.package_name, "paymentservice");
    assert_eq!(m.version, "0.5.0");
}

#[test]
fn test_find_enclosing_manifest_subdirectories() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let sub = root.join("services").join("cart");
    fs::create_dir_all(&sub).unwrap();

    let go_mod = sub.join("go.mod");
    fs::write(&go_mod, "module github.com/demo/cart\n\ngo 1.22\n").unwrap();

    let nested_file = sub.join("src").join("cart.go");
    fs::create_dir_all(nested_file.parent().unwrap()).unwrap();
    fs::write(&nested_file, "package main\n").unwrap();

    let manifest = find_enclosing_manifest(&nested_file, Some(root)).unwrap();
    assert_eq!(manifest.scheme, "scip-go");
    assert_eq!(manifest.package_name, "github.com/demo/cart");
}

#[test]
fn test_synthesize_scip_monikers_and_leaf_reconciliation() {
    let manifest = PackageManifest::new(
        "scip-go",
        "gomod",
        "github.com/open-telemetry/opentelemetry-demo/src/checkoutservice",
        "1.0.0",
        "/repo",
    );

    // Method inside struct
    let m1 = synthesize_moniker(
        Some(&manifest),
        "checkout.go",
        Some("CheckoutService"),
        "PlaceOrder",
        CodeSymbolType::Method,
    );
    assert_eq!(
        m1,
        "scip-go gomod github.com/open-telemetry/opentelemetry-demo/src/checkoutservice 1.0.0 CheckoutService#PlaceOrder()."
    );
    assert!(looks_like_moniker(&m1));
    assert_eq!(moniker_leaf(&m1), Some("PlaceOrder".to_string()));

    // Struct type
    let m2 = synthesize_moniker(
        Some(&manifest),
        "checkout.go",
        None,
        "CheckoutService",
        CodeSymbolType::Struct,
    );
    assert_eq!(
        m2,
        "scip-go gomod github.com/open-telemetry/opentelemetry-demo/src/checkoutservice 1.0.0 CheckoutService#"
    );
    assert!(looks_like_moniker(&m2));
    assert_eq!(moniker_leaf(&m2), Some("CheckoutService".to_string()));
}

#[test]
fn test_type_environment_moniker_binding() {
    let manifest =
        PackageManifest::new("scip-rust", "cargo", "groundcontrol-core", "0.2.2", "/repo");

    let mut env = TypeEnvironment::new().with_manifest(Some(manifest));
    env.register_variable("client".to_string(), "SearchClient".to_string());

    let receiver_moniker = env.resolve_receiver_moniker("client", "query", "src/search.rs");
    assert_eq!(
        receiver_moniker,
        Some("scip-rust cargo groundcontrol-core 0.2.2 SearchClient#query().".to_string())
    );
}

#[test]
fn test_chunker_populates_canonical_name() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let cargo_toml = root.join("Cargo.toml");
    fs::write(&cargo_toml, "[package]\nname = \"demo-crate\"\nversion = \"0.1.0\"\n").unwrap();

    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();
    let lib_rs = src.join("lib.rs");
    let content = r#"
pub struct Engine;

impl Engine {
    pub fn search(&self) {}
}
"#;
    fs::write(&lib_rs, content).unwrap();

    let config = groundcontrol_common::config::ChunkingConfig::default();
    let result = CodeChunker::parse_and_chunk(&lib_rs, content, &config).unwrap();

    assert!(!result.symbols.is_empty());
    let engine_sym = result.symbols.iter().find(|s| s.name == "Engine").unwrap();
    assert!(engine_sym.canonical_name.is_some());
    let moniker = engine_sym.canonical_name.as_ref().unwrap();
    assert!(moniker.contains("demo-crate"));
    assert!(moniker.contains("Engine#"));

    let search_sym = result.symbols.iter().find(|s| s.name == "search").unwrap();
    assert!(search_sym.canonical_name.is_some());
    let search_moniker = search_sym.canonical_name.as_ref().unwrap();
    assert!(search_moniker.contains("Engine#search()."));
}

#[test]
fn test_passive_scip_auto_detection() {
    let tmp = tempdir().unwrap();
    let corpus_dir = tmp.path().join("my-corpus");
    let index_dir = tmp.path().join("index");
    fs::create_dir_all(&corpus_dir).unwrap();
    fs::create_dir_all(&index_dir).unwrap();

    // Create a minimal binary SCIP index file
    let scip_path = corpus_dir.join("index.scip");
    let mut index = scip::types::Index::new();
    let mut doc = scip::types::Document::new();
    doc.relative_path = "src/main.rs".to_string();
    let mut occ = scip::types::Occurrence::new();
    occ.symbol = "scip-rust cargo test-pkg 1.0.0 main().".to_string();
    occ.symbol_roles = scip::types::SymbolRole::Definition.value();
    occ.range = vec![1, 0, 5, 1];
    doc.occurrences.push(occ);
    index.documents.push(doc);

    let mut buf = Vec::new();
    index.write_to_vec(&mut buf).unwrap();
    fs::write(&scip_path, buf).unwrap();

    let config = CorpusConfig {
        name: "test-scip".to_string(),
        path: corpus_dir.to_string_lossy().to_string(),
        ..Default::default()
    };

    let mut manager = CorpusManager::new();
    manager.add_corpus_with_index_dir(config, &index_dir).unwrap();

    let engine = manager.get_engine("test-scip").unwrap();
    let nodes = engine.graph().node_paths();
    assert!(
        nodes.contains(&"scip-rust cargo test-pkg 1.0.0 main().".to_string()),
        "Engine graph should have passively ingested nodes from index.scip: {:?}",
        nodes
    );
}
