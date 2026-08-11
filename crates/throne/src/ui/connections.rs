//! Connections tab table — aligned with upstream `connections` QTableWidget.
//!
//! Columns: Destination (Domain) | Process | Protocol | Outbound | Traffic | Speed

use std::collections::HashMap;
use std::time::Instant;

use gpui::{SharedString, div, prelude::*, px};
use throne_core_client::ConnectionRow;

use crate::theme::Theme;

/// Upstream `ReadableSize` for connection traffic/speed (2 decimals, KiB units).
pub fn readable_size(size: i64) -> String {
    let mut v = size.max(0) as f64;
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut i = 0usize;
    while v >= 1024.0 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.2} {}", UNITS[i])
}

/// Upstream `DisplayDest(dest, domain)`.
pub fn display_dest(dest: &str, domain: &str) -> String {
    if domain.is_empty() {
        return dest.to_string();
    }
    let host = dest.split(':').next().unwrap_or(dest);
    if host == domain {
        dest.to_string()
    } else {
        format!("{dest} ({domain})")
    }
}

/// Protocol cell: `network` + optional ` (protocol)` e.g. `tcp (tls)`.
pub fn display_protocol(network: &str, protocol: &str) -> String {
    if protocol.is_empty() {
        network.to_string()
    } else {
        format!("{network} ({protocol})")
    }
}

/// Traffic cell: `3.15 KiB↑ 5.24 KiB↓`
pub fn display_traffic(upload: i64, download: i64) -> String {
    format!(
        "{}↑ {}↓",
        readable_size(upload),
        readable_size(download)
    )
}

/// Speed cell: `0.00 B/s↑ 0.00 B/s↓`
pub fn display_speed(upload_speed: i64, download_speed: i64) -> String {
    format!(
        "{}/s↑ {}/s↓",
        readable_size(upload_speed),
        readable_size(download_speed)
    )
}

/// Per-connection speed sample for rate derivation (upstream ConnectionLister).
#[derive(Debug, Clone)]
struct SpeedSample {
    upload: i64,
    download: i64,
    at: Instant,
    up_speed: i64,
    down_speed: i64,
}

/// Tracks per-connection upload/download rates across polls.
#[derive(Debug, Default)]
pub struct ConnectionSpeedTracker {
    samples: HashMap<String, SpeedSample>,
}

impl ConnectionSpeedTracker {
    pub fn clear(&mut self) {
        self.samples.clear();
    }

    /// Update rates for the current active set; drops closed connection ids.
    pub fn update(&mut self, rows: &[ConnectionRow]) {
        let now = Instant::now();
        let mut next = HashMap::with_capacity(rows.len());
        for c in rows {
            let (up_speed, down_speed) = if let Some(prev) = self.samples.get(&c.id) {
                let dt = now.duration_since(prev.at).as_secs_f64().max(0.2);
                let d_up = (c.upload - prev.upload).max(0) as f64;
                let d_down = (c.download - prev.download).max(0) as f64;
                ((d_up / dt) as i64, (d_down / dt) as i64)
            } else {
                (0, 0) // first sighting: baseline only
            };
            next.insert(
                c.id.clone(),
                SpeedSample {
                    upload: c.upload,
                    download: c.download,
                    at: now,
                    up_speed,
                    down_speed,
                },
            );
        }
        self.samples = next;
    }

    pub fn speeds(&self, id: &str) -> (i64, i64) {
        self.samples
            .get(id)
            .map(|s| (s.up_speed, s.down_speed))
            .unwrap_or((0, 0))
    }
}

// Fixed column widths (Destination flexes). Keep header + rows in lockstep.
const COL_PROCESS: f32 = 120.;
const COL_PROTO: f32 = 88.;
const COL_OUT: f32 = 72.;
const COL_TRAFFIC: f32 = 140.;
const COL_SPEED: f32 = 150.;

fn col_fixed(width: f32, text: impl Into<SharedString>, color: gpui::Hsla) -> impl IntoElement {
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

/// Header row for the Connections table.
pub fn connections_header() -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .w_full()
        .h(px(26.))
        .px_2()
        .bg(Theme::bg_panel())
        .border_b_1()
        .border_color(Theme::border_light())
        .text_xs()
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(Theme::text_muted())
        .child(col_flex("Destination (Domain)", Theme::text_muted()))
        .child(col_fixed(COL_PROCESS, "Process", Theme::text_muted()))
        .child(col_fixed(COL_PROTO, "Protocol", Theme::text_muted()))
        .child(col_fixed(COL_OUT, "Outbound", Theme::text_muted()))
        .child(col_fixed(COL_TRAFFIC, "Traffic", Theme::text_muted()))
        .child(col_fixed(COL_SPEED, "Speed", Theme::text_muted()))
}

