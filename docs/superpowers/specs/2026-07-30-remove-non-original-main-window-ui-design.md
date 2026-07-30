# Remove Non-Original Main Window UI

## Goal

Align the GPUI main window more closely with the supplied original Throne view through a strictly scoped deletion-only change.

## Scope

- Remove the `System DNS` toggle from the main-window mode controls.
- Remove the non-original runtime summary card that shows the selected profile, mixed inbound, route, and proxy-running detail.
- Preserve all remaining main-window layout, current theme, menus, dialogs, table, logs, bottom status bar, shortcuts, and proxy/core behavior unchanged.

## Implementation

The main-window renderer will omit those two visual children. Any state or runtime polling they depend on remains intact when it supports existing behavior elsewhere. No domain, storage, core-client, or public configuration contract changes are required.

## Verification

Add or update focused source-level rendering contracts that prove the removed labels and summary-card renderer are absent from the main-window composition, while `Tun Mode` and `System Proxy` remain present. Run the targeted crate tests and the workspace test suite.
