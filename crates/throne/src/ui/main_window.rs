//! Main window layout mirrored from upstream `mainwindow.ui`.
//!
//! ```text
//! [Program][Settings][Groups][Routing][Tools] [▶Start] [Tun][DNS][Proxy] | data_view
//! ─────────────────────────────────────────────────────────────────────
//! Group tabs …
//! ┌ Type │ Address │ Name │ Test Result │ Traffic ──────────────────┐
//! │ …                                                               │
//! └─────────────────────────────────────────────────────────────────┘
//! [Logs] [Connections]
//! running | inbound | speed | version
//! ```
//!
//! Toolbar menus are **relative under each button** (no absolute left offsets).
//! Secondary features open as modal dialogs: Basic Settings / Manage Groups / Add from input.

use std::ops::Range;
use std::sync::{Arc, Mutex};

use gpui::{
    AnyElement, ClipboardItem, Context, FocusHandle, KeyDownEvent, SharedString, Window, actions,
    div, prelude::*, px, uniform_list,
};

use throne_core_client::{
    ConnectionRow, CoreConfig, CoreSession, force_clear_system_proxy, set_system_proxy,
};
use throne_domain::{AppState, CoreStatus, GroupId, Profile, TrafficSnapshot};
use throne_import::import_from_url;

use crate::theme::{Theme, latency_color};
use crate::ui::dialogs::{
    Dialog, add_input_body, basic_settings_body, confirm_delete_unavailable_body,
    edit_profile_body, hotkey_settings_body, manage_groups_body, routing_settings_body,
    tun_settings_body,
};
use crate::ui::widgets::{
    TOOLBAR_BTN_GAP, TOOLBAR_BTN_W, TOOLBAR_MENU_TOP, TOOLBAR_PAD_X, menu_item, menu_label,
    menu_separator, modal_shell, mode_checkbox, secondary_btn, start_stop_btn, toolbar_btn,
    toolbar_menu_panel,
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
}

/// Which draft field receives typed characters inside Manage Groups.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum MgFocus {
    #[default]
    NewName,
    EditName,
    EditUrl,
}

/// Profile table sort column (click header to toggle).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum SortColumn {
    #[default]
    None,
    Type,
    Address,
    Name,
    TestResult,
    Traffic,
}

pub struct MainWindow {
    state: AppState,
    focus_handle: FocusHandle,
    search_draft: String,
    db_path_label: String,
    open_menu: OpenMenu,
    /// Bottom panel tab: 0 Logs, 1 Connections
    bottom_tab: usize,
    ctx_menu_at: Option<(f32, f32)>,
    dialog: Dialog,
    mg_focus: MgFocus,
    /// Go `ThroneCore` process + IPC (real Start/Stop).
    /// Behind a mutex so Start/Stop can run off the UI thread.
    core: Arc<Mutex<CoreSession>>,
    /// True while a start/stop background job is in flight (ignore re-clicks).
    core_op_busy: bool,
    /// True while a URL-test / sub-update job is in flight.
    background_busy: bool,
    sort_column: SortColumn,
    /// `true` = ascending (A→Z, low latency first).
    sort_asc: bool,
    /// Live connections from core (Connections tab).
    connections: Vec<ConnectionRow>,
    prev_traffic_at: Option<std::time::Instant>,
    /// Prevent an overdue core request from queuing another poll.
    runtime_poll_busy: bool,
}

impl MainWindow {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.bind_keys([
            gpui::KeyBinding::new("cmd-r", ToggleProxy, None),
            gpui::KeyBinding::new("ctrl-r", ToggleProxy, None),
            gpui::KeyBinding::new("cmd-v", ImportClipboard, None),
            gpui::KeyBinding::new("ctrl-v", ImportClipboard, None),
            gpui::KeyBinding::new("cmd-s", SaveDb, None),
            gpui::KeyBinding::new("ctrl-s", SaveDb, None),
            gpui::KeyBinding::new("cmd-a", SelectAll, None),
            gpui::KeyBinding::new("ctrl-a", SelectAll, None),
            gpui::KeyBinding::new("backspace", DeleteSelected, None),
            gpui::KeyBinding::new("delete", DeleteSelected, None),
            gpui::KeyBinding::new("cmd-shift-c", CopyLogs, None),
            gpui::KeyBinding::new("ctrl-shift-c", CopyLogs, None),
            gpui::KeyBinding::new("cmd-t", UrlTestSelected, None),
            gpui::KeyBinding::new("ctrl-t", UrlTestSelected, None),
            gpui::KeyBinding::new("cmd-shift-g", UrlTestGroup, None),
            gpui::KeyBinding::new("ctrl-shift-g", UrlTestGroup, None),
            gpui::KeyBinding::new("cmd-shift-r", DeleteUnavailable, None),
            gpui::KeyBinding::new("ctrl-shift-r", DeleteUnavailable, None),
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
            db_path_label,
            open_menu: OpenMenu::None,
            bottom_tab: 0,
            ctx_menu_at: None,
            dialog: Dialog::None,
            mg_focus: MgFocus::NewName,
            core: Arc::new(Mutex::new(CoreSession::new(core_cfg))),
            core_op_busy: false,
            background_busy: false,
            sort_column: SortColumn::None,
            sort_asc: true,
            connections: Vec::new(),
            prev_traffic_at: None,
            runtime_poll_busy: false,
        };
        window.spawn_runtime_poller(cx);
        window
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
        if self.sort_column == col {
            if self.sort_asc {
                self.sort_asc = false;
            } else {
                // third click clears sort (back to group order)
                self.sort_column = SortColumn::None;
                self.sort_asc = true;
            }
        } else {
            self.sort_column = col;
            // Latency: ascending = fastest first; strings: A→Z
            self.sort_asc = true;
        }
        cx.notify();
    }

    /// Visible profiles with optional column sort applied.
    fn sorted_profiles(&self) -> Vec<Profile> {
        let mut profiles: Vec<Profile> = self
            .state
            .visible_profiles()
            .into_iter()
            .cloned()
            .collect();
        if self.sort_column == SortColumn::None {
            return profiles;
        }
        let asc = self.sort_asc;
        profiles.sort_by(|a, b| {
            let ord = match self.sort_column {
                SortColumn::None => std::cmp::Ordering::Equal,
                SortColumn::Type => a
                    .profile_type
                    .display_name()
                    .cmp(b.profile_type.display_name()),
                SortColumn::Address => a
                    .display_address()
                    .to_lowercase()
                    .cmp(&b.display_address().to_lowercase()),
                SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortColumn::TestResult => {
                    // Untested (0) sorts after measured values; fail (<0) last.
                    latency_sort_key(a.latency_ms).cmp(&latency_sort_key(b.latency_ms))
                }
                SortColumn::Traffic => {
                    let ta = a.traffic_downlink.saturating_add(a.traffic_uplink);
                    let tb = b.traffic_downlink.saturating_add(b.traffic_uplink);
                    ta.cmp(&tb)
                }
            };
            if asc {
                ord
            } else {
                ord.reverse()
            }
        });
        profiles
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
    }

    fn close_dialog(&mut self) {
        self.dialog = Dialog::None;
    }

    fn toggle_menu(&mut self, menu: OpenMenu, cx: &mut Context<Self>) {
        if !matches!(self.dialog, Dialog::None) {
            return;
        }
        self.open_menu = if self.open_menu == menu {
            OpenMenu::None
        } else {
            menu
        };
        self.ctx_menu_at = None;
        cx.notify();
    }

