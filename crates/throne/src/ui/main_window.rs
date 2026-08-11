//! Main window layout mirrored from upstream `mainwindow.ui`.
//!
//! ```text
//! [Program][Settings][Groups][Routing][Tools] [▶Start] [Tun][DNS][Proxy] | data_view
//! ─────────────────────────────────────────────────────────────────────
//! Group tabs …
//! ┌ Type │ Address │ Name │ Test Result │ Traffic ──────────────────┐
//! │ …                                                               │
//! └─────────────────────────────────────────────────────────────────┘
//! [Logs] [Connections] [Traffic Graph]
//! running | inbound | speed | version
//! ```
//!
//! Toolbar menus are **relative under each button** (no absolute left offsets).
//! Secondary features open as modal dialogs: Basic Settings / Manage Groups / Add from input.

use std::collections::VecDeque;
use std::ops::Range;
use std::sync::{Arc, Mutex};

use gpui::{
    App, Bounds, ClipboardItem, Context, EntityInputHandler, FocusHandle, Focusable, KeyDownEvent,
    Pixels, ScrollHandle, SharedString, Subscription, UTF16Selection, Window, actions, div,
    prelude::*, px, uniform_list,
};

use throne_core_client::{
    AutoSelectorGroupStatus, ConnectionRow, CoreConfig, CoreSession, force_clear_system_proxy,
    set_system_proxy,
};
use throne_domain::{
    AppState, CoreStatus, GroupId, Profile, ProfileId, ProfileSortColumn, ProfileType,
    TrafficSnapshot,
};
use throne_import::{FetchOptions, fetch_url_with_options, import_subscription_response};

use crate::theme::{self, Theme, latency_color};
use crate::ui::dialog_inputs::{DialogInputs, NestedInputs};
use crate::ui::dialogs::{
    AutoSelectorGroupView, AutoSelectorMemberRow, Dialog, EditGroupView, HotkeyField,
    add_input_body, auto_selector_stats_body, basic_settings_body, edit_group_body,
    edit_profile_body, hotkey_settings_body, manage_groups_body, subscription_diff_body,
    traffic_stats_body, tun_settings_body,
};
use crate::ui::routing::{
    RouteEditorTab, RoutingEvent, RoutingNested, RoutingSideEffect, routing_nested_title_owned,
    routing_nested_view, routing_nested_width, routing_settings_view,
};
use crate::ui::speed_graph::{SpeedGraph, speed_graph_element};
use crate::ui::widgets::{
    TOOLBAR_BTN_GAP, TOOLBAR_BTN_W, TOOLBAR_MENU_TOP, TOOLBAR_PAD_X, icon_btn, menu_item,
    menu_item_checked, menu_label, menu_panel, menu_separator, mode_switch, start_stop_btn,
    status_tag, tab_bar, toolbar_btn, StartStopState, ToolbarIcon,
};
use gpui_component::{
    ActiveTheme as _, WindowExt as _,
    button::ButtonVariant,
    dialog::{Dialog as GpuiDialog, DialogButtonProps},
};

actions!(
    throne,
    [
        ToggleProxy,
        ImportClipboard,
        SaveDb,
        SelectAll,
        DeleteSelected,
        UrlTestSelected,
        UrlTestGroup,
        DeleteUnavailable,
        CycleRoute,
        CopyLogs,
        Quit
    ]
);

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum OpenMenu {
    #[default]
    None,
    Program,
    Settings,
    Groups,
    Routing,
    Tools,
    ProfileCtx,
    /// Right-click on a group tab (or empty tab bar area).
    GroupTabCtx,
}

/// Profile table sort column (click header to toggle).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum SortColumn {
    #[default]
    None,
    Type,
    Address,
    Name,
    TestResult,
    Traffic,
}

fn next_sort_state(
    current: SortColumn,
    ascending: bool,
    clicked: SortColumn,
) -> (SortColumn, bool) {
    if current == clicked {
        (clicked, !ascending)
    } else {
        (clicked, true)
    }
}

fn domain_sort_column(column: SortColumn) -> Option<ProfileSortColumn> {
    match column {
        SortColumn::None => None,
        SortColumn::Type => Some(ProfileSortColumn::Type),
        SortColumn::Address => Some(ProfileSortColumn::Address),
        SortColumn::Name => Some(ProfileSortColumn::Name),
        SortColumn::TestResult => Some(ProfileSortColumn::TestResult),
        SortColumn::Traffic => Some(ProfileSortColumn::Traffic),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CoreAction {
    Start,
    Stop,
    Switch(ProfileId),
}

fn next_core_action(status: &CoreStatus, profile_id: ProfileId) -> CoreAction {
    match status {
        // Clicking another node while running/starting always means "switch to that node".
        CoreStatus::Running {
            profile_id: running_id,
            ..
        } if *running_id != profile_id => CoreAction::Switch(profile_id),
        CoreStatus::Starting => CoreAction::Switch(profile_id),
        CoreStatus::Running { .. } => CoreAction::Stop,
        CoreStatus::Stopped | CoreStatus::Stopping | CoreStatus::Error(_) => CoreAction::Start,
    }
}

const FAILED_STOP_PROFILE_LOG: &str =
    "<<<<<<<< Failed to stop, please restart the program.";

fn runtime_profile_display(profile_type: ProfileType, profile_name: &str) -> String {
    format!("[{}] {profile_name}", profile_type.display_name())
}

fn auto_selector_group_view(g: AutoSelectorGroupStatus) -> AutoSelectorGroupView {
    AutoSelectorGroupView {
        tag: g.tag,
        phase: g.phase,
        selected: g.selected,
        pinned: g.pinned,
        balance: g.balance,
        balance_mode: g.balance_mode,
        suspended: g.suspended,
        members_total: g.members_total,
        members_alive: g.members_alive,
        members_qualified: g.members_qualified,
        last_switch_reason: g.last_switch_reason,
        members: g
            .members
            .into_iter()
            .map(|m| AutoSelectorMemberRow {
                // Core tags are `p{id}`; display_name filled by enrich if known.
                display_name: m.tag.clone(),
                tag: m.tag,
                rank: m.rank,
                state: m.state,
                selected: m.selected,
                qualified: m.qualified,
                active: m.active,
                average_ms: m.average_ms,
                failures: m.failures,
                last_error: m.last_error,
            })
            .collect(),
    }
}

/// Map core `p{id}` tags to profile display names when the id is still in the DB.
fn enrich_auto_selector_names(state: &AppState, groups: &mut [AutoSelectorGroupView]) {
    for g in groups {
        for m in &mut g.members {
            if let Some(id) = m.tag.strip_prefix('p').and_then(|s| s.parse::<i64>().ok()) {
                if let Some(p) = state.profile(id) {
                    m.display_name = p.name.clone();
                }
            }
        }
    }
}

fn resolve_stop_profile_display(
    running_profile_display: Option<&String>,
    current_profile_display: Option<String>,
) -> Option<String> {
    running_profile_display.cloned().or(current_profile_display)
}

fn start_profile_log(profile_display: &str) -> String {
    format!(">>>>>>>> Starting profile {profile_display}")
}

fn stop_profile_log(profile_display: &str) -> String {
    format!(">>>>>>>> Stopping profile {profile_display}")
}

fn failed_start_profile_log(profile_display: &str) -> String {
    format!("<<<<<<<< Failed to start profile {profile_display}")
}

fn running_mode_marker(tun_enabled: bool, system_proxy_enabled: bool) -> &'static str {
    match (tun_enabled, system_proxy_enabled) {
        (true, true) => "[Tun+System Proxy]",
        (true, false) => "[Tun]",
        (false, true) => "[System Proxy]",
        (false, false) => "",
    }
}

fn should_scroll_logs_to_bottom(logs_tab_active: bool, previous: &str, current: &str) -> bool {
    logs_tab_active && !current.is_empty() && previous != current
}

fn should_update_rendered_log_text(logs_tab_active: bool) -> bool {
    logs_tab_active
}

fn runtime_poll_health(current_failures: u8, succeeded: bool) -> (u8, bool) {
    if succeeded {
        (0, false)
    } else {
        let failures = current_failures.saturating_add(1);
        (failures, failures >= 3)
    }
}

fn next_runtime_generation(current: u64) -> (u64, u8) {
    (current.wrapping_add(1), 0)
}

/// Upstream `DataViewHtmlGenerator` latency/speedtest progress kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestProgressKind {
    Url,
}

/// Top-right panel state while a group URL/IP test runs (upstream `data_view`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TestProgressPanel {
    kind: TestProgressKind,
    done: usize,
    total: usize,
}

fn test_progress_percent(done: usize, total: usize) -> usize {
    if total == 0 {
        0
    } else {
        100 * done / total
    }
}

/// Upstream `DataViewHtmlGenerator::getProgressBar` — 10-char `#`/`-` bar (tests only).
#[cfg(test)]
fn test_progress_bar(done: usize, total: usize) -> String {
    let filled = if total > 0 { 10 * done / total } else { 0 };
    let mut out = String::with_capacity(10);
    for i in 0..10 {
        out.push(if i < filled { '#' } else { '-' });
    }
    out
}

/// Labels for the top-right test panel (mirrors upstream HTML center text).
#[cfg(test)]
fn test_progress_lines(panel: &TestProgressPanel) -> (Option<String>, String) {
    let verb = match panel.kind {
        TestProgressKind::Url => "Running URL test",
    };
    if panel.total > 1 {
        let bar = format!(
            "{} {}%",
            test_progress_bar(panel.done, panel.total),
            test_progress_percent(panel.done, panel.total)
        );
        let content = format!("{verb} ({} / {})", panel.done, panel.total);
        (Some(bar), content)
    } else {
        (None, verb.to_string())
    }
}

/// UI view model for the top-right test panel — percent drives gpui-component Progress.
fn test_progress_view(panel: &TestProgressPanel) -> (Option<f32>, String) {
    let verb = match panel.kind {
        TestProgressKind::Url => "Running URL test",
    };
    if panel.total > 1 {
        let pct = test_progress_percent(panel.done, panel.total) as f32;
        let content = format!("{verb} ({} / {})", panel.done, panel.total);
        (Some(pct), content)
    } else {
        (None, verb.to_string())
    }
}

fn runtime_poll_is_current(poll_generation: u64, current_generation: u64) -> bool {
    poll_generation == current_generation
}

fn should_queue_recovery_restart(network_recovery_busy: bool) -> bool {
    network_recovery_busy
}

/// Latest node the user wants after the current core op finishes.
///
/// Always keeps the **most recent** click (rapid switching must not stick on
/// the first target — that left the UI on "Starting…" while ignoring later nodes).
#[derive(Default)]
struct PendingProfileSwitch(Option<ProfileId>);

impl PendingProfileSwitch {
    /// Record `profile_id` as the desired target. Returns `true` when the caller
    /// should kick off stop→start (no prior target was queued).
    fn schedule(&mut self, profile_id: ProfileId) -> bool {
        let was_empty = self.0.is_none();
        self.0 = Some(profile_id);
        was_empty
    }

    fn set(&mut self, profile_id: ProfileId) {
        self.0 = Some(profile_id);
    }

    fn peek(&self) -> Option<ProfileId> {
        self.0
    }

    fn clear(&mut self) {
        self.0 = None;
    }

    fn take(&mut self) -> Option<ProfileId> {
        self.0.take()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UpdateOrigin {
    Manual,
    UpdateAll,
}

fn should_show_subscription_diff(origin: UpdateOrigin, enabled: bool) -> bool {
    enabled && origin == UpdateOrigin::Manual
}

fn eligible_subscription_ids<'a>(
    groups: impl IntoIterator<Item = &'a throne_domain::Group>,
) -> Vec<GroupId> {
    groups
        .into_iter()
        .filter(|group| !group.url.trim().is_empty() && !group.archive)
        .map(|group| group.id)
        .collect()
}

fn subscription_fetch_options(
    settings: &throne_domain::AppSettings,
    core_status: &CoreStatus,
) -> Result<FetchOptions, String> {
    if !settings.system_proxy_enabled {
        return Ok(FetchOptions::default());
    }
    if !core_status.is_running() {
        return Err("Request with proxy but no profile started.".into());
    }
    FetchOptions::with_http_proxy(&settings.inbound_address, settings.inbound_socks_port)
}

fn format_subscription_changes(report: &throne_domain::SubscriptionUpdateReport) -> String {
    if report.added.is_empty()
        && report.updated.is_empty()
        && report.deleted.is_empty()
        && report.kept_in_use.is_empty()
    {
        return "Nothing".into();
    }
    let entries = |prefix: &str, changes: &[throne_domain::SubscriptionChange]| {
        changes
            .iter()
            .map(|change| format!("{prefix} {}", change.display))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let mut text = format!(
        "Added {} profiles:\n{}\n\nUpdated {} profiles:\n{}\n\nDeleted {} profiles:\n{}",
        report.added.len(),
        entries("[+]", &report.added),
        report.updated.len(),
        entries("[~]", &report.updated),
        report.deleted.len(),
        entries("[-]", &report.deleted),
    );
    // Upstream 1.2.4: "Still in use, so kept instead of deleted"
    if !report.kept_in_use.is_empty() {
        text.push_str(&format!(
            "\n\nStill in use, so kept instead of deleted:\n{}",
            entries("[=]", &report.kept_in_use)
        ));
    }
    text
}

#[derive(Debug, Default)]
struct SubscriptionUpdateQueue {
    pending: VecDeque<GroupId>,
    succeeded: usize,
    failed: usize,
}

impl SubscriptionUpdateQueue {
    fn new(ids: Vec<GroupId>) -> Self {
        Self {
            pending: ids.into(),
            succeeded: 0,
            failed: 0,
        }
    }

    fn take_next(&mut self) -> Option<GroupId> {
        self.pending.pop_front()
    }

    fn record_result(&mut self, succeeded: bool) {
        if succeeded {
            self.succeeded += 1;
        } else {
            self.failed += 1;
        }
    }

    fn completion_message(&self) -> String {
        format!(
            "Subscription update finished · {} succeeded · {} failed",
            self.succeeded, self.failed
        )
    }

    #[cfg(test)]
    fn is_finished(&self) -> bool {
        self.pending.is_empty()
    }
}

pub struct MainWindow {
    state: AppState,
    focus_handle: FocusHandle,
    search_draft: String,
    db_path_label: String,
    open_menu: OpenMenu,
    /// Bottom panel tab: 0 Logs, 1 Connections, 2 Traffic Graph
    bottom_tab: usize,
    log_scroll_handle: ScrollHandle,
    rendered_log_text: String,
    ctx_menu_at: Option<(f32, f32)>,
    /// Target group for [`OpenMenu::GroupTabCtx`] (`None` = empty tab-bar area).
    ctx_group_id: Option<GroupId>,
    dialog: Dialog,
    /// Real InputState entities for the open dialog (text source of truth while open).
    dialog_inputs: Option<DialogInputs>,
    /// After async remote-fetch, push reloaded simple rules into NestedInputs on next paint.
    pending_nested_simple_sync: bool,
    /// Present / re-present gpui-component `open_dialog` on next paint (no Window at set time).
    pending_gpui_dialog: bool,
    /// Last presented dialog stack: (main kind id, nested kind). Avoids focus-resetting re-open.
    presented_dialog_stack: (u8, NestedKind),
    /// Go `ThroneCore` process + IPC (real Start/Stop).
    /// Behind a mutex so Start/Stop can run off the UI thread.
    core: Arc<Mutex<CoreSession>>,
    /// True while a start/stop background job is in flight (ignore re-clicks).
    core_op_busy: bool,
    /// OS proxy cleanup is running after repeated core health failures.
    network_recovery_busy: bool,
    /// When Tun/Proxy mode changes during a busy start/stop, restart once idle.
    restart_when_idle: bool,
    /// User asked to Stop while Start was still in flight — run stop once idle.
    pending_stop: bool,
    /// Display captured from the profile actually passed to the running core.
    running_profile_display: Option<String>,
    /// Target profile to start once the current core has fully stopped.
    pending_profile_switch: PendingProfileSwitch,
    /// True while a URL-test / sub-update job is in flight.
    background_busy: bool,
    /// Top-right progress while group URL/IP test runs (upstream `data_view`).
    test_progress: Option<TestProgressPanel>,
    subscription_queue: Option<SubscriptionUpdateQueue>,
    sort_column: SortColumn,
    /// `true` = ascending (A→Z, low latency first).
    sort_asc: bool,
    /// Live connections from core (Connections tab).
    connections: Vec<ConnectionRow>,
    /// Live Auto Selector snapshot while stats dialog is open (or last poll).
    auto_selector_snapshot: Vec<AutoSelectorGroupView>,
    prev_traffic_at: Option<std::time::Instant>,
    /// Live rate ring for the Traffic Graph tab (upstream SpeedWidget).
    speed_graph: SpeedGraph,
    /// Historical traffic stats (`throne_stats.db` + minute accumulator).
    traffic_stats_db: Option<std::sync::Arc<throne_storage::TrafficStatsDb>>,
    traffic_stats_mgr: throne_storage::TrafficStatsManager,
    /// Prevent an overdue core request from queuing another poll.
    runtime_poll_busy: bool,
    /// Consecutive QueryStats failures; three means the local proxy is unhealthy.
    runtime_poll_failures: u8,
    /// Invalidates overdue poll results across starts, stops, and profile switches.
    runtime_generation: u64,
    /// Keeps the OS appearance observer alive for the current window.
    _appearance_sub: Option<Subscription>,
}

impl Focusable for MainWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl MainWindow {
    pub fn new(cx: &mut Context<Self>) -> Self {
        // Bind destructive / list shortcuts only in the "Main" key context so they
        // never steal Backspace/Delete/typing while a modal dialog is open.
        // (GPUI dispatches matching KeyBindings BEFORE on_key_down listeners.)
        cx.bind_keys([
            gpui::KeyBinding::new("cmd-r", ToggleProxy, Some("Main")),
            gpui::KeyBinding::new("ctrl-r", ToggleProxy, Some("Main")),
            gpui::KeyBinding::new("cmd-v", ImportClipboard, Some("Main")),
            gpui::KeyBinding::new("ctrl-v", ImportClipboard, Some("Main")),
            gpui::KeyBinding::new("cmd-s", SaveDb, Some("Main")),
            gpui::KeyBinding::new("ctrl-s", SaveDb, Some("Main")),
            gpui::KeyBinding::new("cmd-a", SelectAll, Some("Main")),
            gpui::KeyBinding::new("ctrl-a", SelectAll, Some("Main")),
            gpui::KeyBinding::new("backspace", DeleteSelected, Some("Main")),
            gpui::KeyBinding::new("delete", DeleteSelected, Some("Main")),
            gpui::KeyBinding::new("cmd-shift-c", CopyLogs, Some("Main")),
            gpui::KeyBinding::new("ctrl-shift-c", CopyLogs, Some("Main")),
            gpui::KeyBinding::new("cmd-t", UrlTestSelected, Some("Main")),
            gpui::KeyBinding::new("ctrl-t", UrlTestSelected, Some("Main")),
            gpui::KeyBinding::new("cmd-shift-g", UrlTestGroup, Some("Main")),
            gpui::KeyBinding::new("ctrl-shift-g", UrlTestGroup, Some("Main")),
            gpui::KeyBinding::new("cmd-shift-r", DeleteUnavailable, Some("Main")),
            gpui::KeyBinding::new("ctrl-shift-r", DeleteUnavailable, Some("Main")),
            gpui::KeyBinding::new("cmd-q", Quit, None),
        ]);

        let (mut state, db_path_label) = load_initial_state();
        let mut core_cfg = CoreConfig::default();
        if let Ok(dir) = std::env::var("THRONE_ASSET_DIR") {
            core_cfg.asset_dir = std::path::PathBuf::from(dir);
        } else if !db_path_label.is_empty() {
            if let Some(parent) = std::path::Path::new(&db_path_label).parent() {
                core_cfg.asset_dir = parent.to_path_buf();
                core_cfg.work_dir = parent.to_path_buf();
            }
        }
        // Crash / failed Stop can leave macOS HTTP proxy → 127.0.0.1:2080 with
        // nothing listening, so every browser tab fails. Clear on launch.
        force_clear_system_proxy();
        state.push_log("System proxy cleared on launch (recovery)");

        let window = Self {
            state,
            focus_handle: cx.focus_handle(),
            search_draft: String::new(),
            db_path_label: db_path_label.clone(),
            open_menu: OpenMenu::None,
            bottom_tab: 0,
            log_scroll_handle: ScrollHandle::new(),
            rendered_log_text: String::new(),
            ctx_menu_at: None,
            ctx_group_id: None,
            dialog: Dialog::None,
            dialog_inputs: None,
            pending_nested_simple_sync: false,
            pending_gpui_dialog: false,
            presented_dialog_stack: (0, NestedKind::None),
            core: Arc::new(Mutex::new(CoreSession::new(core_cfg))),
            core_op_busy: false,
            network_recovery_busy: false,
            restart_when_idle: false,
            pending_stop: false,
            running_profile_display: None,
            pending_profile_switch: PendingProfileSwitch::default(),
            background_busy: false,
            test_progress: None,
            subscription_queue: None,
            sort_column: SortColumn::None,
            sort_asc: true,
            connections: Vec::new(),
            auto_selector_snapshot: Vec::new(),
            prev_traffic_at: None,
            speed_graph: SpeedGraph::default(),
            traffic_stats_db: {
                let main = if db_path_label.is_empty() {
                    throne_storage::default_db_path()
                } else {
                    std::path::PathBuf::from(&db_path_label)
                };
                let stats = throne_storage::stats_db_path(&main);
                match throne_storage::TrafficStatsDb::open(&stats) {
                    Ok(db) => {
                        let now = chrono::Local::now().timestamp();
                        let _ = throne_storage::TrafficStatsManager::run_rollup(&db, now, 90);
                        Some(std::sync::Arc::new(db))
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "open throne_stats.db failed");
                        None
                    }
                }
            },
            traffic_stats_mgr: throne_storage::TrafficStatsManager::default(),
            runtime_poll_busy: false,
            runtime_poll_failures: 0,
            runtime_generation: 0,
            _appearance_sub: None,
        };
        window.spawn_runtime_poller(cx);
        window
    }

    /// Attach (or re-attach) the system appearance observer when a window is opened.
    ///
    /// Call from `open_window` so theme tokens follow OS light/dark changes live.
    pub fn attach_window_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_theme_from_window(window, cx);
        self._appearance_sub = Some(cx.observe_window_appearance(window, |this, window, cx| {
            this.sync_theme_from_window(window, cx);
            cx.notify();
        }));
    }

    fn sync_theme_from_window(&self, window: &Window, cx: &mut App) {
        let system_dark = theme::system_is_dark(window.appearance());
        let scheme = theme::apply_preference(&self.state.settings().theme, system_dark);
        // Tray glyph follows scheme (template on macOS; light/dark swap on Win/Linux).
        crate::tray::apply_scheme(scheme);
        // Dock icon: white/black line-art masters (macOS runtime switch).
        crate::dock_icon::apply_for_scheme(scheme.is_dark());
        // Dialog / Button / Switch / TabBar / Alert read gpui-component Theme.
        Self::sync_gpui_component_theme(scheme, None, cx);
    }

    /// Keep gpui-component global Theme locked to the resolved app scheme.
    ///
    /// Dialog chrome (`cx.theme().background`) and widgets ignore `crate::theme`
    /// tokens — without this, dark→light leaves Confirmation dialogs pure black.
    fn sync_gpui_component_theme(
        scheme: theme::ColorScheme,
        window: Option<&mut Window>,
        cx: &mut App,
    ) {
        if !cx.has_global::<gpui_component::Theme>() {
            return;
        }
        let mode = if scheme.is_dark() {
            gpui_component::ThemeMode::Dark
        } else {
            gpui_component::ThemeMode::Light
        };
        // Always re-apply palette (not only when mode flips). Mode can already be
        // correct while colors stay on the previous scheme after a half-init path.
        gpui_component::Theme::change(mode, window, cx);
    }

