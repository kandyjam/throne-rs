//! Real-time Traffic Graph (upstream bottom-tab `SpeedWidget`).
//!
//! Keeps a fixed-size ring of rate samples (bytes/s) for proxy/direct up/down
//! and paints a multi-line chart with GPUI `canvas` + `PathBuilder`.

use gpui::{
    Bounds, Hsla, PathBuilder, Pixels, Window, canvas, div, point, prelude::*, px, rgb,
};
use throne_domain::TrafficSnapshot;

use crate::theme::Theme;

/// Upstream `VIEWABLE` — number of samples shown on the chart.
pub const VIEWABLE: usize = 120;

/// One sample point (bytes per second for each series).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpeedSample {
    pub proxy_up: i64,
    pub proxy_down: i64,
    pub direct_up: i64,
    pub direct_down: i64,
}

impl From<TrafficSnapshot> for SpeedSample {
    fn from(t: TrafficSnapshot) -> Self {
        Self {
            proxy_up: t.proxy_up.max(0),
            proxy_down: t.proxy_down.max(0),
            direct_up: t.direct_up.max(0),
            direct_down: t.direct_down.max(0),
        }
    }
}

impl SpeedSample {
    pub fn max_rate(self) -> i64 {
        self.proxy_up
            .max(self.proxy_down)
            .max(self.direct_up)
            .max(self.direct_down)
    }
}

/// Ring buffer of live rate samples for the Traffic Graph tab.
#[derive(Debug, Clone, Default)]
pub struct SpeedGraph {
    samples: Vec<SpeedSample>,
}

impl SpeedGraph {
    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn push(&mut self, sample: impl Into<SpeedSample>) {
        self.samples.push(sample.into());
        while self.samples.len() > VIEWABLE {
            self.samples.remove(0);
        }
    }

    pub fn samples(&self) -> &[SpeedSample] {
        &self.samples
    }

    pub fn max_y(&self) -> i64 {
        self.samples.iter().map(|s| s.max_rate()).max().unwrap_or(0)
    }
}

/// Nice Y-axis max (upstream `getRoundedYScale`), returns scale in bytes/s.
pub fn nice_y_scale(max_value: i64) -> i64 {
    if max_value <= 0 {
        return 12;
    }
    if max_value <= 12 {
        return 12;
    }
    let mut value = max_value as f64;
    let mut unit_pow = 0i32;
    while value > 1000.0 {
        value /= 1000.0;
        unit_pow += 1;
    }
    let rounded = if value > 100.0 {
        let mut r = ((value / 40.0).floor() as i64) * 40;
        while (r as f64) < value {
            r += 40;
        }
        r as f64
    } else if value > 10.0 {
        let mut r = ((value / 4.0).floor() as i64) * 4;
        while (r as f64) < value {
            r += 4;
        }
        r as f64
    } else {
        const TABLE: [f64; 9] = [1.2, 1.6, 2.0, 2.4, 2.8, 3.2, 4.0, 6.0, 8.0];
        TABLE
            .iter()
            .copied()
            .find(|&r| value <= r)
            .unwrap_or(10.0)
    };
    let mut out = rounded;
    for _ in 0..unit_pow {
        out *= 1000.0;
    }
    out.max(1.0) as i64
}

/// Format a bytes/s rate for axis labels (binary KiB/MiB style like status bar).
pub fn format_rate_label(bytes_per_sec: i64) -> String {
    const UNITS: [&str; 4] = ["B/s", "KB/s", "MB/s", "GB/s"];
    let mut v = bytes_per_sec.max(0) as f64;
    let mut i = 0usize;
    while v >= 1024.0 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes_per_sec}{}", UNITS[i])
    } else if v < 10.0 {
        format!("{v:.1}{}", UNITS[i])
    } else {
        format!("{v:.0}{}", UNITS[i])
    }
}

/// Series colors (upstream DefaultPen / DirectPen).
fn series_colors() -> [(Hsla, bool); 4] {
    // (color, dashed) — proxy solid, direct dashed
    [
        (hsla_rgb(134, 196, 63), false),  // proxy ↑ green
        (hsla_rgb(50, 153, 255), false),  // proxy ↓ blue
        (hsla_rgb(0, 210, 240), true),    // direct ↑ cyan dashed
        (hsla_rgb(235, 220, 42), true),   // direct ↓ yellow dashed
    ]
}

fn hsla_rgb(r: u8, g: u8, b: u8) -> Hsla {
    rgb((r as u32) << 16 | (g as u32) << 8 | b as u32).into()
}

