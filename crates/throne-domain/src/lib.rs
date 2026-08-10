//! Domain layer for Throne-rs.
//!
//! Mirrors the conceptual model of the original Qt Throne client
//! (profiles, groups, routes, runtime status) without Qt dependencies.

mod auto_selector;
mod models;
mod route_simple;
mod store;
mod version;

pub use auto_selector::{
    AutoSelectorConfig, AutoSelectorPlan, AutoSelectorSkip, CustomMemberKind,
    classify_custom_member, is_xray_full_config_member, plan_auto_selector,
    profile_auto_selector, rerank_auto_selector_pool,
};
pub use models::*;
pub use route_simple::SimpleAction;
pub use store::{
    AppState, ProfileSortColumn, StoreError, SubUpdateSummary, SubscriptionChange,
    SubscriptionUpdateReport,
};
pub use version::{NKR_VERSION, display_name, user_agent};
