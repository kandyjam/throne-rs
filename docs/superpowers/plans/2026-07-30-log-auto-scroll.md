# Log Auto-Scroll Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Always bring the Logs panel to its newest rendered line when its log text changes.

**Architecture:** `MainWindow` retains a GPUI `ScrollHandle` and the last rendered log text. The render path compares current text with the previous value, queues `scroll_to_bottom` only for changes, and attaches the handle to the existing scrollable panel.

**Tech Stack:** Rust 2021, GPUI 0.2.2, Cargo tests.

---

### Task 1: Define the scroll-trigger contract

**Files:**
- Modify: `crates/throne/src/ui/main_window.rs:78-110,3240-3275`

- [x] **Step 1: Write the failing test**

```rust
#[test]
fn new_non_empty_log_text_requests_scroll_to_bottom() {
    assert!(should_scroll_logs_to_bottom("old log", "old log\nnew log"));
    assert!(!should_scroll_logs_to_bottom("same", "same"));
    assert!(!should_scroll_logs_to_bottom("old log", ""));
}
```

- [x] **Step 2: Run test to verify it fails**

Run: `rtk cargo test -p throne new_non_empty_log_text_requests_scroll_to_bottom`

Expected: compilation failure because `should_scroll_logs_to_bottom` is not defined.

- [x] **Step 3: Write minimal implementation**

```rust
fn should_scroll_logs_to_bottom(previous: &str, current: &str) -> bool {
    !current.is_empty() && previous != current
}
```

- [x] **Step 4: Run test to verify it passes**

Run: `rtk cargo test -p throne new_non_empty_log_text_requests_scroll_to_bottom`

Expected: one passing test.

### Task 2: Attach the GPUI scroll handle

**Files:**
- Modify: `crates/throne/src/ui/main_window.rs:18-31,88-185,2860-2960`

- [x] **Step 1: Add persistent UI state**

Import `ScrollHandle`, then add `log_scroll_handle: ScrollHandle` and `rendered_log_text: String` to `MainWindow`. Initialize them with `ScrollHandle::new()` and `String::new()` in `MainWindow::new`.

- [x] **Step 2: Queue a scroll only for changed, non-empty logs**

Change `render_bottom_tabs` to take `&mut self`. After `let logs = self.state.logs_text();`, add:

```rust
if should_scroll_logs_to_bottom(&self.rendered_log_text, &logs) {
    self.log_scroll_handle.scroll_to_bottom();
}
self.rendered_log_text = logs.clone();
```

- [x] **Step 3: Track the existing log panel scroll position**

On the element identified as `logs-panel`, add:

```rust
.track_scroll(&self.log_scroll_handle)
```

The existing `.overflow_y_scroll()` remains in place.

- [x] **Step 4: Verify compilation and behavior contract**

Run: `rtk cargo test -p throne new_non_empty_log_text_requests_scroll_to_bottom && rtk cargo check -p throne`

Expected: the focused test passes and the desktop crate compiles without errors.
