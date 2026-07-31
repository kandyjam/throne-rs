# Upstream Group Subscription Update Parity

## Goal

Align the Rust/GPUI group-management subscription workflow with
`upstream/dev` Throne. The source of truth is:

- `src/ui/group/dialog_manage_groups.cpp`
- `src/ui/group/GroupItem.cpp`
- `src/configs/sub/GroupUpdater.cpp`
- `src/global/HTTPRequestHelper.cpp`

This scope includes the group list presentation, single-group refresh,
update-all confirmation and serialization, subscription metadata, change
reporting, and the network routing semantics used by subscription requests.

## User Experience

The Manage Groups dialog presents each group as one list item. Every item shows:

- group type (`Basic`, `Subscription`, or the archived form) and profile count;
- group name;
- subscription URL when present;
- last successful update time when present;
- parsed subscription usage, remaining quota, and expiry when supplied by the
  provider;
- `Update Subscription`, `Edit`, and `Remove` actions as applicable.

The dialog footer provides `New group` and `Update all subscriptions` actions.
Updating all subscriptions first asks for confirmation. Once confirmed, eligible
groups are updated serially in tab order. Groups without a URL and archived
groups are skipped. A second update-all request while one is active is rejected
without starting overlapping work.

A manual single-group refresh shows the change report in a scrollable modal when
`sub_show_change_popup` is enabled. Update-all writes each report to the log but
does not open a modal for every group.

## Network Semantics

The HTTP fetch API returns a structured response containing the body and response
headers. It follows safe redirects, applies the existing Throne user agent, and
uses the existing request timeouts.

When System Proxy mode is enabled, subscription requests explicitly use the
running core's local mixed HTTP proxy at the normalized inbound address and
`inbound_socks_port`. They do not depend on operating-system proxy discovery or
`ureq` environment features. If proxy routing is required but no profile is
running, the request fails with the upstream-equivalent error instead of silently
falling back to a direct connection. When System Proxy mode is disabled, the
request is direct, matching upstream `HttpGet(..., useProxy = false)` behavior.

The updater reads `Subscription-UserInfo` case-insensitively. The header value is
stored in `Group.info` only after a successful fetch and parse/apply operation.
No HWID headers are added in this scope because the Rust settings model does not
currently expose upstream's opt-in HWID controls; adding those controls is a
separate settings-parity feature.

## Update Transaction

An update is prepared off the UI thread and applied to `AppState` as one logical
transaction. Fetching or parsing failure leaves the group's profiles,
`sub_last_update`, and `info` unchanged.

On success, the updater compares imported profiles with the existing group by
stable profile identity:

- unchanged profiles retain their profile IDs and local runtime/test metadata;
- profiles with the same identity but changed remote configuration are updated
  in place and reported as updated;
- new profiles receive new IDs and are reported as added;
- profiles absent from the new subscription are removed and reported as deleted;
- `Group.profile_ids` is reordered to match the remote subscription order.

After the profile transaction succeeds, `sub_last_update` is set to the current
epoch second and `info` is set from the response header. The database is then
persisted. The active profile is not silently stopped or replaced; deletion must
respect the existing active-profile safety contract.

## Reporting

The workflow logs the same meaningful stages as upstream:

1. requesting the named subscription;
2. request finished or request error;
3. processing subscription data;
4. process complete and applying;
5. the per-group added, updated, and deleted report, or `Nothing`.

Success status must not say `0 profiles` when a transport or parse error occurred.
Errors identify the affected group and preserve the provider/network error.

Subscription quota display parses `total`, `upload`, `download`, and `expire`
from the provider header. Missing or malformed `total` suppresses the quota line.
Zero total displays unlimited remaining quota. Byte counts use the application's
existing readable-size convention and expiry uses local short date/time format.

## Component Boundaries

- `throne-import` owns HTTP response capture and subscription-body parsing. Its
  fetch API accepts an explicit optional proxy rather than reading application
  state.
- `throne-domain` owns diffing, identity preservation, ordering, metadata update,
  and transactional state mutation.
- `throne` owns GPUI dialog state, confirmation/diff modals, serial orchestration,
  settings-to-request-option mapping, background execution, and persistence.
- `throne-storage` requires no schema migration because the existing group table
  already persists `info`, `sub_last_update`, `archive`, and `skip_auto_update`.

## Verification Contracts

Automated tests must demonstrate:

- direct and explicit-proxy request configuration are selected correctly;
- response headers are captured and looked up case-insensitively;
- malformed quota headers do not produce misleading usage text;
- unchanged identities retain IDs and local metadata;
- changed, added, and deleted profiles are classified correctly;
- output ordering follows the remote subscription;
- fetch or parse failure changes no group data or metadata;
- successful application updates `info` and `sub_last_update`;
- update-all eligibility skips empty-URL and archived groups in group order;
- only manual single-group updates request a diff modal.

Workspace tests and build must pass. UI verification must exercise group list
rendering, confirmation, busy state, single update success/failure, update-all,
and the scrollable diff modal without overlap or clipped text.

## Out Of Scope

- automatic scheduled subscription refresh;
- upstream `sub_send_hwid` and custom device-header settings;
- upstream `sub_clear` mode;
- broader Basic Settings parity;
- remote route-profile refresh behavior.
