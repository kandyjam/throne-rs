# Original Profile Sorting Design

## Goal

Match upstream Throne profile sorting so a group keeps its sorted profile order across a cold start.

## Behavior

- Clicking a sortable header reorders the active group's profile IDs.
- The reordered profile IDs are persisted with the group in `throne.db`.
- Cold start renders profiles in the persisted group order without reapplying a transient UI sort.
- `Test Result` uses latency by default; measured latency sorts numerically, followed by failed and untested profiles using the upstream-compatible key.
- Clicking the same header again reverses the order. The active header and direction remain session-only state, matching upstream.
- `test_sort_by` remains the persisted selector for the Test Result comparison mode. No new schema fields are introduced.

## Components

- `MainWindow`: converts header clicks into a group sort action and updates the session-only indicator.
- `AppState`: performs the stable profile-ID reorder within the active group.
- Storage: persists the group's reordered `profile_ids` through the existing group save path.

## Error Handling

- If the active group disappears during sorting, leave state unchanged.
- If persistence fails, keep the in-memory order visible and surface the existing status error path.
- Profiles missing from the store are retained deterministically rather than dropped.

## Verification

- A contract test proves Test Result ascending orders measured latency first using upstream-compatible special-value handling.
- A contract test proves descending reverses the comparison.
- A storage round-trip test proves the reordered group profile IDs survive reopening the database.
- Existing sorting and storage tests remain green.
