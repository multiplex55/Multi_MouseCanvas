# MultiMouseCanvas

MultiMouseCanvas is a standalone, Windows-first desktop application for recording global mouse position over time and turning the recording into a visual canvas. While a session is recording, the app samples the cursor position, draws movement lines, and draws dwell shapes that grow while the pointer remains near the same location. Movement and dwell artwork can be colored by the foreground executable identity so activity from different applications is easier to distinguish.

## What the application records

During a recording session, MultiMouseCanvas may store the following data in memory, recovery files, and exported canvas/session data:

- **Mouse position samples**: physical/global cursor coordinates over time.
- **Timestamps**: sample times, recovery save times, session timing, and export filenames based on the export timestamp.
- **Foreground executable identity where available**: process ID, executable name, executable path, and a display label derived from process metadata.
- **Optional window class**: the foreground window class may be captured when the platform adapter can provide it.
- **Session statistics**: sample count, session duration, cursor distance, movement segment count, dwell counts, current dwell duration, and longest dwell duration.
- **Canvas/export data**: movement paths, dwell shapes, colors, canvas dimensions, virtual desktop bounds, background settings, and PNG exports.

## What the application does not record

MultiMouseCanvas is designed for cursor-path visualization, not content surveillance. By default it does **not** collect or store:

- Mouse clicks.
- Keyboard input.
- Screenshots.
- Window contents.
- Text fields or typed text.
- Browser URLs.
- Window titles by default.

## Files and storage locations

MultiMouseCanvas uses the operating system's per-user application configuration location through the Rust `directories` crate with organization/application identifiers `com`, `MultiMouseCanvas`, and `MultiMouseCanvas`.

On Windows, this normally resolves to a path similar to:

```text
%APPDATA%\MultiMouseCanvas\MultiMouseCanvas\config\
```

Stored files include:

- **Settings file**: `settings.json` in the app config directory.
- **Color registry data**: stored inside `settings.json` under the application color registry settings.
- **Autosave/recovery file**: `recovery/autosave.recovery.json` below the same config directory.
- **Canvas export files**: PNG files are written to the configured export directory. The default is the relative directory `exports` unless changed in settings.

Recovery files can contain the current canvas, application color registry, session name, save time, statistics, virtual desktop bounds, and whether the session was completed.

## Application-specific colors

When app-specific coloring is enabled, each sampled foreground executable identity is converted to a stable key. The executable path is used when available; otherwise the executable name is used. MultiMouseCanvas then assigns colors according to the selected color mode:

- **Fixed global color**: every app uses the same configured color.
- **Application-specific / palette once**: the stable key chooses a deterministic color from the configured palette.
- **Random once per app**: the stable key generates a deterministic pseudo-random color the first time the app is seen.

Assigned colors are saved in the color registry. In the settings UI, editing an application's color creates a **manual override**. Manual overrides take precedence over automatic colors until you use **Reset**, which returns that application to its automatic color.

## Basic use

### Start recording

Open MultiMouseCanvas and press **Start recording**. If a previous unexported canvas exists, the app asks whether to clear it, preserve it for export, or cancel the new session.

You can also send a command to a running instance with:

```bash
multi_mouse_canvas --start
```

### Pause and resume

Press **Pause** while recording to stop sampling temporarily. Press **Resume** to continue. Pause/resume creates a recording discontinuity so unrelated movement segments are not connected.

Command-line equivalents for a running instance:

```bash
multi_mouse_canvas --pause
multi_mouse_canvas --resume
```

### Finish a session

Press **Finish session** to finalize active movement and dwell shapes, stop sampling, update statistics, and make the canvas available for export.

Command-line equivalent:

```bash
multi_mouse_canvas --finish
```

### Export PNG

Press **Export PNG** when the canvas is not empty. The file is written to the configured export directory using a session/timestamp-based filename. Existing filenames are avoided by appending a numeric suffix.

### Clear a canvas

Press **Clear canvas** to remove movement paths and dwell shapes from the current canvas. If recording is active, stop or finish the recording first when you want to ensure no new samples are added immediately after clearing.

### Restore or discard recovery data

If MultiMouseCanvas detects incomplete recovery data at startup, it shows a status message. Use:

- **Restore recovery** to load the saved canvas, color registry, statistics, virtual desktop bounds, and completion state.
- **Discard recovery** to delete the recovery file without restoring it.

### Fully stop recording

To fully stop recording/sampling, use **Finish session** or choose **Exit** from the tray/menu and confirm when prompted. Closing the window may hide the app to the tray while recording, depending on the close-window behavior setting; hiding to tray does not stop recording.

## Multi-monitor assumptions and limitations

MultiMouseCanvas records cursor coordinates in a global/virtual desktop coordinate space. The canvas stores virtual desktop bounds so positions from multi-monitor layouts can be mapped into one drawing area.

Current assumptions and limitations:

- The app is Windows-first; non-Windows builds use placeholder tray/capture behavior where platform APIs are unavailable.
- Multi-monitor layouts are treated as one virtual desktop rectangle.
- Monitor arrangement, DPI scaling, and negative monitor coordinates may affect how physical cursor coordinates map to canvas coordinates.
- Changing monitor layout or DPI settings during a session may produce discontinuities or unexpected scaling.
- The canvas visualizes cursor paths; it does not reconstruct individual monitor screenshots or window contents.

## Tray and close-window behavior

On Windows, MultiMouseCanvas creates a system tray menu with commands to show the window, start recording, pause/resume, finish the session, export the current canvas, and exit.

The close-window behavior is configurable:

- **Minimize to tray while recording**: closing the window hides it and recording continues in the tray; when stopped, closing exits.
- **Exit after confirmation**: closing asks for confirmation before exiting and stopping background sampling.
- **Always exit**: closing exits the app.

If you need to guarantee recording is stopped, use **Finish session** or **Exit** rather than only closing/hiding the window.

## Command-line commands and future Multi Launcher integration

The standalone app supports simple command-line commands that can control an already-running instance:

```text
--show      Show the application window
--start     Start recording global mouse position samples
--pause     Pause recording without collecting mouse samples
--resume    Resume recording mouse position samples
--finish    Finish the session and stop recording
--help      Print command-line help
```

These commands are intended to support future Multi Launcher command-line integration. This README only describes command-line integration points; it does not claim full plugin integration with Multi Launcher.

## Development

```bash
cargo test
cargo run
```
