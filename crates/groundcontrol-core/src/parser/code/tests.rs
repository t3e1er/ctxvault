//! Code chunking and AST parsing unit tests.

use std::path::Path;

use groundcontrol_common::config::ChunkingConfig;
use groundcontrol_common::types::{ChunkEmbedPolicy, CodeSymbolType};

use crate::parser::code::chunker::classify_embed_policy;
use crate::parser::code::{CodeChunker, SupportedLanguage};

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

#[test]
fn test_classify_embed_policy() {
    // Containers are always anchors (except impl blocks)
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Struct,
            "struct Internal;",
            SupportedLanguage::Rust,
            "src/lib.rs"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Class,
            "class Secret {}",
            SupportedLanguage::TypeScript,
            "src/lib.ts"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Trait,
            "trait Handler {}",
            SupportedLanguage::Rust,
            "src/lib.rs"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Interface,
            "interface Api {}",
            SupportedLanguage::TypeScript,
            "src/lib.ts"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Enum,
            "enum Status {}",
            SupportedLanguage::Rust,
            "src/lib.rs"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Module,
            "pub mod internal {}",
            SupportedLanguage::Rust,
            "src/lib.rs"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Module,
            "impl<T> Handler for Service<T> {}",
            SupportedLanguage::Rust,
            "src/lib.rs"
        ),
        ChunkEmbedPolicy::GraphOnly
    );

    // Test files are always GraphOnly
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Struct,
            "pub struct TestFixture {}",
            SupportedLanguage::Rust,
            "src/tests.rs"
        ),
        ChunkEmbedPolicy::GraphOnly
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Function,
            "pub fn test_helper() {}",
            SupportedLanguage::Rust,
            "tests/integration.rs"
        ),
        ChunkEmbedPolicy::GraphOnly
    );

    // Rust functions/methods
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Function,
            "pub fn exported() {}",
            SupportedLanguage::Rust,
            "src/lib.rs"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Function,
            "pub(crate) fn crate_visible() {}",
            SupportedLanguage::Rust,
            "src/lib.rs"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Function,
            "#[inline]\npub fn with_attr() {}",
            SupportedLanguage::Rust,
            "src/lib.rs"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Function,
            "fn private_helper() {}",
            SupportedLanguage::Rust,
            "src/lib.rs"
        ),
        ChunkEmbedPolicy::GraphOnly
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Method,
            "fn private_method(&self) {}",
            SupportedLanguage::Rust,
            "src/lib.rs"
        ),
        ChunkEmbedPolicy::GraphOnly
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Function,
            "#[test]\npub fn my_test() {}",
            SupportedLanguage::Rust,
            "src/lib.rs"
        ),
        ChunkEmbedPolicy::GraphOnly
    );

    // Go functions/methods
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Function,
            "func ExportedFunction() {}",
            SupportedLanguage::Go,
            "server.go"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Function,
            "func internalHelper() {}",
            SupportedLanguage::Go,
            "server.go"
        ),
        ChunkEmbedPolicy::GraphOnly
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Method,
            "func (s *Server) ListenAndServe() error {}",
            SupportedLanguage::Go,
            "server.go"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Method,
            "func (s *Server) cleanup() {}",
            SupportedLanguage::Go,
            "server.go"
        ),
        ChunkEmbedPolicy::GraphOnly
    );

    // TypeScript / JavaScript
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Function,
            "export function search() {}",
            SupportedLanguage::TypeScript,
            "src/index.ts"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Function,
            "function localHelper() {}",
            SupportedLanguage::TypeScript,
            "src/index.ts"
        ),
        ChunkEmbedPolicy::GraphOnly
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Function,
            "export default function handler() {}",
            SupportedLanguage::JavaScript,
            "src/index.js"
        ),
        ChunkEmbedPolicy::Anchor
    );

    // Python
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Function,
            "def public_endpoint(): pass",
            SupportedLanguage::Python,
            "api.py"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Function,
            "def _private_worker(): pass",
            SupportedLanguage::Python,
            "api.py"
        ),
        ChunkEmbedPolicy::GraphOnly
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Function,
            "async def fetch_data(): pass",
            SupportedLanguage::Python,
            "api.py"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Function,
            "async def _internal(): pass",
            SupportedLanguage::Python,
            "api.py"
        ),
        ChunkEmbedPolicy::GraphOnly
    );

    // Java / C#
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Method,
            "public void handleRequest() {}",
            SupportedLanguage::Java,
            "Server.java"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Method,
            "private void calculate() {}",
            SupportedLanguage::Java,
            "Server.java"
        ),
        ChunkEmbedPolicy::GraphOnly
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Method,
            "public async Task Execute() {}",
            SupportedLanguage::CSharp,
            "Worker.cs"
        ),
        ChunkEmbedPolicy::Anchor
    );
    assert_eq!(
        classify_embed_policy(
            CodeSymbolType::Method,
            "internal void Init() {}",
            SupportedLanguage::CSharp,
            "Worker.cs"
        ),
        ChunkEmbedPolicy::GraphOnly
    );
}

