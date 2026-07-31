# Upstream Group Subscription Update Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Match upstream Throne's complete Manage Groups subscription-update interaction while preserving existing profiles on failed refreshes and routing requests through the running local proxy when System Proxy mode is enabled.

**Architecture:** `throne-import` will expose a structured, explicitly configured HTTP fetch; `throne-domain` will apply parsed subscription snapshots by stable identity as an atomic state mutation; and the GPUI layer will own confirmation, serial orchestration, dialogs, logs, and persistence. Existing group database columns already cover metadata, so storage schema changes are unnecessary.

**Tech Stack:** Rust 2021, GPUI 0.2, ureq 2, chrono, rusqlite, workspace unit tests.

---

## File Map

- Modify `crates/throne-import/src/fetch.rs`: structured response, explicit HTTP proxy, header lookup, fetch tests.
- Modify `crates/throne-import/src/lib.rs`: structured subscription fetch/import API and exports.
- Modify `crates/throne-domain/src/models.rs`: detailed subscription change report types.
- Modify `crates/throne-domain/src/store.rs`: identity-preserving transactional snapshot application and tests.
- Modify `crates/throne/src/ui/dialogs.rs`: group-card rendering, metadata formatting, confirmation and diff dialog variants, pure UI-policy tests.
- Modify `crates/throne/src/ui/main_window.rs`: single/update-all state machine, request option mapping, background work, logging, modal actions.
- Modify `crates/throne/src/main.rs`: assert that the new confirmation and diff dialog variants remain wired into the UI source.

### Task 1: Structured Subscription HTTP Fetch

**Files:**
- Modify: `crates/throne-import/src/fetch.rs`
- Modify: `crates/throne-import/src/lib.rs`

- [ ] **Step 1: Write failing response and request-option tests**

Add tests around pure construction and header behavior, including:

```rust
#[test]
fn response_header_lookup_is_case_insensitive() {
    let response = FetchResponse {
        body: "vless://example".into(),
        headers: vec![("subscription-userinfo".into(), "total=100".into())],
    };
    assert_eq!(response.header("Subscription-UserInfo"), Some("total=100"));
}

#[test]
fn proxy_url_uses_normalized_mixed_inbound() {
    let options = FetchOptions::with_http_proxy("::", 2080).unwrap();
    assert_eq!(options.proxy_url.as_deref(), Some("http://127.0.0.1:2080"));
}
```

- [ ] **Step 2: Run tests and verify the new API is missing**

Run: `rtk cargo test -p throne-import fetch::tests -- --nocapture`

Expected: FAIL because `FetchResponse` and `FetchOptions` are undefined.

- [ ] **Step 3: Implement explicit request configuration**

