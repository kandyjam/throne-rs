use std::ops::Range;

use gpui::{
    Context, FocusHandle, KeyDownEvent, SharedString, Window, actions, div, prelude::*, px,
    uniform_list,
};

use throne_domain::{AppState, CoreStatus, GroupId, Profile, ProfileId, SystemMode};

use crate::theme::{Theme, latency_color};
use crate::ui::widgets::{h_rule, pill, search_field, section_label, spacer, status_dot, toolbar_button};

actions!(throne, [ToggleProxy, FocusSearch, ImportClipboard, SaveDb, Quit]);

pub struct MainWindow {
    state: AppState,
    focus_handle: FocusHandle,
    /// Local draft for search; applied into state on each change.
    search_draft: String,
    /// Optional path message for last DB save.
    db_path_label: String,
}

impl MainWindow {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.bind_keys([
            gpui::KeyBinding::new("cmd-r", ToggleProxy, None),
            gpui::KeyBinding::new("ctrl-r", ToggleProxy, None),
            gpui::KeyBinding::new("cmd-f", FocusSearch, None),
            gpui::KeyBinding::new("ctrl-f", FocusSearch, None),
            gpui::KeyBinding::new("cmd-v", ImportClipboard, None),
            gpui::KeyBinding::new("ctrl-v", ImportClipboard, None),
            gpui::KeyBinding::new("cmd-s", SaveDb, None),
            gpui::KeyBinding::new("ctrl-s", SaveDb, None),
            gpui::KeyBinding::new("cmd-q", Quit, None),
        ]);

        let (state, db_path_label) = load_initial_state();

