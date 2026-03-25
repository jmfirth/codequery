//! Core types and project utilities for codequery.
//!
//! This crate defines the shared data types — symbols, locations, errors — that
//! all other codequery crates depend on. It sits at the bottom of the dependency
//! graph and owns no parsing or output logic.

#![warn(clippy::pedantic)]

pub mod error;
pub mod symbol;

pub use error::{CoreError, Result};
pub use symbol::{Location, Symbol, SymbolKind, Visibility};
