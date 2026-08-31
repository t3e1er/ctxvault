//! MCP protocol layer: tool registration, transport, request handling.
//!
//! This crate translates MCP JSON-RPC tool calls into `ctxvault-core` operations
//! and formats responses. It contains no domain logic.

pub mod client;
pub mod tools;
pub mod transport;
