//! Main window layout mirrored from upstream `mainwindow.ui`.
//!
//! ```text
//! [Program][Settings][Groups][Routing][Tools] [▶Start] [Tun][DNS][Proxy] | data_view
//! ─────────────────────────────────────────────────────────────────────
//! Group tabs …
//! ┌ Type │ Address │ Name │ Test Result │ Traffic ──────────────────┐
//! │ …                                                               │
//! └─────────────────────────────────────────────────────────────────┘
//! [Logs] [Connections]  (stub tabs)
//! running | inbound | speed
//! ```

use std::ops::Range;

use gpui::{
    Context, FocusHandle, KeyDownEvent, SharedString, Window, actions, div, prelude::*, px,
    uniform_list,
};

use throne_domain::{AppState, CoreStatus, GroupId, Profile, ProfileId};

use crate::theme::{Theme, latency_color};
use crate::ui::widgets::{
    menu_item, menu_label, menu_separator, mode_checkbox, start_stop_btn, toolbar_menu_btn,
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
        CycleRoute,
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

pub struct MainWindow {
    state: AppState,
    focus_handle: FocusHandle,
    search_draft: String,
    db_path_label: String,
    open_menu: OpenMenu,
    /// Bottom panel tab: 0 Logs, 1 Connections
    bottom_tab: usize,
    ctx_menu_at: Option<(f32, f32)>,
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
            gpui::KeyBinding::new("cmd-shift-r", CycleRoute, None),
            gpui::KeyBinding::new("ctrl-shift-r", CycleRoute, None),
            gpui::KeyBinding::new("cmd-q", Quit, None),
        ]);

        let (state, db_path_label) = load_initial_state();
        Self {
            state,
            focus_handle: cx.focus_handle(),
            search_draft: String::new(),
            db_path_label,
            open_menu: OpenMenu::None,
            bottom_tab: 0,
            ctx_menu_at: None,
        }
    }

    fn close_menus(&mut self) {
        self.open_menu = OpenMenu::None;
        self.ctx_menu_at = None;
    }

    fn toggle_menu(&mut self, menu: OpenMenu, cx: &mut Context<Self>) {
        self.open_menu = if self.open_menu == menu {
            OpenMenu::None
        } else {
            menu
        };
        self.ctx_menu_at = None;
        cx.notify();
    }

    fn toggle_proxy(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        match self.state.toggle_selected() {
            Ok(()) => {
                let _ = self.persist_db();
                cx.notify();
            }
            Err(e) => {
                self.state.set_status_message(e.to_string());
                cx.notify();
            }
        }
    }

    fn select_group(&mut self, id: GroupId, cx: &mut Context<Self>) {
        let _ = self.state.set_active_group(id);
        cx.notify();
    }

    fn select_profile(&mut self, id: ProfileId, cx: &mut Context<Self>) {
        let _ = self.state.select_profile(id);
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

        let report = throne_import::import_text(&text);
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
        self.close_menus();
        if let Some(id) = self.state.selected_profile_id() {
            self.state.delete_selected_profiles(&[id]);
            self.state.set_status_message("Deleted");
            let _ = self.persist_db();
        }
        cx.notify();
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        // Single-selection UI for now: select first visible
        if let Some(p) = self.state.visible_profiles().first() {
            let id = p.id;
            let _ = self.state.select_profile(id);
        }
        cx.notify();
    }

    fn url_test_stub(&mut self, cx: &mut Context<Self>) {
        self.close_menus();
        self.state
            .set_status_message("Url Test: core RPC not connected yet");
        cx.notify();
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
        let _ = self.persist_db();
        cx.notify();
    }

    fn set_sys_dns(&mut self, on: bool, cx: &mut Context<Self>) {
        self.state.set_system_dns(on);
        let _ = self.persist_db();
        cx.notify();
    }

    // ─── layout regions (match mainwindow.ui) ───────────────────────────

    fn render_top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let running = self.state.core_status().is_running();
        let entity = cx.entity().clone();

        div()
            .relative()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_2()
            .bg(Theme::bg_panel())
            .border_b_1()
            .border_color(Theme::border_light())
            // Five tool menus
            .child(self.render_tool_cluster(cx))
            // Start / Stop
            .child({
                let e = entity.clone();
                start_stop_btn(running, move |_, _, cx| {
                    e.update(cx, |this, cx| this.toggle_proxy(cx));
                })
            })
            // Mode checkboxes
            .child(
                div()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap_1()
                    .px_2()
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
                        let on = self.state.settings().system_dns_set;
                        mode_checkbox("dns", "System DNS", on, move |_, _, cx| {
                            e.update(cx, |this, cx| {
                                let next = !this.state.settings().system_dns_set;
                                this.set_sys_dns(next, cx);
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
            // data_view (running summary panel)
            .child(self.render_data_view())
    }

    fn render_tool_cluster(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        div()
            .flex()
            .items_center()
            .gap_1()
            .child({
                let e = entity.clone();
                let open = self.open_menu == OpenMenu::Program;
                toolbar_menu_btn("tb-program", "⚙", "Program", open, move |_, _, cx| {
                    e.update(cx, |this, cx| this.toggle_menu(OpenMenu::Program, cx));
                })
            })
            .child({
                let e = entity.clone();
                let open = self.open_menu == OpenMenu::Settings;
                toolbar_menu_btn("tb-settings", "☰", "Settings", open, move |_, _, cx| {
                    e.update(cx, |this, cx| this.toggle_menu(OpenMenu::Settings, cx));
                })
            })
            .child({
                let e = entity.clone();
                let open = self.open_menu == OpenMenu::Groups;
                toolbar_menu_btn("tb-groups", "▦", "Groups", open, move |_, _, cx| {
                    e.update(cx, |this, cx| this.toggle_menu(OpenMenu::Groups, cx));
                })
            })
            .child({
                let e = entity.clone();
                let open = self.open_menu == OpenMenu::Routing;
                toolbar_menu_btn("tb-routing", "⇄", "Routing", open, move |_, _, cx| {
                    e.update(cx, |this, cx| this.toggle_menu(OpenMenu::Routing, cx));
                })
            })
            .child({
                let e = entity.clone();
                let open = self.open_menu == OpenMenu::Tools;
                toolbar_menu_btn("tb-tools", "⚒", "Tools", open, move |_, _, cx| {
                    e.update(cx, |this, cx| this.toggle_menu(OpenMenu::Tools, cx));
                })
            })
    }

    fn render_data_view(&self) -> impl IntoElement {
        let running = self.state.running_label();
        let inbound = self.state.inbound_label();
        let route = self
            .state
            .active_route()
            .map(|r| r.name.clone())
            .unwrap_or_else(|| "—".into());
        let msg = self.state.status_message().to_string();

        div()
            .flex_1()
            .min_w(px(160.))
            .px_3()
            .py_1()
            .bg(Theme::bg_elevated())
            .border_1()
            .border_color(Theme::border_light())
            .rounded_sm()
            .flex()
            .flex_col()
            .justify_center()
            .gap_0p5()
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(if self.state.core_status().is_running() {
                        Theme::success()
                    } else {
                        Theme::text()
                    })
                    .child(running),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .child(format!("{inbound}  ·  Route: {route}")),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .child(msg),
            )
    }

    fn build_menu_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let left = match self.open_menu {
            OpenMenu::Program => px(8.),
            OpenMenu::Settings => px(76.),
            OpenMenu::Groups => px(144.),
            OpenMenu::Routing => px(212.),
            OpenMenu::Tools => px(280.),
            OpenMenu::ProfileCtx => px(200.),
            OpenMenu::None => px(8.),
        };

        let mut panel = div()
            .absolute()
            .top(px(62.))
            .left(left)
            .min_w(px(240.))
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

        let _ = &entity;

        match self.open_menu {
            OpenMenu::Program => {
                panel = panel.child(menu_label("Program"));
                item!("prog-clip", "Add profile from clipboard", |t, cx| {
                    t.import_clipboard(cx)
                });
                item!("prog-start", "Start", |t, cx| t.toggle_proxy(cx));
                item!("prog-stop", "Stop", |t, cx| {
                    if t.state.core_status().is_running() {
                        t.toggle_proxy(cx);
                    } else {
                        cx.notify();
                    }
                });
                panel = panel.child(menu_separator());
                item!("prog-proxy", "Enable System Proxy", |t, cx| {
                    t.set_sys_proxy(true, cx)
                });
                item!("prog-tun", "Enable Tun", |t, cx| t.set_vpn(true, cx));
                item!("prog-off", "Disable", |t, cx| {
                    t.set_sys_proxy(false, cx);
                    t.set_vpn(false, cx);
                    t.set_sys_dns(false, cx);
                });
                panel = panel.child(menu_separator());
                item!("prog-exit", "Exit", |_t, cx| cx.quit());
            }
            OpenMenu::Settings => {
                panel = panel.child(menu_label("Preferences"));
                item!("set-basic", "Basic Settings", |t, cx| {
                    t.state
                        .set_status_message("Basic Settings dialog — not yet ported");
                    t.close_menus();
                    cx.notify();
                });
                item!("set-route", "Routing Settings", |t, cx| {
                    t.state
                        .set_status_message("Routing Settings dialog — not yet ported");
                    t.close_menus();
                    cx.notify();
                });
                item!("set-tun", "Tun Settings", |t, cx| {
                    t.state
                        .set_status_message("Tun Settings dialog — not yet ported");
                    t.close_menus();
                    cx.notify();
                });
                item!("set-hotkey", "Hotkey Settings", |t, cx| {
                    t.state
                        .set_status_message("Hotkey Settings dialog — not yet ported");
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
                item!("g-manage", "Manage Groups", |t, cx| {
                    t.state
                        .set_status_message("Manage Groups dialog — not yet ported");
                    t.close_menus();
                    cx.notify();
                });
                item!("g-update", "Update subscription", |t, cx| {
                    t.state
                        .set_status_message("Update subscription — HTTP fetch not yet ported");
                    t.close_menus();
                    cx.notify();
                });
                item!("g-update-all", "Update all subscriptions", |t, cx| {
                    t.state
                        .set_status_message("Update all subscriptions — not yet ported");
                    t.close_menus();
                    cx.notify();
                });
                panel = panel.child(menu_separator());
                item!("g-urltest", "Url Test Group", |t, cx| t.url_test_stub(cx));
                item!("g-clear", "Clear Group test result", |t, cx| {
                    t.state.set_status_message("Test results cleared (local only)");
                    t.close_menus();
                    cx.notify();
                });
                item!("g-dup", "Remove Duplicates", |t, cx| {
                    t.state
                        .set_status_message("Remove Duplicates — not yet ported");
                    t.close_menus();
                    cx.notify();
                });
                item!("g-unavail", "Remove Unavailable", |t, cx| {
                    t.state
                        .set_status_message("Remove Unavailable — not yet ported");
                    t.close_menus();
                    cx.notify();
                });
                item!("g-insecure", "Remove Insecure Configs", |t, cx| {
                    t.state
                        .set_status_message("Remove Insecure Configs — not yet ported");
                    t.close_menus();
                    cx.notify();
                });
            }
            OpenMenu::Routing => {
                panel = panel.child(menu_label("Routing"));
                item!("r-settings", "Routing Settings", |t, cx| {
                    t.state
                        .set_status_message("Routing Settings — not yet ported");
                    t.close_menus();
                    cx.notify();
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
                item!("t-url", "Url Test Selected", |t, cx| t.url_test_stub(cx));
                item!("t-speed", "Speedtest Selected", |t, cx| {
                    t.state
                        .set_status_message("Speedtest — core RPC not connected");
                    t.close_menus();
                    cx.notify();
                });
                panel = panel.child(menu_separator());
                item!("t-runtime", "Runtime Stats", |t, cx| {
                    t.state
                        .set_status_message("Runtime Stats — not yet ported");
                    t.close_menus();
                    cx.notify();
                });
                item!("t-traffic", "Traffic Stats", |t, cx| {
                    t.state
                        .set_status_message("Traffic Stats — not yet ported");
                    t.close_menus();
                    cx.notify();
                });
                item!("t-update", "Check For Update", |t, cx| {
                    t.state
                        .set_status_message("Check For Update — not yet ported");
                    t.close_menus();
                    cx.notify();
                });
            }
            OpenMenu::ProfileCtx => {
                panel = panel.child(menu_label("Server"));
                item!("c-start", "Start", |t, cx| t.toggle_proxy(cx));
                item!("c-stop", "Stop", |t, cx| {
                    if t.state.core_status().is_running() {
                        t.toggle_proxy(cx);
                    }
                });
                item!("c-clip", "Add profile from clipboard", |t, cx| {
                    t.import_clipboard(cx)
                });
                item!("c-del", "Delete", |t, cx| t.delete_selected(cx));
                item!("c-test", "Url Test Selected", |t, cx| t.url_test_stub(cx));
            }
            OpenMenu::None => {}
        }

        panel
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

    fn render_table_header(&self) -> impl IntoElement {
        // ColType, ColAddress, ColName, ColTestResult, ColTraffic
        div()
            .flex()
            .items_center()
            .px_2()
            .h(px(28.))
            .bg(Theme::bg_panel())
            .border_b_1()
            .border_color(Theme::border_light())
            .text_xs()
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(Theme::text_muted())
            .child(div().w(px(28.)).child("#"))
            .child(div().w(px(90.)).child("Type"))
            .child(div().w(px(180.)).child("Address"))
            .child(div().flex_1().child("Name"))
            .child(div().w(px(140.)).child("Test Result"))
            .child(div().w(px(120.)).child("Traffic"))
    }

    fn render_profile_table(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let profiles: Vec<Profile> = self
            .state
            .visible_profiles()
            .into_iter()
            .cloned()
            .collect();
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
                        let e_click = entity.clone();
                        let e_dbl = entity.clone();
                        let e_ctx = entity.clone();

                        let (bg, fg) = if is_selected {
                            (Theme::bg_selected(), Theme::text_on_selected())
                        } else if display_i % 2 == 1 {
                            (Theme::bg_app(), Theme::text())
                        } else {
                            (Theme::bg_elevated(), Theme::text())
                        };

                        items.push(
                            div()
                                .id(SharedString::from(format!("row-{id}")))
                                .flex()
                                .items_center()
                                .px_2()
                                .h(px(28.))
                                .bg(bg)
                                .text_color(fg)
                                .text_sm()
                                .cursor_pointer()
                                .border_b_1()
                                .border_color(Theme::border_light())
                                .on_click(move |_, _, cx| {
                                    e_click.update(cx, |this, cx| this.select_profile(id, cx));
                                })
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    move |ev: &gpui::MouseDownEvent, _, cx| {
                                        if ev.click_count >= 2 {
                                            e_dbl.update(cx, |this, cx| {
                                                let _ = this.state.select_profile(id);
                                                this.toggle_proxy(cx);
                                            });
                                        }
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
                                .child(
                                    div()
                                        .w(px(28.))
                                        .text_xs()
                                        .text_color(if is_running {
                                            Theme::success()
                                        } else if is_selected {
                                            fg
                                        } else {
                                            Theme::text_muted()
                                        })
                                        .child(row_label),
                                )
                                .child(
                                    div()
                                        .w(px(90.))
                                        .text_color(if insecure {
                                            Theme::danger()
                                        } else {
                                            fg
                                        })
                                        .child(ty),
                                )
                                .child(div().w(px(180.)).child(addr))
                                .child(div().flex_1().child(name))
                                .child(
                                    div()
                                        .w(px(140.))
                                        .text_color(if is_selected {
                                            fg
                                        } else {
                                            lat_color
                                        })
                                        .child(test),
                                )
                                .child(div().w(px(120.)).child(traffic)),
                        );
                    }
                    items
                }),
            )
            .size_full(),
        )
    }

    fn render_bottom_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().clone();
        let tab = self.bottom_tab;
        div()
            .flex()
            .flex_col()
            .h(px(140.))
            .border_t_1()
            .border_color(Theme::border_light())
            .bg(Theme::bg_app())
            .child(
                div()
                    .flex()
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
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .m_1()
                    .p_2()
                    .bg(Theme::bg_elevated())
                    .border_1()
                    .border_color(Theme::border_light())
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .child(if tab == 0 {
                        format!(
                            "Logs\n{}\n(core log stream — not connected)",
                            self.state.status_message()
                        )
                    } else {
                        "Connections\n(live connection table — core RPC not connected)".into()
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
                        // show basename only
                        std::path::Path::new(&self.db_path_label)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("throne.db")
                            .to_string()
                    }),
            )
    }
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
            self.state.tick_traffic_demo();
        }

        let focus = self.focus_handle.clone();
        if !focus.is_focused(window) {
            focus.focus(window);
        }

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
                this.url_test_stub(cx)
            }))
            .on_action(cx.listener(|this, _: &CycleRoute, _, cx| this.cycle_route(cx)))
            .on_action(cx.listener(|_this, _: &Quit, _, cx| cx.quit()))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.close_menus();
                    cx.notify();
                    return;
                }
                // Type-to-filter like improved search UX
                let key = &event.keystroke.key;
                if key == "backspace"
                    && !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && this.open_menu == OpenMenu::None
                {
                    // only filter edit when not using Delete action with selection intent
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
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _ev, _, cx| {
                    // click outside closes menus (approx)
                    if this.open_menu != OpenMenu::None && this.open_menu != OpenMenu::ProfileCtx {
                        // keep open; items handle close
                    }
                    let _ = cx;
                }),
            )
            .size_full()
            .flex()
            .flex_col()
            .bg(Theme::bg_app())
            .text_color(Theme::text())
            .child(self.render_top_bar(cx))
            .child(self.render_group_tabs(cx))
            .child(self.render_table_header())
            .child(self.render_profile_table(cx))
            .child(self.render_bottom_tabs(cx))
            .child(self.render_status_bar())
            .when(self.open_menu != OpenMenu::None, |el| {
                el.child(self.build_menu_panel(cx))
            })
    }
}