        Self {
            state,
            focus_handle: cx.focus_handle(),
            search_draft: String::new(),
            db_path_label,
        }
    }

    fn import_clipboard(&mut self, cx: &mut Context<Self>) {
        // GPUI clipboard API varies; use std env THRONE_IMPORT / demo paste buffer first.
        let text = std::env::var("THRONE_IMPORT")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or_else(read_os_clipboard)
            .unwrap_or_default();

        if text.trim().is_empty() {
            self.state.set_status_message(
                "Import: clipboard empty — set THRONE_IMPORT or copy share links",
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
        let mut msg = format!("Imported {n} · skipped {}", report.skipped);
        if let Some(url) = report.pending_sub_url {
            msg.push_str(&format!(" · pending sub fetch: {url}"));
        }
        for note in report.notes.iter().take(2) {
            msg.push_str(" · ");
            msg.push_str(note);
        }
        if n == 0 && !report.errors.is_empty() {
            msg = report.errors.first().cloned().unwrap_or(msg);
        }
        self.state.set_status_message(msg);
        // Auto-persist after successful import
        if n > 0 {
            let _ = self.persist_db();
        }
        cx.notify();
    }

    fn persist_db(&mut self) -> Result<(), String> {
        let path = throne_storage::default_db_path();
        let db = throne_storage::Database::open(&path).map_err(|e| e.to_string())?;
        db.save_state(&self.state).map_err(|e| e.to_string())?;
        self.db_path_label = path.display().to_string();
        self.state
            .set_status_message(format!("Saved · {}", self.db_path_label));
        Ok(())
    }

    fn save_db(&mut self, cx: &mut Context<Self>) {
        match self.persist_db() {
            Ok(()) => cx.notify(),
            Err(e) => {
                self.state.set_status_message(format!("Save failed: {e}"));
                cx.notify();
            }
        }
    }

    fn toggle_proxy(&mut self, cx: &mut Context<Self>) {
        match self.state.toggle_selected() {
            Ok(()) => cx.notify(),
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

    fn set_mode(&mut self, mode: SystemMode, cx: &mut Context<Self>) {
        self.state.set_system_mode(mode);
        cx.notify();
    }

    fn append_search_char(&mut self, ch: char, cx: &mut Context<Self>) {
        if ch.is_control() {
            return;
        }
        self.search_draft.push(ch);
        self.state.set_search_query(self.search_draft.clone());
        cx.notify();
    }

    fn backspace_search(&mut self, cx: &mut Context<Self>) {
        self.search_draft.pop();
        self.state.set_search_query(self.search_draft.clone());
        cx.notify();
    }

    fn clear_search(&mut self, cx: &mut Context<Self>) {
        self.search_draft.clear();
        self.state.set_search_query("");
        cx.notify();
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let running = self.state.core_status().is_running();
        let start_label = if running { "Stop" } else { "Start" };

        div()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_3()
            .bg(Theme::bg_panel())
            .border_b_1()
            .border_color(Theme::border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(Theme::text())
                            .child("Throne"),
                    )
                    .child(pill("Rust · GPUI", Theme::accent())),
            )
            .child(spacer())
            .child(search_field(
                "search",
                &self.search_draft,
                "Filter profiles… (type while focused)",
                |_, _, _| {},
            ))
            .when(!self.search_draft.is_empty(), {
                let entity = cx.entity().clone();
                move |el| {
                    el.child(toolbar_button("clear-search", "Clear", false, move |_, _, cx| {
                        entity.update(cx, |this, cx| this.clear_search(cx));
                    }))
                }
            })
            .child({
                let entity = cx.entity().clone();
                toolbar_button("import", "Import", false, move |_, _, cx| {
                    entity.update(cx, |this, cx| this.import_clipboard(cx));
                })
            })
            .child({
                let entity = cx.entity().clone();
                toolbar_button("toggle", start_label, true, move |_, _, cx| {
                    entity.update(cx, |this, cx| this.toggle_proxy(cx));
                })
            })
            .child({
                let entity = cx.entity().clone();
                let mode = self.state.system_mode();
                let label = match mode {
                    SystemMode::Off => "Mode: Off",
                    SystemMode::SystemProxy => "Mode: Proxy",
                    SystemMode::VpnTun => "Mode: TUN",
                };
                toolbar_button("mode", label, false, move |_, _, cx| {
                    entity.update(cx, |this, cx| {
                        let next = match this.state.system_mode() {
                            SystemMode::Off => SystemMode::SystemProxy,
                            SystemMode::SystemProxy => SystemMode::VpnTun,
                            SystemMode::VpnTun => SystemMode::Off,
                        };
                        this.set_mode(next, cx);
                    });
                })
            })
            .child({
                let entity = cx.entity().clone();
                toolbar_button("save", "Save", false, move |_, _, cx| {
                    entity.update(cx, |this, cx| this.save_db(cx));
                })
            })
    }

    fn render_group_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.state.active_group_id();
        let mut row = div().flex().gap_1().px_3().py_2().bg(Theme::bg_app());

        for &gid in self.state.group_order() {
            let Some(group) = self.state.group(gid) else {
                continue;
            };
            let selected = gid == active;
            let name = group.name.clone();
            let count = group.profile_ids.len();
            let entity = cx.entity().clone();

            row = row.child(
                div()
                    .id(SharedString::from(format!("group-{gid}")))
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .text_sm()
                    .when(selected, |el| {
                        el.bg(Theme::bg_selected())
                            .text_color(Theme::text())
                            .border_1()
                            .border_color(Theme::accent())
                    })
                    .when(!selected, |el| {
                        el.bg(Theme::bg_elevated())
                            .text_color(Theme::text_muted())
                            .hover(|e| e.bg(Theme::bg_hover()).text_color(Theme::text()))
                    })
                    .child(format!("{name} ({count})"))
                    .on_click(move |_, _, cx| {
                        entity.update(cx, |this, cx| this.select_group(gid, cx));
                    }),
            );
        }

        row
    }

    fn render_profile_header(&self) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .px_4()
            .py_2()
            .bg(Theme::bg_panel())
            .border_b_1()
            .border_color(Theme::border())
            .text_xs()
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(Theme::text_muted())
            .child(div().w(px(28.)).child("#"))
            .child(div().flex_1().child("Name"))
            .child(div().w(px(110.)).child("Type"))
            .child(div().w(px(80.)).child("Latency"))
            .child(div().w(px(160.)).child("Traffic"))
    }

    fn render_profile_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
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

        div().flex_1().min_h(px(0.)).child(
            uniform_list(
                "profiles",
                count,
                cx.processor(move |_this, range: Range<usize>, _window, _cx| {
                    let mut items = Vec::new();
                    for ix in range {
                        let Some(profile) = profiles.get(ix) else {
                            continue;
                        };
                        let id = profile.id;
                        let is_selected = selected == Some(id);
                        let is_running = running_id == Some(id);
                        let name = profile.name.clone();
                        let ty = profile.profile_type.display_name().to_string();
                        let latency = profile.display_latency();
                        let lat_color = latency_color(profile.latency_ms);
                        let traffic = profile.display_traffic();
                        let entity_click = entity.clone();
                        let entity_dbl = entity.clone();

                        items.push(
                            div()
                                .id(SharedString::from(format!("profile-{id}")))
                                .flex()
                                .items_center()
                                .px_4()
                                .h(px(36.))
                                .cursor_pointer()
                                .border_b_1()
                                .border_color(Theme::border())
                                .when(is_selected, |el| el.bg(Theme::bg_selected()))
                                .when(!is_selected, |el| {
                                    el.bg(Theme::bg_app()).hover(|e| e.bg(Theme::bg_hover()))
                                })
                                .on_click(move |_, _, cx| {
                                    entity_click
                                        .update(cx, |this, cx| this.select_profile(id, cx));
                                })
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    move |ev: &gpui::MouseDownEvent, _, cx| {
                                        if ev.click_count >= 2 {
                                            entity_dbl.update(cx, |this, cx| {
                                                let _ = this.state.select_profile(id);
                                                this.toggle_proxy(cx);
                                            });
                                        }
                                    },
                                )
                                .child(
                                    div()
                                        .w(px(28.))
                                        .flex()
                                        .items_center()
                                        .child(status_dot(is_running)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .text_sm()
                                        .text_color(Theme::text())
                                        .child(name),
                                )
                                .child(
                                    div()
                                        .w(px(110.))
                                        .text_xs()
                                        .text_color(Theme::text_muted())
                                        .child(ty),
                                )
                                .child(
                                    div()
                                        .w(px(80.))
                                        .text_xs()
                                        .text_color(lat_color)
                                        .child(latency),
                                )
                                .child(
                                    div()
                                        .w(px(160.))
                                        .text_xs()
                                        .text_color(Theme::text_muted())
                                        .child(traffic),
                                ),
                        );
                    }
                    items
                }),
            )
            .size_full(),
        )
    }

    fn render_status_bar(&self) -> impl IntoElement {
        let status = self.state.core_status().label();
        let traffic = self.state.traffic();
        let msg = self.state.status_message().to_string();
        let running = self.state.core_status().is_running();

        div()
            .flex()
            .items_center()
            .gap_3()
            .px_4()
            .py_2()
            .bg(Theme::bg_panel())
            .border_t_1()
            .border_color(Theme::border())
            .text_xs()
            .text_color(Theme::text_muted())
            .child(status_dot(running))
            .child(
                div()
                    .text_color(if running {
                        Theme::success()
                    } else {
                        Theme::text_muted()
                    })
                    .child(status),
            )
            .child(h_rule())
            .child(format!(
                "↓ {}/s  ↑ {}/s",
                short_rate(traffic.proxy_down),
                short_rate(traffic.proxy_up)
            ))
            .child(spacer())
            .child(msg)
    }

    fn render_sidebar(&self) -> impl IntoElement {
        let selected_name = self
            .state
            .selected_profile_id()
            .and_then(|id| self.state.profile(id))
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "None".into());

        div()
            .w(px(240.))
            .h_full()
            .bg(Theme::bg_panel())
            .border_l_1()
            .border_color(Theme::border())
            .flex()
            .flex_col()
            .p_4()
            .gap_3()
            .child(section_label("SELECTION"))
            .child(
                div()
                    .text_sm()
                    .text_color(Theme::text())
                    .child(selected_name),
            )
            .child(section_label("CORE"))
            .child(
                div()
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .child("Go ThroneCore via local-socket RPC"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .child("Protobuf: core/server/gen/libcore.proto"),
            )
            .child(section_label("SETTINGS (upstream defaults)"))
            .child(
                div()
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .child(format!(
                        "DNS {}",
                        self.state.settings().remote_dns
                    )),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .child(format!(
                        "SOCKS {}:{}",
                        self.state.settings().inbound_address,
                        self.state.settings().inbound_socks_port
                    )),
            )
            .child(section_label("SHORTCUTS"))
            .child(shortcut_row("⌘/Ctrl+R", "Start / Stop"))
            .child(shortcut_row("⌘/Ctrl+V", "Import clipboard"))
            .child(shortcut_row("⌘/Ctrl+S", "Save DB"))
            .child(shortcut_row("Double-click", "Start profile"))
            .child(spacer())
            .child(
                div()
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .child(if self.db_path_label.is_empty() {
                        "DB: memory/demo".into()
                    } else {
                        format!("DB: {}", self.db_path_label)
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(Theme::text_muted())
                    .child("Wave A · upstream-synced import + SQLite"),
            )
    }
}

fn load_initial_state() -> (AppState, String) {
    match throne_storage::open_default() {
        Ok(db) => {
            let path = db.path().display().to_string();
            match throne_storage::load_or_seed_demo(&db) {
                Ok(state) => (state, path),
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

/// Best-effort clipboard read without pulling extra crates.
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
        for cmd in [["xclip", "-selection", "clipboard", "-o"], ["wl-paste"]] {
            if let Ok(out) = Command::new(cmd[0]).args(&cmd[1..]).output() {
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

fn shortcut_row(key: &str, desc: &str) -> impl IntoElement {
    div()
        .flex()
        .justify_between()
        .text_xs()
        .child(div().text_color(Theme::accent()).child(key.to_string()))
        .child(div().text_color(Theme::text_muted()).child(desc.to_string()))
}

fn short_rate(bytes: i64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes.max(0) as f64;
    let mut i = 0usize;
    while v >= 1024.0 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes}{}", UNITS[i])
    } else {
        format!("{v:.1}{}", UNITS[i])
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Demo traffic tick while running — keeps status bar lively without a core.
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
            .on_action(cx.listener(|_this, _: &Quit, _, cx| cx.quit()))
            .on_action(cx.listener(|this, _: &FocusSearch, _, cx| {
                this.state
                    .set_status_message("Filter focused — type to search, Backspace to edit");
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let key = &event.keystroke.key;
                if key == "backspace" {
                    this.backspace_search(cx);
                    return;
                }
                if key == "escape" {
                    this.clear_search(cx);
                    return;
                }
                // Printable single-char keys for MVP filter.
                if key.len() == 1 {
                    if let Some(ch) = key.chars().next() {
                        if !event.keystroke.modifiers.platform
                            && !event.keystroke.modifiers.control
                        {
                            this.append_search_char(ch, cx);
                        }
                    }
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .bg(Theme::bg_app())
            .text_color(Theme::text())
            .child(self.render_toolbar(cx))
            .child(self.render_group_tabs(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h(px(0.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.))
                            .child(self.render_profile_header())
                            .child(self.render_profile_list(cx)),
                    )
                    .child(self.render_sidebar()),
            )
            .child(self.render_status_bar())
    }
}
