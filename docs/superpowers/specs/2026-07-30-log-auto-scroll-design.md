# Log auto-scroll design

## Goal

Always show the latest log line when the Logs tab receives new content, including after the user has manually scrolled upward.

## Design

`MainWindow` owns a GPUI `ScrollHandle` for the logs panel. The panel tracks that handle. On every render of the Logs tab, the handle is instructed to scroll to the bottom after the newest log text has been laid out.

The existing twelve-line preview and the bounded domain log history remain unchanged. Copy and clear actions retain their current behavior.

## Contract and verification

The UI code requests `scroll_to_bottom` whenever the rendered log preview is non-empty. A focused Rust test protects the pure preview behavior; `cargo check -p throne` verifies the GPUI integration compiles.
