//! The host's system tray (T9): a StatusNotifierItem icon with a flat menu
//! over rune's tray. The tray runs on its own thread and answers while the
//! app's window is closed; the events route back through the app handle —
//! show and focus the window, or quit the process. An environment without a
//! tray host (no session bus, plain WM) is an honest absence: the icon is
//! missing, the window interface is untouched.

use rune::{AppHandle, Tray, TrayEvent, TrayIcon, TrayItem, TraySpec};

/// One side of a rounded pixel square: the icon this host draws — a filled
/// disc on a transparent ground, small enough for every tray's geometry.
/// The raw pixels come back beside their size, so the shape is observable
/// where the tray's own types are not.
fn disc_icon(size: u32) -> (u32, Vec<u8>) {
    let center = (size as f32 - 1.0) / 2.0;
    let radius = size as f32 * 0.42;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let inside = dx * dx + dy * dy <= radius * radius;
            // The DataZen accent: a solid blue disc, opaque inside.
            rgba.extend_from_slice(if inside {
                &[38, 108, 208, 255]
            } else {
                &[0, 0, 0, 0]
            });
        }
    }
    (size, rgba)
}

fn icon_pixels(size: u32) -> TrayIcon {
    let (size, rgba) = disc_icon(size);
    TrayIcon::rgba(size, size, rgba)
}

/// The tray's menu: show the window, or quit.
fn menu() -> Vec<TrayItem> {
    vec![
        TrayItem::action("show", "Show DataZen"),
        TrayItem::Separator,
        TrayItem::action("quit", "Quit"),
    ]
}

/// The tray this host installs: its stable id, a tooltip, and the menu —
/// show the window, or quit.
pub fn spec() -> TraySpec {
    TraySpec::new("datazen-native", "DataZen", icon_pixels(32))
        .tooltip("DataZen")
        .menu(menu())
}

/// Install the tray over the running app's handle (T9). True when the icon
/// landed. An environment that cannot host one is logged, not fatal: the
/// window interface continues without the tray.
pub fn install<S: 'static>(handle: AppHandle<S>) -> bool {
    match Tray::new(spec(), move |event| match event {
        // A primary click, or any menu pick but quit, shows the window.
        TrayEvent::Activate { .. } => {
            handle.show();
            handle.focus();
        }
        TrayEvent::Menu(id) if id != "quit" => {
            handle.show();
            handle.focus();
        }
        TrayEvent::Menu(_) => handle.quit(),
        _ => {}
    }) {
        Ok(tray) => {
            // Dropping a Tray removes its icon; the host keeps it for the
            // process's whole life.
            std::mem::forget(tray);
            true
        }
        Err(error) => {
            eprintln!(
                "the system tray is unavailable ({error}); the window interface continues without it"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_and_the_icon_shape_are_what_the_tray_carries() {
        // The menu's items are the public vocabulary the tray renders.
        assert_eq!(
            menu(),
            vec![
                TrayItem::action("show", "Show DataZen"),
                TrayItem::Separator,
                TrayItem::action("quit", "Quit"),
            ]
        );
        // The icon: a 32×32 RGBA sheet with drawn and transparent pixels
        // both (the tray hosts that need an alpha ground keep it).
        let (size, rgba) = disc_icon(32);
        assert_eq!(size, 32);
        assert_eq!(rgba.len(), 32 * 32 * 4, "the icon is 32×32 RGBA");
        assert!(
            rgba.chunks_exact(4).any(|pixel| pixel[3] == 255),
            "some pixel is drawn"
        );
        assert!(
            rgba.chunks_exact(4).any(|pixel| pixel[3] == 0),
            "some pixel is transparent"
        );
        // The spec builds over both.
        let _ = spec();
    }

    #[test]
    fn a_tray_that_cannot_start_is_an_honest_absence_not_a_crash() {
        // This sandbox runs no session bus: whatever the backend answers —
        // an icon-less Ok on a busless desktop, an Err here — the call must
        // return, never panic. The install function's contract is exactly
        // this: the outcome is logged, the window interface continues.
        let outcome = match Tray::new(spec(), |_| {}) {
            Ok(tray) => {
                // A tray that started on a busless desktop stays iconless
                // until a host appears; drop it, the probe owes no icon.
                drop(tray);
                true
            }
            Err(error) => {
                eprintln!("probe: the tray is absent here ({error})");
                false
            }
        };
        // Both outcomes are acceptable on this machine; what is pinned is
        // that the call answered.
        let _ = outcome;
    }
}
