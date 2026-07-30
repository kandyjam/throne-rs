# Remove Non-Original Main Window UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the two non-original UI regions from the GPUI main window while preserving every remaining layout and runtime behavior.

**Architecture:** This is a presentation-only reduction in `MainWindow`. The mode-controls stack retains TUN and system-proxy controls, while the renderer stops adding the runtime-summary card. Existing DNS state and runtime polling are not changed because they may support code outside those rendered regions.

**Tech Stack:** Rust 2021, GPUI 0.2, Cargo test.

---

## File Structure

- `crates/throne/src/main.rs` owns lightweight source-level UI composition contracts used by the binary crate tests.
- `crates/throne/src/ui/main_window.rs` owns the main-window render tree and is the only production file changed.

### Task 1: Lock the removed visual regions with a regression contract

**Files:**
- Modify: `crates/throne/src/main.rs:84`
- Test: `crates/throne/src/main.rs:84`

- [ ] **Step 1: Write the failing test**

Add this test in the existing `tests` module:

```rust
#[test]
fn main_window_omits_non_original_dns_and_runtime_summary_regions() {
    let source = include_str!("ui/main_window.rs");

    assert!(source.contains("mode_checkbox(\"tun\", \"Tun Mode\""));
    assert!(source.contains("mode_checkbox(\"proxy\", \"System Proxy\""));
    assert!(!source.contains("mode_checkbox(\"dns\", \"System DNS\""));
    assert!(!source.contains("fn render_data_view(&self)"));
    assert!(!source.contains(".child(self.render_data_view())"));
}
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run:

```bash
rtk cargo test -p throne main_window_omits_non_original_dns_and_runtime_summary_regions
```

Expected: the test fails because the main-window source still contains the DNS checkbox and `render_data_view`.

### Task 2: Remove only the two non-original rendered regions

**Files:**
- Modify: `crates/throne/src/ui/main_window.rs:1678-1713`
- Modify: `crates/throne/src/ui/main_window.rs:1989-2020`

- [ ] **Step 1: Remove the DNS checkbox child**

Delete only this child from `render_top_bar`:

```rust
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
```

- [ ] **Step 2: Remove the runtime-summary child and its renderer**

In `render_top_bar`, replace the final two chained children with the proxy-mode control as the final child:

```rust
.child({
    let e = entity.clone();
    let on = self.state.settings().system_proxy_enabled;
    mode_checkbox("proxy", "System Proxy", on, move |_, _, cx| {
        e.update(cx, |this, cx| {
            let next = !this.state.settings().system_proxy_enabled;
            this.set_sys_proxy(next, cx);
        });
    })
})
```

Then delete the complete `fn render_data_view(&self) -> impl IntoElement` method. Do not change `set_sys_dns`, DNS settings persistence, polling, or bottom status-bar rendering.

- [ ] **Step 3: Run the focused regression test and verify it passes**

Run:

```bash
rtk cargo test -p throne main_window_omits_non_original_dns_and_runtime_summary_regions
```

Expected: one passing test; both retained original controls are still asserted.

### Task 3: Verify the crate and workspace contracts

**Files:**
- Verify only: `crates/throne/src/main.rs`
- Verify only: `crates/throne/src/ui/main_window.rs`

- [ ] **Step 1: Format the changed Rust files**

Run:

```bash
rtk cargo fmt --check
```

Expected: exit code 0.

- [ ] **Step 2: Run the Throne crate tests**

Run:

```bash
rtk cargo test -p throne
```

Expected: exit code 0.

- [ ] **Step 3: Run the workspace test suite**

Run:

```bash
rtk cargo test --workspace
```

Expected: exit code 0. Report unrelated failures without changing unrelated code.

## Self-Review

- Spec coverage: Task 2 removes exactly the DNS control and runtime summary; it explicitly preserves TUN, system proxy, runtime state, and bottom status behavior.
- Placeholder scan: no deferred requirements or placeholder steps are present.
- Type consistency: the test inspects the two exact production render fragments removed in Task 2.

## Execution Note

No commit step is included because the workspace has pre-existing user changes and the user did not request a commit.
