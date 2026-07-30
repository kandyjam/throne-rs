# Stable Traffic Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent periodic zero samples from replacing a live traffic rate.

**Architecture:** Move the rate-retention rule into `AppState`, so it is unit-testable and the UI poller can ask whether an effective display change occurred. The poller continues collecting one-second samples but redraws only after a visible update.

**Tech Stack:** Rust, GPUI, workspace Cargo tests.

---

### Task 1: Preserve displayed rates

**Files:**
- Modify: `crates/throne-domain/src/store.rs`
- Modify: `crates/throne/src/ui/main_window.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn zero_traffic_sample_preserves_live_rates_while_running() {
    let mut state = AppState::with_demo_data();
    state.toggle_selected().unwrap();
    state.set_traffic(TrafficSnapshot { proxy_up: 1_024, ..Default::default() });
    assert!(!state.update_live_traffic(TrafficSnapshot::default()));
    assert_eq!(state.traffic().proxy_up, 1_024);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p throne-domain zero_traffic_sample_preserves_live_rates_while_running`
Expected: FAIL because `update_live_traffic` does not exist.

- [ ] **Step 3: Write minimal implementation**

```rust
pub fn update_live_traffic(&mut self, sample: TrafficSnapshot) -> bool {
    // Retain each current non-zero directional rate when the new sample is zero.
    // Return whether the effective snapshot changed.
}
```

Make `MainWindow::poll_core_runtime` call this method and set `should_notify`
from its return value.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p throne-domain zero_traffic_sample_preserves_live_rates_while_running`
Expected: PASS.

- [ ] **Step 5: Run affected workspace tests**

Run: `cargo test -p throne-domain && cargo test -p throne`
Expected: PASS.