    fn spawn_runtime_poller(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                smol::Timer::after(std::time::Duration::from_secs(1)).await;
                if this
                    .update(cx, |this, cx| this.poll_core_runtime(cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn toggle_sort(&mut self, col: SortColumn, cx: &mut Context<Self>) {
        (self.sort_column, self.sort_asc) = next_sort_state(self.sort_column, self.sort_asc, col);
        let result = domain_sort_column(self.sort_column)
            .ok_or_else(|| "no sortable column selected".to_string())
            .and_then(|column| {
                self.state
                    .sort_active_group_profiles(column, self.sort_asc)
                    .map_err(|error| error.to_string())
            })
            .and_then(|()| self.persist_db());
        if let Err(error) = result {
            self.state
                .set_status_message(format!("Sort failed: {error}"));
        }
        cx.notify();
    }

    /// Visible profiles follow the active group's persisted profile ID order.
    fn sorted_profiles(&self) -> Vec<Profile> {
        self
            .state
            .visible_profiles()
            .into_iter()
            .cloned()
            .collect()
    }

    fn sort_label(&self, col: SortColumn, base: &str) -> String {
        if self.sort_column != col {
            return base.to_string();
        }
        if self.sort_asc {
            format!("{base} ↑")
        } else {
            format!("{base} ↓")
        }
    }

    fn close_menus(&mut self) {
        self.open_menu = OpenMenu::None;
        self.ctx_menu_at = None;
        self.ctx_group_id = None;
    }

    fn close_dialog(&mut self) {
        self.dialog = Dialog::None;
        self.dialog_inputs = None;
        self.pending_nested_simple_sync = false;
        self.pending_gpui_dialog = false;
        self.presented_dialog_stack = (0, NestedKind::None);
    }

    /// Close app dialog state and any gpui-component Dialog layer.
    fn close_dialog_with_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_dialog();
        window.close_all_dialogs(cx);
    }

    /// Whether this dialog kind is hosted by gpui-component `open_dialog`.
    fn uses_gpui_dialog(dialog: &Dialog) -> bool {
        !matches!(dialog, Dialog::None)
    }

    fn dialog_stack_key(&self) -> (u8, NestedKind) {
        let main = match &self.dialog {
            Dialog::None => 0,
            Dialog::BasicSettings { .. } => 1,
            Dialog::ManageGroups => 2,
            Dialog::EditGroup { .. } => 12,
            Dialog::ConfirmRemoveGroup { .. } => 13,
            Dialog::AddFromInput { .. } => 3,
            Dialog::TunSettings { .. } => 4,
            Dialog::HotkeySettings { .. } => 5,
            Dialog::EditProfile { .. } => 6,
            Dialog::ConfirmDeleteUnavailable { .. } => 7,
            Dialog::ConfirmUpdateAllSubscriptions => 8,
            Dialog::SubscriptionDiff { .. } => 9,
            Dialog::AutoSelectorStats { .. } => 14,
            Dialog::TrafficStats { .. } => 15,
            Dialog::RoutingSettings(d) => {
                return (10, nested_kind(&d.nested));
            }
        };
        (main, NestedKind::None)
    }

    /// Open (or re-stack) gpui-component Dialogs: main layer + optional nested layer.
    /// Nested route editors are **independent** open_dialog layers (not painted inside Routes).
    fn present_gpui_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_gpui_dialog = false;
        // Re-apply component theme *before* Dialog paints so Confirmation chrome
        // matches the current light/dark scheme (not a stale dark palette).
        self.sync_theme_from_window(window, cx);
        let key = self.dialog_stack_key();
        if key == self.presented_dialog_stack && window.has_active_dialog(cx) {
            return;
        }
        window.close_all_dialogs(cx);
        self.presented_dialog_stack = key;
        if key.0 == 0 {
            return;
        }
        let entity = cx.entity().clone();
        window.open_dialog(cx, move |dialog, window, cx| {
            build_gpui_dialog(dialog, entity.clone(), window, cx)
        });
        if key.0 == 10 && key.1 != NestedKind::None {
            let entity = cx.entity().clone();
            window.open_dialog(cx, move |dialog, window, cx| {
                build_nested_routing_dialog(dialog, entity.clone(), window, cx)
            });
        }
    }

    /// Schedule `present_gpui_dialog` for the next paint (async / no Window paths).
    fn request_gpui_dialog(&mut self) {
        if Self::uses_gpui_dialog(&self.dialog) {
            self.presented_dialog_stack = (0, NestedKind::None);
            self.pending_gpui_dialog = true;
        }
    }

    fn toggle_menu(&mut self, menu: OpenMenu, cx: &mut Context<Self>) {
        if !matches!(self.dialog, Dialog::None) {
            return;
        }
        if self.open_menu == menu {
            self.close_menus();
            cx.notify();
            return;
        }
        self.ctx_group_id = None;
        self.show_menu(menu, None, cx);
    }

    /// Open a toolbar dropdown or context menu (content built on paint).
    fn show_menu(&mut self, menu: OpenMenu, at: Option<(f32, f32)>, cx: &mut Context<Self>) {
        self.open_menu = menu;
        self.ctx_menu_at = at;
        cx.notify();
    }

    fn open_basic_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.dialog = Dialog::basic_from_state(&self.state);
        self.dialog_inputs = Some(DialogInputs::basic(window, cx, &self.state));
        self.present_gpui_dialog(window, cx);
        cx.notify();
    }