Introduce the public API:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FetchOptions {
    pub proxy_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchResponse {
    pub body: String,
    pub headers: Vec<(String, String)>,
}

impl FetchResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

pub fn fetch_url_with_options(
    url: &str,
    timeout: Duration,
    options: &FetchOptions,
) -> Result<FetchResponse, String>;
```

Build `ureq::Proxy` only when `proxy_url` is present, collect response headers before consuming the body, and keep `fetch_url`/`fetch_url_with_timeout` as body-only compatibility wrappers. Normalize empty or `::` inbound addresses to `127.0.0.1` in `FetchOptions::with_http_proxy`.

- [ ] **Step 4: Run importer tests**

Run: `rtk cargo test -p throne-import`

Expected: PASS.

- [ ] **Step 5: Commit the fetch boundary**

```bash
rtk git add crates/throne-import/src/fetch.rs crates/throne-import/src/lib.rs
rtk git commit -m "feat: add structured subscription fetch options"
```

### Task 2: Identity-Preserving Subscription Transaction

**Files:**
- Modify: `crates/throne-domain/src/models.rs`
- Modify: `crates/throne-domain/src/store.rs`

- [ ] **Step 1: Add failing domain contract tests**

Replace the coarse replacement tests with contracts proving that unchanged IDs and local latency/traffic survive, changed profiles update in place, removed profiles disappear, new profiles receive IDs, and final ordering follows the remote list:

```rust
#[test]
fn subscription_snapshot_preserves_identity_and_remote_order() {
    let mut state = fixture_with_profiles([old_a(), old_b()]);
    let old_a_id = state.group(1).unwrap().profile_ids[0];
    state.profile_mut_for_test(old_a_id).unwrap().latency_ms = 42;

    let report = state.apply_subscription_snapshot(
        1,
        vec![updated_b(), unchanged_a(), new_c()],
        "upload=1; download=2; total=10",
        1_785_500_000,
    ).unwrap();

    assert_eq!(report.added.len(), 1);
    assert_eq!(report.updated.len(), 1);
    assert_eq!(report.deleted.len(), 0);
    assert_eq!(state.profile(old_a_id).unwrap().latency_ms, 42);
    assert_eq!(state.group(1).unwrap().profile_ids, report.result_order);
}
```

Also add a test that calls parsing separately and verifies that an error path never invokes `apply_subscription_snapshot`, leaving a cloned state equal in groups/profiles/metadata.

- [ ] **Step 2: Run focused domain tests and observe failure**

Run: `rtk cargo test -p throne-domain subscription_snapshot -- --nocapture`

Expected: FAIL because the detailed transaction API and report do not exist.

- [ ] **Step 3: Implement detailed report and transaction**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionChange {
    pub profile_id: ProfileId,
    pub display: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubscriptionUpdateReport {
    pub added: Vec<SubscriptionChange>,
    pub updated: Vec<SubscriptionChange>,
    pub deleted: Vec<SubscriptionChange>,
    pub unchanged: usize,
    pub result_order: Vec<ProfileId>,
}
```

Implement `AppState::apply_subscription_snapshot(group_id, items, info, updated_at)`. Match exact dedupe identities first, then match changed records by the stable endpoint/protocol identity already encoded by domain helpers. Mutate a cloned profile/group working set and replace state only after all checks succeed. Preserve IDs and local test/traffic fields for retained profiles, update remote outbound/name/security fields, and select a valid remaining profile if the prior selection was deleted.

- [ ] **Step 4: Run all domain tests**

Run: `rtk cargo test -p throne-domain`

Expected: PASS.

- [ ] **Step 5: Commit the domain transaction**

```bash
rtk git add crates/throne-domain/src/models.rs crates/throne-domain/src/store.rs
rtk git commit -m "feat: preserve profile identity across subscription updates"
```

### Task 3: Subscription Metadata and Update Policy Helpers

**Files:**
- Modify: `crates/throne/src/ui/dialogs.rs`
- Modify: `crates/throne/src/ui/main_window.rs`

- [ ] **Step 1: Write failing pure-helper tests**

Add tests for quota parsing, group eligibility, diff-modal policy, and request routing:

```rust
#[test]
fn subscription_info_formats_usage_remaining_and_expiry() {
    let text = format_subscription_info(
        "upload=10; download=15; total=100; expire=1785500000",
    ).unwrap();
    assert!(text.contains("Used: 25B"));
    assert!(text.contains("Remain: 75B"));
    assert!(text.contains("Expire:"));
}

#[test]
fn update_all_skips_basic_and_archived_groups_in_order() {
    let groups = vec![basic(1), subscription(2), archived_subscription(3), subscription(4)];
    assert_eq!(eligible_subscription_ids(&groups), vec![2, 4]);
}

#[test]
fn manual_update_only_requests_diff_when_enabled() {
    assert!(should_show_subscription_diff(UpdateOrigin::Manual, true));
    assert!(!should_show_subscription_diff(UpdateOrigin::UpdateAll, true));
}
```

- [ ] **Step 2: Run the focused UI tests and verify failure**

Run: `rtk cargo test -p throne subscription_ -- --nocapture`

Expected: FAIL because the helper functions and `UpdateOrigin` are absent.

- [ ] **Step 3: Implement deterministic helpers**

Add `SubscriptionInfo` parsing with checked integer arithmetic, unlimited display for `total=0`, suppression when `total` is missing/invalid, local-time expiry formatting, `eligible_subscription_ids`, `UpdateOrigin`, `should_show_subscription_diff`, and `subscription_fetch_options(settings, core_status)`.

The routing helper contract is:

```rust
match (settings.system_proxy_enabled, core_status.is_running()) {
    (false, _) => Ok(FetchOptions::default()),
    (true, true) => FetchOptions::with_http_proxy(
        &settings.inbound_address,
        settings.inbound_socks_port,
    ),
    (true, false) => Err("Request with proxy but no profile started.".into()),
}
```

- [ ] **Step 4: Run focused tests**

Run: `rtk cargo test -p throne subscription_`

Expected: PASS.

- [ ] **Step 5: Commit helper behavior**

```bash
rtk git add crates/throne/src/ui/dialogs.rs crates/throne/src/ui/main_window.rs
rtk git commit -m "feat: add subscription update policy helpers"
```

### Task 4: Single-Group Update Orchestration and Diff Dialog

**Files:**
- Modify: `crates/throne-import/src/lib.rs`
- Modify: `crates/throne/src/ui/dialogs.rs`
- Modify: `crates/throne/src/ui/main_window.rs`
- Modify: `crates/throne/src/main.rs`

- [ ] **Step 1: Add a failing importer contract**

Define and test an `import_subscription_response` function that parses a supplied `FetchResponse`, rejects a body with no recognized profiles, and carries `Subscription-UserInfo` without performing state mutation:

```rust
#[test]
fn subscription_response_carries_provider_info() {
    let response = FetchResponse {
        body: "vless://11111111-1111-1111-1111-111111111111@example.com:443#Demo".into(),
        headers: vec![("Subscription-UserInfo".into(), "total=100".into())],
    };
    let imported = import_subscription_response(response).unwrap();
    assert_eq!(imported.user_info.as_deref(), Some("total=100"));
    assert_eq!(imported.report.profiles.len(), 1);
}
```

- [ ] **Step 2: Run the importer test and verify failure**

Run: `rtk cargo test -p throne-import subscription_response_carries_provider_info -- --nocapture`

Expected: FAIL because `import_subscription_response` is absent.

- [ ] **Step 3: Implement the single-group background workflow**

Add `SubscriptionImport { report, user_info }`, then replace `update_subscriptions(false)` with a single-group operation that snapshots URL/name/fetch options, fetches and parses on the background executor, and applies only a successful non-empty import on the UI thread.

Log these stages with the group name:

```text
>>>>>>>> Requesting subscription: NAME
<<<<<<<< Subscription request finished: NAME
>>>>>>>> Processing subscription data...
>>>>>>>> Process complete, applying...
<<<<<<<< Change of NAME:
REPORT
```

On error, log `<<<<<<<< Requesting subscription NAME error: ERROR`, leave state untouched, and show an error status rather than a zero-profile success.

Add `Dialog::SubscriptionDiff { title, body }` and render it using a constrained, scrollable modal. Open it only for a manual refresh when the setting permits. Closing it returns to the refreshed Manage Groups dialog.

Extend the source-level dialog coverage in `crates/throne/src/main.rs` with exact assertions for `SubscriptionDiff` and `ConfirmUpdateAllSubscriptions`, matching the existing `ConfirmDeleteUnavailable` assertion style.

- [ ] **Step 4: Run importer, domain, and throne tests**

Run: `rtk cargo test -p throne-import && rtk cargo test -p throne-domain && rtk cargo test -p throne`

Expected: PASS.

- [ ] **Step 5: Commit the single-update workflow**

```bash
rtk git add crates/throne-import/src/lib.rs crates/throne/src/ui/dialogs.rs crates/throne/src/ui/main_window.rs crates/throne/src/main.rs
rtk git commit -m "feat: align manual subscription refresh workflow"
```

### Task 5: Upstream Manage Groups Layout and Update-All State Machine

**Files:**
- Modify: `crates/throne/src/ui/dialogs.rs`
- Modify: `crates/throne/src/ui/main_window.rs`

- [ ] **Step 1: Add failing state-machine tests**

Extract a small serial queue and test progression without GPUI timing:

```rust
#[test]
fn serial_subscription_queue_advances_in_order_and_finishes() {
    let mut queue = SubscriptionUpdateQueue::new(vec![2, 4]);
    assert_eq!(queue.take_next(), Some(2));
    assert_eq!(queue.take_next(), Some(4));
    assert_eq!(queue.take_next(), None);
    assert!(queue.is_finished());
}
```

Test that an active queue rejects another update-all start and that a failed group still advances to the next eligible group.

- [ ] **Step 2: Run queue tests and verify failure**

Run: `rtk cargo test -p throne subscription_update_queue -- --nocapture`

Expected: FAIL because the queue does not exist.

- [ ] **Step 3: Render upstream-style group items**

Replace the current combined add/edit panel with list items that show type/count, name, URL, formatted metadata, and action buttons. Hide the update action for groups without a URL. Keep edit in a focused group editor dialog/state so the list itself remains scannable. Add footer actions `New group` and `Update all subscriptions`.

Add `Dialog::ConfirmUpdateAllSubscriptions` with `Yes` and `No`. `Yes` creates `SubscriptionUpdateQueue` from current eligible groups, marks the update operation busy, and dispatches the first group. Completion of each group dispatches the next regardless of success. The final completion clears busy state and refreshes the group list. A second request while active writes `The last subscription update has not exited.`

- [ ] **Step 4: Verify UI logic tests and compile**

Run: `rtk cargo test -p throne && rtk cargo check --workspace`

Expected: PASS with no compile errors.

- [ ] **Step 5: Commit the complete group interaction**

```bash
rtk git add crates/throne/src/ui/dialogs.rs crates/throne/src/ui/main_window.rs
rtk git commit -m "feat: align manage groups subscription interaction"
```

### Task 6: End-to-End Verification and Documentation Sync

**Files:**
- Modify: `docs/UPSTREAM_TRACKING.md`

- [ ] **Step 1: Mark the verified parity surface**

Update the Subscription UX tracking note to state that group cards, metadata,
manual diff, serialized update-all, and explicit local-proxy routing are aligned.
Keep HWID headers, scheduled auto-update, and `sub_clear` listed as separate gaps.

- [ ] **Step 2: Run formatting and full workspace verification**

Run: `rtk cargo fmt --all -- --check`

Expected: PASS.

Run: `rtk cargo test --workspace`

Expected: PASS.

Run: `rtk cargo check --workspace`

Expected: PASS.

- [ ] **Step 3: Launch the application for interaction verification**

Run: `rtk cargo run -p throne`

Expected: the app launches; Manage Groups shows unclipped cards at normal and narrow window sizes; single refresh shows progress and one diff modal; update-all asks for confirmation and remains responsive; transport failure preserves old profiles and metadata.

- [ ] **Step 4: Inspect the final scoped diff**

Run: `rtk git diff --check`

Expected: no whitespace errors.

Run: `rtk git status --short`

Expected: only files intentionally changed by this feature plus the user's pre-existing unrelated changes.

- [ ] **Step 5: Commit tracking documentation**

```bash
rtk git add docs/UPSTREAM_TRACKING.md
rtk git commit -m "docs: track group subscription parity"
```

## Completion Criteria

- Manual group updates route exactly according to System Proxy mode and never silently fall back when a proxy is required.
- Failed transport or parsing leaves the existing group snapshot and metadata intact.
- Successful updates retain matching profile IDs and local measurements, reorder by the remote response, and report added/updated/deleted entries.
- Manage Groups exposes upstream-equivalent group details and actions.
- Update-all is confirmed, serialized, non-overlapping, and skips ineligible groups.
- All workspace tests, formatting checks, and compilation pass; the running UI is visually and interactively verified.
