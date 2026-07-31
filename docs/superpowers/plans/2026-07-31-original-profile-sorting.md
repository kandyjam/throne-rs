# Original Profile Sorting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist profile-table sorting by reordering each group's profile IDs, matching upstream Throne across cold starts.

**Architecture:** Move comparison and group-order mutation into `AppState`, where profiles and ordered group IDs already live. Keep the selected header and direction in `MainWindow` for the current session only; each click mutates the active group order and immediately saves through the existing database path.

**Tech Stack:** Rust, GPUI, `throne-domain`, SQLite via `throne-storage`, Cargo tests

---

### Task 1: Domain Sorting Contract

**Files:**
- Modify: `crates/throne-domain/src/store.rs`

- [ ] **Step 1: Write failing tests for latency order and reversal**

Add tests in `store.rs` that create measured, failed, and untested profiles, then assert the active group's visible order:

```rust
#[test]
fn active_group_sort_by_latency_persists_upstream_order() {
    let mut state = AppState::empty();
    let group = state.add_group("G");
    let slow = state.add_profile(group, "slow", ProfileType::Vless);
    let untested = state.add_profile(group, "untested", ProfileType::Vless);
    let failed = state.add_profile(group, "failed", ProfileType::Vless);
    let fast = state.add_profile(group, "fast", ProfileType::Vless);
    state.set_profile_latency(slow, 180);
    state.set_profile_latency(untested, 0);
    state.set_profile_latency(failed, -1);
    state.set_profile_latency(fast, 47);

    state.sort_active_group_profiles(ProfileSortColumn::TestResult, true).unwrap();
    assert_eq!(
        state.visible_profiles().iter().map(|p| p.id).collect::<Vec<_>>(),
        vec![fast, slow, failed, untested],
    );

    state.sort_active_group_profiles(ProfileSortColumn::TestResult, false).unwrap();
    assert_eq!(
        state.visible_profiles().iter().map(|p| p.id).collect::<Vec<_>>(),
        vec![untested, failed, slow, fast],
    );
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run: `rtk cargo test -p throne-domain active_group_sort_by_latency_persists_upstream_order`

Expected: compilation fails because `ProfileSortColumn` and `sort_active_group_profiles` do not exist.

- [ ] **Step 3: Add the domain sort API**

Define the shared comparison enum and the mutating method in `store.rs`:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileSortColumn {
    Type,
    Address,
    Name,
    TestResult,
    Traffic,
}

fn upstream_latency_sort_key(ms: i32) -> i32 {
    if ms == 0 { 100_000 } else if ms < 0 { 99_999 } else { ms }
}

pub fn sort_active_group_profiles(
    &mut self,
    column: ProfileSortColumn,
    ascending: bool,
) -> Result<(), StoreError> {
    let group_id = self.active_group_id;
    let mut ids = self
        .groups
        .get(&group_id)
        .ok_or(StoreError::GroupNotFound(group_id))?
        .profile_ids
        .clone();
    ids.sort_by(|a, b| {
        let ord = match (self.profiles.get(a), self.profiles.get(b)) {
            (Some(a), Some(b)) => compare_profiles(a, b, column),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.cmp(b),
        };
        if ascending { ord } else { ord.reverse() }
    });
    self.groups
        .get_mut(&group_id)
        .expect("active group checked above")
        .profile_ids = ids;
    Ok(())
}
```

Implement `compare_profiles` for the five existing columns. Test Result must use the upstream key `measured -> failed -> untested`; Traffic compares saturated download-plus-upload totals; text comparisons remain case-insensitive as in the current UI sorter.

- [ ] **Step 4: Run domain tests and verify GREEN**

Run: `rtk cargo test -p throne-domain`

Expected: all `throne-domain` tests pass.

### Task 2: Persist Sorting From Header Clicks

**Files:**
- Modify: `crates/throne/src/ui/main_window.rs`

- [ ] **Step 1: Add a failing unit test for upstream two-state toggling**

Extract a pure transition helper and first write its contract:

