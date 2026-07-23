# MultiMouseCanvas

MultiMouseCanvas is a Windows-first Rust desktop application scaffold for visualizing multi-mouse activity on a canvas.

## Phase 1 scope

This initial standalone application provides the desktop shell, recording-state commands, JSON-backed settings, placeholders for capture/export/platform adapters, and an empty canvas preview.

Phase 1 does **not** perform global mouse tracking yet. Cursor samples, dwell state, foreground application data, and Windows capture adapters are placeholders reserved for later implementation phases.

## Development

```bash
cargo test
cargo run
```
