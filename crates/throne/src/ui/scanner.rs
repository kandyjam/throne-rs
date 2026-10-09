//! Native IP-list editor and bounded TCP scan workflow.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

use gpui::{
    div, prelude::*, px, size, App, Bounds, Context, Entity, Render, SharedString, TitlebarOptions,
    Window, WindowBounds, WindowOptions,
};
use gpui_component::{
    input::{InputState, TextareaState},
    scroll::ScrollableElement as _,
    Root,
};
use throne_core_client::{
    CoreConfig, CoreSession, ScanEntry, ScanProbeRequest, ScanProbeResponse, ScanTargetSpec,
    ScanTcpOptions,
};
use throne_domain::{parse_ip_list_text, EndpointSource, IpList, IpListEntry};
use throne_storage::Database;

use crate::{
    theme::Theme,
    ui::widgets::{input_area_tall, input_field_row, primary_btn, secondary_btn, section_hint},
};

type ApplyEndpoint = Box<dyn Fn(EndpointSource, &mut App) -> Result<(), String>>;
static NEXT_SCAN: AtomicU64 = AtomicU64::new(1);

pub fn open(db_path: PathBuf, apply: ApplyEndpoint, cx: &mut App) -> anyhow::Result<()> {
    let bounds = Bounds::centered(None, size(px(760.), px(660.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("IP Lists & Scanner".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| Scanner::new(db_path, apply, window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        },
    )?;
    Ok(())
}

struct Scanner {
    db_path: PathBuf,
    apply: ApplyEndpoint,
    name: Entity<InputState>,
    port: Entity<InputState>,
    entries: Entity<TextareaState>,
    lists: Vec<IpList>,
    selected: Option<i64>,
    notice: String,
    busy: bool,
    cancel: Arc<AtomicBool>,
    results: Vec<IpListEntry>,
}

impl Drop for Scanner {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Scanner {
    fn new(
        db_path: PathBuf,
        apply: ApplyEndpoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).default_value("New IP list"));
        let port = cx.new(|cx| InputState::new(window, cx).default_value("443"));
        let entries = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(8)
                .placeholder("One IP, CIDR or IP:port per line")
        });
        let mut this = Self {
            db_path,
            apply,
            name,
            port,
            entries,
            lists: vec![],
            selected: None,
            notice: "Create or select an IP list. TCP probes run only after Start scan.".into(),
            busy: false,
            cancel: Arc::new(AtomicBool::new(false)),
            results: vec![],
        };
        this.reload();
        this
    }

    fn reload(&mut self) {
        match Database::open(&self.db_path).and_then(|db| db.load_ip_lists()) {
            Ok(lists) => self.lists = lists,
            Err(error) => self.notice = format!("Load IP lists failed: {error}"),
        }
    }

    fn select(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        match Database::open(&self.db_path).and_then(|db| db.load_ip_list(id)) {
            Ok(Some(list)) => {
                self.selected = Some(id);
                self.name.update(cx, |input, cx| {
                    input.set_value(list.name.clone(), window, cx)
                });
                self.entries.update(cx, |input, cx| {
                    input.set_value(format_entries(&list.entries), window, cx)
                });
                self.results.clear();
                self.notice = format!(
                    "{} entries · editing saves a manual copy",
                    list.entries.len()
                );
            }
            Ok(None) => self.notice = "This IP list no longer exists".into(),
            Err(error) => self.notice = error.to_string(),
        }
        cx.notify();
    }

    fn parsed_entries(&self, cx: &App) -> Result<Vec<IpListEntry>, String> {
        let port = self
            .port
            .read(cx)
            .value()
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|p| *p > 0)
            .ok_or("Default port must be 1–65535")?;
        let text = self.entries.read(cx).value();
        if text.len() > 1024 * 1024 {
            return Err("IP-list input exceeds 1 MiB".into());
        }
        let parsed = parse_ip_list_text(&text, port);
        if parsed.rejected > 0 {
            return Err(format!(
                "{} invalid entries; fix them before saving or scanning",
                parsed.rejected
            ));
        }
        if parsed.entries.is_empty() {
            return Err("Enter at least one IP address or CIDR".into());
        }
        Ok(parsed.entries)
    }

    fn save(&mut self, results: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let entries = if results {
            Ok(self.results.clone())
        } else {
            self.parsed_entries(cx)
        };
        let entries = match entries {
            Ok(entries) if !entries.is_empty() => entries,
            Ok(_) => {
                self.notice = "There are no passing results to save".into();
                cx.notify();
                return;
            }
            Err(error) => {
                self.notice = error;
                cx.notify();
                return;
            }
        };
        let name = self.name.read(cx).value().trim().to_owned();
        if name.is_empty() {
            self.notice = "Enter a list name".into();
            cx.notify();
            return;
        }
        let mut list = IpList::new(if results {
            format!("{name} · TCP results")
        } else {
            name
        });
        list.entries = entries;
        match Database::open(&self.db_path).and_then(|db| db.save_ip_list(&mut list)) {
            Ok(()) => {
                self.selected = Some(list.id);
                self.notice = format!("Saved {} entries as {}", list.entries.len(), list.name);
                self.reload();
            }
            Err(error) => self.notice = format!("Save failed: {error}"),
        }
        cx.notify();
    }

    fn apply_endpoint(&mut self, source: EndpointSource, cx: &mut Context<Self>) {
        let description = match &source {
            EndpointSource::Own => "Original profile address restored",
            EndpointSource::Inherit => "Group endpoint inheritance restored",
            EndpointSource::IpList { .. } => "Endpoint list assigned",
            EndpointSource::Address { .. } => "Endpoint address assigned",
        };
        self.notice = match (self.apply)(source, cx) {
            Ok(()) => format!("{description}; restart the profile to apply."),
            Err(error) => format!("Could not update endpoint: {error}"),
        };
        cx.notify();
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let entries = match self.parsed_entries(cx) {
            Ok(entries) => entries,
            Err(error) => {
                self.notice = error;
                cx.notify();
                return;
            }
        };
        self.busy = true;
        self.results.clear();
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&self.cancel);
        let path = self.db_path.clone();
        self.notice = "Scanning TCP · up to 4,096 targets, 256 hosts per CIDR…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let mut config = CoreConfig::default();
                    if let Some(parent) = path.parent() {
                        config.asset_dir = parent.into();
                        config.work_dir = parent.into();
                    }
                    // An isolated idle core never starts a proxy or changes system DNS.
                    let mut core = CoreSession::new(config);
                    let mut request = scan_request(&entries);
                    let mut progress = ScanProgress::default();
                    let mut capped = false;
                    while !cancel.load(Ordering::Relaxed) {
                        let response = match core.scan_probe(&request) {
                            Ok(response) => response,
                            Err(error) => {
                                progress.error = Some(error.to_string());
                                break;
                            }
                        };
                        let (aborted, next, total) =
                            (response.aborted, response.next_cursor, response.total);
                        progress.append(response);
                        if progress.error.is_some()
                            || aborted
                            || next <= request.cursor
                            || next >= total
                        {
                            break;
                        }
                        if next >= 4096 {
                            capped = true;
                            break;
                        }
                        request.cursor = next;
                    }
                    let stopped = cancel.load(Ordering::Relaxed);
                    // Release the native session before this isolated core is dropped.
                    if let Err(error) = core.stop_scan(&request.session_id) {
                        progress
                            .error
                            .get_or_insert_with(|| format!("Session cleanup: {error}"));
                    }
                    progress.passed.sort_by_key(|entry| entry.latency_ms);
                    (progress, stopped, capped)
                })
                .await;
            this.update(cx, |this, cx| {
                this.busy = false;
                let (progress, stopped, capped) = result;
                this.notice = format!(
                    "{} · {} tested · {} passed{}{}",
                    if progress.error.is_some() {
                        "Interrupted"
                    } else if stopped {
                        "Stopped"
                    } else {
                        "Complete"
                    },
                    progress.tested,
                    progress.passed.len(),
                    if capped {
                        " · target limit reached"
                    } else {
                        ""
                    },
                    progress
                        .error
                        .map(|error| format!(" · {error}"))
                        .unwrap_or_default()
                );
                this.results = progress.passed;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

#[derive(Default)]
struct ScanProgress {
    passed: Vec<IpListEntry>,
    tested: usize,
    error: Option<String>,
}

impl ScanProgress {
    fn append(&mut self, response: ScanProbeResponse) {
        self.tested += response.results.len();
        self.passed
            .extend(
                response
                    .results
                    .into_iter()
                    .filter(|r| r.passed)
                    .map(|r| IpListEntry {
                        cidr: r.address,
                        port: r.port as u16,
                        latency_ms: r.tcp_ms,
                    }),
            );
        if !response.error.is_empty() {
            self.error = Some(response.error);
        }
    }
}

fn scan_request(entries: &[IpListEntry]) -> ScanProbeRequest {
    ScanProbeRequest {
        session_id: format!(
            "desktop-tcp-{}-{}",
            std::process::id(),
            NEXT_SCAN.fetch_add(1, Ordering::Relaxed)
        ),
        spec: ScanTargetSpec {
            entries: entries
                .iter()
                .map(|entry| ScanEntry {
                    cidr: entry.cidr.clone(),
                    port: i32::from(entry.port),
                })
                .collect(),
            max_hosts_per_entry: 256,
            shuffle: true,
            seed: 1,
            ..Default::default()
        },
        max_targets: 64,
        concurrency: 16,
        tcp: ScanTcpOptions {
            enabled: true,
            timeout_ms: 1500,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn format_entries(entries: &[IpListEntry]) -> String {
    entries
        .iter()
        .map(|entry| {
            if entry.port == 0 {
                entry.cidr.clone()
            } else if entry.cidr.contains(':') {
                format!("[{}]:{}", entry.cidr, entry.port)
            } else {
                format!("{}:{}", entry.cidr, entry.port)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

impl Render for Scanner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut lists = div().flex().flex_wrap().gap_2();
        for list in &self.lists {
            let id = list.id;
            let entity = cx.entity();
            lists = lists.child(secondary_btn(
                format!("list-{id}"),
                format!(
                    "{}{} ({})",
                    if self.selected == Some(id) {
                        "✓ "
                    } else {
                        ""
                    },
                    list.name,
                    list.entry_count
                ),
                move |_, window, cx| entity.update(cx, |this, cx| this.select(id, window, cx)),
            ));
        }
        let start = cx.entity();
        let stop = cx.entity();
        let save = cx.entity();
        let save_results = cx.entity();
        let apply = cx.entity();
        let own = cx.entity();
        let inherit = cx.entity();
        let mut results = div().flex().flex_col().gap_1();
        for row in self.results.iter().take(50) {
            results = results.child(format!("{}:{} · {} ms", row.cidr, row.port, row.latency_ms));
        }
        div().size_full().bg(Theme::bg_elevated()).text_color(Theme::text())
            .p_4().flex().flex_col().gap_3().overflow_y_scrollbar()
            .child(section_hint("IP lists can replace a profile's server address. Its port and TLS name are preserved."))
            .child(lists)
            .child(input_field_row("List name", &self.name, 110.))
            .child(input_field_row("Default port", &self.port, 110.))
            .child(input_area_tall(&self.entries, 170.))
            .child(div().flex().flex_wrap().gap_2()
                .child(secondary_btn("save-list", "Save as new list", move |_, _, cx| save.update(cx, |this, cx| this.save(false, cx))))
                .child(primary_btn("start-scan", "Start TCP scan", move |_, _, cx| start.update(cx, |this, cx| this.start(cx))))
                .child(secondary_btn("stop-scan", "Stop after batch", move |_, _, cx| stop.update(cx, |this, cx| {
                    this.cancel.store(true, Ordering::Relaxed); cx.notify();
                })))
                .child(secondary_btn("save-results", "Save passing results", move |_, _, cx| save_results.update(cx, |this, cx| this.save(true, cx)))))
            .child(section_hint("Endpoint for the profile selected in the main window:"))
            .child(div().flex().flex_wrap().gap_2()
                .child(secondary_btn("apply-list", "Use for selected profile", move |_, _, cx| apply.update(cx, |this, cx| {
                    if let Some(id) = this.selected {
                        this.apply_endpoint(EndpointSource::IpList { list: id }, cx);
                    }
                    else { this.notice = "Save or select a list first".into(); }
                    cx.notify();
                })))
                .child(secondary_btn("endpoint-own", "Use original address", move |_, _, cx| own.update(cx, |this, cx| {
                    this.apply_endpoint(EndpointSource::Own, cx);
                })))
                .child(secondary_btn("endpoint-inherit", "Inherit group", move |_, _, cx| inherit.update(cx, |this, cx| {
                    this.apply_endpoint(EndpointSource::Inherit, cx);
                }))))
            .child(div().text_sm().child(SharedString::from(self.notice.clone())))
            .child(section_hint("Scans test TCP reachability, not TLS or proxy usability. Stop finishes the current batch (up to about 6 seconds). First 50 passing results shown; all are saved."))
            .child(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_request_is_bounded_and_preserves_explicit_ports() {
        let request = scan_request(&[IpListEntry {
            cidr: "192.0.2.0/24".into(),
            port: 8443,
            latency_ms: -1,
        }]);
        assert_eq!(request.spec.entries[0].port, 8443);
        assert_eq!(request.spec.max_hosts_per_entry, 256);
        assert_eq!(request.max_targets, 64);
        assert_eq!(request.concurrency, 16);
        assert!(!request.session_id.is_empty());
        assert_ne!(request.session_id, scan_request(&[]).session_id);
        assert!(request.tcp.enabled);
        assert!(!request.http.enabled);
    }

    #[test]
    fn editor_entry_format_roundtrips_ipv6_and_ports() {
        let entries = vec![IpListEntry {
            cidr: "2001:db8::1".into(),
            port: 8443,
            latency_ms: -1,
        }];
        let parsed = parse_ip_list_text(&format_entries(&entries), 443);
        assert_eq!(parsed.rejected, 0);
        assert_eq!(parsed.entries[0].cidr, entries[0].cidr);
        assert_eq!(parsed.entries[0].port, 8443);
    }

    #[test]
    fn interrupted_scan_keeps_passes_from_previous_and_partial_batches() {
        let mut progress = ScanProgress::default();
        for error in ["", "network lost"] {
            progress.append(ScanProbeResponse {
                error: error.into(),
                results: vec![throne_core_client::ScanProbeResult {
                    address: "192.0.2.1".into(),
                    port: 443,
                    passed: true,
                    tcp_ms: 10,
                    ..Default::default()
                }],
                ..Default::default()
            });
        }
        assert_eq!(progress.tested, 2);
        assert_eq!(progress.passed.len(), 2);
        assert_eq!(progress.error.as_deref(), Some("network lost"));
    }
}