```rust
#[test]
fn repeated_header_clicks_alternate_without_clearing_sort() {
    assert_eq!(next_sort_state(SortColumn::None, true, SortColumn::TestResult),
               (SortColumn::TestResult, true));
    assert_eq!(next_sort_state(SortColumn::TestResult, true, SortColumn::TestResult),
               (SortColumn::TestResult, false));
    assert_eq!(next_sort_state(SortColumn::TestResult, false, SortColumn::TestResult),
               (SortColumn::TestResult, true));
}
```

- [ ] **Step 2: Run the focused UI test and verify RED**

Run: `rtk cargo test -p throne repeated_header_clicks_alternate_without_clearing_sort`

Expected: compilation fails because `next_sort_state` does not exist.

- [ ] **Step 3: Replace transient sorting with persisted group reordering**

Add the transition and mapping helpers:

```rust
fn next_sort_state(current: SortColumn, asc: bool, clicked: SortColumn) -> (SortColumn, bool) {
    if current == clicked {
        (clicked, !asc)
    } else {
        (clicked, true)
    }
}

fn domain_sort_column(column: SortColumn) -> Option<ProfileSortColumn> {
    match column {
        SortColumn::None => None,
        SortColumn::Type => Some(ProfileSortColumn::Type),
        SortColumn::Address => Some(ProfileSortColumn::Address),
        SortColumn::Name => Some(ProfileSortColumn::Name),
        SortColumn::TestResult => Some(ProfileSortColumn::TestResult),
        SortColumn::Traffic => Some(ProfileSortColumn::Traffic),
    }
}
```

Update `toggle_sort` to compute the new session state, call `state.sort_active_group_profiles`, call `persist_db`, surface a `Sort failed: ...` status on either error, then notify. Replace `sorted_profiles` with a direct clone of `state.visible_profiles()` so rendering always follows the persisted group order. Remove the UI-only `latency_sort_key` helper.

- [ ] **Step 4: Run UI tests and verify GREEN**

Run: `rtk cargo test -p throne repeated_header_clicks_alternate_without_clearing_sort`

Expected: the focused test passes.

### Task 3: Cold-Start Storage Round Trip

**Files:**
- Modify: `crates/throne-storage/src/lib.rs`

- [ ] **Step 1: Add the cold-start contract test**

```rust
#[test]
fn sorted_profile_order_survives_database_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("throne.db");
    let mut state = AppState::empty();
    let group = state.add_group("G");
    let slow = state.add_profile(group, "slow", ProfileType::Vless);
    let fast = state.add_profile(group, "fast", ProfileType::Vless);
    state.set_profile_latency(slow, 200);
    state.set_profile_latency(fast, 40);
    state
        .sort_active_group_profiles(ProfileSortColumn::TestResult, true)
        .unwrap();

    Database::open(&path).unwrap().save_state(&state).unwrap();
    let reopened = Database::open(&path).unwrap().load_state().unwrap();

    assert_eq!(
        reopened.visible_profiles().iter().map(|p| p.id).collect::<Vec<_>>(),
        vec![fast, slow],
    );
}
```

- [ ] **Step 2: Run the cold-start test**

Run: `rtk cargo test -p throne-storage sorted_profile_order_survives_database_reopen`

Expected: PASS because existing `profiles_json` persistence preserves the newly reordered IDs.

- [ ] **Step 3: Run package and workspace verification**

Run:

```bash
rtk cargo test -p throne-domain
rtk cargo test -p throne-storage
rtk cargo test -p throne
rtk cargo check --workspace
rtk git diff --check
```

Expected: all tests and checks pass. If repository-wide formatting still reports pre-existing differences, run `rustfmt --check` only on touched files and report the broader pre-existing failure without rewriting unrelated files.

### Task 4: Manual Cold-Start Verification

**Files:**
- No source changes

- [ ] **Step 1: Build and launch the app**

Run: `rtk cargo run -p throne`

- [ ] **Step 2: Verify persisted behavior**

Click `Test Result` once and confirm the header shows `↑` and measured values are fastest-first. Quit normally, relaunch, and confirm the row order remains fastest-first from the first frame; the header indicator may be absent because direction is session-only, matching upstream.

- [ ] **Step 3: Verify reversal**

Click `Test Result` once after relaunch and confirm the persisted order reverses and remains reversed after another normal quit/relaunch.
