//! Domain layer for Throne-rs.
//!
//! Mirrors the conceptual model of the original Qt Throne client
//! (profiles, groups, routes, runtime status) without Qt dependencies.

mod auto_selector;
mod local_network;
mod models;
mod route_simple;
mod store;
mod version;

pub use auto_selector::{
    classify_custom_member, is_xray_full_config_member, plan_auto_selector, profile_auto_selector,
    rerank_auto_selector_pool, AutoSelectorConfig, AutoSelectorPlan, AutoSelectorSkip,
    CustomMemberKind,
};
pub use local_network::{
    endpoint_host, is_own_address, lan_address, lan_inbound_enabled, lan_inbound_is_wildcard,
};
pub use models::*;
pub use route_simple::SimpleAction;
pub use store::{
    AppState, ProfileSortColumn, StoreError, SubUpdateSummary, SubscriptionChange,
    SubscriptionUpdateReport,
};
pub use version::{display_name, user_agent, NKR_VERSION};