/// One data row.
pub fn connection_row(
    c: &ConnectionRow,
    up_speed: i64,
    down_speed: i64,
    stripe: bool,
) -> impl IntoElement {
    let bg = if stripe {
        Theme::bg_app()
    } else {
        Theme::bg_elevated()
    };
    let fg = Theme::text();
    div()
        .flex()
        .items_center()
        .w_full()
        .h(px(26.))
        .px_2()
        .bg(bg)
        .border_b_1()
        .border_color(Theme::border_light())
        .text_xs()
        .child(col_flex(display_dest(&c.dest, &c.domain), fg))
        .child(col_fixed(COL_PROCESS, c.process.clone(), fg))
        .child(col_fixed(
            COL_PROTO,
            display_protocol(&c.network, &c.protocol),
            fg,
        ))
        .child(col_fixed(COL_OUT, c.outbound.clone(), fg))
        .child(col_fixed(
            COL_TRAFFIC,
            display_traffic(c.upload, c.download),
            fg,
        ))
        .child(col_fixed(
            COL_SPEED,
            display_speed(up_speed, down_speed),
            fg,
        ))
}

/// Full Connections panel body (header + rows or empty state).
pub fn connections_panel(
    running: bool,
    rows: &[ConnectionRow],
    speeds: &ConnectionSpeedTracker,
) -> impl IntoElement {
    if !running {
        return div()
            .text_xs()
            .text_color(Theme::text_muted())
            .child("Connections — start a profile to see live sessions")
            .into_any_element();
    }
    if rows.is_empty() {
        return div()
            .text_xs()
            .text_color(Theme::text_muted())
            .child(
                "Connections — none active (traffic will appear when apps use the proxy)",
            )
            .into_any_element();
    }

    let mut list = div().flex().flex_col().w_full().child(connections_header());
    for (i, c) in rows.iter().take(80).enumerate() {
        let (up_s, down_s) = speeds.speeds(&c.id);
        list = list.child(connection_row(c, up_s, down_s, i % 2 == 1));
    }
    if rows.len() > 80 {
        list = list.child(
            div()
                .px_2()
                .py_1()
                .text_xs()
                .text_color(Theme::text_muted())
                .child(format!("… +{} more", rows.len() - 80)),
        );
    }
    list.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_dest_matches_upstream() {
        assert_eq!(display_dest("1.2.3.4:443", ""), "1.2.3.4:443");
        assert_eq!(display_dest("example.com:443", "example.com"), "example.com:443");
        assert_eq!(
            display_dest("1.2.3.4:443", "cdn.example.com"),
            "1.2.3.4:443 (cdn.example.com)"
        );
    }

    #[test]
    fn protocol_and_traffic_format() {
        assert_eq!(display_protocol("tcp", "tls"), "tcp (tls)");
        assert_eq!(display_protocol("udp", ""), "udp");
        let t = display_traffic(3225, 5365);
        assert!(t.contains("↑"));
        assert!(t.contains("↓"));
        assert!(t.contains("KiB") || t.contains("B"));
        let s = display_speed(0, 0);
        assert!(s.contains("/s↑"));
        assert!(s.contains("/s↓"));
    }

    #[test]
    fn readable_size_two_decimals() {
        assert_eq!(readable_size(0), "0.00 B");
        assert!(readable_size(3225).contains("KiB"));
    }

    #[test]
    fn speed_tracker_first_then_rate() {
        let mut tr = ConnectionSpeedTracker::default();
        let r1 = ConnectionRow {
            id: "a".into(),
            upload: 1000,
            download: 2000,
            ..Default::default()
        };
        tr.update(&[r1.clone()]);
        assert_eq!(tr.speeds("a"), (0, 0));
        std::thread::sleep(std::time::Duration::from_millis(250));
        let r2 = ConnectionRow {
            id: "a".into(),
            upload: 2000,
            download: 4000,
            ..Default::default()
        };
        tr.update(&[r2]);
        let (up, down) = tr.speeds("a");
        assert!(up > 0);
        assert!(down > 0);
    }

}
