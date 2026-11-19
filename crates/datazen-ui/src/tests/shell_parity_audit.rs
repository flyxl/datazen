//! T053 independent audit of the `shell.ts` X-class ruling.
//!
//! `shell_parity.rs` proves the four concerns are present with static probes on
//! one frame. This file adds one observation channel those probes lack: it
//! *varies* the platform window-control inset reported by `Cx` and watches the
//! laid-out titlebar label move. A shell that merely drew a 40pt band but
//! dropped `usePlatform`'s OS-dimension inset would pass the documenting test
//! (height is still 40) yet fail here — so this is the receipt that the
//! platform concern is genuinely wired through `Cx`, not just claimed.

use super::*;

/// An element's laid-out left edge by key, straight from the rendered tree.
fn left_edge(rt: &Runtime<AppState>, key: &str) -> f32 {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("shell key {key} is laid out"))
        .rect[0]
}

/// The self-drawn titlebar must clear the OS window controls: the leading
/// padding comes from `Cx::titlebar_inset()` (app.rs `.pl(inset + 12.0)`), the
/// Rune replacement for React's OS-dimension `usePlatform()` hook. Vary the
/// inset and require the label to travel right by it.
#[test]
fn titlebar_follows_the_platform_window_control_inset() {
    let mut rt = runtime();
    rt.frame(0.0);
    let no_lights_x = left_edge(&rt, "datazen-titlebar-label");

    // AppKit's default traffic-light band consumes ~67pt of leading inset.
    let inset = rune::Chrome::DEFAULT.titlebar_inset();
    assert!(inset > 50.0, "expected a nonzero platform inset, got {inset}");
    rt.set_titlebar_inset(inset);
    rt.frame(0.1);
    rt.frame(5.0);
    let lights_x = left_edge(&rt, "datazen-titlebar-label");

    assert!(
        (lights_x - no_lights_x - inset).abs() < 1.0,
        "titlebar label must clear the window-control inset: no-lights x={no_lights_x}, \
         lights x={lights_x}, inset={inset}"
    );
}