#[test]
fn test_tier1_languages_chunking_and_symbols() {
    let config = ChunkingConfig::default();

    // 1. CUDA
    let cuda_code = "__global__ void my_kernel(int *a) { *a = 1; }";
    let res = CodeChunker::parse_and_chunk(Path::new("kernel.cu"), cuda_code, &config).unwrap();
    assert!(res
        .symbols
        .iter()
        .any(|s| s.name == "my_kernel" && s.symbol_type == CodeSymbolType::Function));

    // 2. Verilog
    let verilog_code = "module counter(input clk); endmodule";
    let res = CodeChunker::parse_and_chunk(Path::new("counter.v"), verilog_code, &config).unwrap();
    assert!(res
        .symbols
        .iter()
        .any(|s| s.name == "counter" && s.symbol_type == CodeSymbolType::Module));

    // 3. Starlark
    let starlark_code = "def helper(x):\n    return x\n";
    let res = CodeChunker::parse_and_chunk(Path::new("rules.bzl"), starlark_code, &config).unwrap();
    assert!(res
        .symbols
        .iter()
        .any(|s| s.name == "helper" && s.symbol_type == CodeSymbolType::Function));

    // 4. Bicep
    let bicep_code =
        "resource stg 'Microsoft.Storage/storageAccounts@2021-04-01' = { name: 'mystg' }";
    let res = CodeChunker::parse_and_chunk(Path::new("main.bicep"), bicep_code, &config).unwrap();
    assert!(res.symbols.iter().any(|s| s.name == "stg" && s.symbol_type == CodeSymbolType::Struct));

    // 5. Gleam
    let gleam_code = "pub fn hello() { }\npub type Person { Person(name: String) }\n";
    let res = CodeChunker::parse_and_chunk(Path::new("main.gleam"), gleam_code, &config).unwrap();
    assert!(res
        .symbols
        .iter()
        .any(|s| s.name == "hello" && s.symbol_type == CodeSymbolType::Function));

    // 6. PowerShell
    let ps_code = "function Get-Data { param($x) }";
    let res = CodeChunker::parse_and_chunk(Path::new("script.ps1"), ps_code, &config).unwrap();
    assert!(res
        .symbols
        .iter()
        .any(|s| s.name == "Get-Data" && s.symbol_type == CodeSymbolType::Function));

    // 7. D
    let d_code = "class Greeter { void greet() {} }\nstruct Point { int x; }";
    let res = CodeChunker::parse_and_chunk(Path::new("app.d"), d_code, &config).unwrap();
    assert!(res
        .symbols
        .iter()
        .any(|s| s.name == "Greeter" && s.symbol_type == CodeSymbolType::Class));
    assert!(res
        .symbols
        .iter()
        .any(|s| s.name == "Point" && s.symbol_type == CodeSymbolType::Struct));

    // 8. WGSL
    let wgsl_code = "struct VertexInput { position: vec3<f32>, };\nfn vs_main() {}";
    let res = CodeChunker::parse_and_chunk(Path::new("shader.wgsl"), wgsl_code, &config).unwrap();
    assert!(res
        .symbols
        .iter()
        .any(|s| s.name == "VertexInput" && s.symbol_type == CodeSymbolType::Struct));
    assert!(res
        .symbols
        .iter()
        .any(|s| s.name == "vs_main" && s.symbol_type == CodeSymbolType::Function));

    // 9. HCL
    let hcl_code = "resource \"aws_s3_bucket\" \"b\" { bucket = \"my-tf-test-bucket\" }";
    let res = CodeChunker::parse_and_chunk(Path::new("main.tf"), hcl_code, &config).unwrap();
    assert!(!res.chunks.is_empty());

    // 10. Nix
    let nix_code = "{ pkgs ? import <nixpkgs> {} }: let f = x: x; in f";
    let res = CodeChunker::parse_and_chunk(Path::new("default.nix"), nix_code, &config).unwrap();
    assert!(!res.chunks.is_empty());

    // 11. TLA+
    let tla_code = "---- MODULE Test ----\nEXTENDS Naturals\nVARIABLE x\nInit == x = 0\n====";
    let res = CodeChunker::parse_and_chunk(Path::new("spec.tla"), tla_code, &config).unwrap();
    assert!(!res.chunks.is_empty());
}

#[test]
fn test_hcl_resource_extraction() {
    let config = ChunkingConfig::default();
    let hcl_code = "resource \"aws_s3_bucket\" \"b\" { bucket = \"my-tf-test-bucket\" }";
    let res = CodeChunker::parse_and_chunk(Path::new("main.tf"), hcl_code, &config).unwrap();
    assert!(
        res.symbols
            .iter()
            .any(|s| s.name == "aws_s3_bucket.b" && s.symbol_type == CodeSymbolType::Struct),
        "HCL resource block must yield Struct symbol named 'aws_s3_bucket.b', got: {:?}",
        res.symbols
    );
    assert!(
        res.symbols.iter().any(|s| s.scope_path == "aws_s3_bucket.b"),
        "HCL resource block must have scope_path 'aws_s3_bucket.b', got: {:?}",
        res.symbols
    );
}
