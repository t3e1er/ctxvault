//! Shared types, traits, and error definitions for the cxtvault engine.
//!
//! This crate contains no business logic — only the contracts that other crates
//! depend on. Keep it lean: adding a dependency here forces it on every consumer.

pub mod config;
pub mod error;
pub mod types;

pub use error::{Error, Result};
