# MultiMouseCanvas

MultiMouseCanvas is a Windows-first tray application that turns global cursor movement into tiled artwork. Recording continues while the window is hidden; **Pause**, **Finish**, or **Exit** explicitly stops collection.

## Recording and privacy

The recorder collects physical desktop cursor coordinates and monotonic sample times, session timing/statistics, and (when available) foreground executable identity/path or window class for stable application-specific colors. Color assignments remain editable. It does **not** collect clicks, keyboard input, screenshots, window contents, typed text, URLs, or window titles by default. No data is sent to a service by the application.

A stationary pointer becomes a live dwell after the configured activation delay. Hardware jitter inside the tolerance radius advances the same dwell without moving its center; movement beyond the radius finalizes it. Each later dwell is independent, even at the same coordinate, so translucent dwells alpha-compose rather than merge. Circle, triangle, and square shapes use the same fill/outline settings in preview and export and grow only to the configured maximum.

## Displays and coordinates

Every connected display is represented at its physical pixel proportions in the Windows virtual-desktop coordinate space. This supports negative X/Y origins, ultrawide displays, unequal resolutions, vertical offsets, touching edges, and physical gaps. A crossing is drawn only when consecutive coordinates belong to a continuous layout. Monitor add/remove, resolution/rotation, origin, and DPI/topology changes finalize live geometry first, preventing a diagonal across unrelated coordinate spaces.

Session bounds are the union of every observed topology. Adding a display left or above expands the bounds into negative coordinates; existing pixels keep their original desktop coordinates. Bounds never shrink during a session, and layout changes never rescale or crop existing artwork. Exports after several changes therefore include historical regions as empty or painted space at their original scale.

## Sparse tiled storage and recovery

Finalized paths and dwells are immediately rasterized into sparse RGBA tiles. Only touched regions allocate tiles; previews upload allocated/visible dirty tiles, and the recorder retains only bounded in-progress geometry—not full raw samples, finalized paths, or finalized dwell lists.

Current recovery is a directory containing `recovery-version`, `manifest.json`, and `tiles/<x>_<y>.png`. Autosave is time-based and rewrites only recovery-dirty tiles, validates temporary output, then atomically publishes the manifest last. Interrupted `.tmp-*` files are harmless. Malformed, unsupported, incomplete, or legacy data is reported and preserved until the user explicitly imports or discards it. The former `autosave.recovery.json` is import-only compatibility data, not the primary format.

Settings are stored in the platform configuration directory (normally `%APPDATA%\\MultiMouseCanvas\\MultiMouseCanvas\\config\\settings.json`). Missing fields receive defaults, future fields are ignored, and numeric settings are validated/clamped. A parse failure uses in-memory defaults but never overwrites or deletes the original file.

## Export

The tiled compositor exports **PNG or WebP**, with approved scale presets/custom validated scale, and either transparent, solid-color, or canvas backgrounds. Monitor outlines/labels are optional export overlays. Large desktops consume memory proportional to the output dimensions during encoding, so reduce scale when the requested image approaches system memory limits.

## Commands and lifecycle

UI, tray, IPC, and CLI all route the same Start, Pause, Resume, Finish, Export, Show, and Exit command model. A second instance forwards commands to the running instance.

```text
--show --start --pause --resume --finish --export --exit --help
```

Closing the visible window can hide it to the tray while recording, depending on settings. Exiting stops and joins the sampler/background engine before process shutdown.

## Performance and validation

Sampling, simplification, tile rasterization, preview upload, and dirty-tile recovery are incremental. Storage and retained geometry scale with touched regions rather than session duration. Normal tests include deterministic accelerated 24-hour timestamps and synthetic continuous/curved movement, dwell/jitter, display crossing/change, pause/resume, application-switch boundaries, and sleep-sized gaps without sleeping. An ignored high-volume diagnostic is intended for Windows release runs and reports throughput while OS tooling records CPU, peak/steady memory, tile/recovery sizes, coalescing, dirty flush, and preview upload durations; resource values are diagnostics rather than brittle unit-test limits.

## Development

```bash
cargo fmt --check
cargo clippy --all-targets --all-features
cargo test --all-targets
cargo test --release windows_long_session_diagnostic -- --ignored --nocapture
```
