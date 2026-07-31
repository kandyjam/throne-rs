# Upstream Runtime Logs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Separate transient status from the log panel and match upstream Throne's profile start/stop log entries.

**Architecture:** Add a status-only mutation to `AppState`, leaving `push_log` as the explicit log path. Add pure runtime-log formatting helpers in `main_window.rs`; start/stop flows append upstream-compatible entries while operational details update only the status strip.

**Tech Stack:** Rust, GPUI, Cargo unit tests

---

### Task 1: Status-Only Domain Contract

**Files:**
- Modify: `crates/throne-domain/src/store.rs`

- [ ] **Step 1: Write the failing status-only test**

Add a test that records `logs_text()`, calls the wished-for API, and verifies the status changes without log history changing:

```rust
#[test]
fn status_only_message_does_not_append_to_logs() {
    let mut state = AppState::empty();
    state.push_log("existing log");
    let logs_before = state.logs_text();

    state.set_status_message_only("Running [Tun]");

    assert_eq!(state.status_message(), "Running [Tun]");
    assert_eq!(state.logs_text(), logs_before);
}
```

- [ ] **Step 2: Verify RED**

Run: `rtk cargo test -p throne-domain status_only_message_does_not_append_to_logs`

Expected: compilation fails because `set_status_message_only` does not exist.

- [ ] **Step 3: Implement the status-only setter**

Add to `AppState`:

```rust
pub fn set_status_message_only(&mut self, msg: impl Into<String>) {
    self.status_message = msg.into();
}
```

- [ ] **Step 4: Verify GREEN**

Run: `rtk cargo test -p throne-domain`

Expected: all domain tests pass.

### Task 2: Upstream Runtime Log Formatting

**Files:**
- Modify: `crates/throne/src/ui/main_window.rs`

- [ ] **Step 1: Write failing formatter tests**

Add tests for the pure display and event helpers:

```rust
#[test]
fn runtime_profile_logs_match_upstream_wording() {
    let display = runtime_profile_display(ProfileType::Vless, "Tokyo");
    assert_eq!(display, "[VLESS] Tokyo");
    assert_eq!(start_profile_log(&display), ">>>>>>>> Starting profile [VLESS] Tokyo");
    assert_eq!(stop_profile_log(&display), ">>>>>>>> Stopping profile [VLESS] Tokyo");
    assert_eq!(failed_start_profile_log(&display), "<<<<<<<< Failed to start profile [VLESS] Tokyo");
    assert_eq!(FAILED_STOP_PROFILE_LOG, "<<<<<<<< Failed to stop, please restart the program.");
}

#[test]
fn running_mode_marker_matches_upstream() {
    assert_eq!(running_mode_marker(true, false), "[Tun]");
    assert_eq!(running_mode_marker(false, true), "[System Proxy]");
    assert_eq!(running_mode_marker(true, true), "[Tun+System Proxy]");
    assert_eq!(running_mode_marker(false, false), "");
}
```

- [ ] **Step 2: Verify RED**

Run: `rtk cargo test -p throne runtime_profile_logs_match_upstream_wording`

Expected: compilation fails because the formatting helpers do not exist.

- [ ] **Step 3: Implement pure helpers**

Implement the functions and constant directly beside the existing transition helpers. Use `ProfileType::display_name()` for the upstream type label and exact upstream punctuation.

- [ ] **Step 4: Route start and stop through separate outputs**

At start kickoff, append `start_profile_log`. At stop kickoff, capture the running profile display and append `stop_profile_log`. On failures append the exact upstream failure entry, then put detailed Rust errors in `set_status_message_only`.

Use `set_status_message_only` for `Starting`, `Stopping`, successful running/stopped summaries, queued operations, TUN privilege/mode messages, and system-proxy application messages. Do not change core state transitions or persistence.

Render the successful running status with `running_mode_marker`, preserving route and mixed inbound details in the status strip.

- [ ] **Step 5: Verify UI GREEN**

Run: `rtk cargo test -p throne`

Expected: all application tests pass.

### Task 3: Regression Verification

**Files:**
- No additional source changes

- [ ] **Step 1: Run affected package tests**

Run:

```bash
rtk cargo test -p throne-domain
rtk cargo test -p throne-storage
rtk cargo test -p throne
```

Expected: all tests pass.

- [ ] **Step 2: Run workspace checks**

Run:

```bash
rtk cargo check --workspace
rtk git diff --check
```

Expected: zero errors and no whitespace failures. Existing dependency warnings may remain.

- [ ] **Step 3: Review scoped diff**

Confirm the changes do not alter core start/stop sequencing, database schemas, or unrelated user edits. Do not commit, merge, or push unless the user explicitly requests it.