    fn open_basic_settings(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        self.dialog = Dialog::basic_from_state(&self.state);
        cx.notify();
    }

    fn open_manage_groups(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        self.mg_focus = MgFocus::NewName;
        self.dialog = Dialog::manage_groups_from_state(&self.state);
        cx.notify();
    }

    fn open_add_from_input(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        self.dialog = Dialog::add_from_input();
        cx.notify();
    }

    fn open_routing_settings(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        self.dialog = Dialog::routing_from_state(&self.state);
        cx.notify();
    }

    fn open_tun_settings(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        self.dialog = Dialog::tun_from_state(&self.state);
        cx.notify();
    }

    fn open_hotkey_settings(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        self.dialog = Dialog::hotkey_from_state(&self.state);
        cx.notify();
    }

    fn open_edit_profile(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        match Dialog::edit_profile_from_state(&self.state) {
            Some(d) => {
                self.dialog = d;
                cx.notify();
            }
            None => {
                self.state
                    .set_status_message("Select a profile to edit");
                cx.notify();
            }
        }
    }

    fn save_edit_profile(&mut self, cx: &mut Context<Self>) {
        let (id, name) = match &self.dialog {
            Dialog::EditProfile { id, name, .. } => (*id, name.clone()),
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

    fn save_routing_settings(&mut self, cx: &mut Context<Self>) {
        if let Dialog::RoutingSettings {
            selected,
            name,
            remote_url,
            auto_update,
            default_outbound,
            ..
        } = &self.dialog
        {
            if *selected < 0 {
                self.state.set_status_message("No route selected");
                cx.notify();
                return;
            }
            let id = *selected;
            let r = self.state.update_route_meta(
                id,
                name.clone(),
                remote_url.clone(),
                *auto_update,
                *default_outbound,
            );
            match r {
                Ok(()) => {
                    let _ = self.state.set_active_route(id);
                    self.close_dialog();
                    let _ = self.persist_db();
                }
                Err(e) => self.state.set_status_message(e.to_string()),
            }
        }
        cx.notify();
    }

    fn fetch_remote_route(&mut self, cx: &mut Context<Self>) {
        let (id, url) = match &self.dialog {
            Dialog::RoutingSettings {
                selected,
                remote_url,
                ..
            } if *selected >= 0 && !remote_url.trim().is_empty() => (*selected, remote_url.clone()),
            _ => {
                self.state
                    .set_status_message("Set a remote URL on the route first");
                cx.notify();
                return;
            }
        };
        if self.background_busy {
            self.state.set_status_message("Busy — wait for current job");
            cx.notify();
            return;
        }
        self.background_busy = true;
        self.state
            .set_status_message(format!("Fetching route · {url} …"));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let body = throne_import::fetch_url(&url).map_err(|e| e)?;
                    let report = throne_import::import_text(&body);
                    if report.routes.is_empty() {
                        // Try as route share JSON specifically
                        if let Some(rr) = throne_import::try_import_routes(&body) {
                            if rr.routes.is_empty() {
                                return Err(format!(
                                    "no route profile in body ({})",
                                    rr.errors.join("; ")
                                ));
                            }
                            return Ok(rr.routes);
                        }
                        return Err(if report.errors.is_empty() {
                            "fetched body is not a route profile".into()
                        } else {
                            report.errors.join("; ")
                        });
                    }
                    Ok(report.routes)
                })
                .await;
            this.update(cx, |this, cx| {
                this.background_busy = false;
                match result {
                    Ok(routes) => {
                        if let Some(r) = routes.into_iter().next() {
                            match this.state.replace_route_content(id, r) {
                                Ok(()) => {
                                    // refresh dialog fields
                                    this.dialog = Dialog::routing_from_state(&this.state);
                                    if let Dialog::RoutingSettings { selected, .. } =
                                        &mut this.dialog
                                    {
                                        *selected = id;
                                    }
                                    let _ = this.persist_db();
                                    this.state.set_status_message("Remote route fetched");
                                }
                                Err(e) => this.state.set_status_message(e.to_string()),
                            }
                        }
                    }
                    Err(e) => this
                        .state
                        .set_status_message(format!("Route fetch failed: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn save_tun_settings(&mut self, cx: &mut Context<Self>) {
        if let Dialog::TunSettings {
            vpn_mtu,
            vpn_strict_route,
            disable_private_range_bypass,
            ..
        } = &self.dialog
        {
            let mtu = vpn_mtu.parse::<i32>().unwrap_or(9000);
            self.state.apply_tun_settings(
                mtu,
                *vpn_strict_route,
                *disable_private_range_bypass,
            );
            self.close_dialog();
            let _ = self.persist_db();
        }
        cx.notify();
    }

    fn save_hotkey_settings(&mut self, cx: &mut Context<Self>) {
        if let Dialog::HotkeySettings {
            start_stop,
            import,
            save,
            url_test,
            copy_logs,
            ..
        } = &self.dialog
        {
            self.state.apply_hotkey_settings(
                start_stop.clone(),
                import.clone(),
                save.clone(),
                url_test.clone(),
                copy_logs.clone(),
            );
            self.close_dialog();
            let _ = self.persist_db();
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

    pub(crate) fn toggle_proxy(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        if self.core_op_busy {
            self.state
                .set_status_message("Core is busy (start/stop in progress)…");
            cx.notify();
            return;
        }
        if self.state.core_status().is_running()
            || matches!(self.state.core_status(), CoreStatus::Starting)
        {
            // Allow Stop even from Starting if user wants to cancel.
            if matches!(self.state.core_status(), CoreStatus::Stopping) {
                return;
            }
            self.stop_proxy(cx);
        } else {
            self.start_proxy(cx);
        }
    }

    fn start_proxy(&mut self, cx: &mut Context<Self>) {
        if self.core_op_busy {
            return;
        }
        let Some(id) = self.state.selected_profile_id() else {
            self.state
                .set_status_message("Select a profile before Start");
            cx.notify();
            return;
        };
        let Some(profile) = self.state.profile(id).cloned() else {
            self.state.set_status_message("Profile not found");
            cx.notify();
            return;
        };

        self.core_op_busy = true;
        self.state.set_core_status(CoreStatus::Starting);
        self.state
            .set_status_message(format!("Starting {} …", profile.name));
        cx.notify();

        let settings = self.state.settings().clone();
        let route = self.state.active_route().cloned();
        let apply_proxy = settings.system_proxy_enabled;
        let core = Arc::clone(&self.core);
        let profile_name = profile.name.clone();
        let profile_id = profile.id;
        let port = settings.inbound_socks_port;
        let addr = settings.inbound_address.clone();
        let route_label = route
            .as_ref()
            .map(|r| r.name.clone())
            .unwrap_or_else(|| "default".into());

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let mut guard = core
                        .lock()
                        .map_err(|e| format!("core lock poisoned: {e}"))?;
                    guard
                        .start_profile(&profile, &settings, route.as_ref(), apply_proxy)
                        .map_err(|e| e.to_string())
                })
                .await;

            this.update(cx, |this, cx| {
                this.core_op_busy = false;
                match result {
                    Ok(()) => {
                        this.state.set_core_status(CoreStatus::Running {
                            profile_id,
                            profile_name: profile_name.clone(),
                        });
                        let mut msg = format!(
                            "Running · {profile_name} · route {route_label} · mixed {addr}:{port}"
                        );
                        if apply_proxy {
                            msg.push_str(" · system proxy ON");
                        } else {
                            msg.push_str(
                                " · tip: enable System Proxy or point apps to this port",
                            );
                        }
                        this.state.set_status_message(msg);
                        let _ = this.persist_db();
                    }
                    Err(e) => {
                        this.state.set_core_status(CoreStatus::Error(e.clone()));
                        this.state.set_status_message(format!("Start failed: {e}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn stop_proxy(&mut self, cx: &mut Context<Self>) {
        if self.core_op_busy && matches!(self.state.core_status(), CoreStatus::Stopping) {
            return;
        }
        self.core_op_busy = true;
        self.state.set_core_status(CoreStatus::Stopping);
        self.state.set_status_message("Stopping…");
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
                // Always mark stopped locally — stop_profile force-kills core.
                this.state.set_core_status(CoreStatus::Stopped);
                match result {
                    Ok(()) => this.state.set_status_message("Core stopped"),
                    Err(e) => this
                        .state
                        .set_status_message(format!("Stopped (with errors): {e}")),
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
        self.state
            .set_status_message(format!("URL Test group · {n} profile(s) …"));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut guard = core
                        .lock()
                        .map_err(|e| format!("core lock: {e}"))?;
                    // Chunk to avoid huge configs / timeouts.
                    let mut all = Vec::new();
                    for chunk in profiles.chunks(16) {
                        let refs: Vec<&Profile> = chunk.iter().collect();
                        let rows = guard
                            .url_test_profiles(&refs, &settings)
                            .map_err(|e| e.to_string())?;
                        all.extend(
                            rows.into_iter()
                                .map(|(pid, r)| (pid, r.latency_ms, r.error)),
                        );
                    }
                    Ok::<Vec<(i64, i32, String)>, String>(all)
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
                        let updated = this.state.apply_url_test_results(&apply);
                        let ok = rows
                            .iter()
                            .filter(|(_, l, e)| e.is_empty() && *l > 0)
                            .count();
                        this.state.set_status_message(format!(
                            "URL Test done · {ok}/{updated} ok"
                        ));
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

    fn delete_unavailable(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        let group_id = self.state.active_group_id();
        let count = self.state.unavailable_profile_ids_in_group(group_id).len();
        if count == 0 {
            self.state
                .set_status_message("No unavailable profiles to delete");
        } else {
            self.dialog = Dialog::ConfirmDeleteUnavailable { group_id, count };
        }
        cx.notify();
    }

    fn confirm_delete_unavailable(&mut self, cx: &mut Context<Self>) {
        let Dialog::ConfirmDeleteUnavailable { group_id, .. } = &self.dialog else {
            return;
        };
        let removed = self.state.remove_unavailable_in_group(*group_id);
        self.close_dialog();
        self.state
            .set_status_message(format!("Deleted {removed} unavailable profile(s)"));
        if removed > 0 {
            let _ = self.persist_db();
        }
        cx.notify();
    }

    fn update_subscription(&mut self, all: bool, cx: &mut Context<Self>) {
        self.close_menus();
        if self.background_busy {
            self.state.set_status_message("Busy — wait for current job");
            cx.notify();
            return;
        }
        let groups: Vec<(GroupId, String, String)> = if all {
            self.state
                .all_groups()
                .into_iter()
                .filter(|g| !g.url.trim().is_empty())
                .map(|g| (g.id, g.name.clone(), g.url.clone()))
                .collect()
        } else {
            let gid = self.state.active_group_id();
            self.state
                .group(gid)
                .filter(|g| !g.url.trim().is_empty())
                .map(|g| vec![(g.id, g.name.clone(), g.url.clone())])
                .unwrap_or_default()
        };
        if groups.is_empty() {
            self.state.set_status_message(
                "No subscription URL on this group — set one in Manage Groups",
            );
            cx.notify();
            return;
        }
        self.background_busy = true;
        self.state
            .set_status_message(format!("Updating {} subscription(s) …", groups.len()));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut reports = Vec::new();
                    for (id, name, url) in groups {
                        let report = import_from_url(&url);
                        reports.push((id, name, report));
                    }
                    reports
                })
                .await;
            this.update(cx, |this, cx| {
                this.background_busy = false;
                let mut total = 0usize;
                let mut added = 0usize;
                let mut removed = 0usize;
                let mut kept = 0usize;
                let mut errs = Vec::new();
                let show_sec = this.state.settings().show_config_security;
                for (gid, name, report) in result {
                    if !report.errors.is_empty() && report.profiles.is_empty() {
                        errs.push(format!("{name}: {}", report.errors.join("; ")));
                        continue;
                    }
                    let items: Vec<_> = report
                        .profiles
                        .into_iter()
                        .map(|p| {
                            let insecure = show_sec
                                && (p.outbound.insecure == Some(true)
                                    || p.outbound.tls == Some(false)
                                    || p.source.contains("insecure=1"));
                            (p.name, p.profile_type, p.outbound, insecure)
                        })
                        .collect();
                    match this.state.replace_group_profiles(gid, items) {
                        Ok(s) => {
                            total += s.total;
                            added += s.added;
                            removed += s.removed;
                            kept += s.kept;
                        }
                        Err(e) => errs.push(format!("{name}: {e}")),
                    }
                }
                let summary = format!(
                    "Subscription updated · {total} profile(s) · +{added} −{removed} · {kept} kept"
                );
                if !errs.is_empty() {
                    this.state.set_status_message(format!(
                        "{summary}; {}",
                        errs.join(" · ")
                    ));
                } else {
                    this.state.set_status_message(summary);
                }
                let _ = this.persist_db();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn poll_core_runtime(&mut self, cx: &mut Context<Self>) {
        if !self.state.core_status().is_running() || self.core_op_busy || self.runtime_poll_busy {
            return;
        }
        self.runtime_poll_busy = true;
        let core = Arc::clone(&self.core);
        let want_conn = self.bottom_tab == 1;
        cx.spawn(async move |this, cx| {
            let snap = cx
                .background_executor()
                .spawn(async move {
                    let mut guard = core.lock().ok()?;
                    let stats = guard.query_stats().ok();
                    let conns = if want_conn {
                        guard.query_connections().unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    Some((stats, conns))
                })
                .await;
            this.update(cx, |this, cx| {
                this.runtime_poll_busy = false;
                if let Some((stats, conns)) = snap {
                    if let Some(cum) = stats {
                        let now = std::time::Instant::now();
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
                        traffic_changed = this.state.update_live_traffic(rates);
                        if let CoreStatus::Running { profile_id, .. } = this.state.core_status() {
                            this.state
                                .set_profile_traffic(*profile_id, cum.proxy_down, cum.proxy_up);
                        }
                    }
                    if want_conn {
                        this.connections = conns;
                    }
                    if traffic_changed || want_conn {
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    fn cycle_route(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        self.state.cycle_active_route();
        let _ = self.persist_db();
        cx.notify();
    }

    fn set_vpn(&mut self, on: bool, cx: &mut Context<Self>) {
        self.state.set_spmode_vpn(on);
        let _ = self.persist_db();
        cx.notify();
    }

    fn set_sys_proxy(&mut self, on: bool, cx: &mut Context<Self>) {
        self.state.set_spmode_system_proxy(on);
        let s = self.state.settings();
        let host = s.inbound_address.clone();
        let port = s.inbound_socks_port;
        let running = self.state.core_status().is_running();
        let _ = self.persist_db();

        // networksetup can block — never run it on the UI thread.
        if on && !running {
            self.state.set_status_message(
                "System Proxy will apply on next Start (core not running yet)",
            );
            cx.notify();
            return;
        }

        self.state.set_status_message(if on {
            "Enabling System Proxy…"
        } else {
            "Disabling System Proxy…"
        });
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                    let mut traffic_changed = false;
                .background_spawn(async move { set_system_proxy(on, &host, port) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(()) if on => this
                        .state
                        .set_status_message(format!("System Proxy ON → 127.0.0.1:{port}")),
                    Ok(()) => this.state.set_status_message("System Proxy OFF"),
                    Err(e) => this
                        .state
                        .set_status_message(format!("System Proxy failed: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

                            traffic_changed |= cum.proxy_down > 0 || cum.proxy_up > 0;
    fn set_sys_dns(&mut self, on: bool, cx: &mut Context<Self>) {
        self.state.set_system_dns(on);
        let _ = self.persist_db();
        cx.notify();
    }

    fn save_basic_settings(&mut self, cx: &mut Context<Self>) {
        if let Dialog::BasicSettings {
            inbound_address,
            inbound_port,
            test_url,
            remote_dns,
            direct_dns,
            log_level,
            ruleset_mirror,
            adblock_enable,
            ..
        } = &self.dialog
        {
            let port = inbound_port.parse::<i32>().unwrap_or(2080);
            self.state.apply_basic_settings(
                inbound_address.clone(),
                port,
                test_url.clone(),
                remote_dns.clone(),
                direct_dns.clone(),
                log_level.clone(),
                *ruleset_mirror,
                *adblock_enable,
            );
            let _ = self.persist_db();
            self.close_dialog();
            cx.notify();
        }
    }

    fn manage_add_group(&mut self, cx: &mut Context<Self>) {
        if let Dialog::ManageGroups { new_name, .. } = &self.dialog {
            let name = new_name.trim().to_string();
            if name.is_empty() {
                self.state
                    .set_status_message("Group name cannot be empty");
                cx.notify();
                return;
            }
            let id = self.state.add_group(name);
            let _ = self.persist_db();
            // refresh dialog state
            self.dialog = Dialog::manage_groups_from_state(&self.state);
            if let Dialog::ManageGroups {
                selected,
                edit_name,
                edit_url,
                ..
            } = &mut self.dialog
            {
                *selected = Some(id);
                if let Some(g) = self.state.group(id) {
                    *edit_name = g.name.clone();
                    *edit_url = g.url.clone();
                }
            }
            self.mg_focus = MgFocus::EditName;
            self.state.set_status_message(format!("Group {id} added"));
            cx.notify();
        }
    }

    fn manage_apply(&mut self, cx: &mut Context<Self>) {
        if let Dialog::ManageGroups {
            selected,
            edit_name,
            edit_url,
            ..
        } = &self.dialog
        {
            let Some(id) = *selected else {
                self.state.set_status_message("No group selected");
                cx.notify();
                return;
            };
            let name = edit_name.clone();
            let url = edit_url.clone();
            if let Err(e) = self.state.rename_group(id, name) {
                self.state.set_status_message(e.to_string());
                cx.notify();
                return;
            }
            if let Err(e) = self.state.set_group_url(id, url) {
                self.state.set_status_message(e.to_string());
                cx.notify();
                return;
            }
            let _ = self.persist_db();
            self.state
                .set_status_message(format!("Group {id} updated"));
            // keep dialog open with refreshed list
            let sel = id;
            self.dialog = Dialog::manage_groups_from_state(&self.state);
            if let Dialog::ManageGroups {
                selected,
                edit_name,
                edit_url,
                ..
            } = &mut self.dialog
            {
                *selected = Some(sel);
                if let Some(g) = self.state.group(sel) {
                    *edit_name = g.name.clone();
                    *edit_url = g.url.clone();
                }
            }
            cx.notify();
        }
    }

    fn manage_delete(&mut self, cx: &mut Context<Self>) {
        if let Dialog::ManageGroups { selected, .. } = &self.dialog {
            let Some(id) = *selected else {
                self.state.set_status_message("No group selected");
                cx.notify();
                return;
            };
            match self.state.delete_group(id) {
                Ok(()) => {
                    let _ = self.persist_db();
                    self.dialog = Dialog::manage_groups_from_state(&self.state);
                    self.state
                        .set_status_message(format!("Group {id} deleted"));
                    cx.notify();
                }
                Err(e) => {
                    self.state.set_status_message(e.to_string());
                    cx.notify();
                }
            }
        }
    }

    fn add_input_ok(&mut self, cx: &mut Context<Self>) {
        if let Dialog::AddFromInput { text } = &self.dialog {
            let t = text.clone();
            self.close_dialog();
            self.import_text(&t, cx);
        }
    }

    fn handle_dialog_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        if matches!(self.dialog, Dialog::None) {
            return false;
        }
        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.close_dialog();
            cx.notify();
            return true;
        }

        // character / backspace into focused field
        let is_back = key == "backspace" || key == "delete";
        let ch = if key.len() == 1
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
        {
            key.chars().next().filter(|c| !c.is_control())
        } else if key == "space" {
            Some(' ')
        } else if key == "enter" && matches!(self.dialog, Dialog::AddFromInput { .. }) {
            // allow newlines in add-from-input
            Some('\n')
        } else if key == "enter" && matches!(self.dialog, Dialog::EditProfile { .. }) {
            self.save_edit_profile(cx);
            return true;
        } else {
            None
        };

        if !is_back && ch.is_none() {
            return true; // swallow other keys while dialog open
        }

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
                if is_back {
                    field.pop();
                } else if let Some(c) = ch {
                    if *focus == 1 {
                        // port: digits only
                        if c.is_ascii_digit() {
                            field.push(c);
                        }
                    } else {
                        field.push(c);
                    }
                }
                cx.notify();
                true
            }
            Dialog::ManageGroups {
                new_name,
                edit_name,
                edit_url,
                ..
            } => {
                let field = match self.mg_focus {
                    MgFocus::NewName => new_name,
                    MgFocus::EditName => edit_name,
                    MgFocus::EditUrl => edit_url,
                };
                if is_back {
                    field.pop();
                } else if let Some(c) = ch {
                    if c != '\n' {
                        field.push(c);
                    }
                }
                cx.notify();
                true
            }
            Dialog::AddFromInput { text } => {
                if is_back {
                    text.pop();
                } else if let Some(c) = ch {
                    text.push(c);
                }
                cx.notify();
                true
            }
            Dialog::RoutingSettings {
                name,
                remote_url,
                focus,
                ..
            } => {
                let field = if *focus == 0 { name } else { remote_url };
                if is_back {
                    field.pop();
                } else if let Some(c) = ch {
                    if c != '\n' {
                        field.push(c);
                    }
                }
                cx.notify();
                true
            }
            Dialog::TunSettings {
                vpn_mtu,
                focus_mtu,
                ..
            } => {
                if !*focus_mtu {
                    return true;
                }
                if is_back {
                    vpn_mtu.pop();
                } else if let Some(c) = ch {
                    if c.is_ascii_digit() {
                        vpn_mtu.push(c);
                    }
                }
                cx.notify();
                true
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
                if is_back {
                    field.pop();
                } else if let Some(c) = ch {
                    if c != '\n' {
                        field.push(c);
                    }
                }
                cx.notify();
                true
            }
            Dialog::EditProfile { name, .. } => {
                if is_back {
                    name.pop();
                } else if let Some(c) = ch {
                    if c != '\n' {
                        name.push(c);
                    }
                }
                cx.notify();
                true
            }
            Dialog::ConfirmDeleteUnavailable { .. } => true,
            Dialog::None => false,
        }
    }

    // ─── layout regions ─────────────────────────────────────────────────

    fn render_top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let running = self.state.core_status().is_running();
        let entity = cx.entity().clone();

        div()
            .relative()
            .flex()
            .items_start()
            .gap_2()
            .px_2()
            .py_2()
            .bg(Theme::bg_panel())
            .border_b_1()
            .border_color(Theme::border_light())
            .child(self.render_tool_cluster(cx))
            .child({
                let e = entity.clone();
                start_stop_btn(running, move |_, _, cx| {
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
                    .h(px(56.))
                    .child({
                        let e = entity.clone();
                        let on = self.state.settings().tun_mode_enabled;
                        mode_checkbox("tun", "Tun Mode", on, move |_, _, cx| {
                            e.update(cx, |this, cx| {
                                let next = !this.state.settings().tun_mode_enabled;
                                this.set_vpn(next, cx);
                            });
                        })
                    })
                    .child({
                        let e = entity.clone();
                        let on = self.state.settings().system_proxy_enabled;
                        mode_checkbox("proxy", "System Proxy", on, move |_, _, cx| {
                            e.update(cx, |this, cx| {
                                let next = !this.state.settings().system_proxy_enabled;
                                this.set_sys_proxy(next, cx);
                            });
                        })
                    }),
            )
    }

    fn menu_items_for(&self, menu: OpenMenu, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let mut panel = div().flex().flex_col().py_0p5();

        macro_rules! item {
            ($id:expr, $label:expr, $body:expr) => {{
                let e = entity.clone();
                panel = panel.child(menu_item($id, $label, move |_, _, cx| {
                    e.update(cx, $body);
                }));
            }};
        }

        match menu {
            OpenMenu::Program => {
                panel = panel.child(menu_label("Program"));
                item!("prog-input", "Add profile from input", |t, cx| {
                    t.open_add_from_input(cx)
                });
                item!("prog-clip", "Add profile from clipboard", |t, cx| {
                    t.import_clipboard(cx)
                });
                item!("prog-start", "Start", |t, cx| t.toggle_proxy(cx));
                item!("prog-stop", "Stop", |t, cx| {
                    if t.state.core_status().is_running() {
                        t.toggle_proxy(cx);
                    } else {
                        t.close_menus();
                        cx.notify();
                    }
                });
                panel = panel.child(menu_separator());
                item!("prog-proxy", "Enable System Proxy", |t, cx| {
                    t.set_sys_proxy(true, cx);
                    t.close_menus();
                });
                item!("prog-tun", "Enable Tun", |t, cx| {
                    t.set_vpn(true, cx);
                    t.close_menus();
                });
                item!("prog-off", "Disable", |t, cx| {
                    t.set_sys_proxy(false, cx);
                    t.set_vpn(false, cx);
                    t.set_sys_dns(false, cx);
                    t.close_menus();
                });
                panel = panel.child(menu_separator());
                item!("prog-exit", "Exit", |_t, cx| cx.quit());
            }
            OpenMenu::Settings => {
                panel = panel.child(menu_label("Preferences"));
                item!("set-basic", "Basic Settings", |t, cx| {
                    t.open_basic_settings(cx)
                });
                item!("set-route", "Routing Settings", |t, cx| {
                    t.open_routing_settings(cx)
                });
                item!("set-tun", "Tun Settings", |t, cx| t.open_tun_settings(cx));
                item!("set-hotkey", "Hotkey Settings", |t, cx| {
                    t.open_hotkey_settings(cx)
                });
                item!("set-clear-proxy", "Clear system proxy now", |t, cx| {
                    force_clear_system_proxy();
                    t.state
                        .set_status_message("System proxy force-cleared on all interfaces");
                    t.close_menus();
                    cx.notify();
                });
                panel = panel.child(menu_separator());
                item!("set-folder", "Open Config Folder", |t, cx| {
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
                    t.close_menus();
                    cx.notify();
                });
                item!("set-save", "Save database", |t, cx| t.save_db(cx));
            }
            OpenMenu::Groups => {
                panel = panel.child(menu_label("Groups"));
                item!("g-manage", "Manage Groups", |t, cx| t.open_manage_groups(cx));
                item!("g-update", "Update subscription", |t, cx| {
                    t.update_subscription(false, cx)
                });
                item!("g-update-all", "Update all subscriptions", |t, cx| {
                    t.update_subscription(true, cx)
                });
                panel = panel.child(menu_separator());
                item!("g-urltest", "Url Test Group", |t, cx| t.url_test_group(cx));
                item!("g-clear", "Clear Group test result", |t, cx| {
                    let gid = t.state.active_group_id();
                    t.state.clear_test_results_in_group(gid);
                    t.close_menus();
                    let _ = t.persist_db();
                    cx.notify();
                });
                item!("g-dup", "Remove Duplicates", |t, cx| {
                    let gid = t.state.active_group_id();
                    t.state.remove_duplicates_in_group(gid);
                    t.close_menus();
                    let _ = t.persist_db();
                    cx.notify();
                });
                item!("g-unavail", "Remove Unavailable", |t, cx| t.delete_unavailable(cx));
                item!("g-invalid", "Remove Invalid Configs", |t, cx| {
                    let gid = t.state.active_group_id();
                    t.state.remove_invalid_in_group(gid);
                    t.close_menus();
                    let _ = t.persist_db();
                    cx.notify();
                });
                item!("g-insecure", "Remove Insecure Configs", |t, cx| {
                    let gid = t.state.active_group_id();
                    t.state.remove_insecure_in_group(gid);
                    t.close_menus();
                    let _ = t.persist_db();
                    cx.notify();
                });
            }
            OpenMenu::Routing => {
                panel = panel.child(menu_label("Routing"));
                item!("r-settings", "Routing Settings", |t, cx| {
                    t.open_routing_settings(cx)
                });
                item!("r-cycle", "Next route profile", |t, cx| t.cycle_route(cx));
                panel = panel.child(menu_separator());
                for r in self.state.all_routes() {
                    let id = r.id;
                    let name = r.summary();
                    let e = entity.clone();
                    let active = self.state.active_route().is_some_and(|a| a.id == id);
                    let label = if active {
                        format!("● {name}")
                    } else {
                        format!("○ {name}")
                    };
                    panel = panel.child(menu_item(
                        SharedString::from(format!("route-{id}")),
                        label,
                        move |_, _, cx| {
                            e.update(cx, |t, cx| {
                                let _ = t.state.set_active_route(id);
                                let _ = t.persist_db();
                                t.close_menus();
                                cx.notify();
                            });
                        },
                    ));
                }
            }
            OpenMenu::Tools => {
                panel = panel.child(menu_label("Tools"));
                item!("t-url", "Url Test Selected", |t, cx| t.url_test_selected(cx));
                item!("t-url-group", "Url Test Group (⌘⇧G)", |t, cx| t.url_test_group(cx));
                item!("t-delete-unavailable", "Delete Unavailable (⌘⇧R)", |t, cx| {
                    t.delete_unavailable(cx)
                });
                item!("t-speed", "Speedtest Selected", |t, cx| {
                    t.speed_test_selected(cx)
                });
                item!("t-ip", "IP Test Selected", |t, cx| t.ip_test_selected(cx));
                panel = panel.child(menu_separator());
                item!("t-runtime", "Runtime Stats", |t, cx| {
                    t.bottom_tab = 1;
                    t.state.set_status_message(
                        "Connections tab shows live sessions while core is running",
                    );
                    t.close_menus();
                    cx.notify();
                });
                item!("t-traffic", "Traffic Stats", |t, cx| {
                    let label = t.state.speed_label();
                    t.state.set_status_message(if label.is_empty() {
                        "Traffic Stats — start a profile to see live rates".into()
                    } else {
                        label.replace('\n', " · ")
                    });
                    t.close_menus();
                    cx.notify();
                });
                item!("t-update", "Check For Update", |t, cx| {
                    t.state.set_status_message(format!(
                        "Current version {} · throne-rs rewrite (no auto-update yet)",
                        throne_domain::NKR_VERSION
                    ));
                    t.close_menus();
                    cx.notify();
                });
                panel = panel.child(menu_separator());
                panel = panel.child(menu_label(format!(
                    "Version {}",
                    throne_domain::NKR_VERSION
                )));
            }
            OpenMenu::ProfileCtx | OpenMenu::None => {}
        }

        panel
    }

    fn render_tool_cluster(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();

        // Buttons only — dropdown panels are root overlays so they paint above the table.
        let menus = [
            (OpenMenu::Program, "tb-program", "⚙", "Program"),
            (OpenMenu::Settings, "tb-settings", "☰", "Settings"),
            (OpenMenu::Groups, "tb-groups", "▦", "Groups"),
            (OpenMenu::Routing, "tb-routing", "⇄", "Routing"),
            (OpenMenu::Tools, "tb-tools", "⚒", "Tools"),
        ];

        let mut row = div().flex().items_center().gap_1();
        for (menu, id, glyph, label) in menus {
            let e = entity.clone();
            let open = self.open_menu == menu;
            row = row.child(toolbar_btn(id, glyph, label, open, move |_, _, cx| {
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
            OpenMenu::ProfileCtx | OpenMenu::None => None,
        }
    }

    /// Root-level dropdown so menus are not covered by group tabs / table.
    fn render_toolbar_menu_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let menu = self.open_menu;
        let Some(idx) = Self::toolbar_menu_index(menu) else {
            return div().into_any_element();
        };
        let left = TOOLBAR_PAD_X + idx as f32 * (TOOLBAR_BTN_W + TOOLBAR_BTN_GAP);
        let items = self.menu_items_for(menu, cx);
        // Dim strip is optional; panel alone is enough. Click-away closes via Esc.
        toolbar_menu_panel(
            SharedString::from(format!("tb-overlay-{idx}")),
            TOOLBAR_MENU_TOP,
            left,
            items,
        )
        .into_any_element()
    }

    fn render_ctx_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let (x, y) = self.ctx_menu_at.unwrap_or((200., 200.));
        let mut panel = div()
            .absolute()
            .top(px(y))
            .left(px(x))
            .min_w(px(220.))
            .py_1()
            .bg(Theme::bg_elevated())
            .border_1()
            .border_color(Theme::border_light())
            .rounded_sm()
            .shadow_md();

        macro_rules! item {
            ($id:expr, $label:expr, $body:expr) => {{
                let e = entity.clone();
                panel = panel.child(menu_item($id, $label, move |_, _, cx| {
                    e.update(cx, $body);
                }));
            }};
        }

        panel = panel.child(menu_label("Server"));
        item!("c-start", "Start", |t, cx| t.toggle_proxy(cx));
        item!("c-stop", "Stop", |t, cx| {
            if t.state.core_status().is_running() {
                t.toggle_proxy(cx);
            } else {
                t.close_menus();
                cx.notify();
            }
        });
        item!("c-input", "Add profile from input", |t, cx| {
            t.open_add_from_input(cx)
        });
        item!("c-clip", "Add profile from clipboard", |t, cx| {
            t.import_clipboard(cx)
        });
        item!("c-edit", "Edit profile…", |t, cx| t.open_edit_profile(cx));
        item!("c-del", "Delete", |t, cx| t.delete_selected(cx));
        item!("c-test", "Url Test Selected", |t, cx| t.url_test_selected(cx));
        panel
    }

    fn render_dialog_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity().clone();
        match &self.dialog {
            Dialog::None => div().into_any_element(),
            Dialog::BasicSettings {
                inbound_address,
                inbound_port,
                test_url,
                remote_dns,
                direct_dns,
                log_level,
                ruleset_mirror,
                adblock_enable,
                focus,
            } => {
                let e_close = entity.clone();
                let e_focus = entity.clone();
                let e_mirror = entity.clone();
                let e_adblock = entity.clone();
                let e_save = entity.clone();
                let e_cancel = entity.clone();
                modal_shell(
                    "Basic Settings",
                    basic_settings_body(
                        inbound_address,
                        inbound_port,
                        test_url,
                        remote_dns,
                        direct_dns,
                        log_level,
                        *ruleset_mirror,
                        *adblock_enable,
                        *focus,
                        move |idx, _, cx| {
                            e_focus.update(cx, |t, cx| {
                                if let Dialog::BasicSettings { focus, .. } = &mut t.dialog {
                                    *focus = idx;
                                }
                                cx.notify();
                            });
                        },
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
                        move |_, cx| {
                            e_save.update(cx, |t, cx| t.save_basic_settings(cx));
                        },
                        move |_, cx| {
                            e_cancel.update(cx, |t, cx| {
                                t.close_dialog();
                                cx.notify();
                            });
                        },
                    ),
                    move |_, _, cx| {
                        e_close.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                    },
                )
                .into_any_element()
            }
            Dialog::ManageGroups {
                new_name,
                selected,
                edit_name,
                edit_url,
                focus_new,
            } => {
                let e_close = entity.clone();
                let e_sel = entity.clone();
                let e_fn = entity.clone();
                let e_fen = entity.clone();
                let e_feu = entity.clone();
                let e_add = entity.clone();
                let e_apply = entity.clone();
                let e_del = entity.clone();
                let e_x = entity.clone();
                modal_shell(
                    "Manage Groups",
                    manage_groups_body(
                        &self.state,
                        new_name,
                        *selected,
                        edit_name,
                        edit_url,
                        *focus_new || self.mg_focus == MgFocus::NewName,
                        move |id, _, cx| {
                            e_sel.update(cx, |t, cx| {
                                if let Dialog::ManageGroups {
                                    selected,
                                    edit_name,
                                    edit_url,
                                    focus_new,
                                    ..
                                } = &mut t.dialog
                                {
                                    *selected = Some(id);
                                    *focus_new = false;
                                    if let Some(g) = t.state.group(id) {
                                        *edit_name = g.name.clone();
                                        *edit_url = g.url.clone();
                                    }
                                }
                                t.mg_focus = MgFocus::EditName;
                                cx.notify();
                            });
                        },
                        move |_, cx| {
                            e_fn.update(cx, |t, cx| {
                                if let Dialog::ManageGroups { focus_new, .. } = &mut t.dialog {
                                    *focus_new = true;
                                }
                                t.mg_focus = MgFocus::NewName;
                                cx.notify();
                            });
                        },
                        move |_, cx| {
                            e_fen.update(cx, |t, cx| {
                                if let Dialog::ManageGroups { focus_new, .. } = &mut t.dialog {
                                    *focus_new = false;
                                }
                                t.mg_focus = MgFocus::EditName;
                                cx.notify();
                            });
                        },
                        move |_, cx| {
                            e_feu.update(cx, |t, cx| {
                                if let Dialog::ManageGroups { focus_new, .. } = &mut t.dialog {
                                    *focus_new = false;
                                }
                                t.mg_focus = MgFocus::EditUrl;
                                cx.notify();
                            });
                        },
                        move |_, cx| {
                            e_add.update(cx, |t, cx| t.manage_add_group(cx));
                        },
                        move |_, cx| {
                            e_apply.update(cx, |t, cx| t.manage_apply(cx));
                        },
                        move |_, cx| {
                            e_del.update(cx, |t, cx| t.manage_delete(cx));
                        },
                        move |_, cx| {
                            e_x.update(cx, |t, cx| {
                                t.close_dialog();
                                cx.notify();
                            });
                        },
                    ),
                    move |_, _, cx| {
                        e_close.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                    },
                )
                .into_any_element()
            }
            Dialog::AddFromInput { text } => {
                let e_close = entity.clone();
                let e_ok = entity.clone();
                let e_cancel = entity.clone();
                modal_shell(
                    "Add profile from input",
                    add_input_body(
                        text,
                        move |_, cx| {
                            e_ok.update(cx, |t, cx| t.add_input_ok(cx));
                        },
                        move |_, cx| {
                            e_cancel.update(cx, |t, cx| {
                                t.close_dialog();
                                cx.notify();
                            });
                        },
                    ),
                    move |_, _, cx| {
                        e_close.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                    },
                )
                .into_any_element()
            }
            Dialog::RoutingSettings {
                selected,
                name,
                remote_url,
                auto_update,
                default_outbound,
                focus,
            } => {
                let e_close = entity.clone();
                let e_sel = entity.clone();
                let e_focus = entity.clone();
                let e_auto = entity.clone();
                let e_out = entity.clone();
                let e_fetch = entity.clone();
                let e_save = entity.clone();
                let e_cancel = entity.clone();
                modal_shell(
                    "Routing Settings",
                    routing_settings_body(
                        &self.state,
                        *selected,
                        name,
                        remote_url,
                        *auto_update,
                        *default_outbound,
                        *focus,
                        move |id, _, cx| {
                            e_sel.update(cx, |t, cx| {
                                if let Some(r) = t.state.active_route().filter(|x| x.id == id) {
                                    let _ = r;
                                }
                                if let Some(r) = t.state.all_routes().into_iter().find(|x| x.id == id)
                                {
                                    let name = r.name.clone();
                                    let remote_url = r.remote_url.clone();
                                    let auto_update = r.auto_update;
                                    let default_outbound = r.default_outbound;
                                    if let Dialog::RoutingSettings {
                                        selected,
                                        name: n,
                                        remote_url: u,
                                        auto_update: a,
                                        default_outbound: d,
                                        focus,
                                    } = &mut t.dialog
                                    {
                                        *selected = id;
                                        *n = name;
                                        *u = remote_url;
                                        *a = auto_update;
                                        *d = default_outbound;
                                        *focus = 0;
                                    }
                                    let _ = t.state.set_active_route(id);
                                }
                                cx.notify();
                            });
                        },
                        move |idx, _, cx| {
                            e_focus.update(cx, |t, cx| {
                                if let Dialog::RoutingSettings { focus, .. } = &mut t.dialog {
                                    *focus = idx;
                                }
                                cx.notify();
                            });
                        },
                        move |_, cx| {
                            e_auto.update(cx, |t, cx| {
                                if let Dialog::RoutingSettings { auto_update, .. } = &mut t.dialog {
                                    *auto_update = !*auto_update;
                                }
                                cx.notify();
                            });
                        },
                        move |_, cx| {
                            e_out.update(cx, |t, cx| {
                                if let Dialog::RoutingSettings {
                                    default_outbound, ..
                                } = &mut t.dialog
                                {
                                    *default_outbound = default_outbound.cycle_builtin();
                                }
                                cx.notify();
                            });
                        },
                        move |_, cx| {
                            e_fetch.update(cx, |t, cx| t.fetch_remote_route(cx));
                        },
                        move |_, cx| {
                            e_save.update(cx, |t, cx| t.save_routing_settings(cx));
                        },
                        move |_, cx| {
                            e_cancel.update(cx, |t, cx| {
                                t.close_dialog();
                                cx.notify();
                            });
                        },
                    ),
                    move |_, _, cx| {
                        e_close.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                    },
                )
                .into_any_element()
            }
            Dialog::TunSettings {
                vpn_mtu,
                vpn_strict_route,
                disable_private_range_bypass,
                focus_mtu,
            } => {
                let e_close = entity.clone();
                let e_focus = entity.clone();
                let e_strict = entity.clone();
                let e_bypass = entity.clone();
                let e_save = entity.clone();
                let e_cancel = entity.clone();
                modal_shell(
                    "Tun Settings",
                    tun_settings_body(
                        vpn_mtu,
                        *vpn_strict_route,
                        *disable_private_range_bypass,
                        *focus_mtu,
                        move |_, cx| {
                            e_focus.update(cx, |t, cx| {
                                if let Dialog::TunSettings { focus_mtu, .. } = &mut t.dialog {
                                    *focus_mtu = true;
                                }
                                cx.notify();
                            });
                        },
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
                                    // checkbox shows !disable; toggle means flip disable flag
                                    *disable_private_range_bypass =
                                        !*disable_private_range_bypass;
                                }
                                cx.notify();
                            });
                        },
                        move |_, cx| {
                            e_save.update(cx, |t, cx| t.save_tun_settings(cx));
                        },
                        move |_, cx| {
                            e_cancel.update(cx, |t, cx| {
                                t.close_dialog();
                                cx.notify();
                            });
                        },
                    ),
                    move |_, _, cx| {
                        e_close.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                    },
                )
                .into_any_element()
            }
            Dialog::HotkeySettings {
                start_stop,
                import,
                save,
                url_test,
                copy_logs,
                focus,
            } => {
                let e_close = entity.clone();
                let e_focus = entity.clone();
                let e_save = entity.clone();
                let e_cancel = entity.clone();
                modal_shell(
                    "Hotkey Settings",
                    hotkey_settings_body(
                        start_stop,
                        import,
                        save,
                        url_test,
                        copy_logs,
                        *focus,
                        move |idx, _, cx| {
                            e_focus.update(cx, |t, cx| {
                                if let Dialog::HotkeySettings { focus, .. } = &mut t.dialog {
                                    *focus = idx;
                                }
                                cx.notify();
                            });
                        },
                        move |_, cx| {
                            e_save.update(cx, |t, cx| t.save_hotkey_settings(cx));
                        },
                        move |_, cx| {
                            e_cancel.update(cx, |t, cx| {
                                t.close_dialog();
                                cx.notify();
                            });
                        },
                    ),
                    move |_, _, cx| {
                        e_close.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                    },
                )
                .into_any_element()
            }
            Dialog::EditProfile {
                name,
                type_label,
                ..
            } => {
                let e_close = entity.clone();
                let e_save = entity.clone();
                let e_cancel = entity.clone();
                modal_shell(
                    "Edit Profile",
                    edit_profile_body(
                        name,
                        type_label,
                        move |_, cx| {
                            e_save.update(cx, |t, cx| t.save_edit_profile(cx));
                        },
                        move |_, cx| {
                            e_cancel.update(cx, |t, cx| {
                                t.close_dialog();
                                cx.notify();
                            });
                        },
                    ),
                    move |_, _, cx| {
                        e_close.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                    },
                )
                .into_any_element()
            }
            Dialog::ConfirmDeleteUnavailable { count, .. } => {
                let e_close = entity.clone();
                let e_confirm = entity.clone();
                let e_cancel = entity.clone();
                modal_shell(
                    "Confirmation",
                    confirm_delete_unavailable_body(
                        *count,
                        move |_, cx| {
                            e_confirm.update(cx, |t, cx| t.confirm_delete_unavailable(cx));
                        },
                        move |_, cx| {
                            e_cancel.update(cx, |t, cx| {
                                t.close_dialog();
                                cx.notify();
                            });
                        },
                    ),
                    move |_, _, cx| {
                        e_close.update(cx, |t, cx| {
                            t.close_dialog();
                            cx.notify();
                        });
                    },
                )
                .into_any_element()
            }
        }
    }

    fn render_group_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.state.active_group_id();
        let mut row = div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .pt_2()
            .pb_1()
            .bg(Theme::bg_app());

        for &gid in self.state.group_order() {
            let Some(group) = self.state.group(gid) else {
                continue;
            };
            let selected = gid == active;
            let name = if group.name.is_empty() {
                format!("Group {gid}")
            } else {
                group.name.clone()
            };
            let entity = cx.entity().clone();
            row = row.child(
                div()
                    .id(SharedString::from(format!("tab-{gid}")))
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .border_1()
                    .border_color(if selected {
                        Theme::tab_selected_border()
                    } else {
                        Theme::border()
                    })
                    .bg(if selected {
                        Theme::bg_elevated()
                    } else {
                        Theme::bg_panel()
                    })
                    .text_sm()
                    .text_color(Theme::text())
                    .cursor_pointer()
                    .child(name)
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |this, cx| this.select_group(gid, cx));
                    }),
            );
        }
        row
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
                        let addr = profile.display_address();
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
                                                this.toggle_proxy(cx);
                                            } else {
                                                cx.notify();
                                            }
                                        });
                                    },
                                )
                                .on_mouse_down(
                                    gpui::MouseButton::Right,
                                    move |ev: &gpui::MouseDownEvent, window, cx| {
                                        let pos = ev.position;
                                        e_ctx.update(cx, |this, cx| {
                                            let _ = this.state.select_profile(id);
                                            this.open_menu = OpenMenu::ProfileCtx;
                                            this.ctx_menu_at =
                                                Some((pos.x.into(), pos.y.into()));
                                            let _ = window;
                                            cx.notify();
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

    fn render_bottom_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let tab = self.bottom_tab;
        let logs = self.state.logs_text();
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
                        tab_btn("Logs", tab == 0, move |_, _, cx| {
                            e.update(cx, |t, cx| {
                                t.bottom_tab = 0;
                                cx.notify();
                            });
                        })
                    })
                    .child({
                        let e = entity.clone();
                        tab_btn("Connections", tab == 1, move |_, _, cx| {
                            e.update(cx, |t, cx| {
                                t.bottom_tab = 1;
                                cx.notify();
                            });
                        })
                    })
                    .child(div().flex_1())
                    .when(tab == 0, |row| {
                        let e_copy = entity.clone();
                        let e_clear = entity.clone();
                        row.child(secondary_btn("log-copy", "Copy", move |_, _, cx| {
                            e_copy.update(cx, |t, cx| t.copy_logs(cx));
                        }))
                        .child(secondary_btn("log-clear", "Clear", move |_, _, cx| {
                            e_clear.update(cx, |t, cx| {
                                t.state.clear_logs();
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
                    }),
            )
    }

    fn render_status_bar(&self) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_4()
            .px_3()
            .py_1p5()
            .bg(Theme::bg_panel())
            .border_t_1()
            .border_color(Theme::border_light())
            .text_xs()
            .text_color(Theme::text())
            .child(
                div()
                    .flex_1()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(if self.state.core_status().is_running() {
                        Theme::success()
                    } else {
                        Theme::text()
                    })
                    .child(self.state.running_label()),
            )
            .child(div().flex_1().child(self.state.inbound_label()))
            .child(
                div()
                    .flex_1()
                    .text_color(Theme::text_muted())
                    .child(self.state.speed_label()),
            )
            .child(
                div()
                    .text_color(Theme::text_muted())
                    .child(if self.db_path_label.is_empty() {
                        String::new()
                    } else {
                        std::path::Path::new(&self.db_path_label)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("throne.db")
                            .to_string()
                    }),
            )
            .child(
                div()
                    .text_color(Theme::text_muted())
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

/// Sort key for latency: measured values first (by ms), then untested (0), fail (<0) last.
fn latency_sort_key(ms: i32) -> (u8, i32) {
    if ms > 0 {
        (0, ms)
    } else if ms == 0 {
        (1, 0)
    } else {
        (2, ms)
    }
}

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

fn tab_btn(
    label: &'static str,
    selected: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("btab-{label}")))
        .px_2()
        .py_0p5()
        .rounded_sm()
        .border_1()
        .border_color(if selected {
            Theme::tab_selected_border()
        } else {
            Theme::border()
        })
        .bg(if selected {
            Theme::bg_elevated()
        } else {
            Theme::bg_panel()
        })
        .text_xs()
        .cursor_pointer()
        .child(label)
        .on_click(on_click)
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

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.state.core_status().is_running() {
            self.poll_core_runtime(cx);
        }

        let focus = self.focus_handle.clone();
        if !focus.is_focused(window) {
            focus.focus(window);
        }

        let dialog_open = !matches!(self.dialog, Dialog::None);
        let ctx_open = self.open_menu == OpenMenu::ProfileCtx;
        let toolbar_menu_open = Self::toolbar_menu_index(self.open_menu).is_some();

        div()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &ToggleProxy, _, cx| this.toggle_proxy(cx)))
            .on_action(cx.listener(|this, _: &ImportClipboard, _, cx| {
                this.import_clipboard(cx)
            }))
            .on_action(cx.listener(|this, _: &SaveDb, _, cx| this.save_db(cx)))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| this.select_all(cx)))
            .on_action(cx.listener(|this, _: &DeleteSelected, _, cx| {
                this.delete_selected(cx)
            }))
            .on_action(cx.listener(|this, _: &UrlTestSelected, _, cx| {
                this.url_test_selected(cx)
            }))
            .on_action(cx.listener(|this, _: &UrlTestGroup, _, cx| this.url_test_group(cx)))
            .on_action(cx.listener(|this, _: &DeleteUnavailable, _, cx| {
                this.delete_unavailable(cx)
            }))
            .on_action(cx.listener(|this, _: &CycleRoute, _, cx| this.cycle_route(cx)))
            .on_action(cx.listener(|this, _: &CopyLogs, _, cx| this.copy_logs(cx)))
            .on_action(cx.listener(|_this, _: &Quit, _, cx| cx.quit()))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if this.handle_dialog_key(event, cx) {
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
            .when(ctx_open, |el| el.child(self.render_ctx_menu(cx)))
            .when(dialog_open, |el| el.child(self.render_dialog_overlay(cx)))
    }
}