/// Paint the chart into `bounds` (caller supplies a full-size canvas).
pub fn paint_speed_graph(bounds: Bounds<Pixels>, samples: &[SpeedSample], window: &mut Window) {
    if f32::from(bounds.size.width) < 8.0 || f32::from(bounds.size.height) < 8.0 {
        return;
    }

    let pad_l = px(52.);
    let pad_r = px(8.);
    let pad_t = px(18.);
    let pad_b = px(14.);
    let origin = bounds.origin;
    let plot = Bounds {
        origin: point(origin.x + pad_l, origin.y + pad_t),
        size: gpui::size(
            (bounds.size.width - pad_l - pad_r).max(px(1.)),
            (bounds.size.height - pad_t - pad_b).max(px(1.)),
        ),
    };

    let y_max = nice_y_scale(samples.iter().map(|s| s.max_rate()).max().unwrap_or(0));
    let y_max_f = y_max.max(1) as f32;

    // Horizontal grid at 0/25/50/75/100% (Y labels are sibling UI).
    let grid = Theme::border_light();
    for frac in [1.0_f32, 0.75, 0.5, 0.25, 0.0] {
        let y = plot.origin.y + plot.size.height * (1.0 - frac);
        let mut line = PathBuilder::stroke(px(1.));
        line.move_to(point(plot.origin.x, y));
        line.line_to(point(plot.origin.x + plot.size.width, y));
        if let Ok(path) = line.build() {
            window.paint_path(path, grid);
        }
    }

    if samples.len() < 2 {
        return;
    }

    let n = samples.len();
    let w = f32::from(plot.size.width);
    let h = f32::from(plot.size.height);
    let series: [fn(&SpeedSample) -> i64; 4] = [
        |s| s.proxy_up,
        |s| s.proxy_down,
        |s| s.direct_up,
        |s| s.direct_down,
    ];
    let colors = series_colors();

    for (si, getter) in series.iter().enumerate() {
        let (color, dashed) = colors[si];
        let mut builder = PathBuilder::stroke(px(1.5));
        if dashed {
            builder = builder.dash_array(&[px(4.), px(2.)]);
        }
        let mut started = false;
        for (i, sample) in samples.iter().enumerate() {
            let x = plot.origin.x + px(w * (i as f32) / ((n - 1) as f32).max(1.0));
            let v = getter(sample).max(0) as f32;
            let y = plot.origin.y + px(h * (1.0 - (v / y_max_f).clamp(0.0, 1.0)));
            if !started {
                builder.move_to(point(x, y));
                started = true;
            } else {
                builder.line_to(point(x, y));
            }
        }
        if let Ok(path) = builder.build() {
            window.paint_path(path, color);
        }
    }
}

/// Bottom-panel Traffic Graph body: legend + live chart.
pub fn speed_graph_element(graph: &SpeedGraph) -> impl IntoElement {
    let samples = graph.samples().to_vec();
    let empty = samples.is_empty();
    let y_max = nice_y_scale(graph.max_y());
    let y_labels = [
        format_rate_label(y_max),
        format_rate_label(y_max * 3 / 4),
        format_rate_label(y_max / 2),
        format_rate_label(y_max / 4),
        format_rate_label(0),
    ];
    let latest = samples.last().copied().unwrap_or_default();

    div()
        .flex()
        .flex_col()
        .size_full()
        .gap_1()
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .text_xs()
                .child(legend_chip(hsla_rgb(134, 196, 63), "Proxy ↑"))
                .child(legend_chip(hsla_rgb(50, 153, 255), "Proxy ↓"))
                .child(legend_chip(hsla_rgb(0, 210, 240), "Direct ↑"))
                .child(legend_chip(hsla_rgb(235, 220, 42), "Direct ↓"))
                .child(div().flex_1())
                .child(
                    div()
                        .text_color(Theme::text_muted())
                        .child(if empty {
                            "Start a profile to record rates".to_string()
                        } else {
                            format!(
                                "P ↑{} ↓{} · D ↑{} ↓{}",
                                format_rate_label(latest.proxy_up),
                                format_rate_label(latest.proxy_down),
                                format_rate_label(latest.direct_up),
                                format_rate_label(latest.direct_down),
                            )
                        }),
                ),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .min_h(px(80.))
                .child(
                    div()
                        .w(px(48.))
                        .flex()
                        .flex_col()
                        .justify_between()
                        .text_xs()
                        .text_color(Theme::text_muted())
                        .children(y_labels.into_iter().map(|l| div().child(l))),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h(px(80.))
                        .child(
                            canvas(
                                move |_, _, _| {},
                                move |bounds, _, window, _| {
                                    paint_speed_graph(bounds, &samples, window);
                                },
                            )
                            .size_full(),
                        ),
                ),
        )
}

fn legend_chip(color: Hsla, label: &'static str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(
            div()
                .w(px(10.))
                .h(px(3.))
                .rounded_full()
                .bg(color),
        )
        .child(div().text_color(Theme::text_muted()).child(label))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_caps_at_viewable() {
        let mut g = SpeedGraph::default();
        for i in 0..(VIEWABLE + 20) {
            g.push(SpeedSample {
                proxy_up: i as i64,
                ..Default::default()
            });
        }
        assert_eq!(g.samples().len(), VIEWABLE);
        assert_eq!(g.samples().first().unwrap().proxy_up, 20);
        assert_eq!(g.samples().last().unwrap().proxy_up, (VIEWABLE + 19) as i64);
    }

    #[test]
    fn nice_scale_rounds_up() {
        assert_eq!(nice_y_scale(0), 12);
        assert!(nice_y_scale(1500) >= 1500);
        assert!(nice_y_scale(100) >= 100);
    }

    #[test]
    fn format_rate_has_unit() {
        assert!(format_rate_label(0).ends_with("B/s"));
        assert!(format_rate_label(2048).contains("KB/s") || format_rate_label(2048).contains("K"));
    }
}
