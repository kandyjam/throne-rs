# Group URL Test Shortcuts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Match original Throne shortcuts for active-group latency testing and confirmed removal of unavailable nodes.

**Architecture:** Keep group URL testing in `MainWindow`; bind the original actions there. Keep deletion eligibility in `AppState`, then gate mutation through a compact GPUI confirmation dialog.

**Tech Stack:** Rust 2021, GPUI, Cargo unit tests.

---

### Task 1: Protect unavailable-node semantics

**Files:**
- Modify: `crates/throne-domain/src/store.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn unavailable_removal_is_scoped_to_active_group_and_excludes_untested_profiles() {
    // negative latency in the active group is removable; zero is not;
    // a negative latency in another group remains.
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `rtk cargo test -p throne-domain unavailable_removal_is_scoped_to_active_group_and_excludes_untested_profiles`

Expected: FAIL because the selection API is absent.

- [ ] **Step 3: Add the minimal domain API**

```rust
pub fn unavailable_profile_ids_in_group(&self, group_id: GroupId) -> Vec<ProfileId> {
    self.group(group_id)
        .into_iter()
        .flat_map(|group| group.profile_ids.iter().copied())
        .filter(|id| self.profile(*id).is_some_and(|profile| profile.latency_ms < 0))
        .collect()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `rtk cargo test -p throne-domain unavailable_removal_is_scoped_to_active_group_and_excludes_untested_profiles`

Expected: PASS.

### Task 2: Bind original actions and require confirmation

**Files:**
- Modify: `crates/throne/src/ui/main_window.rs`
- Modify: `crates/throne/src/ui/dialogs.rs`

- [ ] **Step 1: Write source-contract tests**

```rust
assert!(source.contains("cmd-shift-g"));
assert!(source.contains("ctrl-shift-g"));
assert!(source.contains("cmd-shift-r"));
assert!(source.contains("ctrl-shift-r"));
assert!(source.contains("ConfirmDeleteUnavailable"));
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `rtk cargo test -p throne original_group_actions_have_original_shortcuts`

Expected: FAIL because the group and delete bindings differ.

- [ ] **Step 3: Implement the minimal UI flow**

```rust
// DeleteUnavailable opens Dialog::ConfirmDeleteUnavailable when candidates exist.
// Confirm invokes delete_selected_profiles, persists, and closes the dialog.
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `rtk cargo test -p throne original_group_actions_have_original_shortcuts`

Expected: PASS.

### Task 3: Verify integration

**Files:**
- Modify: `crates/throne-domain/src/store.rs`
- Modify: `crates/throne/src/ui/main_window.rs`
- Modify: `crates/throne/src/ui/dialogs.rs`

- [ ] **Step 1: Run focused tests**

Run: `rtk cargo test -p throne-domain && rtk cargo test -p throne`

Expected: PASS.

- [ ] **Step 2: Run workspace tests**

Run: `rtk cargo test --workspace`

Expected: PASS.
