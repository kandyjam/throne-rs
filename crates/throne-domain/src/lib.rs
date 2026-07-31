//! Domain layer for Throne-rs.
//!
//! Mirrors the conceptual model of the original Qt Throne client
//! (profiles, groups, routes, runtime status) without Qt dependencies.

mod models;
mod route_simple;
mod store;
mod version;

pub use models::*;
pub use route_simple::SimpleAction;
pub use store::{AppState, ProfileSortColumn, StoreError, SubUpdateSummary};
pub use version::{NKR_VERSION, display_name, user_agent};