    fn open_manage_groups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.dialog = Dialog::manage_groups_from_state(&self.state);
        self.dialog_inputs = None;
        self.present_gpui_dialog(window, cx);
        cx.notify();
    }

    fn open_edit_group_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = Dialog::edit_group_new();
        self.dialog_inputs = Some(DialogInputs::edit_group(window, cx, "", ""));
        self.present_gpui_dialog(window, cx);
        cx.notify();
    }

    fn open_edit_group(&mut self, id: GroupId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = Dialog::edit_group_from_state(&self.state, id) else {
            self.state.set_status_message("Group not found");
            cx.notify();
            return;
        };
        let (name, url) = self
            .state
            .group(id)
            .map(|g| (g.name.clone(), g.url.clone()))
            .unwrap_or_default();
        self.dialog = d;
        self.dialog_inputs = Some(DialogInputs::edit_group(window, cx, &name, &url));
        self.present_gpui_dialog(window, cx);
        cx.notify();
    }

    fn return_to_manage_groups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = Dialog::manage_groups_from_state(&self.state);
        self.dialog_inputs = None;
        self.present_gpui_dialog(window, cx);
        cx.notify();
    }

    fn proxy_display_label(&self, proxy_id: i64) -> String {
        if proxy_id < 0 {
            return "None".into();
        }
        let Some(p) = self.state.profile(proxy_id as ProfileId) else {
            return "INVALID".into();
        };
        let gname = self
            .state
            .group(p.group_id)
            .map(|g| g.name.as_str())
            .unwrap_or("?");
        format!("[{gname}] {}", p.name)
    }

    /// Ordered proxy ids for front/landing cycle (None + all profiles).
    fn group_proxy_cycle_ids(&self) -> Vec<i64> {
        let mut ids = vec![-1_i64];
        for g in self.state.all_groups() {
            for &pid in &g.profile_ids {
                ids.push(pid as i64);
            }
        }
        ids
    }

    fn cycle_group_proxy(ids: &[i64], current: i64) -> i64 {
        if ids.is_empty() {
            return -1;
        }
        let Some(ix) = ids.iter().position(|&x| x == current) else {
            return ids[0];
        };
        ids[(ix + 1) % ids.len()]
    }

    fn open_add_from_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.dialog = Dialog::add_from_input();
        self.dialog_inputs = Some(DialogInputs::add_from_input(window, cx));
        self.present_gpui_dialog(window, cx);
        cx.notify();
    }

    /// Upstream 1.2.3 Auto Selector: create a selector tracking the active group.
    fn create_auto_selector(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        let home = self.state.active_group_id();
        // Track the active group (subscription group is the common case).
        let tracked = home;
        match self.state.add_auto_selector(home, tracked, "") {
            Ok(id) => {
                let _ = self.state.select_profile(id);
                let _ = self.persist_db();
                if let Ok(plan) = self.state.plan_auto_selector_profile(id) {
                    self.state.set_status_message(format!(
                        "Auto Selector created · {}",
                        plan.summary()
                    ));
                } else {
                    self.state
                        .set_status_message("Auto Selector created — URL Test the group, then Start");
                }
            }
            Err(e) => {
                self.state
                    .set_status_message(format!("Could not create Auto Selector: {e}"));
            }
        }
        cx.notify();
    }

    /// Upstream 1.2.3 multi-file import: pick one or more files and import contents.
    ///
    /// Uses a native macOS/Linux file panel via `osascript` / `zenity` when available;
    /// falls back to reading paths from the clipboard (newline-separated).
    fn import_from_files(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        let paths = pick_import_files();
        if paths.is_empty() {
            // Fallback: clipboard may hold file paths (multi-line).
            let text = cx
                .read_from_clipboard()
                .and_then(|item| item.text().map(|s| s.to_string()))
                .or_else(read_os_clipboard)
                .unwrap_or_default();
            let clip_paths: Vec<_> = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && std::path::Path::new(l).is_file())
                .map(std::path::PathBuf::from)
                .collect();
            if clip_paths.is_empty() {
                self.state.set_status_message(
                    "No files selected. Tip: copy file paths to the clipboard, then retry.",
                );
                cx.notify();
                return;
            }
            self.import_files_list(&clip_paths, cx);
            return;
        }
        self.import_files_list(&paths, cx);
    }

    fn import_files_list(&mut self, paths: &[std::path::PathBuf], cx: &mut Context<Self>) {
        let mut combined = String::new();
        let mut ok = 0usize;
        let mut err = 0usize;
        for path in paths {
            match std::fs::read_to_string(path) {
                Ok(body) => {
                    if !combined.is_empty() {
                        combined.push('\n');
                    }
                    combined.push_str(body.trim());
                    combined.push('\n');
                    ok += 1;
                }
                Err(e) => {
                    self.state
                        .push_log(format!("Import file failed · {}: {e}", path.display()));
                    err += 1;
                }
            }
        }
        if combined.trim().is_empty() {
            self.state
                .set_status_message(format!("No readable content ({ok} ok, {err} failed)"));
            cx.notify();
            return;
        }
        self.state.push_log(format!(
            "Importing from {ok} file(s){}",
            if err > 0 {
                format!(" · {err} failed")
            } else {
                String::new()
            }
        ));
        self.import_text(&combined, cx);
    }

    fn open_routing_settings(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        self.close_menus();
        self.dialog = Dialog::routing_from_state(&self.state);
        // Main DNS/Warp/Hijack fields → real Input (created now or lazily in render).
        if let Some(window) = window {
            let settings = match &self.dialog {
                Dialog::RoutingSettings(d) => d.settings.clone(),
                _ => self.state.settings().clone(),
            };
            self.dialog_inputs = Some(DialogInputs::routing(window, cx, &settings));
            self.present_gpui_dialog(window, cx);
        } else {
            self.dialog_inputs = None;
            self.request_gpui_dialog();
        }
        cx.notify();
    }

    /// Ensure RoutingInputs exist (tray open has no Window until next paint).
    fn ensure_routing_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.dialog, Dialog::RoutingSettings(_)) {
            return;
        }
        if matches!(self.dialog_inputs, Some(DialogInputs::Routing(_))) {
            return;
        }
        let settings = match &self.dialog {
            Dialog::RoutingSettings(d) => d.settings.clone(),
            _ => return,
        };
        self.dialog_inputs = Some(DialogInputs::routing(window, cx, &settings));
    }

    /// Called by [`AppShell`] **before** painting the dialog layer (not from
    /// MainWindow::render) so `open_dialog` builders never re-enter this entity.
    pub fn prepare_dialog_layer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_routing_inputs(window, cx);
        if self.pending_nested_simple_sync {
            self.push_simple_to_nested_inputs(window, cx);
            self.push_rule_to_nested_inputs(window, cx);
            self.pending_nested_simple_sync = false;
        }
        if self.pending_gpui_dialog {
            self.present_gpui_dialog(window, cx);
        }
    }

    fn open_tun_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.dialog = Dialog::tun_from_state(&self.state);
        let mtu = match &self.dialog {
            Dialog::TunSettings { vpn_mtu, .. } => vpn_mtu.clone(),
            _ => "9000".into(),
        };
        self.dialog_inputs = Some(DialogInputs::tun(window, cx, &mtu));
        self.present_gpui_dialog(window, cx);
        cx.notify();
    }

    fn open_hotkey_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.dialog = Dialog::hotkey_from_state(&self.state);
        // Capture fields write into `Dialog::HotkeySettings` strings (no InputState).
        self.dialog_inputs = None;
        self.present_gpui_dialog(window, cx);
        cx.notify();
    }

    fn open_edit_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        match Dialog::edit_profile_from_state(&self.state) {
            Some(d) => {
                let name = match &d {
                    Dialog::EditProfile { name, .. } => name.clone(),
                    _ => String::new(),
                };
                self.dialog = d;
                self.dialog_inputs = Some(DialogInputs::edit_profile(window, cx, &name));
                self.present_gpui_dialog(window, cx);
                cx.notify();
            }
            None => {
                self.state
                    .set_status_message("Select a profile to edit");
                cx.notify();
            }
        }
    }

    /// Snapshot running profile into config_meta so history survives rename/delete.
    fn snapshot_running_profile_traffic_meta(&mut self, profile_id: ProfileId) {
        let Some(db) = self.traffic_stats_db.as_ref() else {
            return;
        };
        let now = chrono::Local::now().timestamp();
        let (name, group_name, type_name, server) =
            if let Some(p) = self.state.profile(profile_id) {
                let gname = self
                    .state
                    .all_groups()
                    .into_iter()
                    .find(|g| g.id == p.group_id)
                    .map(|g| g.name.clone())
                    .unwrap_or_default();
                (
                    p.name.clone(),
                    gname,
                    p.profile_type.as_str().to_string(),
                    p.display_address(),
                )
            } else {
                (format!("Profile #{profile_id}"), String::new(), String::new(), String::new())
            };
        let _ = db.upsert_config_meta(&throne_storage::ConfigMetaRow {
            profile_id,
            name,
            group_name,
            type_name,
            server_address: server,
            first_seen: now,
            last_seen: now,
        });
        let _ = db.upsert_config_meta(&throne_storage::ConfigMetaRow {
            profile_id: throne_storage::DIRECT_STAT_PROFILE_ID,
            name: "Direct".into(),
            group_name: String::new(),
            type_name: "direct".into(),
            server_address: String::new(),
            first_seen: now,
            last_seen: now,
        });
    }

    fn open_traffic_stats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_menu = OpenMenu::None;
        // Flush in-progress minute so the dialog is up to date.
        if let Some(db) = self.traffic_stats_db.clone() {
            let _ = self.traffic_stats_mgr.flush(&db);
        }
        self.dialog = Dialog::TrafficStats {
            period: 0,
            tab: 0,
            summary: String::new(),
            breakdown_lines: Vec::new(),
            bars: Vec::new(),
            notice: String::new(),
        };
        self.refresh_traffic_stats_dialog();
        self.dialog_inputs = None;
        self.present_gpui_dialog(window, cx);
        cx.notify();
    }

    /// Rebuild summary / bars / breakdown for the open Traffic Stats dialog.
    fn refresh_traffic_stats_dialog(&mut self) {
        let (period, tab) = match &self.dialog {
            Dialog::TrafficStats { period, tab, .. } => (*period, *tab),
            _ => return,
        };
        let Some(db) = self.traffic_stats_db.clone() else {
            if let Dialog::TrafficStats { notice, .. } = &mut self.dialog {
                *notice = "throne_stats.db unavailable".into();
            }
            return;
        };
        let _ = self.traffic_stats_mgr.flush(&db);

        let now = chrono::Local::now().timestamp();
        let window_secs = match period {
            1 => 7 * 86400,
            2 => 30 * 86400,
            3 => 90 * 86400,
            _ => 24 * 3600,
        };
        let bucket_secs = if period == 0 { 3600 } else { 86400 };
        let from = now - window_secs;
        let tz_offset = chrono::Local::now().offset().local_minus_utc() as i64;

        let series = if tab == 1 {
            db.query_app_series(from, now, bucket_secs, tz_offset)
        } else {
            db.query_config_series(from, now, bucket_secs, tz_offset)
        };
        let series = series.unwrap_or_default();

        let mut by_bucket: std::collections::HashMap<i64, (i64, i64)> =
            std::collections::HashMap::new();
        let mut total_up = 0i64;
        let mut total_down = 0i64;
        for pt in &series {
            by_bucket.insert(pt.bucket_start, (pt.up, pt.down));
            total_up = total_up.saturating_add(pt.up);
            total_down = total_down.saturating_add(pt.down);
        }
        let aligned_from = ((from + tz_offset) / bucket_secs) * bucket_secs - tz_offset;
        let mut bars = Vec::new();
        let mut b = aligned_from;
        while b < now {
            let (up, down) = by_bucket.get(&b).copied().unwrap_or((0, 0));
            let label = if bucket_secs >= 86400 {
                chrono::DateTime::from_timestamp(b, 0)
                    .map(|dt| dt.with_timezone(&chrono::Local).format("%m/%d").to_string())
                    .unwrap_or_default()
            } else {
                chrono::DateTime::from_timestamp(b, 0)
                    .map(|dt| dt.with_timezone(&chrono::Local).format("%H:%M").to_string())
                    .unwrap_or_default()
            };
            bars.push((label, down, up));
            b += bucket_secs;
        }

        let summary = format!(
            "Download: {}     Upload: {}     Total: {}",
            throne_domain::human_bytes(total_down),
            throne_domain::human_bytes(total_up),
            throne_domain::human_bytes(total_down.saturating_add(total_up)),
        );

        let mut breakdown_lines = Vec::new();
        const MAX_ROWS: usize = 9;
        if tab == 1 {
            let mut usage = db.query_app_usage(from, now).unwrap_or_default();
            usage.sort_by_key(|u| std::cmp::Reverse(u.down.saturating_add(u.up)));
            let shown = usage.len().min(MAX_ROWS);
            for u in usage.iter().take(shown) {
                let name = if u.process_name.is_empty() {
                    "Unknown"
                } else {
                    u.process_name.as_str()
                };
                breakdown_lines.push(format!(
                    "{name}  ↓{}  ↑{}  Σ{}",
                    throne_domain::human_bytes(u.down),
                    throne_domain::human_bytes(u.up),
                    throne_domain::human_bytes(u.down.saturating_add(u.up)),
                ));
            }
            if usage.len() > MAX_ROWS {
                let (od, ou) = usage[MAX_ROWS..].iter().fold((0i64, 0i64), |(d, u), row| {
                    (d + row.down, u + row.up)
                });
                breakdown_lines.push(format!(
                    "Other  ↓{}  ↑{}  Σ{}",
                    throne_domain::human_bytes(od),
                    throne_domain::human_bytes(ou),
                    throne_domain::human_bytes(od + ou),
                ));
            }
        } else {
            let mut usage = db.query_config_usage(from, now).unwrap_or_default();
            usage.sort_by_key(|u| std::cmp::Reverse(u.down.saturating_add(u.up)));
            let meta = db.all_config_meta().unwrap_or_default();
            let meta_map: std::collections::HashMap<i64, _> =
                meta.into_iter().map(|m| (m.profile_id, m)).collect();
            let shown = usage.len().min(MAX_ROWS);
            for u in usage.iter().take(shown) {
                let name = if let Some(m) = meta_map.get(&u.profile_id) {
                    if m.name.is_empty() {
                        format!("Profile #{}", u.profile_id)
                    } else if m.group_name.is_empty() {
                        m.name.clone()
                    } else {
                        format!("{} · {}", m.name, m.group_name)
                    }
                } else if u.profile_id == throne_storage::DIRECT_STAT_PROFILE_ID {
                    "Direct".into()
                } else if let Some(p) = self.state.profile(u.profile_id) {
                    p.name.clone()
                } else {
                    format!("Profile #{} (deleted)", u.profile_id)
                };
                breakdown_lines.push(format!(
                    "{name}  ↓{}  ↑{}  Σ{}",
                    throne_domain::human_bytes(u.down),
                    throne_domain::human_bytes(u.up),
                    throne_domain::human_bytes(u.down.saturating_add(u.up)),
                ));
            }
            if usage.len() > MAX_ROWS {
                let (od, ou) = usage[MAX_ROWS..].iter().fold((0i64, 0i64), |(d, u), row| {
                    (d + row.down, u + row.up)
                });
                breakdown_lines.push(format!(
                    "Other  ↓{}  ↑{}  Σ{}",
                    throne_domain::human_bytes(od),
                    throne_domain::human_bytes(ou),
                    throne_domain::human_bytes(od + ou),
                ));
            }
        }

        if let Dialog::TrafficStats {
            summary: s,
            breakdown_lines: lines,
            bars: b,
            notice,
            ..
        } = &mut self.dialog
        {
            *s = summary;
            *lines = breakdown_lines;
            *b = bars;
            *notice = String::new();
        }
    }

    /// Upstream Tools → Auto Selector Stats (live while a selector is running).
    fn open_auto_selector_stats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        let running_is_selector = matches!(
            self.state.core_status(),
            CoreStatus::Running { profile_id, .. }
                if self
                    .state
                    .profile(*profile_id)
                    .is_some_and(|p| p.profile_type == ProfileType::AutoSelector)
        );
        let notice = if !self.state.core_status().is_running() {
            "Core is not running — start an Auto Selector profile first.".into()
        } else if !running_is_selector {
            "Running profile is not an Auto Selector — stats may be empty.".into()
        } else {
            String::new()
        };
        self.dialog = Dialog::AutoSelectorStats {
            only_problems: false,
            selected_member: String::new(),
            notice,
        };
        self.dialog_inputs = None;
        self.present_gpui_dialog(window, cx);
        // Kick an immediate snapshot.
        self.refresh_auto_selector_stats(cx);
        cx.notify();
    }

    fn refresh_auto_selector_stats(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.dialog, Dialog::AutoSelectorStats { .. }) {
            return;
        }
        if !self.state.core_status().is_running() || self.core_op_busy {
            return;
        }
        let core = Arc::clone(&self.core);
        let poll_generation = self.runtime_generation;
        cx.spawn(async move |this, cx| {
            let snap = cx
                .background_executor()
                .spawn(async move {
                    let mut guard = core
                        .lock()
                        .map_err(|_| "core session lock poisoned".to_string())?;
                    guard
                        .query_auto_selectors()
                        .map_err(|e| e.to_string())
                })
                .await;
            this.update(cx, |this, cx| {
                if !runtime_poll_is_current(poll_generation, this.runtime_generation) {
                    return;
                }
                if !matches!(this.dialog, Dialog::AutoSelectorStats { .. }) {
                    return;
                }
                match snap {
                    Ok(groups) => {
                        let mut views: Vec<_> = groups
                            .into_iter()
                            .map(auto_selector_group_view)
                            .collect();
                        enrich_auto_selector_names(&this.state, &mut views);
                        this.auto_selector_snapshot = views;
                        if let Dialog::AutoSelectorStats { notice, .. } = &mut this.dialog {
                            if this.auto_selector_snapshot.is_empty() {
                                *notice =
                                    "Core reports no auto-selector groups (rebuild ThroneCore 1.2.3?)."
                                        .into();
                            } else if notice.contains("no auto-selector")
                                || notice.contains("not running")
                            {
                                notice.clear();
                            }
                        }
                        // Re-present so table text refreshes (gpui dialog content is snapshot).
                        this.presented_dialog_stack = (0, NestedKind::None);
                        this.request_gpui_dialog();
                        cx.notify();
                    }
                    Err(e) => {
                        if let Dialog::AutoSelectorStats { notice, .. } = &mut this.dialog {
                            *notice = format!("QueryAutoSelectors failed: {e}");
                        }
                        this.presented_dialog_stack = (0, NestedKind::None);
                        this.request_gpui_dialog();
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn auto_selector_action(&mut self, action: &str, member: &str, cx: &mut Context<Self>) {
        if !self.state.core_status().is_running() {
            if let Dialog::AutoSelectorStats { notice, .. } = &mut self.dialog {
                *notice = "Core is not running".into();
            }
            cx.notify();
            return;
        }
        let tag = self
            .auto_selector_snapshot
            .first()
            .map(|g| g.tag.clone())
            .unwrap_or_default();
        let core = Arc::clone(&self.core);
        let action = action.to_string();
        let member = member.to_string();
        let action_for_msg = action.clone();
        let member_for_msg = member.clone();
        let poll_generation = self.runtime_generation;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut guard = core
                        .lock()
                        .map_err(|_| "core session lock poisoned".to_string())?;
                    guard
                        .auto_selector_action(&tag, &action, &member)
                        .map_err(|e| e.to_string())
                })
                .await;
            this.update(cx, |this, cx| {
                if !runtime_poll_is_current(poll_generation, this.runtime_generation) {
                    return;
                }
                match result {
                    Ok(()) => {
                        if let Dialog::AutoSelectorStats { notice, .. } = &mut this.dialog {
                            *notice = match action_for_msg.as_str() {
                                "recheck" => "Recheck requested".into(),
                                "select" if member_for_msg.is_empty() => "Pin released".into(),
                                "select" => format!("Pinned {member_for_msg}"),
                                _ => "OK".into(),
                            };
                        }
                        this.refresh_auto_selector_stats(cx);
                    }
                    Err(e) => {
                        if let Dialog::AutoSelectorStats { notice, .. } = &mut this.dialog {
                            *notice = format!("Action failed: {e}");
                        }
                        this.presented_dialog_stack = (0, NestedKind::None);
                        this.request_gpui_dialog();
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn save_edit_profile(&mut self, cx: &mut Context<Self>) {
        let id = match &self.dialog {
            Dialog::EditProfile { id, .. } => *id,
            _ => return,
        };
        let name = match &self.dialog_inputs {
            Some(DialogInputs::EditProfile { name }) => {
                DialogInputs::read_string(name, cx)
            }
            _ => return,
        };
        match self.state.rename_profile(id, name) {
            Ok(()) => {
                self.close_dialog();
                let _ = self.persist_db();
                self.state.set_status_message("Profile renamed");
            }
            Err(e) => {
                self.state
                    .set_status_message(format!("Rename failed: {e}"));
            }
        }
        cx.notify();
    }

    fn handle_routing_event(
        &mut self,
        ev: RoutingEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sync_dns_presets = matches!(
            &ev,
            RoutingEvent::Cycle("remote_dns_preset") | RoutingEvent::Cycle("direct_dns_preset")
        );
        // Pull NestedInputs into draft before actions that read name/url/simple/rules/import.
        let needs_sync = matches!(
            &ev,
            RoutingEvent::NestedAction(
                "editor-ok"
                    | "raw-ok"
                    | "import-ok"
                    | "remote-preview"
                    | "remote-fetch"
                    | "rule-new"
                    | "rule-del"
                    | "rule-up"
                    | "rule-down"
            ) | RoutingEvent::ReTab(_)
                | RoutingEvent::ReSelectRule(_)
        );
        if needs_sync {
            self.sync_nested_inputs_into_draft(cx);
        }
        let switching_to_basic = matches!(&ev, RoutingEvent::ReTab(RouteEditorTab::Basic));
        let push_rule_after = matches!(
            &ev,
            RoutingEvent::ReSelectRule(_)
                | RoutingEvent::NestedAction("rule-new" | "rule-del" | "rule-up" | "rule-down")
                | RoutingEvent::ReTab(RouteEditorTab::Advanced)
        );

        let Dialog::RoutingSettings(draft) = &mut self.dialog else {
            return;
        };
        let before_nested = nested_kind(&draft.nested);
        let effect = draft.apply_event(ev);
        let after_nested = if let Dialog::RoutingSettings(d) = &self.dialog {
            nested_kind(&d.nested)
        } else {
            NestedKind::None
        };

        // ▼ DNS presets update draft.settings — push into Input widgets.
        if sync_dns_presets {
            if let (Dialog::RoutingSettings(d), Some(DialogInputs::Routing(inputs))) =
                (&self.dialog, &self.dialog_inputs)
            {
                DialogInputs::set_string(
                    &inputs.remote_dns,
                    d.settings.remote_dns.clone(),
                    window,
                    cx,
                );
                DialogInputs::set_string(
                    &inputs.direct_dns,
                    d.settings.direct_dns.clone(),
                    window,
                    cx,
                );
            }
        }

        // Create / clear NestedInputs when the nested form kind changes.
        if before_nested != after_nested {
            self.rebuild_nested_inputs(window, cx);
            // Independent open_dialog layer for nested — re-stack main ± nested.
            self.presented_dialog_stack = (0, NestedKind::None);
            self.present_gpui_dialog(window, cx);
        } else if switching_to_basic {
            // Advanced → Basic reloads simple_* strings; push into NestedInputs.
            self.push_simple_to_nested_inputs(window, cx);
        } else if push_rule_after {
            // Selected rule changed — load its attrs into NestedInputs.
            self.push_rule_to_nested_inputs(window, cx);
        }

        self.dispatch_routing_effect(effect, window, cx);
    }

    /// Write NestedInputs values into the open nested draft (before OK / fetch / tab switch).
    fn sync_nested_inputs_into_draft(&mut self, cx: &App) {
        let Some(DialogInputs::Routing(inputs)) = &self.dialog_inputs else {
            return;
        };
        let Some(nested) = &inputs.nested else {
            return;
        };
        let Dialog::RoutingSettings(draft) = &mut self.dialog else {
            return;
        };
        match (&mut draft.nested, nested) {
            (RoutingNested::RouteEditor(ed), NestedInputs::RouteEditor(re)) => {
                ed.profile.name = DialogInputs::read_string(&re.name, cx);
                ed.profile.remote_url = DialogInputs::read_string(&re.url, cx);
                ed.simple_direct = DialogInputs::read_string(&re.simple_direct, cx);
                ed.simple_proxy = DialogInputs::read_string(&re.simple_proxy, cx);
                ed.simple_block = DialogInputs::read_string(&re.simple_block, cx);
                ed.simple_warp = DialogInputs::read_string(&re.simple_warp, cx);
                // Advanced rule fields → currently selected rule.
                if let Some(i) = ed.selected_rule {
                    if let Some(r) = ed.profile.rules.get_mut(i) {
                        r.name = DialogInputs::read_string(&re.rule_name, cx);
                        r.protocol = DialogInputs::read_string(&re.rule_protocol, cx);
                        r.domain = lines_to_vec(&DialogInputs::read_string(&re.rule_domain, cx));
                        r.domain_suffix =
                            lines_to_vec(&DialogInputs::read_string(&re.rule_suffix, cx));
                        r.ip_cidr = lines_to_vec(&DialogInputs::read_string(&re.rule_ip, cx));
                    }
                }
            }
            (RoutingNested::RawEditor(ed), NestedInputs::RawEditor(raw)) => {
                ed.name = DialogInputs::read_string(&raw.name, cx);
                ed.raw_route = DialogInputs::read_string(&raw.json, cx);
            }
            (RoutingNested::ImportPaste { text }, NestedInputs::ImportPaste { text: inp }) => {
                *text = DialogInputs::read_string(inp, cx);
            }
            _ => {}
        }
    }

    /// Push draft simple_* strings into NestedInputs (after Advanced→Basic or remote fetch).
    fn push_simple_to_nested_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let simple = match &self.dialog {
            Dialog::RoutingSettings(d) => match &d.nested {
                RoutingNested::RouteEditor(ed) => Some((
                    ed.simple_direct.clone(),
                    ed.simple_proxy.clone(),
                    ed.simple_block.clone(),
                    ed.simple_warp.clone(),
                )),
                _ => None,
            },
            _ => None,
        };
        let Some((direct, proxy, block, warp)) = simple else {
            return;
        };
        let Some(DialogInputs::Routing(inputs)) = &self.dialog_inputs else {
            return;
        };
        let Some(NestedInputs::RouteEditor(re)) = &inputs.nested else {
            return;
        };
        DialogInputs::set_string(&re.simple_direct, direct, window, cx);
        DialogInputs::set_string(&re.simple_proxy, proxy, window, cx);
        DialogInputs::set_string(&re.simple_block, block, window, cx);
        DialogInputs::set_string(&re.simple_warp, warp, window, cx);
    }

    /// Push the currently selected rule's attrs into NestedInputs (or clear if none).
    fn push_rule_to_nested_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rule = match &self.dialog {
            Dialog::RoutingSettings(d) => match &d.nested {
                RoutingNested::RouteEditor(ed) => ed
                    .selected_rule
                    .and_then(|i| ed.profile.rules.get(i))
                    .map(|r| {
                        (
                            r.name.clone(),
                            r.protocol.clone(),
                            r.domain.join("\n"),
                            r.domain_suffix.join("\n"),
                            r.ip_cidr.join("\n"),
                        )
                    }),
                _ => None,
            },
            _ => None,
        };
        let Some(DialogInputs::Routing(inputs)) = &self.dialog_inputs else {
            return;
        };
        let Some(NestedInputs::RouteEditor(re)) = &inputs.nested else {
            return;
        };
        let (name, protocol, domain, suffix, ip) =
            rule.unwrap_or_else(|| (String::new(), String::new(), String::new(), String::new(), String::new()));
        DialogInputs::set_string(&re.rule_name, name, window, cx);
        DialogInputs::set_string(&re.rule_protocol, protocol, window, cx);
        DialogInputs::set_string(&re.rule_domain, domain, window, cx);
        DialogInputs::set_string(&re.rule_suffix, suffix, window, cx);
        DialogInputs::set_string(&re.rule_ip, ip, window, cx);
    }

    /// Allocate NestedInputs for the current nested form, or clear them.
    fn rebuild_nested_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(DialogInputs::Routing(inputs)) = self.dialog_inputs.as_mut() else {
            return;
        };
        let Dialog::RoutingSettings(draft) = &self.dialog else {
            inputs.clear_nested();
            return;
        };
        match &draft.nested {
            RoutingNested::RouteEditor(ed) => {
                inputs.set_nested(NestedInputs::route_editor(
                    window,
                    cx,
                    &ed.profile.name,
                    &ed.profile.remote_url,
                    &ed.simple_direct,
                    &ed.simple_proxy,
                    &ed.simple_block,
                    &ed.simple_warp,
                ));
            }
            RoutingNested::RawEditor(ed) => {
                inputs.set_nested(NestedInputs::raw_editor(
                    window,
                    cx,
                    &ed.name,
                    &ed.raw_route,
                ));
            }
            RoutingNested::ImportPaste { text } => {
                inputs.set_nested(NestedInputs::import_paste(window, cx, text));
            }
            RoutingNested::None
            | RoutingNested::NewMenu
            | RoutingNested::UpdateMenu { .. }
            | RoutingNested::Notice { .. } => {
                inputs.clear_nested();
            }
        }
        self.pending_nested_simple_sync = false;
    }

    /// Whether NestedInputs currently owns keyboard input (skip fake-caret).
    fn nested_inputs_own_typing(&self) -> bool {
        let Some(DialogInputs::Routing(inputs)) = &self.dialog_inputs else {
            return false;
        };
        let Some(nested) = &inputs.nested else {
            return false;
        };
        let Dialog::RoutingSettings(draft) = &self.dialog else {
            return false;
        };
        match (&draft.nested, nested) {
            (RoutingNested::ImportPaste { .. }, NestedInputs::ImportPaste { .. }) => true,
            // Structured + raw route editors are fully Input-backed.
            (RoutingNested::RouteEditor(_), NestedInputs::RouteEditor(_)) => true,
            (RoutingNested::RawEditor(_), NestedInputs::RawEditor(_)) => true,
            _ => false,
        }
    }

    fn dispatch_routing_effect(
        &mut self,
        effect: RoutingSideEffect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match effect {
            RoutingSideEffect::None => {
                cx.notify();
            }
            RoutingSideEffect::Close => {
                self.close_dialog_with_window(window, cx);
                cx.notify();
            }
            RoutingSideEffect::Commit => {
                self.commit_routing_settings(cx);
                // Only dismiss the gpui layer when commit closed the app dialog.
                if matches!(self.dialog, Dialog::None) {
                    window.close_all_dialogs(cx);
                }
            }
            RoutingSideEffect::CopyClipboard(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                if let Dialog::RoutingSettings(d) = &mut self.dialog {
                    d.notice = "Copied!".into();
                }
                self.state.set_status_message("Route share link copied");
                cx.notify();
            }
            RoutingSideEffect::TryClipboardImport => {
                let clip = cx
                    .read_from_clipboard()
                    .and_then(|c| c.text().map(|s| s.to_string()))
                    .or_else(read_os_clipboard)
                    .unwrap_or_default();
                let clip = clip.trim().to_string();
                if !clip.is_empty() {
                    if self.try_import_route_text(&clip, true, window, cx) {
                        return;
                    }
                }
                if let Dialog::RoutingSettings(d) = &mut self.dialog {
                    d.nested = RoutingNested::ImportPaste {
                        text: String::new(),
                    };
                }
                self.rebuild_nested_inputs(window, cx);
                cx.notify();
            }
            RoutingSideEffect::ImportText(text) => {
                let _ = self.try_import_route_text(&text, false, window, cx);
                cx.notify();
            }
            RoutingSideEffect::UpdateRemotes(profiles) => {
                self.update_remote_routes(profiles, cx);
            }
            RoutingSideEffect::FetchRemote { url, apply } => {
                self.fetch_remote_into_editor(url, apply, cx);
            }
            RoutingSideEffect::WarpGenerate => {
                if let Dialog::RoutingSettings(d) = &mut self.dialog {
                    d.nested = RoutingNested::Notice {
                        title: "Generate Warp Config".into(),
                        body: "WARP auto-generate needs a running core (GenWgKeyPair). \
                               Start the proxy first, or fill Endpoint / keys manually."
                            .into(),
                    };
                }
                // Notice has no NestedInputs
                if let Some(DialogInputs::Routing(inputs)) = self.dialog_inputs.as_mut() {
                    inputs.clear_nested();
                }
                cx.notify();
            }
        }
    }

    fn commit_routing_settings(&mut self, cx: &mut Context<Self>) {
        // Pull live Input values into the draft before validate/commit.
        if let (Dialog::RoutingSettings(draft), Some(DialogInputs::Routing(inputs))) =
            (&mut self.dialog, &self.dialog_inputs)
        {
            inputs.apply_to_settings(&mut draft.settings, cx);
        }

        let Dialog::RoutingSettings(draft) = &self.dialog else {
            return;
        };
        // Validate DNS hijack rules (prefer Input text when present).
        let rules_text = if let Some(DialogInputs::Routing(inputs)) = &self.dialog_inputs {
            DialogInputs::read_string(&inputs.dns_rules, cx)
        } else {
            draft.dns_rules_text()
        };
        if !crate::ui::routing::RoutingDraft::validate_dns_rules(&rules_text) {
            if let Dialog::RoutingSettings(d) = &mut self.dialog {
                d.nested = RoutingNested::Notice {
                    title: "Invalid settings".into(),
                    body: "DNS Rules are not valid".into(),
                };
            }
            cx.notify();
            return;
        }
        if draft.routes.is_empty() {
            if let Dialog::RoutingSettings(d) = &mut self.dialog {
                d.nested = RoutingNested::Notice {
                    title: "Invalid settings".into(),
                    body: "Routing profile cannot be empty".into(),
                };
            }
            cx.notify();
            return;
        }
        let routes = draft.routes.clone();
        let active_id = draft.active_id;
        let mut settings = draft.settings.clone();
        settings.current_route_id = active_id;
        // DNS rules already applied via RoutingInputs::apply_to_settings when present.

        match self.state.commit_routes(routes, active_id) {
            Ok(()) => {
                self.state.apply_routing_dialog_settings(settings);
                // re-sync active id after commit (ids may have been reassigned)
                if let Some(id) = self.state.active_route().map(|r| r.id) {
                    let _ = self.state.set_active_route(id);
                }
                self.close_dialog();
                let _ = self.persist_db();
                // Route rules / DNS are compiled only at Start — bounce if live.
                self.state.push_log("Routing settings saved");
                self.reload_core_for_route_change("Routing settings saved", cx);
            }
            Err(e) => {
                if let Dialog::RoutingSettings(d) = &mut self.dialog {
                    d.notice = e.to_string();
                }
                cx.notify();
            }
        }
    }

    /// Rebuild core config after route profile edits or active-route switches.
    ///
    /// Upstream reloads on route change; we Stop→Start so Direct/Proxy `suffix:`
    /// rules take effect without a manual restart.
    fn reload_core_for_route_change(&mut self, reason: &str, cx: &mut Context<Self>) {
        let live = self.state.core_status().is_running()
            || matches!(self.state.core_status(), CoreStatus::Starting)
            || self.core_op_busy;
        if !live {
            self.state
                .set_status_message_only(format!("{reason} (apply on next Start)"));
            cx.notify();
            return;
        }
        if should_queue_recovery_restart(self.network_recovery_busy) {
            self.restart_when_idle = true;
            self.state
                .set_status_message_only(format!("{reason} · restart queued (restoring network)…"));
            cx.notify();
            return;
        }
        if self.state.core_status().is_running()
            || matches!(self.state.core_status(), CoreStatus::Starting)
        {
            self.restart_when_idle = true;
            self.state
                .set_status_message_only(format!("{reason} · restarting core…"));
            self.stop_proxy(cx);
        } else {
            // Stop/start already in flight — apply when idle.
            self.restart_when_idle = true;
            self.state
                .set_status_message_only(format!("{reason} · restart queued…"));
            cx.notify();
        }
    }

    /// Import route text into the open routing draft. Returns true if handled.
    fn try_import_route_text(
        &mut self,
        text: &str,
        from_clipboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let report = match throne_import::try_import_routes(text) {
            Some(r) => r,
            None => {
                let r = throne_import::import_route_payload(text);
                if r.routes.is_empty() {
                    if !from_clipboard {
                        if let Dialog::RoutingSettings(d) = &mut self.dialog {
                            d.nested = RoutingNested::Notice {
                                title: "Invalid input".into(),
                                body: format!(
                                    "Could not import this routing profile:\n{}",
                                    r.errors.join("; ")
                                ),
                            };
                        }
                        if let Some(DialogInputs::Routing(inputs)) = self.dialog_inputs.as_mut() {
                            inputs.clear_nested();
                        }
                        cx.notify();
                    }
                    return false;
                }
                r
            }
        };
        if report.routes.is_empty() {
            if !from_clipboard {
                if let Dialog::RoutingSettings(d) = &mut self.dialog {
                    d.nested = RoutingNested::Notice {
                        title: "Invalid input".into(),
                        body: format!(
                            "Could not import this routing profile:\n{}",
                            report.errors.join("; ")
                        ),
                    };
                }
                if let Some(DialogInputs::Routing(inputs)) = self.dialog_inputs.as_mut() {
                    inputs.clear_nested();
                }
                cx.notify();
            }
            return false;
        }

        let is_remote_link = text.contains("remoteRoute")
            || report.notes.iter().any(|n| n.contains("remoteRoute"));
        if is_remote_link && report.routes.iter().all(|r| r.is_remote) {
            let mut to_update = Vec::new();
            if let Dialog::RoutingSettings(d) = &mut self.dialog {
                for mut p in report.routes {
                    p.id = -1;
                    d.routes.push(p.clone());
                    to_update.push(p);
                }
                d.selected_idx = d.routes.len().saturating_sub(1);
                d.nested = RoutingNested::None;
                d.notice = format!("Added {} remote profile(s)", to_update.len());
            }
            if let Some(DialogInputs::Routing(inputs)) = self.dialog_inputs.as_mut() {
                inputs.clear_nested();
            }
            cx.notify();
            self.update_remote_routes(to_update, cx);
            return true;
        }

        let was_legacy = report.notes.iter().any(|n| n.contains("legacy"));
        if let Dialog::RoutingSettings(d) = &mut self.dialog {
            let name = report.routes[0].name.clone();
            d.apply_imported_profile(report.routes[0].clone(), was_legacy);
            if from_clipboard {
                d.notice = if was_legacy {
                    "Imported routing rule list from clipboard".into()
                } else {
                    format!("Imported «{name}» from clipboard")
                };
            }
            if !report.warnings.is_empty() {
                d.notice = report.warnings.join("; ");
            }
        }
        // Legacy array opens RouteEditor — allocate NestedInputs for Name/URL.
        self.rebuild_nested_inputs(window, cx);
        cx.notify();
        true
    }

    fn update_remote_routes(
        &mut self,
        profiles: Vec<throne_domain::RouteProfile>,
        cx: &mut Context<Self>,
    ) {
        if self.background_busy {
            if let Dialog::RoutingSettings(d) = &mut self.dialog {
                d.notice = "Busy — wait for current job".into();
            }
            cx.notify();
            return;
        }
        if profiles.is_empty() {
            return;
        }
        self.background_busy = true;
        let total = profiles.len();
        if let Dialog::RoutingSettings(d) = &mut self.dialog {
            d.notice = if total <= 1 {
                "Updating...".into()
            } else {
                format!("Updating (1 / {total})")
            };
        }
        cx.notify();

        let jobs: Vec<(i64, String, String)> = profiles
            .into_iter()
            .map(|p| (p.id, p.remote_url.clone(), p.name.clone()))
            .collect();

        cx.spawn(async move |this, cx| {
            let mut updated = Vec::new();
            let mut failures = Vec::new();
            for (i, (id, url, name)) in jobs.into_iter().enumerate() {
                let current = i + 1;
                this.update(cx, |this, cx| {
                    if let Dialog::RoutingSettings(d) = &mut this.dialog {
                        d.notice = if total <= 1 {
                            "Updating...".into()
                        } else {
                            format!("Updating ({current} / {total})")
                        };
                    }
                    cx.notify();
                })
                .ok();
                let url_for_match = url.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let body = throne_import::fetch_url(&url)?;
                        fetch_route_from_body(&body)
                    })
                    .await;
                match result {
                    Ok(r) => updated.push((id, r, url_for_match)),
                    Err(e) => failures.push(format!("{name}: {e}")),
                }
            }
            this.update(cx, |this, cx| {
                this.background_busy = false;
                if let Dialog::RoutingSettings(d) = &mut this.dialog {
                    d.apply_remote_update_results(updated.clone());
                    d.notice = if failures.is_empty() {
                        format!("Updated {} remote routing profile(s).", updated.len())
                    } else {
                        format!(
                            "Updated {}, failed {}:\n{}",
                            updated.len(),
                            failures.len(),
                            failures.join("\n")
                        )
                    };
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn fetch_remote_into_editor(&mut self, url: String, apply: bool, cx: &mut Context<Self>) {
        if self.background_busy {
            if let Dialog::RoutingSettings(d) = &mut self.dialog {
                d.notice = "Busy — wait for current job".into();
            }
            cx.notify();
            return;
        }
        self.background_busy = true;
        if let Dialog::RoutingSettings(d) = &mut self.dialog {
            d.notice = format!("Fetching {url} …");
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let body = throne_import::fetch_url(&url)?;
                    fetch_route_from_body(&body)
                })
                .await;
            this.update(cx, |this, cx| {
                this.background_busy = false;
                match result {
                    Ok(r) => {
                        if let Dialog::RoutingSettings(d) = &mut this.dialog {
                            d.apply_remote_fetch_to_editor(r, apply);
                        }
                        if apply {
                            // Simple rules reloaded into draft; push into NestedInputs on next paint.
                            this.pending_nested_simple_sync = true;
                        }
                    }
                    Err(e) => {
                        if let Dialog::RoutingSettings(d) = &mut this.dialog {
                            d.notice = format!("Route fetch failed: {e}");
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn save_tun_settings(&mut self, cx: &mut Context<Self>) {
        let (vpn_strict_route, disable_private_range_bypass) = match &self.dialog {
            Dialog::TunSettings {
                vpn_strict_route,
                disable_private_range_bypass,
                ..
            } => (*vpn_strict_route, *disable_private_range_bypass),
            _ => return,
        };
        let mtu_str = match &self.dialog_inputs {
            Some(DialogInputs::Tun { mtu }) => DialogInputs::read_string(mtu, cx),
            _ => return,
        };
        let mtu = mtu_str.parse::<i32>().unwrap_or(1500);
        self.state.apply_tun_settings(
            mtu,
            vpn_strict_route,
            disable_private_range_bypass,
            None,
        );
        self.close_dialog();
        let _ = self.persist_db();
        cx.notify();
    }

    fn save_hotkey_settings(&mut self, cx: &mut Context<Self>) {
        let Dialog::HotkeySettings {
            start_stop,
            import,
            save,
            url_test,
            copy_logs,
            ..
        } = &self.dialog
        else {
            return;
        };
        self.state.apply_hotkey_settings(
            start_stop.clone(),
            import.clone(),
            save.clone(),
            url_test.clone(),
            copy_logs.clone(),
        );
        self.close_dialog();
        let _ = self.persist_db();
        cx.notify();
    }

    fn set_hotkey_field(&mut self, field: HotkeyField, chord: String, cx: &mut Context<Self>) {
        let Dialog::HotkeySettings {
            start_stop,
            import,
            save,
            url_test,
            copy_logs,
            focus,
        } = &mut self.dialog
        else {
            return;
        };
        match field {
            HotkeyField::StartStop => {
                *start_stop = chord;
                *focus = 0;
            }
            HotkeyField::Import => {
                *import = chord;
                *focus = 1;
            }
            HotkeyField::Save => {
                *save = chord;
                *focus = 2;
            }
            HotkeyField::UrlTest => {
                *url_test = chord;
                *focus = 3;
            }
            HotkeyField::CopyLogs => {
                *copy_logs = chord;
                *focus = 4;
            }
        }
        cx.notify();
    }

    fn ip_test_selected(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        if self.background_busy {
            self.state.set_status_message("Busy — wait for current job");
            cx.notify();
            return;
        }
        let Some(id) = self.state.selected_profile_id() else {
            self.state.set_status_message("Select a profile for IP Test");
            cx.notify();
            return;
        };
        let Some(profile) = self.state.profile(id).cloned() else {
            return;
        };
        let settings = self.state.settings().clone();
        let core = Arc::clone(&self.core);
        self.background_busy = true;
        self.state
            .set_status_message(format!("IP Test · {} …", profile.name));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut guard = core.lock().map_err(|e| format!("lock: {e}"))?;
                    let rows = guard
                        .ip_test_profiles(&[&profile], &settings)
                        .map_err(|e| e.to_string())?;
                    Ok::<_, String>(rows)
                })
                .await;
            this.update(cx, |this, cx| {
                this.background_busy = false;
                match result {
                    Ok(rows) => {
                        if let Some((pid, r)) = rows.first() {
                            this.state.set_profile_ip_country(
                                *pid,
                                &r.ip,
                                &r.country_code,
                            );
                            if r.error.is_empty() {
                                this.state.set_status_message(format!(
                                    "IP Test · {} ({})",
                                    r.ip,
                                    if r.country_code.is_empty() {
                                        "?"
                                    } else {
                                        r.country_code.as_str()
                                    }
                                ));
                            } else {
                                this.state
                                    .set_status_message(format!("IP Test fail · {}", r.error));
                            }
                            let _ = this.persist_db();
                        } else {
                            this.state.set_status_message("IP Test: empty result");
                        }
                    }
                    Err(e) => this.state.set_status_message(format!("IP Test failed: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn speed_test_selected(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        if self.background_busy {
            self.state.set_status_message("Busy — wait for current job");
            cx.notify();
            return;
        }
        let selected = self.state.selected_profile_id();
        let Some(id) = selected else {
            self.state
                .set_status_message("Select a profile for Speedtest");
            cx.notify();
            return;
        };
        let profile = self.state.profile(id).cloned();
        let settings = self.state.settings().clone();
        let test_current = matches!(
            self.state.core_status(),
            CoreStatus::Running { profile_id, .. } if *profile_id == id
        );
        let core = Arc::clone(&self.core);
        self.background_busy = true;
        self.state.set_status_message("Speedtest (simple DL) …");
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut guard = core.lock().map_err(|e| format!("lock: {e}"))?;
                    guard
                        .speed_test_simple(profile.as_ref(), &settings, test_current)
                        .map_err(|e| e.to_string())
                })
                .await;
            this.update(cx, |this, cx| {
                this.background_busy = false;
                match result {
                    Ok(r) => {
                        if r.error.is_empty() {
                            this.state.set_profile_speeds(
                                id,
                                &r.dl_speed,
                                &r.ul_speed,
                                r.latency,
                            );
                            this.state.set_status_message(format!(
                                "Speedtest · ↓{} ↑{} · {} ms",
                                if r.dl_speed.is_empty() {
                                    "-"
                                } else {
                                    r.dl_speed.as_str()
                                },
                                if r.ul_speed.is_empty() {
                                    "-"
                                } else {
                                    r.ul_speed.as_str()
                                },
                                r.latency
                            ));
                            let _ = this.persist_db();
                        } else {
                            this.state
                                .set_status_message(format!("Speedtest fail · {}", r.error));
                        }
                    }
                    Err(e) => this
                        .state
                        .set_status_message(format!("Speedtest failed: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Snapshot for tray checkmarks (upstream SettingsRepo / AutoRun flags).
    pub(crate) fn tray_menu_state(&self) -> crate::tray::TrayMenuState {
        let s = self.state.settings();
        crate::tray::TrayMenuState {
            start_with_system: s.start_with_system,
            remember_last: s.remember_enable,
            allow_lan: crate::tray::allow_lan_from_address(&s.inbound_address),
            system_proxy: s.system_proxy_enabled,
            tun: s.tun_mode_enabled,
        }
    }

    fn sync_tray_menu(&self) {
        crate::tray::sync_menu_state(self.tray_menu_state());
    }

    /// Upstream tray "Select Server" — show main list (popup selector later).
    pub(crate) fn tray_select_server(&mut self, cx: &mut Context<Self>) {
        self.state
            .set_status_message_only("Select Server — pick a profile in the main window");
        cx.notify();
    }

    /// Upstream tray "Select Routing" — open Routes dialog.
    pub(crate) fn tray_select_routing(&mut self, cx: &mut Context<Self>) {
        self.open_routing_settings(None, cx);
    }

    pub(crate) fn tray_toggle_start_with_system(&mut self, cx: &mut Context<Self>) {
        let next = !self.state.settings().start_with_system;
        self.state.settings_mut().start_with_system = next;
        let _ = self.persist_db();
        // Best-effort OS registration is platform-specific; preference is always saved.
        self.state.set_status_message_only(if next {
            "Start with system: enabled (preference saved)"
        } else {
            "Start with system: disabled"
        });
        self.sync_tray_menu();
        cx.notify();
    }

    pub(crate) fn tray_toggle_remember_last(&mut self, cx: &mut Context<Self>) {
        let next = !self.state.settings().remember_enable;
        self.state.settings_mut().remember_enable = next;
        if next {
            // Capture current selection as remember_id when enabling (upstream Save).
            if let Some(id) = self.state.selected_profile_id() {
                self.state.settings_mut().remember_id = id;
            }
        }
        let _ = self.persist_db();
        self.state.set_status_message_only(if next {
            "Remember last profile: on"
        } else {
            "Remember last profile: off"
        });
        self.sync_tray_menu();
        cx.notify();
    }

    /// Upstream `actionAllow_LAN`: toggle mixed inbound between `::` (LAN) and
    /// `127.0.0.1` (loopback only), persist, refresh tray check, then if a
    /// profile is running ask-equivalent restart so the new listen takes effect.
    pub(crate) fn tray_toggle_allow_lan(&mut self, cx: &mut Context<Self>) {
        let allow = !crate::tray::allow_lan_from_address(&self.state.settings().inbound_address);
        let addr = crate::tray::inbound_address_for_allow_lan(allow).to_string();
        let port = self.state.settings().inbound_socks_port;
        // Preserve other basic settings while updating inbound address.
        let s = self.state.settings().clone();
        self.state.apply_basic_settings(
            addr,
            port,
            s.test_latency_url,
            s.remote_dns,
            s.direct_dns,
            s.log_level,
            s.ruleset_mirror,
            s.adblock_enable,
        );
        let _ = self.persist_db();
        let msg = if allow {
            "Allow other devices to connect: on (inbound ::)"
        } else {
            "Allow other devices to connect: off (127.0.0.1)"
        };
        self.sync_tray_menu();
        // Upstream UpdateSettings + "Settings changed, restart proxy?" when
        // started_id >= 0. Auto-restart here (same path as route changes).
        if self.state.core_status().is_running()
            || matches!(self.state.core_status(), CoreStatus::Starting)
            || self.core_op_busy
        {
            self.reload_core_for_route_change(msg, cx);
        } else {
            self.state.set_status_message_only(msg);
            cx.notify();
        }
    }

    pub(crate) fn tray_set_system_proxy(&mut self, on: bool, cx: &mut Context<Self>) {
        self.set_sys_proxy(on, cx);
        self.sync_tray_menu();
    }

    pub(crate) fn tray_set_tun(&mut self, on: bool, cx: &mut Context<Self>) {
        self.set_vpn(on, cx);
        self.sync_tray_menu();
    }

    pub(crate) fn tray_disable_spmode(&mut self, cx: &mut Context<Self>) {
        // Upstream menu_spmode_disabled: both off.
        self.set_sys_proxy(false, cx);
        self.set_vpn(false, cx);
        self.sync_tray_menu();
    }

    /// Upstream `actionRestart_Proxy` / "Restart Core" — stop + kill core process.
    pub(crate) fn tray_restart_core(&mut self, cx: &mut Context<Self>) {
        self.state
            .set_status_message_only("Restart Core — stopping…");
        if should_queue_recovery_restart(self.network_recovery_busy) {
            self.restart_when_idle = true;
            self.state
                .set_status_message_only("Restart Core queued — restoring network first…");
            self.sync_tray_menu();
            cx.notify();
            return;
        }
        if self.state.core_status().is_running()
            || matches!(self.state.core_status(), CoreStatus::Starting)
        {
            self.restart_when_idle = true;
            self.stop_proxy(cx);
        } else {
            // Not running — just clear status.
            self.state
                .set_status_message_only("Restart Core — core was not running");
            cx.notify();
        }
        self.sync_tray_menu();
    }

    pub(crate) fn toggle_proxy(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        if self.core_op_busy {
            if should_queue_recovery_restart(self.network_recovery_busy) {
                self.restart_when_idle = true;
                self.state
                    .set_status_message_only("Start queued — restoring network first…");
            } else if matches!(self.state.core_status(), CoreStatus::Starting) {
                // Cancel in-flight start → stop when it finishes.
                self.pending_stop = true;
                self.restart_when_idle = false;
                self.pending_profile_switch.clear();
                self.state
                    .set_status_message_only("Stop queued — finishing current start first…");
            } else {
                self.state
                    .set_status_message_only("Core is busy (start/stop in progress)…");
            }
            cx.notify();
            return;
        }
        if self.state.core_status().is_running()
            || matches!(self.state.core_status(), CoreStatus::Starting)
        {
            if matches!(self.state.core_status(), CoreStatus::Stopping) {
                return;
            }
            self.pending_profile_switch.clear();
            self.pending_stop = false;
            self.stop_proxy(cx);
        } else {
            self.start_proxy(cx);
        }
    }

    fn activate_profile(&mut self, profile_id: ProfileId, cx: &mut Context<Self>) {
        match next_core_action(self.state.core_status(), profile_id) {
            CoreAction::Switch(profile_id) => self.switch_profile(profile_id, cx),
            CoreAction::Start | CoreAction::Stop => self.toggle_proxy(cx),
        }
    }

    fn switch_profile(&mut self, profile_id: ProfileId, cx: &mut Context<Self>) {
        // Highlight the node immediately so rapid clicks feel responsive.
        let _ = self.state.select_profile(profile_id);
        let name = self
            .state
            .profile(profile_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| format!("#{profile_id}"));

        // Always keep the latest desired node (overwrite prior queue entry).
        let kick_stop = self.pending_profile_switch.schedule(profile_id);
        self.pending_stop = false;
        self.restart_when_idle = false;

        if self.core_op_busy {
            self.state
                .set_status_message_only(format!("Switch queued → {name}"));
            cx.notify();
            return;
        }

        if self.state.core_status().is_running()
            || matches!(self.state.core_status(), CoreStatus::Starting)
        {
            if kick_stop || matches!(self.state.core_status(), CoreStatus::Running { .. }) {
                self.state
                    .set_status_message_only(format!("Switching → {name}…"));
                self.stop_proxy(cx);
            }
        } else {
            // Idle: start the queued profile directly.
            let _ = self.pending_profile_switch.take();
            self.start_proxy(cx);
        }
        cx.notify();
    }

    fn start_proxy(&mut self, cx: &mut Context<Self>) {
        if self.core_op_busy {
            // Tun/Proxy mode changes during busy start/stop → restart once idle.
            self.restart_when_idle = true;
            return;
        }
        // Prefer an explicit switch target over whatever row is selected.
        if let Some(id) = self.pending_profile_switch.take() {
            let _ = self.state.select_profile(id);
        }
        let Some(id) = self.state.selected_profile_id() else {
            self.state
                .set_status_message_only("Select a profile before Start");
            cx.notify();
            return;
        };
        let Some(profile) = self.state.profile(id).cloned() else {
            self.state.set_status_message_only("Profile not found");
            cx.notify();
            return;
        };

        self.core_op_busy = true;
        self.pending_stop = false;
        (self.runtime_generation, self.runtime_poll_failures) =
            next_runtime_generation(self.runtime_generation);
        self.state.set_core_status(CoreStatus::Starting);
        let profile_display = runtime_profile_display(profile.profile_type, &profile.name);
        self.running_profile_display = Some(profile_display.clone());
        self.state.push_log(start_profile_log(&profile_display));
        self.state
            .set_status_message_only(format!("Starting {} …", profile.name));
        cx.notify();

        // Snapshot settings at kickoff; re-read proxy/tun flags after Start for apply.
        let settings = self.state.settings().clone();
        let route = self.state.active_route().cloned();
        let core = Arc::clone(&self.core);
        let profile_name = profile.name.clone();
        let profile_id = profile.id;
        let port = settings.inbound_socks_port;
        let addr = settings.inbound_address.clone();
        let route_label = route
            .as_ref()
            .map(|r| r.name.clone())
            .unwrap_or_else(|| "default".into());

        // Upstream 1.2.3 Auto Selector: resolve members before Start.
        let auto_build = if profile.profile_type == ProfileType::AutoSelector {
            match self.state.resolve_auto_selector_members(profile_id) {
                Ok((mut cfg, plan, members)) => {
                    self.state.push_log(format!(
                        "[Auto selector] {} · starting with {} member(s)",
                        plan.summary(),
                        members.len()
                    ));
                    cfg.last_built = plan.build.clone();
                    cfg.last_built_at = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    cfg.pool = plan.pool.clone();
                    // Persist ranking / last_built into the selector profile.
                    let _ = self.state.update_profile_outbound_json(
                        profile_id,
                        cfg.to_outbound_json(),
                    );
                    Some(throne_core_client::AutoSelectorBuild {
                        config: cfg,
                        members,
                    })
                }
                Err(e) => {
                    self.core_op_busy = false;
                    self.running_profile_display = None;
                    self.state.set_core_status(CoreStatus::Error(e.to_string()));
                    self.state
                        .set_status_message_only(format!("Auto Selector: {e}"));
                    self.state
                        .push_log(failed_start_profile_log(&profile_display));
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let mut guard = core
                        .lock()
                        .map_err(|e| format!("core lock poisoned: {e}"))?;
                    // Always pass apply_system_proxy=false here; we apply from
                    // live settings after Start so mid-start toggles win.
                    guard
                        .start_profile_ex(
                            &profile,
                            &settings,
                            route.as_ref(),
                            auto_build.as_ref(),
                            false,
                        )
                        .map_err(|e| e.to_string())
                })
                .await;

            this.update(cx, |this, cx| {
                this.core_op_busy = false;
                if let Err(e) = &result {
                    this.state
                        .push_log(failed_start_profile_log(&profile_display));
                    this.state
                        .set_status_message_only(format!("Start failed: {e}"));
                }

                // 1) User asked to stop while we were starting.
                if this.pending_stop {
                    this.pending_stop = false;
                    this.restart_when_idle = false;
                    match &result {
                        Ok(()) => {
                            this.state.set_core_status(CoreStatus::Running {
                                profile_id,
                                profile_name: profile_name.clone(),
                            });
                        }
                        Err(e) => {
                            this.state.set_core_status(CoreStatus::Error(e.clone()));
                        }
                    }
                    this.stop_proxy(cx);
                    return;
                }

                // 2) User clicked another node while starting — switch to latest.
                if let Some(want) = this.pending_profile_switch.peek() {
                    if want != profile_id {
                        if result.is_ok() {
                            this.state.set_core_status(CoreStatus::Running {
                                profile_id,
                                profile_name: profile_name.clone(),
                            });
                            this.stop_proxy(cx); // stop complete → starts `want`
                        } else {
                            // Start failed; just start the desired node.
                            let want = this.pending_profile_switch.take().unwrap_or(want);
                            let _ = this.state.select_profile(want);
                            this.state.set_core_status(CoreStatus::Stopped);
                            this.start_proxy(cx);
                        }
                        return;
                    }
                    this.pending_profile_switch.clear();
                }

                match result {
                    Ok(()) => {
                        this.state.set_core_status(CoreStatus::Running {
                            profile_id,
                            profile_name: profile_name.clone(),
                        });
                        // Apply system proxy from *current* settings (not kickoff snapshot).
                        let live = this.state.settings();
                        let tun = live.tun_mode_enabled;
                        let proxy_on = live.system_proxy_enabled;
                        if proxy_on {
                            let host = throne_core_client::proxy_client_host(&live.inbound_address);
                            let port = live.inbound_socks_port;
                            cx.background_spawn(async move {
                                if let Err(e) =
                                    throne_core_client::set_system_proxy(true, &host, port)
                                {
                                    tracing::warn!(%e, "system proxy enable after Start failed");
                                }
                            })
                            .detach();
                        }
                        let show_addr = throne_core_client::proxy_client_host(&addr);
                        let marker = running_mode_marker(tun, proxy_on);
                        let mut msg = format!(
                            "Running · {profile_name} · route {route_label} · mixed {show_addr}:{port}"
                        );
                        if !marker.is_empty() {
                            msg.push_str(" · ");
                            msg.push_str(marker);
                        }
                        this.state.set_status_message_only(msg);
                        this.snapshot_running_profile_traffic_meta(profile_id);
                        let _ = this.persist_db();
                        // Flush any Start-time core lines (sing-box boot + first dials).
                        let _ = this.drain_core_logs_into_ui();
                    }
                    Err(e) => {
                        this.state.set_core_status(CoreStatus::Error(e.clone()));
                        this.state
                            .set_status_message_only(format!("Start failed: {e}"));
                        // Surface core decode / panic lines even on failed Start.
                        let _ = this.drain_core_logs_into_ui();
                    }
                }
                if this.restart_when_idle {
                    this.restart_when_idle = false;
                    // Mode changed mid-start — bounce with latest settings.
                    if this.state.core_status().is_running() {
                        this.pending_profile_switch.set(profile_id);
                        this.stop_proxy(cx);
                    } else {
                        this.start_proxy(cx);
                    }
                    return;
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn stop_proxy(&mut self, cx: &mut Context<Self>) {
        if self.core_op_busy {
            if matches!(self.state.core_status(), CoreStatus::Stopping) {
                return;
            }
            // Start still running — queue stop (or keep switch target).
            if self.pending_profile_switch.peek().is_none() {
                self.pending_stop = true;
                self.state
                    .set_status_message_only("Stop queued — finishing current op…");
            }
            cx.notify();
            return;
        }
        let stop_profile_id = match self.state.core_status() {
            CoreStatus::Running { profile_id, .. } => Some(*profile_id),
            _ => self.state.selected_profile_id(),
        };
        let current_profile_display = stop_profile_id
            .and_then(|profile_id| self.state.profile(profile_id))
            .map(|profile| runtime_profile_display(profile.profile_type, &profile.name));
        let stop_profile_display = resolve_stop_profile_display(
            self.running_profile_display.as_ref(),
            current_profile_display,
        );
        self.core_op_busy = true;
        self.pending_stop = false;
        (self.runtime_generation, self.runtime_poll_failures) =
            next_runtime_generation(self.runtime_generation);
        if let Some(profile_display) = stop_profile_display {
            self.state.push_log(stop_profile_log(&profile_display));
        }
        self.state.set_core_status(CoreStatus::Stopping);
        self.state.set_status_message_only("Stopping…");
        cx.notify();

        let settings = self.state.settings().clone();
        let core = Arc::clone(&self.core);

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let mut guard = core
                        .lock()
                        .map_err(|e| format!("core lock poisoned: {e}"))?;
                    guard.stop_profile(&settings).map_err(|e| e.to_string())
                })
                .await;

            this.update(cx, |this, cx| {
                this.core_op_busy = false;
                let switch_target = this.pending_profile_switch.take();
                // Always mark stopped locally — stop_profile force-kills core.
                this.state.set_core_status(CoreStatus::Stopped);
                this.prev_traffic_at = None;
                if let Some(db) = this.traffic_stats_db.clone() {
                    let _ = this.traffic_stats_mgr.flush(&db);
                }
                // Keep graph history after stop so user can still inspect it;
                // clear only on health-fail recovery. (Upstream SpeedWidget keeps
                // history until Clear is called.)
                match result {
                    Ok(()) => this.state.set_status_message_only("Core stopped"),
                    Err(e) => {
                        this.state.push_log(FAILED_STOP_PROFILE_LOG);
                        this.state
                            .set_status_message_only(format!("Stopped (with errors): {e}"));
                    }
                }
                this.running_profile_display = None;
                if this.pending_stop {
                    this.pending_stop = false;
                    // Already stopped.
                }
                if this.restart_when_idle {
                    this.restart_when_idle = false;
                    this.start_proxy(cx);
                    cx.notify();
                    return;
                }
                if let Some(profile_id) = switch_target {
                    if this.state.select_profile(profile_id).is_ok() {
                        this.start_proxy(cx);
                        return;
                    }
                    this.state.set_core_status(CoreStatus::Error(
                        "Selected profile was removed while switching".into(),
                    ));
                    this.state
                        .set_status_message_only("Switch failed: selected profile was removed");
                }
                let _ = this.persist_db();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn select_group(&mut self, id: GroupId, cx: &mut Context<Self>) {
        let _ = self.state.set_active_group(id);
        cx.notify();
    }

    fn import_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if text.trim().is_empty() {
            self.state
                .set_status_message("Import: empty input");
            cx.notify();
            return;
        }
        let report = throne_import::import_text(text);
        let insecure_flag = self.state.settings().show_config_security;
        let items = report.profiles.into_iter().map(|p| {
            let insecure = insecure_flag
                && (p.outbound.insecure == Some(true)
                    || p.outbound.tls == Some(false)
                    || p.source.contains("insecure=1"));
            (p.name, p.profile_type, p.outbound, insecure)
        });
        let n = self.state.import_profiles(items);
        let nr = self.state.import_routes(report.routes);
        let mut msg = if n + nr > 0 {
            format!("Added {n} profile(s), {nr} route(s)")
        } else {
            report
                .errors
                .first()
                .cloned()
                .unwrap_or_else(|| "Nothing imported".into())
        };
        if let Some(url) = report.pending_sub_url {
            msg.push_str(&format!(" · subscription URL pending: {url}"));
        }
        self.state.set_status_message(msg);
        if n > 0 || nr > 0 {
            let _ = self.persist_db();
        }
        cx.notify();
    }

    fn import_clipboard(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        let text = std::env::var("THRONE_IMPORT")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(read_os_clipboard)
            .unwrap_or_default();

        if text.trim().is_empty() {
            self.state.set_status_message(
                "Add profile from clipboard: clipboard empty (or set THRONE_IMPORT)",
            );
            cx.notify();
            return;
        }
        self.import_text(&text, cx);
    }

    fn persist_db(&mut self) -> Result<(), String> {
        // Full rewrite save requires every profile.gid to exist in groups.
        self.state.sanitize_group_profile_refs();
        let path = if self.db_path_label.is_empty() {
            throne_storage::default_db_path()
        } else {
            std::path::PathBuf::from(&self.db_path_label)
        };
        let db = throne_storage::Database::open(&path).map_err(|e| e.to_string())?;
        db.save_state(&self.state).map_err(|e| e.to_string())?;
        self.db_path_label = path.display().to_string();
        Ok(())
    }

    fn save_db(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        match self.persist_db() {
            Ok(()) => {
                self.state
                    .set_status_message(format!("Saved · {}", self.db_path_label));
                cx.notify();
            }
            Err(e) => {
                self.state.set_status_message(format!("Save failed: {e}"));
                cx.notify();
            }
        }
    }

    fn delete_selected(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.dialog, Dialog::None) {
            // dialog handles backspace as text edit
            return;
        }
        self.close_menus();
        if let Some(id) = self.state.selected_profile_id() {
            self.state.delete_selected_profiles(&[id]);
            self.state.set_status_message("Deleted");
            let _ = self.persist_db();
        }
        cx.notify();
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.state.visible_profiles().first() {
            let id = p.id;
            let _ = self.state.select_profile(id);
        }
        cx.notify();
    }

    fn url_test_selected(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        if self.background_busy {
            self.state.set_status_message("Busy — wait for current job");
            cx.notify();
            return;
        }
        let selected = self.state.selected_profile_id();
        let Some(id) = selected else {
            self.state.set_status_message("Select a profile to URL Test");
            cx.notify();
            return;
        };
        let Some(profile) = self.state.profile(id).cloned() else {
            self.state.set_status_message("Profile not found");
            cx.notify();
            return;
        };
        let settings = self.state.settings().clone();
        let running_id = self
            .state
            .core_status()
            .is_running()
            .then(|| match self.state.core_status() {
                CoreStatus::Running { profile_id, .. } => Some(*profile_id),
                _ => None,
            })
            .flatten();
        let test_current = running_id == Some(id);
        let url = settings.test_latency_url.clone();
        let core = Arc::clone(&self.core);
        self.background_busy = true;
        self.state
            .set_status_message(format!("URL Test · {} …", profile.name));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut guard = core
                        .lock()
                        .map_err(|e| format!("core lock: {e}"))?;
                    if test_current {
                        let r = guard
                            .url_test_current(&url, 8000)
                            .map_err(|e| e.to_string())?;
                        Ok::<Vec<(i64, i32, String)>, String>(vec![(
                            id,
                            r.latency_ms,
                            r.error,
                        )])
                    } else {
                        let refs: Vec<&Profile> = vec![&profile];
                        let rows = guard
                            .url_test_profiles(&refs, &settings)
                            .map_err(|e| e.to_string())?;
                        Ok(rows
                            .into_iter()
                            .map(|(pid, r)| (pid, r.latency_ms, r.error))
                            .collect())
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                this.background_busy = false;
                match result {
                    Ok(rows) => {
                        let apply: Vec<(i64, i32, &str)> = rows
                            .iter()
                            .map(|(a, b, c)| (*a, *b, c.as_str()))
                            .collect();
                        this.state.apply_url_test_results(&apply);
                        if let Some((_, lat, err)) = rows.first() {
                            if err.is_empty() && *lat > 0 {
                                this.state
                                    .set_status_message(format!("URL Test OK · {lat} ms"));
                            } else {
                                this.state.set_status_message(format!(
                                    "URL Test fail · {}",
                                    if err.is_empty() { "timeout/error" } else { err }
                                ));
                            }
                        }
                        let _ = this.persist_db();
                    }
                    Err(e) => this.state.set_status_message(format!("URL Test failed: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn url_test_group(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        if self.background_busy {
            self.state.set_status_message("Busy — wait for current job");
            cx.notify();
            return;
        }
        let profiles: Vec<Profile> = self
            .state
            .all_profiles()
            .into_iter()
            .filter(|profile| profile.group_id == self.state.active_group_id())
            .cloned()
            .collect();
        if profiles.is_empty() {
            self.state.set_status_message("No profiles in group to test");
            cx.notify();
            return;
        }
        let settings = self.state.settings().clone();
        let n = profiles.len();
        let core = Arc::clone(&self.core);
        self.background_busy = true;
        // Upstream `dataViewHtmlGenerator_.seedLatencyTest(Url, size)` + data_view.
        self.test_progress = Some(TestProgressPanel {
            kind: TestProgressKind::Url,
            done: 0,
            total: n,
        });
        self.state
            .set_status_message(format!("URL Test group · {n} profile(s) …"));
        cx.notify();
        let chunks: Vec<Vec<Profile>> = profiles.chunks(16).map(|c| c.to_vec()).collect();
        cx.spawn(async move |this, cx| {
            let mut total_ok = 0usize;
            let mut total_updated = 0usize;
            let mut done = 0usize;
            let mut fatal: Option<String> = None;

            for chunk in chunks {
                let chunk_len = chunk.len();
                let core = Arc::clone(&core);
                let settings = settings.clone();
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        let mut guard = core
                            .lock()
                            .map_err(|e| format!("core lock: {e}"))?;
                        let refs: Vec<&Profile> = chunk.iter().collect();
                        let rows = guard
                            .url_test_profiles(&refs, &settings)
                            .map_err(|e| e.to_string())?;
                        Ok::<Vec<(i64, i32, String)>, String>(
                            rows.into_iter()
                                .map(|(pid, r)| (pid, r.latency_ms, r.error))
                                .collect(),
                        )
                    })
                    .await;

                match result {
                    Ok(rows) => {
                        let apply: Vec<(i64, i32, &str)> = rows
                            .iter()
                            .map(|(a, b, c)| (*a, *b, c.as_str()))
                            .collect();
                        let ok = rows
                            .iter()
                            .filter(|(_, l, e)| e.is_empty() && *l > 0)
                            .count();
                        done = (done + chunk_len).min(n);
                        let _ = this.update(cx, |this, cx| {
                            total_updated += this.state.apply_url_test_results(&apply);
                            total_ok += ok;
                            if let Some(panel) = this.test_progress.as_mut() {
                                panel.done = done;
                            }
                            // Intermediate status stays in the top-right panel;
                            // avoid flooding logs with per-chunk progress lines.
                            cx.notify();
                        });
                    }
                    Err(e) => {
                        fatal = Some(e);
                        break;
                    }
                }
            }

            this.update(cx, |this, cx| {
                this.background_busy = false;
                this.test_progress = None;
                if let Some(e) = fatal {
                    this.state
                        .set_status_message(format!("URL Test failed: {e}"));
                    // Keep any completed chunk results from earlier batches.
                    if total_updated > 0 {
                        let _ = this.persist_db();
                    }
                } else {
                    this.state.set_status_message(format!(
                        "URL Test done · {total_ok}/{total_updated} ok"
                    ));
                    let _ = this.persist_db();
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn delete_unavailable(&mut self, window: Option<&mut Window>, cx: &mut Context<Self>) {
        self.close_menus();
        let group_id = self.state.active_group_id();
        let count = self.state.unavailable_profile_ids_in_group(group_id).len();
        if count == 0 {
            self.state
                .set_status_message("No unavailable profiles to delete");
        } else {
            self.dialog = Dialog::ConfirmDeleteUnavailable { group_id, count };
            self.dialog_inputs = None;
            if let Some(window) = window {
                self.present_gpui_dialog(window, cx);
            } else {
                self.request_gpui_dialog();
            }
        }
        cx.notify();
    }

    fn confirm_delete_unavailable(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Dialog::ConfirmDeleteUnavailable { group_id, .. } = &self.dialog else {
            return;
        };
        let removed = self.state.remove_unavailable_in_group(*group_id);
        self.close_dialog_with_window(window, cx);
        self.state
            .set_status_message(format!("Deleted {removed} unavailable profile(s)"));
        if removed > 0 {
            let _ = self.persist_db();
        }
        cx.notify();
    }

    fn update_subscription(
        &mut self,
        all: bool,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        self.close_menus();
        if all {
            if self.subscription_queue.is_some() || self.background_busy {
                self.state
                    .push_log("The last subscription update has not exited.");
            } else {
                self.dialog = Dialog::ConfirmUpdateAllSubscriptions;
                self.dialog_inputs = None;
                if let Some(window) = window {
                    self.present_gpui_dialog(window, cx);
                } else {
                    self.request_gpui_dialog();
                }
            }
            cx.notify();
            return;
        }
        self.start_subscription_group(self.state.active_group_id(), UpdateOrigin::Manual, cx);
    }

    fn confirm_update_all_subscriptions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids = eligible_subscription_ids(self.state.all_groups());
        // Restore Manage Groups under a fresh gpui dialog.
        self.dialog = Dialog::manage_groups_from_state(&self.state);
        self.dialog_inputs = None;
        self.present_gpui_dialog(window, cx);
        if ids.is_empty() {
            self.state.set_status_message("No subscriptions to update");
            cx.notify();
            return;
        }
        self.subscription_queue = Some(SubscriptionUpdateQueue::new(ids));
        self.start_next_subscription_update(cx);
    }

    fn start_next_subscription_update(&mut self, cx: &mut Context<Self>) {
        let next = self
            .subscription_queue
            .as_mut()
            .and_then(SubscriptionUpdateQueue::take_next);
        if let Some(group_id) = next {
            self.start_subscription_group(group_id, UpdateOrigin::UpdateAll, cx);
        } else {
            let message = self
                .subscription_queue
                .as_ref()
                .map(SubscriptionUpdateQueue::completion_message)
                .unwrap_or_else(|| "Subscription update finished".into());
            self.subscription_queue = None;
            self.background_busy = false;
            self.state.set_status_message(message);
            cx.notify();
        }
    }

    fn start_subscription_group(
        &mut self,
        group_id: GroupId,
        origin: UpdateOrigin,
        cx: &mut Context<Self>,
    ) {
        if self.background_busy {
            self.state.set_status_message("Busy — wait for current job");
            cx.notify();
            return;
        }
        let Some(group) = self.state.group(group_id) else {
            self.state.set_status_message("Subscription group not found");
            cx.notify();
            return;
        };
        if group.url.trim().is_empty() || group.archive {
            self.state
                .set_status_message("This group has no updatable subscription");
            if origin == UpdateOrigin::UpdateAll {
                if let Some(queue) = self.subscription_queue.as_mut() {
                    queue.record_result(false);
                }
                self.start_next_subscription_update(cx);
            }
            return;
        }
        let name = group.name.clone();
        let url = group.url.clone();
        let options = match subscription_fetch_options(self.state.settings(), self.state.core_status()) {
            Ok(options) => options,
            Err(error) => {
                self.state.set_status_message(format!("{name}: {error}"));
                if origin == UpdateOrigin::UpdateAll {
                    if let Some(queue) = self.subscription_queue.as_mut() {
                        queue.record_result(false);
                    }
                    self.start_next_subscription_update(cx);
                }
                return;
            }
        };
        self.background_busy = true;
        self.state
            .push_log(format!(">>>>>>>> Requesting subscription: {name}"));
        self.state
            .set_status_message(format!("Updating subscription {name} …"));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let response = fetch_url_with_options(
                        &url,
                        std::time::Duration::from_secs(30),
                        &options,
                    )?;
                    import_subscription_response(response)
                })
                .await;
            this.update(cx, |this, cx| {
                this.background_busy = false;
                let mut succeeded = false;
                match result {
                    Ok(imported) => {
                        this.state.push_log(format!(
                            "<<<<<<<< Subscription request finished: {name}"
                        ));
                        this.state
                            .push_log(">>>>>>>> Processing subscription data...");
                        let show_security = this.state.settings().show_config_security;
                        let items = imported
                            .report
                            .profiles
                            .into_iter()
                            .map(|profile| {
                                let insecure = show_security
                                    && (profile.outbound.insecure == Some(true)
                                        || profile.outbound.tls == Some(false)
                                        || profile.source.contains("insecure=1"));
                                (profile.name, profile.profile_type, profile.outbound, insecure)
                            })
                            .collect();
                        this.state
                            .push_log(">>>>>>>> Process complete, applying...");
                        let previous_state = this.state.clone();
                        match this.state.apply_subscription_snapshot(
                            group_id,
                            items,
                            imported.user_info.unwrap_or_default(),
                            chrono::Utc::now().timestamp(),
                        ) {
                            Ok(report) => {
                                let body = format_subscription_changes(&report);
                                match this.persist_db() {
                                    Ok(()) => {
                                        succeeded = true;
                                        this.state.push_log(format!(
                                            "<<<<<<<< Change of {name}:\n{body}"
                                        ));
                                        this.state.set_status_message(format!(
                                            "Subscription updated · {} profile(s) · +{} ~{} −{} · {} kept",
                                            report.result_order.len(),
                                            report.added.len(),
                                            report.updated.len(),
                                            report.deleted.len(),
                                            report.unchanged,
                                        ));
                                        if should_show_subscription_diff(
                                            origin,
                                            this.state.settings().sub_show_change_popup,
                                        ) {
                                            this.dialog = Dialog::SubscriptionDiff {
                                                title: format!("Change of {name}"),
                                                body,
                                            };
                                            this.dialog_inputs = None;
                                            this.request_gpui_dialog();
                                        }
                                    }
                                    Err(error) => {
                                        this.state = previous_state;
                                        this.state.set_status_message(format!(
                                            "Subscription update {name} failed to persist: {error}"
                                        ));
                                    }
                                }
                            }
                            Err(error) => this.state.set_status_message(format!(
                                "Subscription update {name} failed: {error}"
                            )),
                        }
                    }
                    Err(error) => {
                        this.state.push_log(format!(
                            "<<<<<<<< Requesting subscription {name} error: {error}"
                        ));
                        this.state.set_status_message(format!(
                            "Subscription update {name} failed: {error}"
                        ));
                    }
                }
                if origin == UpdateOrigin::UpdateAll {
                    if let Some(queue) = this.subscription_queue.as_mut() {
                        queue.record_result(succeeded);
                    }
                    this.start_next_subscription_update(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Move buffered ThroneCore stdout/stderr lines into the Logs panel.
    /// Returns true when at least one line was appended.
    fn drain_core_logs_into_ui(&mut self) -> bool {
        let lines = match self.core.lock() {
            Ok(guard) => guard.take_core_logs(),
            Err(_) => return false,
        };
        if lines.is_empty() {
            return false;
        }
        for line in lines {
            self.state.push_log_only(line);
        }
        true
    }

    fn poll_core_runtime(&mut self, cx: &mut Context<Self>) {
        // Always pull core stdout/stderr so Logs shows inbound/outbound traffic
        // like upstream, and so pipe buffers cannot block ThroneCore.
        let logs_changed = self.drain_core_logs_into_ui();

        if !self.state.core_status().is_running() || self.core_op_busy || self.runtime_poll_busy {
            if logs_changed {
                cx.notify();
            }
            return;
        }
        self.runtime_poll_busy = true;
        let poll_generation = self.runtime_generation;
        let core = Arc::clone(&self.core);
        let want_conn = self.bottom_tab == 1;
        let want_auto = matches!(self.dialog, Dialog::AutoSelectorStats { .. });
        cx.spawn(async move |this, cx| {
            let snap = cx
                .background_executor()
                .spawn(async move {
                    let mut guard = core
                        .lock()
                        .map_err(|_| "core session lock poisoned".to_string())?;
                    // Drain logs under the same lock so lines stay ordered with stats.
                    let core_logs = guard.take_core_logs();
                    let stats = guard.query_stats().map_err(|error| error.to_string())?;
                    let conns = if want_conn {
                        guard.query_connections().unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    let auto = if want_auto {
                        guard.query_auto_selectors().unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    Ok::<_, String>((stats, conns, core_logs, auto))
                })
                .await;
            this.update(cx, |this, cx| {
                this.runtime_poll_busy = false;
                if !runtime_poll_is_current(poll_generation, this.runtime_generation) {
                    return;
                }
                let (failures, recover_network) =
                    runtime_poll_health(this.runtime_poll_failures, snap.is_ok());
                this.runtime_poll_failures = failures;
                match snap {
                    Ok((cum, conns, core_logs, auto)) => {
                        let mut logs_changed = false;
                        for line in core_logs {
                            this.state.push_log_only(line);
                            logs_changed = true;
                        }
                        let now = std::time::Instant::now();
                        let had_prev_sample = this.prev_traffic_at.is_some();
                        let rates = if let Some(prev_at) = this.prev_traffic_at {
                            let dt = now.duration_since(prev_at).as_secs_f64().max(0.4);
                            let rate = |bytes: i64| (bytes.max(0) as f64 / dt) as i64;
                            TrafficSnapshot {
                                proxy_up: rate(cum.proxy_up),
                                proxy_down: rate(cum.proxy_down),
                                direct_up: rate(cum.direct_up),
                                direct_down: rate(cum.direct_down),
                            }
                        } else {
                            TrafficSnapshot::default()
                        };
                        this.prev_traffic_at = Some(now);
                        // Upstream SpeedWidget: push one point per poll after the first baseline.
                        if had_prev_sample {
                            this.speed_graph.push(rates.clone());
                        }
                        let traffic_changed = this.state.update_live_traffic(rates);
                        let running_id = match this.state.core_status() {
                            CoreStatus::Running { profile_id, .. } => Some(*profile_id),
                            _ => None,
                        };
                        if let Some(profile_id) = running_id {
                            this.state.set_profile_traffic(
                                profile_id,
                                cum.proxy_down,
                                cum.proxy_up,
                            );
                            // Historical stats: QueryStats values are per-interval deltas.
                            if let Some(db) = this.traffic_stats_db.clone() {
                                let now_secs = chrono::Local::now().timestamp();
                                let _ = this.traffic_stats_mgr.add_config_delta(
                                    &db,
                                    profile_id,
                                    cum.proxy_up,
                                    cum.proxy_down,
                                    now_secs,
                                );
                                let _ = this.traffic_stats_mgr.add_config_delta(
                                    &db,
                                    throne_storage::DIRECT_STAT_PROFILE_ID,
                                    cum.direct_up,
                                    cum.direct_down,
                                    now_secs,
                                );
                            }
                        }
                        if want_conn {
                            this.connections = conns;
                        }
                        let mut auto_changed = false;
                        if want_auto && matches!(this.dialog, Dialog::AutoSelectorStats { .. }) {
                            let mut next: Vec<_> = auto
                                .into_iter()
                                .map(auto_selector_group_view)
                                .collect();
                            enrich_auto_selector_names(&this.state, &mut next);
                            if next != this.auto_selector_snapshot {
                                this.auto_selector_snapshot = next;
                                auto_changed = true;
                                // Refresh dialog body with new rows.
                                this.presented_dialog_stack = (0, NestedKind::None);
                                this.request_gpui_dialog();
                            }
                        }
                        let graph_tab = this.bottom_tab == 2;
                        if traffic_changed || want_conn || logs_changed || auto_changed || graph_tab
                        {
                            cx.notify();
                        }
                    }
                    Err(error) if recover_network => {
                        (this.runtime_generation, this.runtime_poll_failures) =
                            next_runtime_generation(this.runtime_generation);
                        this.core_op_busy = true;
                        this.network_recovery_busy = true;
                        this.prev_traffic_at = None;
                        this.speed_graph.clear();
                        this.connections.clear();
                        this.state.set_core_status(CoreStatus::Error(format!(
                            "Core health check failed: {error}"
                        )));
                        this.state.push_log(format!(
                            "Core health check failed 3 times; clearing system proxy: {error}"
                        ));
                        cx.spawn(async move |this, cx| {
                            cx.background_spawn(async move {
                                force_clear_system_proxy();
                                let _ = throne_core_client::set_tun_system_dns(false, "");
                            })
                            .await;
                            this.update(cx, |this, cx| {
                                this.network_recovery_busy = false;
                                this.core_op_busy = false;
                                if this.pending_stop {
                                    this.pending_stop = false;
                                    this.stop_proxy(cx);
                                } else if this.restart_when_idle {
                                    this.restart_when_idle = false;
                                    this.start_proxy(cx);
                                } else {
                                    cx.notify();
                                }
                            })
                            .ok();
                        })
                        .detach();
                        cx.notify();
                    }
                    Err(_) => {}
                }
            })
            .ok();
        })
        .detach();
    }

    fn cycle_route(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        let prev = self.state.active_route().map(|r| r.id);
        self.state.cycle_active_route();
        let _ = self.persist_db();
        let next = self.state.active_route().map(|r| r.id);
        if prev != next {
            let label = self
                .state
                .active_route()
                .map(|r| r.name.clone())
                .unwrap_or_else(|| "route".into());
            self.reload_core_for_route_change(&format!("Active route · {label}"), cx);
        } else {
            cx.notify();
        }
    }

    fn select_active_route(&mut self, id: i64, cx: &mut Context<Self>) {
        let prev = self.state.active_route().map(|r| r.id);
        if prev == Some(id) {
            self.close_menus();
            cx.notify();
            return;
        }
        if self.state.set_active_route(id).is_err() {
            self.close_menus();
            cx.notify();
            return;
        }
        let _ = self.persist_db();
        self.close_menus();
        let label = self
            .state
            .active_route()
            .map(|r| r.name.clone())
            .unwrap_or_else(|| "route".into());
        self.reload_core_for_route_change(&format!("Active route · {label}"), cx);
    }

    fn set_vpn(&mut self, on: bool, cx: &mut Context<Self>) {
        // Upstream `MainWindow::set_spmode_vpn`:
        //   if (enable == spmode_vpn) return;
        //   if (enable) {
        //     if (!IsAdmin()) {
        //       if (!get_elevated_permissions()) { refresh_status(); return; }
        //     }
        //   }
        //   spmode_vpn = enable;
        //   if (started_id >= 0) profile_start(...);
        if on == self.state.settings().tun_mode_enabled {
            return;
        }

        if on {
            let core = Arc::clone(&self.core);
            self.state
                .set_status_message_only("Checking Tun privileges…");
            cx.notify();
            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_spawn(async move {
                        let mut guard = core
                            .lock()
                            .map_err(|e| format!("core lock poisoned: {e}"))?;
                        // IsAdmin || get_elevated_permissions
                        guard
                            .request_tun_privileges_for_toggle()
                            .map_err(|e| e.to_string())
                    })
                    .await;
                this.update(cx, |this, cx| {
                    match result {
                        Ok(true) => {
                            this.state.set_spmode_vpn(true);
                            let _ = this.persist_db();
                            // Upstream: if started_id >= 0 → profile_start (rebuild with tun-in).
                            // Tun checkbox alone does nothing until Start applies the inbound.
                            if this.state.core_status().is_running()
                                || matches!(this.state.core_status(), CoreStatus::Starting)
                            {
                                this.state.set_status_message_only(
                                    "Tun Mode enabled — restarting profile with TUN…",
                                );
                                this.start_proxy(cx);
                            } else {
                                this.state.set_status_message_only(
                                    "Tun Mode enabled — press Start to apply system-wide TUN",
                                );
                            }
                        }
                        Ok(false) => {
                            // Should not happen — gate returns Ok(true) or Err.
                            this.state.set_spmode_vpn(false);
                            let _ = this.persist_db();
                        }
                        Err(e) => {
                            // Upstream: get_elevated_permissions failed → leave Tun off.
                            this.state.set_spmode_vpn(false);
                            let _ = this.persist_db();
                            let msg = e
                                .strip_prefix("tun privilege required: ")
                                .unwrap_or(&e);
                            this.state.set_status_message_only(msg.to_string());
                        }
                    }
                    this.sync_tray_menu();
                    cx.notify();
                })
                .ok();
            })
            .detach();
            return;
        }

        self.state.set_spmode_vpn(false);
        let _ = self.persist_db();
        self.state.set_status_message_only("Tun Mode disabled");
        self.sync_tray_menu();
        if self.state.core_status().is_running() {
            self.start_proxy(cx);
        }
        cx.notify();
    }

    fn set_sys_proxy(&mut self, on: bool, cx: &mut Context<Self>) {
        self.state.set_spmode_system_proxy(on);
        let s = self.state.settings();
        let host = s.inbound_address.clone();
        let port = s.inbound_socks_port;
        let running = self.state.core_status().is_running();
        let _ = self.persist_db();
        self.sync_tray_menu();

        // networksetup can block — never run it on the UI thread.
        if on && !running {
            self.state.set_status_message_only(
                "System Proxy will apply on next Start (core not running yet)",
            );
            cx.notify();
            return;
        }

        self.state.set_status_message_only(if on {
            "Enabling System Proxy…"
        } else {
            "Disabling System Proxy…"
        });
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { set_system_proxy(on, &host, port) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(()) if on => this
                        .state
                        .set_status_message_only(format!("System Proxy ON → 127.0.0.1:{port}")),
                    Ok(()) => this.state.set_status_message_only("System Proxy OFF"),
                    Err(e) => this
                        .state
                        .set_status_message_only(format!("System Proxy failed: {e}")),
                }
                this.sync_tray_menu();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn set_sys_dns(&mut self, on: bool, cx: &mut Context<Self>) {
        self.state.set_system_dns(on);
        let _ = self.persist_db();
        cx.notify();
    }

    fn save_basic_settings(&mut self, cx: &mut Context<Self>) {
        let (ruleset_mirror, adblock_enable) = match &self.dialog {
            Dialog::BasicSettings {
                ruleset_mirror,
                adblock_enable,
                ..
            } => (*ruleset_mirror, *adblock_enable),
            _ => return,
        };
        let Some(DialogInputs::Basic {
            inbound_address,
            inbound_port,
            test_url,
            remote_dns,
            direct_dns,
            log_level,
        }) = &self.dialog_inputs
        else {
            return;
        };
        let inbound_address = DialogInputs::read_string(inbound_address, cx);
        let inbound_port = DialogInputs::read_string(inbound_port, cx);
        let test_url = DialogInputs::read_string(test_url, cx);
        let remote_dns = DialogInputs::read_string(remote_dns, cx);
        let direct_dns = DialogInputs::read_string(direct_dns, cx);
        let log_level = DialogInputs::read_string(log_level, cx);
        let port = inbound_port.parse::<i32>().unwrap_or(2080);
        self.state.apply_basic_settings(
            inbound_address,
            port,
            test_url,
            remote_dns,
            direct_dns,
            log_level,
            ruleset_mirror,
            adblock_enable,
        );
        let _ = self.persist_db();
        self.close_dialog();
        cx.notify();
    }

    fn edit_group_ok(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Dialog::EditGroup {
            group_id,
            is_subscription,
            skip_auto_update,
            auto_clear_unavailable,
            front_proxy_id,
            landing_proxy_id,
        } = &self.dialog
        else {
            return;
        };
        let group_id = *group_id;
        let is_subscription = *is_subscription;
        let skip_auto_update = *skip_auto_update;
        let auto_clear_unavailable = *auto_clear_unavailable;
        let front_proxy_id = *front_proxy_id;
        let landing_proxy_id = *landing_proxy_id;

        let (name, url_raw) = match &self.dialog_inputs {
            Some(DialogInputs::EditGroup { name, url }) => (
                DialogInputs::read_string(name, cx),
                DialogInputs::read_string(url, cx),
            ),
            _ => return,
        };
        let url = if is_subscription {
            url_raw
        } else {
            String::new()
        };

        let result = if let Some(id) = group_id {
            self.state
                .apply_group_edit(
                    id,
                    name,
                    url,
                    skip_auto_update,
                    auto_clear_unavailable,
                    front_proxy_id,
                    landing_proxy_id,
                )
                .map(|_| id)
        } else {
            self.state.create_group_from_edit(
                name,
                url,
                skip_auto_update,
                auto_clear_unavailable,
                front_proxy_id,
                landing_proxy_id,
            )
        };

        match result {
            Ok(id) => {
                let _ = self.persist_db();
                self.state
                    .set_status_message(format!("Group {id} saved"));
                self.return_to_manage_groups(window, cx);
            }
            Err(e) => {
                self.state.set_status_message(e.to_string());
                cx.notify();
            }
        }
    }

    fn confirm_remove_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Dialog::ConfirmRemoveGroup { group_id, .. } = &self.dialog else {
            return;
        };
        let id = *group_id;
        match self.state.delete_group(id) {
            Ok(()) => {
                let _ = self.persist_db();
                self.state
                    .set_status_message(format!("Group {id} removed"));
                self.return_to_manage_groups(window, cx);
            }
            Err(e) => {
                self.state.set_status_message(e.to_string());
                self.return_to_manage_groups(window, cx);
            }
        }
    }

    fn copy_group_share_links(&mut self, deep: bool, cx: &mut Context<Self>) {
        let Dialog::EditGroup {
            group_id: Some(gid),
            ..
        } = &self.dialog
        else {
            return;
        };
        let Some(g) = self.state.group(*gid) else {
            return;
        };
        let mut links = Vec::new();
        for &pid in &g.profile_ids {
            if let Some(p) = self.state.profile(pid) {
                if !p.outbound_json.trim().is_empty() {
                    if deep {
                        // Deep link: keep JSON outbound blob (best-effort without full exporter).
                        links.push(p.outbound_json.clone());
                    } else if let Some(line) = p.outbound_json.lines().next() {
                        links.push(line.trim().to_string());
                    }
                } else {
                    links.push(format!("# {} ({})", p.name, p.profile_type.display_name()));
                }
            }
        }
        let text = links.join("\n");
        if text.is_empty() {
            self.state
                .set_status_message("No shareable links in this group");
        } else {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.state.set_status_message("Copied");
        }
        cx.notify();
    }

    fn add_input_ok(&mut self, cx: &mut Context<Self>) {
        let t = match &self.dialog_inputs {
            Some(DialogInputs::AddFromInput { text }) => DialogInputs::read_string(text, cx),
            _ => return,
        };
        self.close_dialog();
        self.import_text(&t, cx);
    }

    fn dialog_is_open(&self) -> bool {
        !matches!(self.dialog, Dialog::None)
    }

    /// Mutable access to the string field currently focused inside a dialog.
    /// Used by keyboard + EntityInputHandler (IME) paths.
    fn focused_dialog_field_mut(&mut self) -> Option<(&mut String, bool /*allow_nl*/)> {
        match &mut self.dialog {
            Dialog::BasicSettings {
                inbound_address,
                inbound_port,
                test_url,
                remote_dns,
                direct_dns,
                log_level,
                focus,
                ..
            } => {
                let field = match *focus {
                    0 => inbound_address,
                    1 => inbound_port,
                    2 => test_url,
                    3 => remote_dns,
                    4 => direct_dns,
                    _ => log_level,
                };
                Some((field, false))
            }
            Dialog::ManageGroups
            | Dialog::ConfirmRemoveGroup { .. }
            | Dialog::ConfirmUpdateAllSubscriptions
            | Dialog::SubscriptionDiff { .. }
            | Dialog::AutoSelectorStats { .. }
            | Dialog::TrafficStats { .. } => None,
            Dialog::EditGroup { .. } => None,
            Dialog::AddFromInput { text } => Some((text, true)),
            Dialog::RoutingSettings(draft) => {
                // Ensure a field is focused so IME/typing always has a target.
                if draft.focus == crate::ui::routing::RtFocus::None {
                    draft.focus = match draft.tab {
                        crate::ui::routing::RoutingTab::Dns => {
                            crate::ui::routing::RtFocus::RemoteDns
                        }
                        crate::ui::routing::RoutingTab::Warp => {
                            crate::ui::routing::RtFocus::WarpEp
                        }
                        crate::ui::routing::RoutingTab::Hijack => {
                            crate::ui::routing::RtFocus::DnsPort
                        }
                        _ => crate::ui::routing::RtFocus::None,
                    };
                }
                let s = &mut draft.settings;
                let field: Option<(&mut String, bool)> = match draft.focus {
                    crate::ui::routing::RtFocus::RemoteDns => Some((&mut s.remote_dns, false)),
                    crate::ui::routing::RtFocus::DirectDns => Some((&mut s.direct_dns, false)),
                    crate::ui::routing::RtFocus::LocalOverride => {
                        Some((&mut s.core_box_underlying_dns, false))
                    }
                    crate::ui::routing::RtFocus::CacheCap => {
                        // digits stored as string temporarily — handled specially
                        None
                    }
                    crate::ui::routing::RtFocus::DnsObject => Some((&mut s.dns_object, true)),
                    crate::ui::routing::RtFocus::DnsV4 => Some((&mut s.dns_v4_resp, false)),
                    crate::ui::routing::RtFocus::DnsV6 => Some((&mut s.dns_v6_resp, false)),
                    crate::ui::routing::RtFocus::DnsPort => None,
                    crate::ui::routing::RtFocus::RedirectAddr => {
                        Some((&mut s.redirect_listen_address, false))
                    }
                    crate::ui::routing::RtFocus::RedirectPort => None,
                    crate::ui::routing::RtFocus::WarpEp => Some((&mut s.warp_ep, false)),
                    crate::ui::routing::RtFocus::WarpPriv => Some((&mut s.warp_private_key, false)),
                    crate::ui::routing::RtFocus::WarpPub => Some((&mut s.warp_public_key, false)),
                    crate::ui::routing::RtFocus::WarpAddrs
                    | crate::ui::routing::RtFocus::WarpReserved
                    | crate::ui::routing::RtFocus::DnsRules
                    | crate::ui::routing::RtFocus::None => None,
                };
                field
            }
            Dialog::TunSettings {
                vpn_mtu,
                focus_mtu,
                ..
            } => {
                if *focus_mtu {
                    Some((vpn_mtu, false))
                } else {
                    None
                }
            }
            Dialog::HotkeySettings {
                start_stop,
                import,
                save,
                url_test,
                copy_logs,
                focus,
            } => {
                let field = match *focus {
                    0 => start_stop,
                    1 => import,
                    2 => save,
                    3 => url_test,
                    _ => copy_logs,
                };
                Some((field, false))
            }
            Dialog::EditProfile { name, .. } => Some((name, false)),
            Dialog::ConfirmDeleteUnavailable { .. } | Dialog::None => None,
        }
    }

    /// Insert/delete text into the focused dialog field (end-cursor model).
    fn apply_dialog_text_edit(&mut self, text: &str, is_backspace: bool, cx: &mut Context<Self>) {
        // Routing: main DNS/Warp/Hijack + nested editors use Input when NestedInputs present;
        // fallback fake-caret only when NestedInputs is missing.
        if matches!(self.dialog, Dialog::RoutingSettings(_)) {
            let nested_active = matches!(
                &self.dialog,
                Dialog::RoutingSettings(d) if !matches!(d.nested, RoutingNested::None)
            );
            let has_routing_inputs = matches!(self.dialog_inputs, Some(DialogInputs::Routing(_)));
            let nested_owns = self.nested_inputs_own_typing();
            if nested_active {
                if nested_owns {
                    return;
                }
                if let Dialog::RoutingSettings(draft) = &mut self.dialog {
                    draft.handle_key(if is_backspace { None } else { Some(text) }, is_backspace);
                }
                cx.notify();
            } else if !has_routing_inputs {
                if let Dialog::RoutingSettings(draft) = &mut self.dialog {
                    draft.handle_key(if is_backspace { None } else { Some(text) }, is_backspace);
                }
                cx.notify();
            }
            return;
        }

        let digits_only = matches!(
            &self.dialog,
            Dialog::BasicSettings { focus: 1, .. }
                | Dialog::TunSettings {
                    focus_mtu: true,
                    ..
                }
        );

        let Some((field, allow_nl)) = self.focused_dialog_field_mut() else {
            return;
        };

        if is_backspace {
            field.pop();
        } else {
            for c in text.chars() {
                if c == '\n' || c == '\r' {
                    if allow_nl {
                        field.push('\n');
                    }
                    continue;
                }
                if c.is_control() {
                    continue;
                }
                if digits_only && !c.is_ascii_digit() {
                    continue;
                }
                field.push(c);
            }
        }
        cx.notify();
    }

    fn handle_dialog_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        if matches!(self.dialog, Dialog::None) {
            return false;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            // gpui-component Dialog owns Esc when a layer is open.
            if Self::uses_gpui_dialog(&self.dialog) {
                return false;
            }
            self.close_dialog();
            cx.notify();
            return true;
        }

        if key == "enter" && matches!(self.dialog, Dialog::EditProfile { .. }) {
            self.save_edit_profile(cx);
            return true;
        }

        // Hotkey capture rows own keystrokes (QKeySequenceEdit-style).
        if matches!(self.dialog, Dialog::HotkeySettings { .. }) {
            return false;
        }

        // Real InputState owns typing for non-routing dialogs, routing main tabs
        // (when nested is closed), and NestedInputs-backed nested fields.
        if let Some(inputs) = &self.dialog_inputs {
            match inputs {
                DialogInputs::Routing(_) => {
                    if let Dialog::RoutingSettings(d) = &self.dialog {
                        if matches!(d.nested, RoutingNested::None) {
                            return false;
                        }
                        if self.nested_inputs_own_typing() {
                            return false;
                        }
                        // NestedInputs missing or non-text nested (menus) — fake-caret path.
                    }
                }
                _ => return false,
            }
        }

        // Prefer GPUI key_char (IME / layout-correct typed text).
        let is_back = key == "backspace" || key == "delete";
        let typed: Option<String> = if is_back {
            None
        } else if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            None
        } else if let Some(kc) = event.keystroke.key_char.as_ref() {
            if kc.chars().all(|c| c.is_control()) {
                None
            } else {
                Some(kc.clone())
            }
        } else if key == "space" {
            Some(" ".into())
        } else if key == "enter"
            && matches!(
                self.dialog,
                Dialog::AddFromInput { .. } | Dialog::RoutingSettings(_)
            )
        {
            Some("\n".into())
        } else if key.len() == 1 {
            key.chars()
                .next()
                .filter(|c| !c.is_control())
                .map(|c| c.to_string())
        } else {
            None
        };

        if !is_back && typed.is_none() {
            return true; // swallow other keys while dialog open
        }

        self.apply_dialog_text_edit(typed.as_deref().unwrap_or(""), is_back, cx);
        true
    }

    // ─── layout regions ─────────────────────────────────────────────────

    fn render_top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let start_stop_state = match self.state.core_status() {
            CoreStatus::Running { .. } => StartStopState::Stop,
            CoreStatus::Starting => StartStopState::Starting,
            CoreStatus::Stopping => StartStopState::Stopping,
            CoreStatus::Stopped | CoreStatus::Error(_) => StartStopState::Start,
        };
        let entity = cx.entity().clone();

        div()
            .relative()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_2()
            .bg(Theme::bg_panel())
            .border_b_1()
            .border_color(Theme::border_light())
            .child(self.render_tool_cluster(cx))
            .child({
                let e = entity.clone();
                start_stop_btn(start_stop_state, move |_, _, cx| {
                    e.update(cx, |this, cx| this.toggle_proxy(cx));
                })
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap_1()
                    .px_2()
                    .h(px(52.))
                    .child({
                        let e = entity.clone();
                        let on = self.state.settings().tun_mode_enabled;
                        mode_switch("tun", "Tun Mode", on, move |_, _, cx| {
                            e.update(cx, |this, cx| {
                                let next = !this.state.settings().tun_mode_enabled;
                                this.set_vpn(next, cx);
                            });
                        })
                    })
                    .child({
                        let e = entity.clone();
                        let on = self.state.settings().system_proxy_enabled;
                        mode_switch("proxy", "System Proxy", on, move |_, _, cx| {
                            e.update(cx, |this, cx| {
                                let next = !this.state.settings().system_proxy_enabled;
                                this.set_sys_proxy(next, cx);
                            });
                        })
                    }),
            )
            // Upstream `data_view` (top-right): group test progress / download report.
            .child(self.render_test_progress_panel())
    }

    /// Top-right test progress (upstream `QTextBrowser data_view` latency section).
    fn render_test_progress_panel(&self) -> impl IntoElement {
        let mut panel = div()
            .id("test-progress-panel")
            .flex_1()
            .min_w(px(120.))
            .h(px(52.))
            .px_2()
            .flex()
            .flex_col()
            .justify_center()
            .items_center()
            .gap_0p5();

        if let Some(progress) = self.test_progress.as_ref() {
            let (pct, content) = test_progress_view(progress);
            if let Some(pct) = pct {
                panel = panel.child(
                    div()
                        .w_full()
                        .max_w(px(220.))
                        .child(gpui_component::progress::Progress::new().value(pct)),
                );
            }
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .child(content),
            );
        }

        panel
    }

    /// Compact dropdown body for the open toolbar / context menu.
    fn menu_items_for(&self, menu: OpenMenu, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let mut panel = div().flex().flex_col();

        macro_rules! item {
            ($id:expr, $label:expr, |$t:ident, $w:ident, $cx:ident| $($body:tt)*) => {{
                let e = entity.clone();
                panel = panel.child(menu_item($id, $label, move |_, window, cx| {
                    e.update(cx, |this, cx| {
                        let $t = this;
                        let $w = window;
                        let $cx = cx;
                        // Block so single-expr bodies don't glue onto close_menus.
                        {
                            $($body)*
                        }
                        // Always dismiss after an item runs (open_* already closes too).
                        $t.close_menus();
                        $cx.notify();
                    });
                }));
            }};
        }

        match menu {
            OpenMenu::Program => {
                panel = panel.child(menu_label("Program"));
                // Upstream 1.2.3: Program → New Profile (+ Auto Selector)
                item!("prog-new-auto", "New Auto Selector", |t, _w, cx| {
                    t.create_auto_selector(cx);
                });
                item!("prog-input", "Add profile from input", |t, w, cx| {
                    t.open_add_from_input(w, cx);
                });
                item!("prog-clip", "Add profile from clipboard", |t, _w, cx| {
                    t.import_clipboard(cx);
                });
                item!("prog-files", "Import from file(s)…", |t, _w, cx| {
                    t.import_from_files(cx);
                });
                item!("prog-start", "Start", |t, _w, cx| t.toggle_proxy(cx));
                item!("prog-stop", "Stop", |t, _w, cx| {
                    if t.state.core_status().is_running() {
                        t.toggle_proxy(cx);
                    }
                });
                panel = panel.child(menu_separator());
                item!("prog-proxy", "Enable System Proxy", |t, _w, cx| {
                    t.set_sys_proxy(true, cx);
                });
                item!("prog-tun", "Enable Tun", |t, _w, cx| {
                    t.set_vpn(true, cx);
                });
                item!("prog-off", "Disable", |t, _w, cx| {
                    t.set_sys_proxy(false, cx);
                    t.set_vpn(false, cx);
                    t.set_sys_dns(false, cx);
                });
                panel = panel.child(menu_separator());
                item!("prog-exit", "Exit", |_t, _w, cx| cx.quit());
            }
            OpenMenu::Settings => {
                panel = panel.child(menu_label("Preferences"));
                item!("set-basic", "Basic Settings", |t, w, cx| t.open_basic_settings(w, cx));
                item!("set-route", "Routing Settings", |t, w, cx| {
                    t.open_routing_settings(Some(w), cx);
                });
                item!("set-tun", "Tun Settings", |t, w, cx| t.open_tun_settings(w, cx));
                item!("set-hotkey", "Hotkey Settings", |t, w, cx| t.open_hotkey_settings(w, cx));
                item!("set-clear-proxy", "Clear system proxy now", |t, _w, cx| {
                    force_clear_system_proxy();
                    t.state
                        .set_status_message("System proxy force-cleared on all interfaces");
                });
                panel = panel.child(menu_separator());
                item!("set-folder", "Open Config Folder", |t, _w, cx| {
                    let path = if t.db_path_label.is_empty() {
                        throne_storage::default_db_path()
                    } else {
                        std::path::PathBuf::from(&t.db_path_label)
                    };
                    if let Some(dir) = path.parent() {
                        let _ = std::process::Command::new("open").arg(dir).spawn();
                        t.state
                            .set_status_message(format!("Opened {}", dir.display()));
                    }
                });
                item!("set-save", "Save database", |t, _w, cx| t.save_db(cx));
            }
            OpenMenu::Groups => {
                panel = panel.child(menu_label("Groups"));
                item!("g-manage", "Manage Groups", |t, w, cx| t.open_manage_groups(w, cx));
                item!("g-update", "Update subscription", |t, w, cx| {
                    t.update_subscription(false, Some(w), cx);
                });
                item!("g-update-all", "Update all subscriptions", |t, w, cx| {
                    t.update_subscription(true, Some(w), cx);
                });
                panel = panel.child(menu_separator());
                item!("g-urltest", "Url Test Group", |t, _w, cx| t.url_test_group(cx));
                item!("g-clear", "Clear Group test result", |t, _w, cx| {
                    let gid = t.state.active_group_id();
                    t.state.clear_test_results_in_group(gid);
                    let _ = t.persist_db();
                });
                item!("g-dup", "Remove Duplicates", |t, _w, cx| {
                    let gid = t.state.active_group_id();
                    t.state.remove_duplicates_in_group(gid);
                    let _ = t.persist_db();
                });
                item!("g-unavail", "Remove Unavailable", |t, w, cx| {
                    t.delete_unavailable(Some(w), cx);
                });
                item!("g-invalid", "Remove Invalid Configs", |t, _w, cx| {
                    let gid = t.state.active_group_id();
                    t.state.remove_invalid_in_group(gid);
                    let _ = t.persist_db();
                });
                item!("g-insecure", "Remove Insecure Configs", |t, _w, cx| {
                    let gid = t.state.active_group_id();
                    t.state.remove_insecure_in_group(gid);
                    let _ = t.persist_db();
                });
            }
            OpenMenu::Routing => {
                panel = panel.child(menu_label("Routing"));
                item!("r-settings", "Routing Settings", |t, w, cx| {
                    t.open_routing_settings(Some(w), cx);
                });
                item!("r-cycle", "Next route profile", |t, _w, cx| t.cycle_route(cx));
                let routes: Vec<_> = self
                    .state
                    .all_routes()
                    .into_iter()
                    .map(|r| {
                        let active = self.state.active_route().is_some_and(|a| a.id == r.id);
                        (r.id, r.summary(), active)
                    })
                    .collect();
                if !routes.is_empty() {
                    panel = panel.child(menu_separator());
                    for (id, name, active) in routes {
                        let e = entity.clone();
                        panel = panel.child(menu_item_checked(
                            SharedString::from(format!("route-{id}")),
                            name,
                            active,
                            move |_, _, cx| {
                                e.update(cx, |t, cx| {
                                    t.select_active_route(id, cx);
                                });
                            },
                        ));
                    }
                }
            }
            OpenMenu::Tools => {
                panel = panel.child(menu_label("Tools"));
                item!("t-url", "Url Test Selected", |t, _w, cx| t.url_test_selected(cx));
                item!("t-url-group", "Url Test Group (⌘⇧G)", |t, _w, cx| {
                    t.url_test_group(cx);
                });
                item!("t-delete-unavailable", "Delete Unavailable (⌘⇧R)", |t, w, cx| {
                    t.delete_unavailable(Some(w), cx);
                });
                item!("t-speed", "Speedtest Selected", |t, _w, cx| {
                    t.speed_test_selected(cx);
                });
                item!("t-ip", "IP Test Selected", |t, _w, cx| t.ip_test_selected(cx));
                panel = panel.child(menu_separator());
                item!("t-runtime", "Runtime Stats", |t, _w, cx| {
                    t.bottom_tab = 1;
                    t.state.set_status_message(
                        "Connections tab shows live sessions while core is running",
                    );
                });
                item!("t-auto-sel", "Auto Selector Stats", |t, w, cx| {
                    t.open_auto_selector_stats(w, cx);
                });
                item!("t-traffic", "Traffic Stats", |t, w, cx| {
                    t.open_traffic_stats(w, cx);
                });
                item!("t-update", "Check For Update", |t, _w, cx| {
                    t.state.set_status_message(format!(
                        "Current version {} · throne-rs rewrite (no auto-update yet)",
                        throne_domain::NKR_VERSION
                    ));
                });
                panel = panel.child(menu_separator());
                panel = panel.child(menu_label(format!(
                    "Version {}",
                    throne_domain::NKR_VERSION
                )));
            }
            OpenMenu::ProfileCtx => {
                panel = panel.child(menu_label("Server"));
                item!("c-start", "Start", |t, _w, cx| t.toggle_proxy(cx));
                item!("c-stop", "Stop", |t, _w, cx| {
                    if t.state.core_status().is_running() {
                        t.toggle_proxy(cx);
                    }
                });
                item!("c-input", "Add profile from input", |t, w, cx| {
                    t.open_add_from_input(w, cx);
                });
                item!("c-clip", "Add profile from clipboard", |t, _w, cx| {
                    t.import_clipboard(cx);
                });
                item!("c-edit", "Edit profile…", |t, w, cx| t.open_edit_profile(w, cx));
                item!("c-del", "Delete", |t, _w, cx| t.delete_selected(cx));
                item!("c-test", "Url Test Selected", |t, _w, cx| t.url_test_selected(cx));
            }
            OpenMenu::GroupTabCtx => {
                item!("gt-add", "Add new Group", |t, w, cx| {
                    t.open_edit_group_new(w, cx);
                });
                if let Some(gid) = self.ctx_group_id {
                    item!("gt-edit", "Edit selected Group", |t, w, cx| {
                        t.open_edit_group(gid, w, cx);
                    });
                    if self.state.group_order().len() > 1 {
                        let name = self
                            .state
                            .group(gid)
                            .map(|g| {
                                if g.name.is_empty() {
                                    format!("Group {gid}")
                                } else {
                                    g.name.clone()
                                }
                            })
                            .unwrap_or_else(|| format!("Group {gid}"));
                        item!("gt-del", "Delete selected Group", |t, w, cx| {
                            t.dialog = Dialog::ConfirmRemoveGroup {
                                group_id: gid,
                                name: name.clone(),
                            };
                            t.dialog_inputs = None;
                            t.present_gpui_dialog(w, cx);
                        });
                    }
                    let has_url = self
                        .state
                        .group(gid)
                        .is_some_and(|g| !g.url.trim().is_empty() && !g.archive);
                    if has_url {
                        item!("gt-upd", "Update subscription", |t, _w, cx| {
                            t.start_subscription_group(gid, UpdateOrigin::Manual, cx);
                        });
                    }
                }
            }
            OpenMenu::None => {}
        }

        panel
    }

    fn render_tool_cluster(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();

        // Buttons only — dropdown panels are root overlays so they paint above the table.
        let menus = [
            (OpenMenu::Program, "tb-program", ToolbarIcon::Program, "Program"),
            (OpenMenu::Settings, "tb-settings", ToolbarIcon::Settings, "Settings"),
            (OpenMenu::Groups, "tb-groups", ToolbarIcon::Groups, "Groups"),
            (OpenMenu::Routing, "tb-routing", ToolbarIcon::Routing, "Routing"),
            (OpenMenu::Tools, "tb-tools", ToolbarIcon::Tools, "Tools"),
        ];

        // No outer chrome — individual `toolbar_btn` borders are enough.
        let mut row = div().flex().items_center().gap_1();
        for (menu, id, icon, label) in menus {
            let e = entity.clone();
            let open = self.open_menu == menu;
            row = row.child(toolbar_btn(id, icon, label, open, move |_, _, cx| {
                e.update(cx, |this, cx| this.toggle_menu(menu, cx));
            }));
        }
        row
    }

    /// Index of the open toolbar menu button (0=Program … 4=Tools).
    fn toolbar_menu_index(menu: OpenMenu) -> Option<usize> {
        match menu {
            OpenMenu::Program => Some(0),
            OpenMenu::Settings => Some(1),
            OpenMenu::Groups => Some(2),
            OpenMenu::Routing => Some(3),
            OpenMenu::Tools => Some(4),
            OpenMenu::ProfileCtx | OpenMenu::GroupTabCtx | OpenMenu::None => None,
        }
    }

    /// Root-level dropdown — paints after group tabs so it is never covered.
    fn render_toolbar_menu_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(idx) = Self::toolbar_menu_index(self.open_menu) else {
            return div().into_any_element();
        };
        // Align with tool-cluster: pad_x + n × (btn + gap). No extra offset.
        let left = TOOLBAR_PAD_X + idx as f32 * (TOOLBAR_BTN_W + TOOLBAR_BTN_GAP);
        let items = self.menu_items_for(self.open_menu, cx);
        div()
            .id(SharedString::from(format!("tb-overlay-{idx}")))
            .absolute()
            .top(px(TOOLBAR_MENU_TOP))
            .left(px(left))
            .occlude()
            .child(menu_panel(
                SharedString::from(format!("tb-menu-{idx}")),
                200.,
                items,
            ))
            .into_any_element()
    }

    /// Profile / group-tab context menu at the click position.
    fn render_ctx_popup(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (x, y) = self.ctx_menu_at.unwrap_or((200., 200.));
        let items = self.menu_items_for(self.open_menu, cx);
        div()
            .id("ctx-popup")
            .absolute()
            .top(px(y))
            .left(px(x))
            .occlude()
            .child(menu_panel("ctx-menu", 200., items))
            .into_any_element()
    }

    fn render_group_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // Custom tabs (not TabBar) so each chip can take a right-click context menu
        // matching upstream group tab bar behaviour.
        let active = self.state.active_group_id();
        let entity = cx.entity().clone();
        let e_empty = entity.clone();

        let mut row = div()
            .id("group-tabs")
            .flex()
            .flex_1()
            .items_center()
            .gap_1()
            .min_h(px(32.))
            .on_mouse_down(gpui::MouseButton::Right, move |ev: &gpui::MouseDownEvent, _, cx| {
                // Empty tab-bar area → only "Add new Group" (upstream).
                let pos = ev.position;
                e_empty.update(cx, |this, cx| {
                    this.ctx_group_id = None;
                    this.show_menu(
                        OpenMenu::GroupTabCtx,
                        Some((pos.x.into(), pos.y.into())),
                        cx,
                    );
                });
            });

        for &gid in self.state.group_order() {
            let Some(group) = self.state.group(gid) else {
                continue;
            };
            let name = if group.name.is_empty() {
                format!("Group {gid}")
            } else {
                group.name.clone()
            };
            let sel = gid == active;
            let e_left = entity.clone();
            let e_right = entity.clone();
            row = row.child(
                div()
                    .id(SharedString::from(format!("gtab-{gid}")))
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .text_sm()
                    .border_1()
                    .border_color(if sel {
                        Theme::accent()
                    } else {
                        Theme::border_light()
                    })
                    .bg(if sel {
                        Theme::accent_soft()
                    } else {
                        Theme::bg_elevated()
                    })
                    .text_color(if sel {
                        Theme::accent()
                    } else {
                        Theme::text()
                    })
                    .hover(|s| s.bg(Theme::bg_hover()))
                    .child(name)
                    .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                        e_left.update(cx, |this, cx| this.select_group(gid, cx));
                    })
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        move |ev: &gpui::MouseDownEvent, _, cx| {
                            cx.stop_propagation(); // don't also fire empty-bar "Add only" menu
                            let pos = ev.position;
                            e_right.update(cx, |this, cx| {
                                // Upstream selects the clicked tab before showing the menu.
                                let _ = this.state.set_active_group(gid);
                                this.ctx_group_id = Some(gid);
                                this.show_menu(
                                    OpenMenu::GroupTabCtx,
                                    Some((pos.x.into(), pos.y.into())),
                                    cx,
                                );
                            });
                        },
                    ),
            );
        }

        div()
            .flex()
            .items_center()
            .px_2()
            .pt_2()
            .pb_1()
            .bg(Theme::bg_app())
            .child(row)
    }

    fn render_table_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // Column widths must match row cells exactly. Click toggles sort.
        let entity = cx.entity().clone();
        let active = self.sort_column;
        let muted = Theme::text_muted();
        let accent = Theme::accent();

        let hdr_fixed = |width: f32,
                         id: &'static str,
                         col: SortColumn,
                         label: String,
                         e: gpui::Entity<MainWindow>| {
            let color = if active == col { accent } else { muted };
            div()
                .id(SharedString::from(id))
                .w(px(width))
                .min_w(px(width))
                .max_w(px(width))
                .flex_shrink_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .pr_2()
                .h_full()
                .flex()
                .items_center()
                .cursor_pointer()
                .hover(|s| s.text_color(Theme::accent()))
                .text_color(color)
                .child(label)
                .on_click(move |_, _, cx| {
                    e.update(cx, |this, cx| this.toggle_sort(col, cx));
                })
        };

        let name_label = self.sort_label(SortColumn::Name, "Name");
        let e_name = entity.clone();
        let name_color = if active == SortColumn::Name {
            accent
        } else {
            muted
        };

        div()
            .flex()
            .items_center()
            .w_full()
            .px_2()
            .h(px(28.))
            .bg(Theme::bg_panel())
            .border_b_1()
            .border_color(Theme::border_light())
            .text_xs()
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(Theme::text_muted())
            // # is display index only — not sortable
            .child(col_fixed(COL_IDX, "#", Theme::text_muted()))
            .child(hdr_fixed(
                COL_TYPE,
                "h-type",
                SortColumn::Type,
                self.sort_label(SortColumn::Type, "Type"),
                entity.clone(),
            ))
            .child(hdr_fixed(
                COL_ADDR,
                "h-addr",
                SortColumn::Address,
                self.sort_label(SortColumn::Address, "Address"),
                entity.clone(),
            ))
            .child(
                div()
                    .id("h-name")
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .pr_2()
                    .h_full()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .hover(|s| s.text_color(Theme::accent()))
                    .text_color(name_color)
                    .child(name_label)
                    .on_click(move |_, _, cx| {
                        e_name.update(cx, |this, cx| this.toggle_sort(SortColumn::Name, cx));
                    }),
            )
            .child(hdr_fixed(
                COL_TEST,
                "h-test",
                SortColumn::TestResult,
                self.sort_label(SortColumn::TestResult, "Test Result"),
                entity.clone(),
            ))
            .child(hdr_fixed(
                COL_TRAFFIC,
                "h-traf",
                SortColumn::Traffic,
                self.sort_label(SortColumn::Traffic, "Traffic"),
                entity,
            ))
    }

    fn render_profile_table(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let profiles: Vec<Profile> = self.sorted_profiles();
        let count = profiles.len();
        let selected = self.state.selected_profile_id();
        let running_id = match self.state.core_status() {
            CoreStatus::Running { profile_id, .. } => Some(*profile_id),
            _ => None,
        };
        let entity = cx.entity().clone();
        let show_sec = self.state.settings().show_config_security;
        // Resolve Auto Selector → tracked group name for the Address column.
        let group_names: std::collections::HashMap<GroupId, String> = self
            .state
            .all_groups()
            .into_iter()
            .map(|g| (g.id, g.name.clone()))
            .collect();

        div().flex_1().min_h(px(120.)).bg(Theme::bg_elevated()).child(
            uniform_list(
                "profiles",
                count,
                cx.processor(move |_this, range: Range<usize>, _window, _cx| {
                    let mut items = Vec::new();
                    for (display_i, ix) in range.clone().enumerate() {
                        let Some(profile) = profiles.get(ix) else {
                            continue;
                        };
                        let id = profile.id;
                        let is_selected = selected == Some(id);
                        let is_running = running_id == Some(id);
                        let row_label = if is_running {
                            "✓".to_string()
                        } else {
                            (ix + 1).to_string()
                        };
                        let ty = profile.display_type();
                        let addr = if profile.profile_type == ProfileType::AutoSelector {
                            throne_domain::profile_auto_selector(profile)
                                .and_then(|c| group_names.get(&c.gid).cloned())
                                .map(|n| format!("group · {n}"))
                                .unwrap_or_else(|| profile.display_address())
                        } else {
                            profile.display_address()
                        };
                        let name = profile.name.clone();
                        let test = profile.display_test_result();
                        let traffic = profile.display_traffic();
                        let lat_color = latency_color(profile.latency_ms);
                        let insecure = show_sec && profile.insecure;
                        let e_select = entity.clone();
                        let e_ctx = entity.clone();

                        let (bg, fg) = if is_selected && !is_running {
                            (Theme::bg_selected(), Theme::text_on_selected())
                        } else if display_i % 2 == 1 {
                            (Theme::bg_app(), Theme::text())
                        } else {
                            (Theme::bg_elevated(), Theme::text())
                        };

                        let idx_color = if is_running {
                            Theme::success()
                        } else if is_selected {
                            fg
                        } else {
                            Theme::text_muted()
                        };
                        let row_color = if is_running {
                            Theme::success()
                        } else {
                            fg
                        };
                        let type_color = if insecure {
                            Theme::danger()
                        } else {
                            row_color
                        };
                        let test_color = if is_running || is_selected {
                            row_color
                        } else {
                            lat_color
                        };

                        items.push(
                            div()
                                .id(SharedString::from(format!("row-{id}")))
                                .flex()
                                .items_center()
                                .w_full()
                                .px_2()
                                .h(px(28.))
                                .bg(bg)
                                .text_color(row_color)
                                .text_sm()
                                .cursor_pointer()
                                .border_b_1()
                                .border_color(Theme::border_light())
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    move |ev: &gpui::MouseDownEvent, _, cx| {
                                        e_select.update(cx, |this, cx| {
                                            let _ = this.state.select_profile(id);
                                            if ev.click_count >= 2 {
                                                this.activate_profile(id, cx);
                                            } else {
                                                cx.notify();
                                            }
                                        });
                                    },
                                )
                                .on_mouse_down(
                                    gpui::MouseButton::Right,
                                    move |ev: &gpui::MouseDownEvent, _, cx| {
                                        let pos = ev.position;
                                        e_ctx.update(cx, |this, cx| {
                                            let _ = this.state.select_profile(id);
                                            this.ctx_group_id = None;
                                            this.show_menu(
                                                OpenMenu::ProfileCtx,
                                                Some((pos.x.into(), pos.y.into())),
                                                cx,
                                            );
                                        });
                                    },
                                )
                                .child(col_fixed(COL_IDX, row_label, idx_color))
                                .child(col_fixed(COL_TYPE, ty, type_color))
                                .child(col_fixed(COL_ADDR, addr, row_color))
                                .child(col_flex(name, row_color))
                                .child(col_fixed(COL_TEST, test, test_color))
                                .child(col_fixed(COL_TRAFFIC, traffic, row_color)),
                        );
                    }
                    items
                }),
            )
            .size_full(),
        )
    }

    fn copy_logs(&mut self, cx: &mut Context<Self>) {
        let text = self.state.logs_text();
        if text.trim().is_empty() {
            self.state.set_status_message("Logs empty — nothing to copy");
            cx.notify();
            return;
        }
        let n = text.lines().filter(|l| !l.trim().is_empty()).count();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.state
            .set_status_message(format!("Logs copied ({n} lines) · ⌘⇧C"));
        cx.notify();
    }

    fn render_bottom_tabs(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let tab = self.bottom_tab;
        let logs = self.state.logs_text();
        if should_scroll_logs_to_bottom(tab == 0, &self.rendered_log_text, &logs) {
            self.log_scroll_handle.scroll_to_bottom();
        }
        if should_update_rendered_log_text(tab == 0) {
            self.rendered_log_text = logs.clone();
        }
        let log_preview = if logs.is_empty() {
            "(no log lines yet)".to_string()
        } else {
            // Show newest lines first in the cramped panel for scanability.
            let lines: Vec<&str> = logs.lines().collect();
            let n = lines.len();
            let start = n.saturating_sub(12);
            lines[start..].join("\n")
        };
        div()
            .flex()
            .flex_col()
            .h(px(160.))
            .border_t_1()
            .border_color(Theme::border_light())
            .bg(Theme::bg_app())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .pt_1()
                    .child({
                        let e = entity.clone();
                        tab_bar(
                            "bottom-tabs",
                            tab,
                            [
                                SharedString::from("Logs"),
                                SharedString::from("Connections"),
                                SharedString::from("Traffic Graph"),
                            ],
                            move |ix, _, cx| {
                                e.update(cx, |t, cx| {
                                    t.bottom_tab = *ix;
                                    cx.notify();
                                });
                            },
                        )
                    })
                    .child(div().flex_1())
                    .when(tab == 0, |row| {
                        let e_copy = entity.clone();
                        let e_clear = entity.clone();
                        row.child(icon_btn("log-copy", "icons/copy.svg", move |_, _, cx| {
                            e_copy.update(cx, |t, cx| t.copy_logs(cx));
                        }))
                        .child(icon_btn("log-clear", "icons/trash.svg", move |_, _, cx| {
                            e_clear.update(cx, |t, cx| {
                                t.state.clear_logs();
                                cx.notify();
                            });
                        }))
                    })
                    .when(tab == 2, |row| {
                        let e_clear = entity.clone();
                        row.child(icon_btn("graph-clear", "icons/trash.svg", move |_, _, cx| {
                            e_clear.update(cx, |t, cx| {
                                t.speed_graph.clear();
                                t.state.set_status_message_only("Traffic Graph cleared");
                                cx.notify();
                            });
                        }))
                    }),
            )
            .child(
                div()
                    .id("logs-panel")
                    .flex_1()
                    .m_1()
                    .p_2()
                    .bg(Theme::bg_elevated())
                    .border_1()
                    .border_color(Theme::border_light())
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .overflow_y_scroll()
                    .track_scroll(&self.log_scroll_handle)
                    .when(tab == 0, {
                        let e = entity.clone();
                        move |el| {
                            el.cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    e.update(cx, |t, cx| t.copy_logs(cx));
                                })
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_color(Theme::text_muted())
                                                .child("Click panel or Copy · ⌘⇧C to copy all logs"),
                                        )
                                        .child(
                                            div()
                                                .text_color(Theme::text())
                                                .child(log_preview),
                                        ),
                                )
                        }
                    })
                    .when(tab == 1, |el| {
                        let body = if !self.state.core_status().is_running() {
                            "Connections — start a profile to see live sessions".to_string()
                        } else if self.connections.is_empty() {
                            "Connections — none active (traffic will appear when apps use the proxy)"
                                .to_string()
                        } else {
                            let mut lines = vec![format!(
                                "{:<6} {:<8} {:<22} {:<10} {}",
                                "net", "proto", "dest", "outbound", "process"
                            )];
                            for c in self.connections.iter().take(40) {
                                let dest = if c.domain.is_empty() {
                                    c.dest.chars().take(22).collect::<String>()
                                } else {
                                    c.domain.chars().take(22).collect::<String>()
                                };
                                lines.push(format!(
                                    "{:<6} {:<8} {:<22} {:<10} {}",
                                    c.network.chars().take(6).collect::<String>(),
                                    c.protocol.chars().take(8).collect::<String>(),
                                    dest,
                                    c.outbound.chars().take(10).collect::<String>(),
                                    c.process.chars().take(16).collect::<String>(),
                                ));
                            }
                            if self.connections.len() > 40 {
                                lines.push(format!(
                                    "… +{} more",
                                    self.connections.len() - 40
                                ));
                            }
                            lines.join("\n")
                        };
                        el.child(div().text_color(Theme::text()).child(body))
                    })
                    .when(tab == 2, |el| {
                        el.overflow_hidden()
                            .child(speed_graph_element(&self.speed_graph))
                    }),
            )
    }

    fn render_status_bar(&self) -> impl IntoElement {
        let running = self.state.core_status().is_running();
        div()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .py_1()
            .bg(Theme::bg_panel())
            .border_t_1()
            .border_color(Theme::border_light())
            .text_xs()
            .text_color(Theme::text())
            .child(
                div()
                    .flex_1()
                    .child(status_tag(running, self.state.running_label())),
            )
            .child(div().flex_1().text_color(Theme::text_muted()).child(self.state.inbound_label()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .text_color(Theme::text_muted())
                    .child(self.state.speed_label())
                    .child(if self.db_path_label.is_empty() {
                        String::new()
                    } else {
                        std::path::Path::new(&self.db_path_label)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("throne.db")
                            .to_string()
                    })
                    .child(throne_domain::NKR_VERSION),
            )
    }
}

// Profile table columns (header + rows must stay in lockstep).
const COL_IDX: f32 = 32.;
const COL_TYPE: f32 = 88.;
const COL_ADDR: f32 = 200.;
const COL_TEST: f32 = 100.;
const COL_TRAFFIC: f32 = 140.;

fn col_fixed(
    width: f32,
    text: impl Into<SharedString>,
    color: gpui::Hsla,
) -> impl IntoElement {
    div()
        .w(px(width))
        .min_w(px(width))
        .max_w(px(width))
        .flex_shrink_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .pr_2()
        .text_color(color)
        .child(text.into())
}

fn col_flex(text: impl Into<SharedString>, color: gpui::Hsla) -> impl IntoElement {
    // flex_1 + min_w(0) is required so the name column can shrink and ellipsize
    // instead of painting over Test Result / Traffic.
    div()
        .flex_1()
        .min_w(px(0.))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .pr_2()
        .text_color(color)
        .child(text.into())
}

fn load_initial_state() -> (AppState, String) {
    match throne_storage::open_default() {
        Ok(db) => {
            let path = db.path().display().to_string();
            let legacy = throne_storage::Database::looks_like_throne_db(&path);
            match throne_storage::load_or_seed_demo(&db) {
                Ok(mut state) => {
                    if legacy {
                        let n = state.all_profiles().len();
                        let g = state.all_groups().len();
                        state.set_status_message(format!(
                            "Opened Throne DB · {g} groups · {n} profiles"
                        ));
                    }
                    (state, path)
                }
                Err(e) => {
                    let mut s = AppState::with_demo_data();
                    s.set_status_message(format!("DB load failed ({e}); using demo"));
                    (s, path)
                }
            }
        }
        Err(e) => {
            let mut s = AppState::with_demo_data();
            s.set_status_message(format!("DB open failed ({e}); using demo"));
            (s, String::new())
        }
    }
}

/// Parse a fetched HTTP body into a single route profile.
fn fetch_route_from_body(body: &str) -> Result<throne_domain::RouteProfile, String> {
    if let Some(rr) = throne_import::try_import_routes(body) {
        if let Some(r) = rr.routes.into_iter().next() {
            return Ok(r);
        }
        if !rr.errors.is_empty() {
            return Err(rr.errors.join("; "));
        }
    }
    let rr = throne_import::import_route_payload(body);
    if let Some(r) = rr.routes.into_iter().next() {
        return Ok(r);
    }
    let report = throne_import::import_text(body);
    if let Some(r) = report.routes.into_iter().next() {
        return Ok(r);
    }
    let mut errs = rr.errors;
    errs.extend(report.errors);
    Err(if errs.is_empty() {
        "fetched body is not a route profile".into()
    } else {
        errs.join("; ")
    })
}

/// Native multi-file picker (upstream 1.2.3 "import from file" multi-select).
fn pick_import_files() -> Vec<std::path::PathBuf> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        // AppleScript choose file with multiple selections enabled.
        let script = r#"
set theFiles to choose file with prompt "Import profiles" with multiple selections allowed
set out to ""
repeat with f in theFiles
    set out to out & (POSIX path of f) & linefeed
end repeat
return out
"#;
        let out = Command::new("osascript").args(["-e", script]).output();
        if let Ok(out) = out {
            if out.status.success() {
                return String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(std::path::PathBuf::from)
                    .collect();
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        // zenity / kdialog multi-select when present.
        if let Ok(out) = Command::new("zenity")
            .args([
                "--file-selection",
                "--multiple",
                "--separator=\n",
                "--title=Import profiles",
            ])
            .output()
        {
            if out.status.success() {
                return String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(std::path::PathBuf::from)
                    .collect();
            }
        }
    }
    Vec::new()
}

fn read_os_clipboard() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let out = Command::new("pbpaste").output().ok()?;
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).to_string();
            if !s.trim().is_empty() {
                return Some(s);
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        for args in [
            &["xclip", "-selection", "clipboard", "-o"][..],
            &["wl-paste"][..],
        ] {
            if let Ok(out) = Command::new(args[0]).args(&args[1..]).output() {
                if out.status.success() {
                    let s = String::from_utf8_lossy(&out.stdout).to_string();
                    if !s.trim().is_empty() {
                        return Some(s);
                    }
                }
            }
        }
    }
    None
}

/// macOS IME / system text input path for dialog fields.
/// Without this, printable keys often never reach `on_key_down` as usable text.
impl EntityInputHandler for MainWindow {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let (field, _) = self.focused_dialog_field_mut()?;
        let len = field.len();
        let start = range.start.min(len);
        let end = range.end.min(len);
        *adjusted_range = Some(start..end);
        Some(field.get(start..end)?.to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let (field, _) = self.focused_dialog_field_mut()?;
        // End-cursor model: caret always at end.
        let n = field.encode_utf16().count();
        Some(UTF16Selection {
            range: n..n,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.dialog_is_open() {
            return;
        }
        if text.is_empty() {
            // IME delete / clear
            self.apply_dialog_text_edit("", true, cx);
        } else {
            self.apply_dialog_text_edit(text, false, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Composition: treat as plain replace for our end-cursor fields.
        self.replace_text_in_range(range, new_text, window, cx);
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(element_bounds)
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let (field, _) = self.focused_dialog_field_mut()?;
        Some(field.encode_utf16().count())
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Keep paint tokens + gpui-component chrome aligned with settings / OS appearance.
        self.sync_theme_from_window(window, cx);
        // Dialog present / NestedInputs sync runs in AppShell::prepare_dialog_layer
        // (must not open_dialog builders from inside this render).

        if self.state.core_status().is_running() {
            self.poll_core_runtime(cx);
        }

        let focus = self.focus_handle.clone();
        // Don't steal focus from gpui-component Dialog / Input.
        let gpui_dialog_active = window.has_active_dialog(cx);
        if !gpui_dialog_active && !focus.is_focused(window) {
            focus.focus(window);
        }

        let dialog_open = !matches!(self.dialog, Dialog::None);
        let ctx_open = self.open_menu == OpenMenu::ProfileCtx;
        let group_tab_ctx_open = self.open_menu == OpenMenu::GroupTabCtx;
        let toolbar_menu_open = Self::toolbar_menu_index(self.open_menu).is_some();

        // Key context: destructive Main shortcuts only when no modal is open.
        let key_ctx = if dialog_open || gpui_dialog_active {
            "Dialog"
        } else {
            "Main"
        };

        div()
            .track_focus(&self.focus_handle)
            .key_context(key_ctx)
            .on_action(cx.listener(|this, _: &ToggleProxy, _, cx| this.toggle_proxy(cx)))
            .on_action(cx.listener(|this, _: &ImportClipboard, _, cx| {
                this.import_clipboard(cx)
            }))
            .on_action(cx.listener(|this, _: &SaveDb, _, cx| this.save_db(cx)))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| this.select_all(cx)))
            .on_action(cx.listener(|this, _: &DeleteSelected, _, cx| {
                // Safety: even if a binding leaks into Dialog context, never
                // delete profiles while a modal is capturing the keyboard.
                if this.dialog_is_open() {
                    this.apply_dialog_text_edit("", true, cx);
                    return;
                }
                this.delete_selected(cx)
            }))
            .on_action(cx.listener(|this, _: &UrlTestSelected, _, cx| {
                this.url_test_selected(cx)
            }))
            .on_action(cx.listener(|this, _: &UrlTestGroup, _, cx| this.url_test_group(cx)))
            .on_action(cx.listener(|this, _: &DeleteUnavailable, window, cx| {
                this.delete_unavailable(Some(window), cx)
            }))
            .on_action(cx.listener(|this, _: &CycleRoute, _, cx| this.cycle_route(cx)))
            .on_action(cx.listener(|this, _: &CopyLogs, _, cx| this.copy_logs(cx)))
            .on_action(cx.listener(|_this, _: &Quit, _, cx| cx.quit()))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.handle_dialog_key(event, cx) {
                    // Mark handled so macOS doesn't drop the keystroke on the floor.
                    cx.stop_propagation();
                    window.prevent_default();
                    return;
                }
                if event.keystroke.key == "escape" {
                    this.close_menus();
                    cx.notify();
                    return;
                }
                let key = &event.keystroke.key;
                if key == "backspace"
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && this.open_menu == OpenMenu::None
                {
                    if this.search_draft.is_empty() {
                        return;
                    }
                    this.search_draft.pop();
                    this.state.set_search_query(this.search_draft.clone());
                    cx.notify();
                    return;
                }
                if key.len() == 1
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && this.open_menu == OpenMenu::None
                {
                    if let Some(ch) = key.chars().next() {
                        if !ch.is_control() {
                            this.search_draft.push(ch);
                            this.state.set_search_query(this.search_draft.clone());
                            cx.notify();
                        }
                    }
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(Theme::bg_app())
            .text_color(Theme::text())
            .relative()
            .child(self.render_top_bar(cx))
            .child(self.render_group_tabs(cx))
            .child(self.render_table_header(cx))
            .child(self.render_profile_table(cx))
            .child(self.render_bottom_tabs(cx))
            .child(self.render_status_bar())
            // Overlays last = painted on top (toolbar menus, context menu, dialogs).
            .when(toolbar_menu_open, |el| {
                // Full-window transparent catcher closes menu on outside click.
                let e = cx.entity().clone();
                el.child(
                    div()
                        .id("tb-menu-catcher")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                            e.update(cx, |this, cx| {
                                this.close_menus();
                                cx.notify();
                            });
                        }),
                )
                .child(self.render_toolbar_menu_overlay(cx))
            })
            .when(ctx_open || group_tab_ctx_open, |el| {
                el.child(self.render_ctx_popup(cx))
            })
            // Dialog layer is painted by [`AppShell`] (sibling of this view) so
            // open_dialog builders can safely read MainWindow without re-entrancy.
    }
}

/// Coarse kind of [`RoutingNested`] — used to detect open/close transitions for NestedInputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NestedKind {
    None,
    NewMenu,
    UpdateMenu,
    ImportPaste,
    RouteEditor,
    RawEditor,
    Notice,
}

fn nested_kind(n: &RoutingNested) -> NestedKind {
    match n {
        RoutingNested::None => NestedKind::None,
        RoutingNested::NewMenu => NestedKind::NewMenu,
        RoutingNested::UpdateMenu { .. } => NestedKind::UpdateMenu,
        RoutingNested::ImportPaste { .. } => NestedKind::ImportPaste,
        RoutingNested::RouteEditor(_) => NestedKind::RouteEditor,
        RoutingNested::RawEditor(_) => NestedKind::RawEditor,
        RoutingNested::Notice { .. } => NestedKind::Notice,
    }
}

/// Split multi-line Input text into trimmed non-empty rule list entries.
fn lines_to_vec(s: &str) -> Vec<String> {
    s.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Build a gpui-component Dialog for the current MainWindow dialog state.
///
/// Rebuilds every paint via `Root::render_dialog_layer` so toggles / list
/// selection stay live. Focus is owned by the ActiveDialog in Root.
fn build_gpui_dialog(
    dialog: GpuiDialog,
    entity: gpui::Entity<MainWindow>,
    window: &mut Window,
    cx: &mut App,
) -> GpuiDialog {
    let this = entity.read(cx);
    let on_dismiss = {
        let entity = entity.clone();
        move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
            entity.update(cx, |t, cx| {
                t.close_dialog();
                cx.notify();
            });
            // Framework also pops the layer; clear any stack leftovers.
            window.close_all_dialogs(cx);
        }
    };

    match &this.dialog {
        Dialog::BasicSettings {
            ruleset_mirror,
            adblock_enable,
            ..
        } => {
            let Some(DialogInputs::Basic {
                inbound_address,
                inbound_port,
                test_url,
                remote_dns,
                direct_dns,
                log_level,
            }) = this.dialog_inputs.as_ref()
            else {
                return dialog.title("Basic Settings").child(div().child("…"));
            };
            let ruleset_mirror = *ruleset_mirror;
            let adblock_enable = *adblock_enable;
            let e_mirror = entity.clone();
            let e_adblock = entity.clone();
            let e_save = entity.clone();
            let e_cancel = entity.clone();
            dialog
                .title("Basic Settings")
                .w(px(520.))
                .overlay_closable(true)
                .on_cancel({
                    let entity = entity.clone();
                    move |_, _, cx| {
                        entity.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                        true
                    }
                })
                .on_close(on_dismiss)
                .child(basic_settings_body(
                    inbound_address,
                    inbound_port,
                    test_url,
                    remote_dns,
                    direct_dns,
                    log_level,
                    ruleset_mirror,
                    adblock_enable,
                    move |_, cx| {
                        e_mirror.update(cx, |t, cx| {
                            if let Dialog::BasicSettings {
                                ruleset_mirror, ..
                            } = &mut t.dialog
                            {
                                *ruleset_mirror = ruleset_mirror.cycle();
                            }
                            cx.notify();
                        });
                    },
                    move |_, cx| {
                        e_adblock.update(cx, |t, cx| {
                            if let Dialog::BasicSettings {
                                adblock_enable, ..
                            } = &mut t.dialog
                            {
                                *adblock_enable = !*adblock_enable;
                            }
                            cx.notify();
                        });
                    },
                    move |window, cx| {
                        e_save.update(cx, |t, cx| t.save_basic_settings(cx));
                        window.close_all_dialogs(cx);
                    },
                    move |window, cx| {
                        e_cancel.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                        window.close_all_dialogs(cx);
                    },
                ))
        }
        Dialog::ManageGroups => {
            let e_edit = entity.clone();
            let e_rm = entity.clone();
            let e_update = entity.clone();
            let e_new = entity.clone();
            let e_update_all = entity.clone();
            dialog
                .title("Groups")
                .w(px(640.))
                .on_cancel({
                    let entity = entity.clone();
                    move |_, _, cx| {
                        entity.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                        true
                    }
                })
                .on_close(on_dismiss)
                .child(manage_groups_body(
                    &this.state,
                    move |id, window, cx| {
                        e_edit.update(cx, |t, cx| t.open_edit_group(id, window, cx));
                    },
                    move |id, name, window, cx| {
                        e_rm.update(cx, |t, cx| {
                            t.dialog = Dialog::ConfirmRemoveGroup {
                                group_id: id,
                                name,
                            };
                            t.dialog_inputs = None;
                            t.present_gpui_dialog(window, cx);
                            cx.notify();
                        });
                    },
                    move |id, _, cx| {
                        e_update.update(cx, |t, cx| {
                            t.start_subscription_group(id, UpdateOrigin::Manual, cx)
                        });
                    },
                    move |window, cx| {
                        e_new.update(cx, |t, cx| t.open_edit_group_new(window, cx));
                    },
                    move |window, cx| {
                        e_update_all
                            .update(cx, |t, cx| t.update_subscription(true, Some(window), cx));
                    },
                ))
        }
        Dialog::EditGroup {
            group_id,
            is_subscription,
            skip_auto_update,
            auto_clear_unavailable,
            front_proxy_id,
            landing_proxy_id,
        } => {
            let Some(DialogInputs::EditGroup { name, url }) = this.dialog_inputs.as_ref() else {
                return dialog.title("Edit Group").child(div().child("…"));
            };
            let profile_count = group_id
                .and_then(|id| this.state.group(id))
                .map(|g| g.profile_ids.len())
                .unwrap_or(0);
            let view = EditGroupView {
                group_id: *group_id,
                is_subscription: *is_subscription,
                skip_auto_update: *skip_auto_update,
                auto_clear_unavailable: *auto_clear_unavailable,
                front_label: this.proxy_display_label(*front_proxy_id).into(),
                landing_label: this.proxy_display_label(*landing_proxy_id).into(),
                profile_count,
            };
            let e_type = entity.clone();
            let e_front = entity.clone();
            let e_land = entity.clone();
            let e_clear = entity.clone();
            let e_skip = entity.clone();
            let e_copy = entity.clone();
            let e_deep = entity.clone();
            let e_ok = entity.clone();
            let e_cancel = entity.clone();
            let title = if group_id.is_some() {
                "Edit Group"
            } else {
                "New group"
            };
            // Cap dialog height; body scrolls (OK/Cancel stay visible below).
            let dialog_max_h = (window.viewport_size().height * 0.82).max(px(360.));
            // Leave room for title chrome + footer actions inside the dialog.
            let content_max_h = (f32::from(dialog_max_h) - 120.).clamp(220., 560.);
            dialog
                .title(title)
                .w(px(420.))
                .max_h(dialog_max_h)
                .on_cancel({
                    let entity = entity.clone();
                    move |_, window, cx| {
                        entity.update(cx, |t, cx| t.return_to_manage_groups(window, cx));
                        true
                    }
                })
                .on_close({
                    let entity = entity.clone();
                    move |_, window, cx| {
                        entity.update(cx, |t, cx| {
                            // Prefer return to list unless fully dismissed.
                            if matches!(t.dialog, Dialog::EditGroup { .. }) {
                                t.return_to_manage_groups(window, cx);
                            }
                        });
                    }
                })
                .child(edit_group_body(
                    &view,
                    name,
                    url,
                    content_max_h,
                    move |_, cx| {
                        e_type.update(cx, |t, cx| {
                            if let Dialog::EditGroup {
                                group_id: None,
                                is_subscription,
                                ..
                            } = &mut t.dialog
                            {
                                *is_subscription = !*is_subscription;
                            }
                            cx.notify();
                        });
                    },
                    move |_, cx| {
                        e_front.update(cx, |t, cx| {
                            let ids = t.group_proxy_cycle_ids();
                            if let Dialog::EditGroup {
                                front_proxy_id, ..
                            } = &mut t.dialog
                            {
                                *front_proxy_id =
                                    MainWindow::cycle_group_proxy(&ids, *front_proxy_id);
                            }
                            cx.notify();
                        });
                    },
                    move |_, cx| {
                        e_land.update(cx, |t, cx| {
                            let ids = t.group_proxy_cycle_ids();
                            if let Dialog::EditGroup {
                                landing_proxy_id, ..
                            } = &mut t.dialog
                            {
                                *landing_proxy_id =
                                    MainWindow::cycle_group_proxy(&ids, *landing_proxy_id);
                            }
                            cx.notify();
                        });
                    },
                    move |_, cx| {
                        e_clear.update(cx, |t, cx| {
                            if let Dialog::EditGroup {
                                auto_clear_unavailable,
                                ..
                            } = &mut t.dialog
                            {
                                *auto_clear_unavailable = !*auto_clear_unavailable;
                            }
                            cx.notify();
                        });
                    },
                    move |_, cx| {
                        e_skip.update(cx, |t, cx| {
                            if let Dialog::EditGroup {
                                skip_auto_update, ..
                            } = &mut t.dialog
                            {
                                *skip_auto_update = !*skip_auto_update;
                            }
                            cx.notify();
                        });
                    },
                    move |_, cx| {
                        e_copy.update(cx, |t, cx| t.copy_group_share_links(false, cx));
                    },
                    move |_, cx| {
                        e_deep.update(cx, |t, cx| t.copy_group_share_links(true, cx));
                    },
                    move |window, cx| {
                        e_ok.update(cx, |t, cx| t.edit_group_ok(window, cx));
                    },
                    move |window, cx| {
                        e_cancel.update(cx, |t, cx| t.return_to_manage_groups(window, cx));
                    },
                ))
        }
        Dialog::ConfirmRemoveGroup { name, .. } => {
            let e_yes = entity.clone();
            let name = name.clone();
            dialog
                .title("Confirmation")
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("Yes")
                        .cancel_text("No"),
                )
                .w(px(420.))
                .on_ok(move |_, window, cx| {
                    e_yes.update(cx, |t, cx| t.confirm_remove_group(window, cx));
                    true
                })
                .on_cancel({
                    let entity = entity.clone();
                    move |_, window, cx| {
                        entity.update(cx, |t, cx| t.return_to_manage_groups(window, cx));
                        true
                    }
                })
                .on_close({
                    let entity = entity.clone();
                    move |_, window, cx| {
                        entity.update(cx, |t, cx| {
                            if matches!(t.dialog, Dialog::ConfirmRemoveGroup { .. }) {
                                t.return_to_manage_groups(window, cx);
                            }
                        });
                    }
                })
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().foreground)
                        .child(format!("Remove {name}?")),
                )
        }
        Dialog::AddFromInput { .. } => {
            let Some(DialogInputs::AddFromInput { text }) = this.dialog_inputs.as_ref() else {
                return dialog.title("Add profile from input").child(div().child("…"));
            };
            let hint =
                crate::ui::dialogs::detect_hint_for_text(&DialogInputs::read_string(text, cx));
            let e_ok = entity.clone();
            let e_cancel = entity.clone();
            dialog
                .title("Add profile from input")
                .w(px(520.))
                .on_cancel({
                    let entity = entity.clone();
                    move |_, _, cx| {
                        entity.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                        true
                    }
                })
                .on_close(on_dismiss)
                .child(add_input_body(
                    text,
                    hint,
                    move |window, cx| {
                        e_ok.update(cx, |t, cx| t.add_input_ok(cx));
                        window.close_all_dialogs(cx);
                    },
                    move |window, cx| {
                        e_cancel.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                        window.close_all_dialogs(cx);
                    },
                ))
        }
        Dialog::TunSettings {
            vpn_strict_route,
            disable_private_range_bypass,
            ..
        } => {
            let Some(DialogInputs::Tun { mtu }) = this.dialog_inputs.as_ref() else {
                return dialog.title("Tun Settings").child(div().child("…"));
            };
            let vpn_strict_route = *vpn_strict_route;
            let disable_private_range_bypass = *disable_private_range_bypass;
            let e_strict = entity.clone();
            let e_bypass = entity.clone();
            let e_save = entity.clone();
            let e_cancel = entity.clone();
            dialog
                .title("Tun Settings")
                .w(px(480.))
                .on_cancel({
                    let entity = entity.clone();
                    move |_, _, cx| {
                        entity.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                        true
                    }
                })
                .on_close(on_dismiss)
                .child(tun_settings_body(
                    mtu,
                    vpn_strict_route,
                    disable_private_range_bypass,
                    move |_, cx| {
                        e_strict.update(cx, |t, cx| {
                            if let Dialog::TunSettings {
                                vpn_strict_route, ..
                            } = &mut t.dialog
                            {
                                *vpn_strict_route = !*vpn_strict_route;
                            }
                            cx.notify();
                        });
                    },
                    move |_, cx| {
                        e_bypass.update(cx, |t, cx| {
                            if let Dialog::TunSettings {
                                disable_private_range_bypass,
                                ..
                            } = &mut t.dialog
                            {
                                *disable_private_range_bypass = !*disable_private_range_bypass;
                            }
                            cx.notify();
                        });
                    },
                    move |window, cx| {
                        e_save.update(cx, |t, cx| t.save_tun_settings(cx));
                        window.close_all_dialogs(cx);
                    },
                    move |window, cx| {
                        e_cancel.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                        window.close_all_dialogs(cx);
                    },
                ))
        }
        Dialog::HotkeySettings {
            start_stop,
            import,
            save,
            url_test,
            copy_logs,
            ..
        } => {
            let e_save = entity.clone();
            let e_cancel = entity.clone();
            let e_capture = entity.clone();
            dialog
                .title("Hotkey Settings")
                .w(px(480.))
                .on_cancel({
                    let entity = entity.clone();
                    move |_, _, cx| {
                        entity.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                        true
                    }
                })
                .on_close(on_dismiss)
                .child(hotkey_settings_body(
                    start_stop,
                    import,
                    save,
                    url_test,
                    copy_logs,
                    move |field, chord, _window, cx| {
                        e_capture.update(cx, |t, cx| t.set_hotkey_field(field, chord, cx));
                    },
                    move |window, cx| {
                        e_save.update(cx, |t, cx| t.save_hotkey_settings(cx));
                        window.close_all_dialogs(cx);
                    },
                    move |window, cx| {
                        e_cancel.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                        window.close_all_dialogs(cx);
                    },
                ))
        }
        Dialog::EditProfile { type_label, .. } => {
            let Some(DialogInputs::EditProfile { name }) = this.dialog_inputs.as_ref() else {
                return dialog.title("Edit Profile").child(div().child("…"));
            };
            let type_label = type_label.clone();
            let e_save = entity.clone();
            let e_cancel = entity.clone();
            dialog
                .title("Edit Profile")
                .w(px(420.))
                .on_cancel({
                    let entity = entity.clone();
                    move |_, _, cx| {
                        entity.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                        true
                    }
                })
                .on_close(on_dismiss)
                .child(edit_profile_body(
                    name,
                    &type_label,
                    move |window, cx| {
                        e_save.update(cx, |t, cx| t.save_edit_profile(cx));
                        window.close_all_dialogs(cx);
                    },
                    move |window, cx| {
                        e_cancel.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                        window.close_all_dialogs(cx);
                    },
                ))
        }
        Dialog::ConfirmDeleteUnavailable { count, .. } => {
            let count = *count;
            let e_confirm = entity.clone();
            dialog
                .title("Confirmation")
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("Remove")
                        .ok_variant(ButtonVariant::Danger)
                        .cancel_text("Cancel"),
                )
                .w(px(420.))
                .on_ok(move |_, window, cx| {
                    e_confirm.update(cx, |t, cx| t.confirm_delete_unavailable(window, cx));
                    true
                })
                .on_cancel({
                    let entity = entity.clone();
                    move |_, _, cx| {
                        entity.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                        true
                    }
                })
                .on_close(on_dismiss)
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().foreground)
                        .child(format!("Remove {count} unavailable item(s)?")),
                )
        }
        Dialog::ConfirmUpdateAllSubscriptions => {
            let e_confirm = entity.clone();
            // Esc / Cancel: restore Manage Groups after the confirm layer pops
            // (present on next paint via pending_gpui_dialog — avoid race with close_dialog).
            let restore_manage = {
                let entity = entity.clone();
                move |_window: &mut Window, cx: &mut App| {
                    entity.update(cx, |t, cx| {
                        t.dialog = Dialog::manage_groups_from_state(&t.state);
                        t.dialog_inputs = None;
                        t.pending_gpui_dialog = true;
                        cx.notify();
                    });
                }
            };
            let restore_manage_cancel = restore_manage.clone();
            dialog
                .title("Confirmation")
                .confirm()
                .button_props(
                    DialogButtonProps::default()
                        .ok_text("Yes")
                        .cancel_text("No"),
                )
                .w(px(420.))
                .on_ok(move |_, window, cx| {
                    e_confirm.update(cx, |t, cx| {
                        t.confirm_update_all_subscriptions(window, cx);
                    });
                    true
                })
                .on_cancel(move |_, window, cx| {
                    restore_manage_cancel(window, cx);
                    true
                })
                .on_close(move |_, _, _| {
                    // State already set in on_cancel; paint will present.
                })
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().foreground)
                        .child("Update all subscriptions?"),
                )
        }
        Dialog::SubscriptionDiff { title, body } => {
            let title = title.clone();
            let body = body.clone();
            let restore_manage = {
                let entity = entity.clone();
                move |_window: &mut Window, cx: &mut App| {
                    entity.update(cx, |t, cx| {
                        t.dialog = Dialog::manage_groups_from_state(&t.state);
                        t.dialog_inputs = None;
                        t.pending_gpui_dialog = true;
                        cx.notify();
                    });
                }
            };
            let restore_ok = restore_manage.clone();
            let restore_cancel = restore_manage;
            dialog
                .title(title)
                .alert()
                .button_props(DialogButtonProps::default().ok_text("Close"))
                .w(px(560.))
                .on_ok(move |_, window, cx| {
                    restore_ok(window, cx);
                    true
                })
                .on_cancel(move |_, window, cx| {
                    restore_cancel(window, cx);
                    true
                })
                .on_close(move |_, _, _| {})
                .child(subscription_diff_body(&body))
        }
        Dialog::TrafficStats {
            period,
            tab,
            summary,
            breakdown_lines,
            bars,
            notice,
        } => {
            let period = *period;
            let tab = *tab;
            let summary = summary.clone();
            let breakdown_lines = breakdown_lines.clone();
            let bars = bars.clone();
            let notice = notice.clone();
            let e_period = entity.clone();
            let e_tab = entity.clone();
            let e_refresh = entity.clone();
            let e_close = entity.clone();
            let max_h = (window.viewport_size().height * 0.82).max(px(360.));
            dialog
                .title("Traffic Stats")
                .w(px(640.))
                .max_h(max_h)
                .overlay_closable(true)
                .on_cancel({
                    let entity = entity.clone();
                    move |_, _, cx| {
                        entity.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                        true
                    }
                })
                .on_close(on_dismiss)
                .child(traffic_stats_body(
                    period,
                    tab,
                    &summary,
                    &breakdown_lines,
                    &bars,
                    &notice,
                    move |p, _, cx| {
                        e_period.update(cx, |t, cx| {
                            if let Dialog::TrafficStats { period, .. } = &mut t.dialog {
                                *period = p;
                            }
                            t.refresh_traffic_stats_dialog();
                            t.presented_dialog_stack = (0, NestedKind::None);
                            t.request_gpui_dialog();
                            cx.notify();
                        });
                    },
                    move |tb, _, cx| {
                        e_tab.update(cx, |t, cx| {
                            if let Dialog::TrafficStats { tab, .. } = &mut t.dialog {
                                *tab = tb;
                            }
                            t.refresh_traffic_stats_dialog();
                            t.presented_dialog_stack = (0, NestedKind::None);
                            t.request_gpui_dialog();
                            cx.notify();
                        });
                    },
                    move |_, cx| {
                        e_refresh.update(cx, |t, cx| {
                            t.refresh_traffic_stats_dialog();
                            t.presented_dialog_stack = (0, NestedKind::None);
                            t.request_gpui_dialog();
                            cx.notify();
                        });
                    },
                    move |window, cx| {
                        e_close.update(cx, |t, cx| {
                            t.close_dialog_with_window(window, cx);
                            cx.notify();
                        });
                    },
                ))
        }
        Dialog::AutoSelectorStats {
            only_problems,
            selected_member,
            notice,
        } => {
            let only_problems = *only_problems;
            let selected_member = selected_member.clone();
            let notice = notice.clone();
            let groups = this.auto_selector_snapshot.clone();
            let e_toggle = entity.clone();
            let e_select = entity.clone();
            let e_recheck = entity.clone();
            let e_pin = entity.clone();
            let e_release = entity.clone();
            let e_close = entity.clone();
            let max_h = (window.viewport_size().height * 0.82).max(px(360.));
            dialog
                .title("Auto Selector")
                .w(px(720.))
                .max_h(max_h)
                .overlay_closable(true)
                .on_cancel({
                    let entity = entity.clone();
                    move |_, _, cx| {
                        entity.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                        true
                    }
                })
                .on_close(on_dismiss)
                .child(auto_selector_stats_body(
                    &groups,
                    only_problems,
                    &selected_member,
                    &notice,
                    move |_, cx| {
                        e_toggle.update(cx, |t, cx| {
                            if let Dialog::AutoSelectorStats {
                                only_problems, ..
                            } = &mut t.dialog
                            {
                                *only_problems = !*only_problems;
                            }
                            t.presented_dialog_stack = (0, NestedKind::None);
                            t.request_gpui_dialog();
                            cx.notify();
                        });
                    },
                    move |tag, _, cx| {
                        e_select.update(cx, |t, cx| {
                            if let Dialog::AutoSelectorStats {
                                selected_member, ..
                            } = &mut t.dialog
                            {
                                *selected_member = if *selected_member == tag {
                                    String::new()
                                } else {
                                    tag
                                };
                            }
                            t.presented_dialog_stack = (0, NestedKind::None);
                            t.request_gpui_dialog();
                            cx.notify();
                        });
                    },
                    move |_, cx| {
                        e_recheck.update(cx, |t, cx| {
                            t.auto_selector_action("recheck", "", cx);
                        });
                    },
                    move |_, cx| {
                        e_pin.update(cx, |t, cx| {
                            let member = match &t.dialog {
                                Dialog::AutoSelectorStats {
                                    selected_member, ..
                                } => selected_member.clone(),
                                _ => String::new(),
                            };
                            if member.is_empty() {
                                if let Dialog::AutoSelectorStats { notice, .. } = &mut t.dialog {
                                    *notice = "Select a member row first".into();
                                }
                                t.presented_dialog_stack = (0, NestedKind::None);
                                t.request_gpui_dialog();
                                cx.notify();
                            } else {
                                t.auto_selector_action("select", &member, cx);
                            }
                        });
                    },
                    move |_, cx| {
                        e_release.update(cx, |t, cx| {
                            t.auto_selector_action("select", "", cx);
                        });
                    },
                    move |window, cx| {
                        e_close.update(cx, |t, cx| {
                            t.close_dialog_with_window(window, cx);
                            cx.notify();
                        });
                    },
                ))
        }
        Dialog::RoutingSettings(draft) => {
            let entity_ev = entity.clone();
            let inputs = this.dialog_inputs.as_ref().and_then(|d| d.as_routing());
            // Cap height so Dialog's content area scrolls instead of clipping past the viewport.
            let max_h = (window.viewport_size().height * 0.82).max(px(360.));
            dialog
                .title("Routes")
                // Upstream DialogManageRoutes geometry: 800×600.
                .w(px(800.))
                .max_h(max_h)
                .overlay_closable(true)
                .on_cancel({
                    let entity = entity.clone();
                    move |_, _, cx| {
                        entity.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                        true
                    }
                })
                .on_close(on_dismiss)
                .child(routing_settings_view(draft, inputs, move |ev, window, cx| {
                    entity_ev.update(cx, |t, cx| t.handle_routing_event(ev, window, cx));
                }))
        }
        Dialog::None => dialog.title("").child(div()),
    }
}

/// Independent open_dialog layer for routing nested UIs (Route Profile, menus, …).
/// Closing only clears `draft.nested` — the main Routes dialog stays open underneath.
fn build_nested_routing_dialog(
    dialog: GpuiDialog,
    entity: gpui::Entity<MainWindow>,
    window: &mut Window,
    cx: &mut App,
) -> GpuiDialog {
    let this = entity.read(cx);
    let Dialog::RoutingSettings(draft) = &this.dialog else {
        return dialog.title("").child(div());
    };
    if matches!(draft.nested, RoutingNested::None) {
        return dialog.title("").child(div());
    }
    let title = routing_nested_title_owned(draft).unwrap_or_else(|| "…".into());
    let width = routing_nested_width(draft);
    let inputs = this.dialog_inputs.as_ref().and_then(|d| d.as_routing());
    let entity_ev = entity.clone();
    let body = routing_nested_view(draft, inputs, move |ev, window, cx| {
        entity_ev.update(cx, |t, cx| t.handle_routing_event(ev, window, cx));
    });
    // Clear nested draft only — the framework pops this layer once after cancel/close.
    // Do NOT call close_dialog here or Routes underneath will also be dismissed.
    let clear_nested = {
        let entity = entity.clone();
        move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut App| {
            entity.update(cx, |t, cx| {
                if let Dialog::RoutingSettings(d) = &mut t.dialog {
                    d.nested = RoutingNested::None;
                }
                if let Some(DialogInputs::Routing(inputs)) = t.dialog_inputs.as_mut() {
                    inputs.clear_nested();
                }
                let (main, _) = t.dialog_stack_key();
                t.presented_dialog_stack = (main, NestedKind::None);
                cx.notify();
            });
        }
    };
    let clear_nested_cancel = clear_nested.clone();
    // Dialog sits at ~10% from top; cap height so content scrolls instead of clipping.
    let max_h = (window.viewport_size().height * 0.82).max(px(360.));
    dialog
        .title(title)
        .w(px(width))
        .max_h(max_h)
        .overlay_closable(true)
        .on_cancel(move |ev, window, cx| {
            clear_nested_cancel(ev, window, cx);
            true
        })
        .on_close(clear_nested)
        .children(body)
}

#[cfg(test)]
mod tests {
    use super::{
        CoreAction, FAILED_STOP_PROFILE_LOG, PendingProfileSwitch, SortColumn,
        SubscriptionUpdateQueue, TestProgressKind, TestProgressPanel, UpdateOrigin,
        eligible_subscription_ids, failed_start_profile_log, next_core_action,
        next_runtime_generation, next_sort_state, resolve_stop_profile_display,
        running_mode_marker, runtime_poll_health, runtime_poll_is_current,
        runtime_profile_display, should_queue_recovery_restart, should_scroll_logs_to_bottom,
        should_show_subscription_diff, should_update_rendered_log_text, start_profile_log,
        stop_profile_log, subscription_fetch_options, test_progress_bar, test_progress_lines,
        test_progress_percent,
    };
    use throne_domain::{AppSettings, CoreStatus, Group, ProfileType};

    #[test]
    fn update_all_skips_basic_and_archived_groups_in_order() {
        let basic = Group::new(1, "basic");
        let mut first = Group::new(2, "first");
        first.url = "https://first.example/sub".into();
        let mut archived = Group::new(3, "archived");
        archived.url = "https://archived.example/sub".into();
        archived.archive = true;
        let mut second = Group::new(4, "second");
        second.url = "https://second.example/sub".into();

        assert_eq!(
            eligible_subscription_ids([&basic, &first, &archived, &second]),
            vec![2, 4]
        );
    }

    #[test]
    fn manual_update_only_requests_diff_when_enabled() {
        assert!(should_show_subscription_diff(UpdateOrigin::Manual, true));
        assert!(!should_show_subscription_diff(UpdateOrigin::UpdateAll, true));
        assert!(!should_show_subscription_diff(UpdateOrigin::Manual, false));
    }

    #[test]
    fn system_proxy_fetch_requires_running_profile() {
        let mut settings = AppSettings::default();
        settings.system_proxy_enabled = true;
        assert!(subscription_fetch_options(&settings, &CoreStatus::Stopped).is_err());

        let options = subscription_fetch_options(
            &settings,
            &CoreStatus::Running {
                profile_id: 1,
                profile_name: "node".into(),
            },
        )
        .unwrap();
        assert_eq!(
            options.proxy_url.as_deref(),
            Some("http://127.0.0.1:2080")
        );
    }

    #[test]
    fn subscription_update_queue_advances_serially() {
        let mut queue = SubscriptionUpdateQueue::new(vec![2, 4]);
        assert_eq!(queue.take_next(), Some(2));
        queue.record_result(true);
        assert_eq!(queue.take_next(), Some(4));
        queue.record_result(false);
        assert_eq!(queue.take_next(), None);
        assert!(queue.is_finished());
        assert_eq!(queue.completion_message(), "Subscription update finished · 1 succeeded · 1 failed");
    }

    #[test]
    fn runtime_poll_tolerates_two_consecutive_failures() {
        assert_eq!(runtime_poll_health(0, false), (1, false));
        assert_eq!(runtime_poll_health(1, false), (2, false));
    }

    #[test]
    fn runtime_poll_recovers_direct_network_after_three_failures() {
        assert_eq!(runtime_poll_health(2, false), (3, true));
    }

    #[test]
    fn successful_runtime_poll_resets_failure_count() {
        assert_eq!(runtime_poll_health(2, true), (0, false));
    }

    #[test]
    fn lifecycle_transition_resets_poll_failures_and_advances_generation() {
        assert_eq!(next_runtime_generation(7), (8, 0));
    }

    #[test]
    fn stale_poll_result_cannot_recover_a_newer_core_run() {
        assert!(runtime_poll_is_current(8, 8));
        assert!(!runtime_poll_is_current(7, 8));
    }

    #[test]
    fn start_during_network_recovery_is_queued() {
        assert!(should_queue_recovery_restart(true));
        assert!(!should_queue_recovery_restart(false));
    }

    #[test]
    fn runtime_profile_logs_match_upstream_format() {
        let profile = runtime_profile_display(ProfileType::Vless, "Tokyo");

        assert_eq!(profile, "[VLESS] Tokyo");
        assert_eq!(start_profile_log(&profile), ">>>>>>>> Starting profile [VLESS] Tokyo");
        assert_eq!(stop_profile_log(&profile), ">>>>>>>> Stopping profile [VLESS] Tokyo");
        assert_eq!(
            failed_start_profile_log(&profile),
            "<<<<<<<< Failed to start profile [VLESS] Tokyo"
        );
        assert_eq!(
            FAILED_STOP_PROFILE_LOG,
            "<<<<<<<< Failed to stop, please restart the program."
        );
    }

    #[test]
    fn running_mode_marker_matches_enabled_runtime_modes() {
        assert_eq!(running_mode_marker(true, false), "[Tun]");
        assert_eq!(running_mode_marker(false, true), "[System Proxy]");
        assert_eq!(running_mode_marker(true, true), "[Tun+System Proxy]");
        assert_eq!(running_mode_marker(false, false), "");
    }

    #[test]
    fn preserved_runtime_display_wins_when_current_profile_lookup_is_absent_or_changed() {
        let preserved = "[VLESS] Tokyo".to_owned();

        assert_eq!(
            resolve_stop_profile_display(Some(&preserved), None),
            Some("[VLESS] Tokyo".to_owned())
        );
        assert_eq!(
            resolve_stop_profile_display(Some(&preserved), Some("[VLESS] Renamed".to_owned())),
            Some("[VLESS] Tokyo".to_owned())
        );
    }

    #[test]
    fn repeated_header_clicks_alternate_without_clearing_sort() {
        assert_eq!(
            next_sort_state(SortColumn::None, true, SortColumn::TestResult),
            (SortColumn::TestResult, true)
        );
        assert_eq!(
            next_sort_state(SortColumn::TestResult, true, SortColumn::TestResult),
            (SortColumn::TestResult, false)
        );
        assert_eq!(
            next_sort_state(SortColumn::TestResult, false, SortColumn::TestResult),
            (SortColumn::TestResult, true)
        );
    }

    #[test]
    fn new_non_empty_log_text_requests_scroll_to_bottom() {
        assert!(!should_scroll_logs_to_bottom(
            false,
            "old log",
            "old log\nnew log"
        ));
        assert!(should_scroll_logs_to_bottom(
            true,
            "old log",
            "old log\nnew log"
        ));
        assert!(!should_scroll_logs_to_bottom(true, "same", "same"));
        assert!(!should_scroll_logs_to_bottom(true, "old log", ""));
    }

    #[test]
    fn inactive_logs_tab_preserves_snapshot_until_returning_to_logs() {
        assert!(!should_update_rendered_log_text(false));
        assert!(should_update_rendered_log_text(true));

        let mut initial_snapshot = String::new();
        if should_update_rendered_log_text(true) {
            initial_snapshot = "old log".to_owned();
        }
        assert_eq!(initial_snapshot, "old log");

        let changed_logs = "old log\nnew log";
        if should_update_rendered_log_text(false) {
            initial_snapshot = changed_logs.to_owned();
        }
        assert!(!should_scroll_logs_to_bottom(
            false,
            &initial_snapshot,
            changed_logs
        ));
        assert_eq!(initial_snapshot, "old log");
        assert!(should_scroll_logs_to_bottom(
            true,
            &initial_snapshot,
            changed_logs
        ));
    }

    #[test]
    fn double_clicking_a_different_profile_while_running_requests_a_switch() {
        let running = CoreStatus::Running {
            profile_id: 1,
            profile_name: "Taiwan 04".into(),
        };

        assert_eq!(next_core_action(&running, 2), CoreAction::Switch(2));
    }

    #[test]
    fn clicking_another_profile_while_starting_queues_a_switch_not_stop() {
        assert_eq!(
            next_core_action(&CoreStatus::Starting, 9),
            CoreAction::Switch(9)
        );
    }

    #[test]
    fn pending_switch_keeps_the_latest_target_while_stop_is_in_progress() {
        let mut pending = PendingProfileSwitch::default();

        assert!(pending.schedule(2));
        // Rapid click on another node must replace the target.
        assert!(!pending.schedule(3));
        assert_eq!(pending.peek(), Some(3));
        assert_eq!(pending.take(), Some(3));
    }

    #[test]
    fn url_test_progress_bar_matches_upstream_hash_dash_meter() {
        assert_eq!(test_progress_bar(0, 100), "----------");
        assert_eq!(test_progress_bar(50, 100), "#####-----");
        assert_eq!(test_progress_bar(100, 100), "##########");
        assert_eq!(test_progress_percent(1, 3), 33);
        assert_eq!(test_progress_percent(0, 0), 0);
    }

    #[test]
    fn url_test_progress_lines_include_count_when_group_has_multiple_profiles() {
        let multi = TestProgressPanel {
            kind: TestProgressKind::Url,
            done: 16,
            total: 48,
        };
        let (bar, content) = test_progress_lines(&multi);
        assert_eq!(bar.as_deref(), Some("###------- 33%"));
        assert_eq!(content, "Running URL test (16 / 48)");

        let single = TestProgressPanel {
            kind: TestProgressKind::Url,
            done: 0,
            total: 1,
        };
        let (bar, content) = test_progress_lines(&single);
        assert!(bar.is_none());
        assert_eq!(content, "Running URL test");
    }

}
