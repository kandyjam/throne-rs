# Stable Traffic Display

## Problem

The runtime poller samples core traffic once per second and overwrites the
displayed rates with every sample. Empty samples therefore replace a recently
observed non-zero rate with `0B`, causing the status label to flash.

## Decision

Keep the most recently observed non-zero rate for each direction while the
core remains running. Ignore zero samples for a direction that already has a
displayed value. Reset all rates when the core stops, so a subsequent run
starts from a clean state.

## Data Flow

`CoreSession::query_stats` produces the periodic counters. `MainWindow`
normalizes them into rates and passes them to `AppState`. A small state-level
merge operation preserves prior non-zero directional rates, allowing the
window to notify GPUI only when the effective visible snapshot changes.

## Error Handling

Failed statistics requests continue to leave the current display intact. A
successful zero sample is treated as no new displayed rate while running.

## Test Contract

Given a running core with a non-zero displayed rate, applying an all-zero
sample keeps the non-zero values unchanged. Stopping the core clears the
snapshot.
