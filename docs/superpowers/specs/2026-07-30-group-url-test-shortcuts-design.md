# Group URL Test Shortcuts Design

## Goal

Match the original Throne desktop actions for testing every profile in the active group and removing unavailable profiles.

## Behavior

- `Ctrl+Shift+G` runs a URL latency test for every profile in the active group.
- `Ctrl+Shift+R` finds only active-group profiles whose `latency_ms` is negative.
- Removing profiles requires confirmation and leaves untested (`latency_ms == 0`) and reachable profiles intact.
- The GPUI client additionally accepts the matching Command shortcuts on macOS, consistent with its existing cross-platform bindings.

## Implementation

`MainWindow` owns shortcut bindings and the asynchronous group test. `AppState` owns the pure unavailable-profile selection/removal behavior. A small dialog state renders a confirmation message before persistence and deletion.

## Verification

Domain tests prove unavailable selection is scoped to the active group and excludes untested profiles. UI-source contract tests prove the original shortcut bindings and confirmation flow remain present.
