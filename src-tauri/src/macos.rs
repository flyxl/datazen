//! macOS window customization: overlay title bar and traffic light positioning.
//!
//! Replicates tao's `inset_traffic_lights` algorithm via native Cocoa APIs
//! so that programmatically created sub-windows get the same button layout
//! as the main window. Also provides `apply_overlay_titlebar` to apply
//! the overlay title bar style after window creation (avoiding Windows
//! incompatibility from putting `titleBarStyle: Overlay` in tauri.conf.json).

use objc2::msg_send;
use objc2::runtime::AnyObject;
use objc2_foundation::NSRect;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

const X: f64 = 16.0;
const Y: f64 = 18.0;

const CLOSE: isize = 0;
const MINIATURIZE: isize = 1;
const ZOOM: isize = 2;

/// Apply overlay-style title bar: transparent, hidden title, full-size content.
/// This replaces `titleBarStyle: "Overlay"` + `hiddenTitle: true` from the JSON
/// config, which is macOS-specific and causes click-through issues on Windows.
pub fn apply_overlay_titlebar(window: &impl HasWindowHandle) {
    let Ok(handle) = window.window_handle() else { return };
    if let RawWindowHandle::AppKit(h) = handle.as_raw() {
        unsafe {
            let ns_view = h.ns_view.as_ptr() as *mut AnyObject;
            let ns_window: *mut AnyObject = msg_send![ns_view, window];
            if ns_window.is_null() {
                return;
            }
            // NSWindowStyleMask flags
            let mask: usize = msg_send![ns_window, styleMask];
            let full_size_content_view: usize = 1 << 15; // NSWindowStyleMaskFullSizeContentView
            let _: () = msg_send![ns_window, setStyleMask: mask | full_size_content_view];
            let _: () = msg_send![ns_window, setTitlebarAppearsTransparent: true];
            let _: () = msg_send![ns_window, setTitleVisibility: 1_isize]; // NSWindowTitleHidden
        }
    }
}

/// Apply custom traffic light positions on any type that exposes a window handle.
pub fn apply_traffic_lights(window: &impl HasWindowHandle) {
    let Ok(handle) = window.window_handle() else { return };
    if let RawWindowHandle::AppKit(h) = handle.as_raw() {
        unsafe {
            let ns_view = h.ns_view.as_ptr() as *mut AnyObject;
            let ns_window: *mut AnyObject = msg_send![ns_view, window];
            if !ns_window.is_null() {
                inset_traffic_lights(ns_window);
            }
        }
    }
}

/// Low-level: reposition the three standard window buttons on an NSWindow.
///
/// Mirrors the algorithm in `tao::platform_impl::macos::view::inset_traffic_lights`.
unsafe fn inset_traffic_lights(ns_window: *mut AnyObject) {
    let close: *mut AnyObject = msg_send![ns_window, standardWindowButton: CLOSE];
    if close.is_null() {
        return;
    }
    let miniaturize: *mut AnyObject = msg_send![ns_window, standardWindowButton: MINIATURIZE];
    let zoom: *mut AnyObject = msg_send![ns_window, standardWindowButton: ZOOM];

    let tb_container: *mut AnyObject = {
        let sv: *mut AnyObject = msg_send![close, superview];
        msg_send![sv, superview]
    };
    if tb_container.is_null() {
        return;
    }

    let close_rect: NSRect = msg_send![close, frame];
    let tb_height = close_rect.size.height + Y;

    let mut tb_rect: NSRect = msg_send![tb_container, frame];
    tb_rect.size.height = tb_height;
    let win_frame: NSRect = msg_send![ns_window, frame];
    tb_rect.origin.y = win_frame.size.height - tb_height;
    let _: () = msg_send![tb_container, setFrame: tb_rect];

    let mini_rect: NSRect = msg_send![miniaturize, frame];
    let space = mini_rect.origin.x - close_rect.origin.x;

    for (i, button) in [close, miniaturize, zoom].iter().enumerate() {
        let mut rect: NSRect = msg_send![*button, frame];
        rect.origin.x = X + (i as f64 * space);
        let _: () = msg_send![*button, setFrameOrigin: rect.origin];
    }
}
