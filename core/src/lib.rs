//! Quota Deck core: types, incremental log reading, watching and persistence.
//!
//! Hard constraints enforced by this crate (see `CLAUDE.md`):
//!
//! - No HTTP client dependency and no listening socket. Nothing here talks to a network.
//! - Log files are opened read-only and never written to.
//! - Log files are never read from byte zero on a warm start; reads resume from a stored cursor.

pub mod cursor;
pub mod discovery;
pub mod engine;
pub mod error;
pub mod events;
pub mod history;
pub mod horizon;
pub mod pace;
pub mod paths;
pub mod plan;
pub mod pricing;
pub mod provider;
pub mod reader;
pub mod store;
pub mod tail;
pub mod types;
pub mod watcher;

pub use error::{Error, Result};
