# Task 12 Report — System tray + close-to-tray

## Review fix (2026-08-09)

- **Exit prevention / close-to-tray** now require `tray_enabled` in addition to `close_to_tray` and active monitoring (same gate as tray visibility). Native Quit / Cmd+Q works when tray is disabled; tray Quit still sets `ALLOW_EXIT`.
- **CloseRequested hide-instead-of-close** scoped to main window (`label == "main"`) only; settings, connection, and dashboard windows close normally.
