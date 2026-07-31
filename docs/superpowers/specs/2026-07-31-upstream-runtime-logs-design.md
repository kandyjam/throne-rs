# Upstream Runtime Log Compatibility Design

## Goal

Match upstream Throne's runtime log entries without removing useful Rust UI status information.

## Behavior

The log panel and current status are separate outputs. Status-only messages update the status strip but do not append a log line.

Runtime log entries use upstream wording and profile display names:

- `>>>>>>>> Starting profile [TYPE] Name`
- `>>>>>>>> Stopping profile [TYPE] Name`
- `<<<<<<<< Failed to start profile [TYPE] Name`
- `<<<<<<<< Failed to stop, please restart the program.`

Operational details such as route, mixed inbound address, TUN/system-proxy mode, privilege checks, queued operations, and successful completion remain visible as status-only messages. The running mode marker follows upstream status notation: `[Tun]`, `[System Proxy]`, or `[Tun+System Proxy]`.

## Architecture

`AppState` gains a status-only setter alongside the existing log-appending setter. `MainWindow` uses the status-only path for transient runtime state and appends logs explicitly only at the same start/stop boundaries as upstream.

A pure profile display helper produces upstream's `[TYPE] Name` representation so start, stop, and failure messages cannot drift apart.

## Failure Handling

Start and stop failures append the upstream failure log entry. Detailed Rust error text remains available in the status strip, preserving diagnostics without changing the compatible log message.

## Tests

- A domain test proves status-only updates do not append to log history.
- UI unit tests lock the upstream profile display and start/stop message strings.
- Existing start/stop transition tests remain green.
