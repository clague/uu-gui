# iced fork patches

Local fork of iced (`github.com/iced-rs/iced` rev `a8231fd9`,
0.15.0-dev), wired into `uugui/Cargo.toml` as a path dependency:

```toml
iced = { path = "../iced-src", default-features = false, features = [
    "tiny-skia", "crisp", "thread-pool", "image-without-codecs",
] }
```

It exists so the renderer can be patched locally while remaining the
tiny-skia (CPU) backend. Refresh from upstream before re-applying.

## Patches

### 1. `window::set_visible` action (added)
(`runtime/src/window.rs`, `winit/src/lib.rs`)

The app creates its window **hidden** (decorated, caption stripped by a
subclass hook) and shows it right after with a raw `ShowWindow` — because
iced has NO `window::set_visible` task, winit's internal `VISIBLE` flag
stayed `false`. On the next window-flag diff (maximize / minimize via the
custom title bar buttons), winit's `apply_diff` fell through to

```rust
if !new.contains(WindowFlags::VISIBLE) {
    ShowWindow(window, SW_HIDE);
}
```

and HID the window while the process kept running (looked like a flash
exit). The patch adds the missing plumbing:

- `runtime/src/window.rs`: new `window::Action::SetVisible(Id, bool)`
  variant + `iced::window::set_visible(id, visible)` task helper.
- `iced_winit/src/lib.rs`: handler arm calling `window.raw.set_visible`.

the app then calls `set_visible(id, true)` right after its boot hook, so
all later window diffs are consistent.

```diff
--- a/runtime/src/window.rs
+++ b/runtime/src/window.rs
@@ runtime window Action enum
+    SetVisible(Id, bool),

@@ runtime window tasks
+pub fn set_visible<T>(id: Id, visible: bool) -> Task<T> {
+    task::effect(crate::Action::Window(Action::SetVisible(id, visible)))
+}
--- a/iced_winit/src/lib.rs
+++ b/iced_winit/src/lib.rs
@@ runtime window Action handler
+            window::Action::SetVisible(id, visible) => {
+                if let Some(window) = window_manager.get_mut(id) {
+                    window.raw.set_visible(visible);
+                }
+            }
```

### 2. Re-export `iced::task::blocking`
(`src/lib.rs`)

upstream `iced::task` only exposes `Handle`/`Task`; the runtime's
`Task::blocking` (closure on a fresh std thread → `mpsc` → stream task)
is the dependency-free substitute for tokio rental in this app, so
re-export it:

```diff
     pub mod task {
     //! Create runtime tasks.
-    pub use crate::runtime::task::{Handle, Task};
+    pub use crate::runtime::task::{blocking, Handle, Task};
```

### 3. `draw_quad` fast path for flat opaque rects
(`tiny_skia/src/engine.rs`)

Upstream builds an anti-aliased `rounded_rectangle` path and scans it for
EVERY quad — even for plain, borderless, cornerless rectangles whose edges
land on whole physical pixels, where AA is pure waste.

uugui's UI is mostly large flat dark surfaces (window/card backgrounds,
device cards by area, game rows). A scroll frame profile showed quads at
~70% of raster cost (5.7ms of 8ms); after the fast path the measured
mixed-frame totals dropped by roughly 30–45% (idle frames 1.8 → 0.9ms).

Condition for the fast path (all must hold, else the normal path runs):

- `Background::Color` with alpha = 1.0
- no border (`border.width == 0`)
- no shadow (`shadow.color.a == 0`)
- zero border radius (all four corners)
- bounds aligned to the physical pixel grid (±0.05px)

The fast path draws the exact rectangle with
`anti_alias: false` and returns; everything else (borders, radius,
gradients, shadows) keeps the original code path.

```diff
+        if let Background::Color(color) = background {
+            let tb = quad.bounds * transformation;
+            let eps = 0.05;
+            let aligned = [tb.x, tb.y, tb.x + tb.width, tb.y + tb.height]
+                .iter()
+                .all(|v| (v.round() - v).abs() < eps);
+            if color.a == 1.0
+                && quad.border.width == 0.0
+                && quad.shadow.color.a == 0.0
+                && aligned
+                && fill_border_radius.iter().all(|r| *r == 0.0)
+            {
+                let rect = tiny_skia::PathBuilder::from_rect(
+                    tiny_skia::Rect::from_xywh(
+                        quad.bounds.x, quad.bounds.y,
+                        quad.bounds.width, quad.bounds.height,
+                    ).expect("Build fast quad rectangle"),
+                );
+                pixels.fill_path(
+                    &rect,
+                    &tiny_skia::Paint {
+                        shader: tiny_skia::Shader::SolidColor(into_color(*color)),
+                        anti_alias: false,
+                        ..tiny_skia::Paint::default()
+                    },
+                    tiny_skia::FillRule::EvenOdd,
+                    transform,
+                    Some(&*clip_mask),
+                );
+                return;
+            }
+        }
```

## History

An `UU_RASTERTIME` raster profiler (per-frame fill/text/image/clip
timing) and a quad-phase split probe lived here temporarily to find the
hotspot above; they have been removed.
