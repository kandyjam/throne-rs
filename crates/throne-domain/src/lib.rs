//! Domain layer for Throne-rs.
//!
//! Mirrors the conceptual model of the original Qt Throne client
//! (profiles, groups, routes, runtime status) without Qt dependencies.

mod auto_selector;
mod endpoint_source;
mod ip_list;
mod ip_scan;
mod local_network;
mod models;
mod route_simple;
mod security;
mod store;
mod version;

pub use auto_selector::{
    classify_custom_member, is_xray_full_config_member, plan_auto_selector, profile_auto_selector,
    rerank_auto_selector_pool, AutoSelectorConfig, AutoSelectorPlan, AutoSelectorSkip,
    CustomMemberKind,
};
pub use endpoint_source::{
    effective_endpoint_source, endpoint_override_blocker, materialize_profile_endpoint,
    resolve_endpoint_source, EndpointError, EndpointSource,
};
pub use ip_list::{
    normalize_ip_cidr, parse_ip_list_text, IpList, IpListEntry, IpListParseResult, IpListRole,
    IpListSourceKind,
};
pub use ip_scan::*;
pub use local_network::{
    connection_route_rule, endpoint_host, is_own_address, lan_address, lan_inbound_enabled,
    lan_inbound_is_wildcard,
};
pub use models::*;
pub use route_simple::{connection_route_targets, ConnectionRouteTarget, SimpleAction};
pub use security::{is_private_host, profile_security, SecurityInfo, SecurityLevel};
pub use store::{
    AppState, ProfileSortColumn, StoreError, SubUpdateSummary, SubscriptionChange,
    SubscriptionUpdateReport,
};
pub use version::{display_name, user_agent, NKR_VERSION};
