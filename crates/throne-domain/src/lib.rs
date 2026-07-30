//! Domain layer for Throne-rs.
//!
//! Mirrors the conceptual model of the original Qt Throne client
//! (profiles, groups, routes, runtime status) without Qt dependencies.

mod models;
mod store;

pub use models::*;
pub use store::{AppState, StoreError};
