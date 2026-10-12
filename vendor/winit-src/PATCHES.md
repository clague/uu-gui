# winit fork patches

This is a local fork of the winit revision pinned by iced (iced-rs/winit
rev `48116469` = winit 0.30.13). `uugui/Cargo.toml` routes the build here
via:

```toml
[patch."https://github.com/iced-rs/winit.git"]
winit = { path = "../winit-src" }
```

Keep this fork in sync with upstream as long as iced builds against a git
winit; refresh `window_state.rs` etc. from upstream before applying these
patches again.

## Patches

### 1. `adjust_rect`: no caption reservation for app-managed title bars
(`src/platform_impl/windows/window_state.rs`)

winit computes the outer window rect from a requested surface size via
`AdjustWindowRectEx`, which reserves the **caption height** for decorated
windows. uugui keeps the native frame (WS_CAPTION + WS_THICKFRAME) but
strips the caption itself by overriding `WM_NCCALCSIZE` (the caption area
becomes client space where the app draws its own title bar). The reserved
caption would never exist, so the client came out ~30 logical px taller
than requested.

Fix: for `MARKER_DECORATIONS` windows, drop `WS_CAPTION` from the style
used for the outer-rect computation (frame metrics remain). Equivalent to
the subject of upstream PR #4100, which has not been merged as of
2026-10 (issue #3730 still open).

```diff
--- a/src/platform_impl/windows/window_state.rs
+++ b/src/platform_impl/windows/window_state.rs
@@ -434,6 +434,12 @@ impl WindowFlags {
             // `WM_NCCALCSIZE`.
             if !self.contains(WindowFlags::MARKER_DECORATIONS) {
                 style &= !(WS_CAPTION | WS_SIZEBOX);
+            } else {
+                // The application straps the caption itself (custom title
+                // bar via `WM_NCCALCSIZE`), so the outer rect must not
+                // reserve caption height — otherwise the client ends up
+                // taller than requested.
+                style &= !WS_CAPTION;
             }

             util::win_to_err({
```
