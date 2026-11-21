//! Port of `components/sql-editor/paste/multiCursorPointer.ts` (T302,
//! split) — the pointer gestures that grow a multi-cursor selection.
//!
//! CodeMirror's `EditorView.clickAddsSelectionRange` REPLACES its own
//! predicate rather than extending it (`@codemirror/view:4841`):
//!
//! ```js
//! function addsSelectionRange(view, event) {
//!   let facet = view.state.facet(clickAddsSelectionRange);
//!   return facet.length ? facet[0](event) : browser.mac ? event.metaKey : event.ctrlKey;
//! }
//! ```
//!
//! The host registered `(e) => e.altKey && !e.shiftKey`, which silently
//! dropped the platform chord: Cmd+click on macOS and Ctrl+click on
//! Windows/Linux did not add a cursor. This module restores that chord
//! alongside Alt+click instead of inventing a new mapping, so the host
//! and a stock CodeMirror agree on what a bare modified click means.
//!
//! ## Why the platform chord rather than one chord everywhere
//!
//! - **macOS ⌘+click.** System-wide this is the "click through /
//!   ignore" gesture, and in a browser it opens a link in a background
//!   tab. Inside this app it is unclaimed: the SQL editor renders no
//!   links, and the webview is a WKWebView whose click-through gesture
//!   is not delivered to page content. This is the same chord Sublime
//!   Text and VS Code use for "add cursor", so the muscle memory
//!   transfers.
//! - **macOS ⌃+click is deliberately NOT a multi-cursor chord.** On
//!   macOS Control+click is the secondary-click gesture: WebKit raises
//!   `contextmenu` for it. The SQL editor binds `contextmenu` to the
//!   statement menu (`createDomEventHandlers`), so claiming ⌃+click
//!   would fire a cursor-add and a context menu from one gesture.
//!   CodeMirror's own default agrees, which is why the platform switch
//!   is one-sided.
//! - **Windows/Linux Ctrl+click.** No click-through or
//!   secondary-click meaning exists for Ctrl in WinWebView2, so it is
//!   free.
//!
//! ## Modifier conflict table
//!
//! | gesture                  | behaviour                                            |
//! | ------------------------ | ---------------------------------------------------- |
//! | click                    | replace the selection with one cursor (CodeMirror)   |
//! | Alt/Option + click       | add a cursor                                         |
//! | ⌘ + click (macOS)        | add a cursor                                         |
//! | Ctrl + click (Win/Linux) | add a cursor                                         |
//! | Shift + click            | extend the selection — never a multi-cursor chord    |
//! | ⌘⌥ / Ctrl+Alt + click    | add a cursor (Alt decides; Alt+**drag** is rectangular) |
//! | ⌘ + drag (macOS)         | add a range while dragging (restores the CM default)  |
//! | Alt + drag               | rectangular / column selection (unchanged)            |
//!
//! A click is not a drag, so the ⌘⌥ and Ctrl+Alt combinations resolve
//! to "add a cursor" here while their drag counterparts stay
//! rectangular.
//!
//! Known deltas: `isMacOS()` reads `navigator` in React; the predicate
//! takes the platform the host resolved ([`Platform`], the
//! `extensions/types.ts` port's rule — the same seam
//! `is_macos`/`execute_shortcut_label` already use).

use crate::components::sql_editor::extensions::is_macos;
use crate::components::sql_editor::sql_editor_context_menu::Platform;

/// The modifier flags of one pointer event — the predicate's whole
/// input (CodeMirror's `clickAddsSelectionRange` predicate face: a
/// click, never a drag).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PointerModifiers {
    /// Alt / Option.
    pub alt: bool,
    /// Control.
    pub ctrl: bool,
    /// Command (the macOS ⌘ key).
    pub meta: bool,
    /// Shift.
    pub shift: bool,
}

/// Whether a pointer event should grow the selection with a new cursor
/// (`clickAddsCursor`).
///
/// A pure predicate over the modifier flags (the platform resolved by
/// the host), so the modifier table above can be asserted directly,
/// without mounting an editor.
pub(crate) fn click_adds_cursor(modifiers: PointerModifiers, platform: Platform) -> bool {
    // Shift+click extends the current selection. It is deliberately
    // excluded on every platform: it was excluded before this change
    // and is a distinct, still-supported gesture.
    if modifiers.shift {
        return false;
    }
    // Alt/Option+click is the pre-existing host chord and wins over
    // everything else, so ⌘⌥ and Ctrl+Alt are "add a cursor" and never
    // ambiguous.
    if modifiers.alt {
        return true;
    }
    // The platform chord, restoring CodeMirror's overridden default.
    if is_macos(platform) {
        modifiers.meta
    } else {
        modifiers.ctrl
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn evt(alt: bool, ctrl: bool, meta: bool, shift: bool) -> PointerModifiers {
        PointerModifiers { alt, ctrl, meta, shift }
    }

    /// The non-macOS table, every row
    /// (`multiCursorExitEscape.journey.test.ts`'s "agrees with every
    /// row of the documented table" — jsdom's empty
    /// `navigator.platform` means the main suite only ever exercises
    /// this branch).
    #[test]
    fn agrees_with_every_row_of_the_documented_non_mac_table() {
        let windows = Platform::Windows;
        // No modifier → CodeMirror replaces the selection; the
        // predicate says "do not add a range", which is exactly what
        // makes the default happen.
        assert!(!click_adds_cursor(evt(false, false, false, false), windows));
        assert!(click_adds_cursor(evt(true, false, false, false), windows));
        assert!(click_adds_cursor(evt(false, true, false, false), windows));
        // The macOS-only chord, excluded here.
        assert!(!click_adds_cursor(evt(false, false, true, false), windows));
        assert!(!click_adds_cursor(evt(false, false, false, true), windows));
        // Shift wins over Alt on every platform.
        assert!(!click_adds_cursor(evt(true, false, false, true), windows));
        // Alt decides Ctrl+Alt: "add a cursor", never ambiguous.
        assert!(click_adds_cursor(evt(true, true, false, false), windows));
        assert!(click_adds_cursor(evt(true, false, true, false), windows));
    }

    /// The macOS table (`multiCursorPointerClick.macos.test.ts`): the
    /// inversion versus the non-mac branch is the whole point — Ctrl
    /// and Cmd swap roles, and Ctrl+click is excluded because the
    /// browser turns it into a `contextmenu` (the statement context
    /// menu), not a cursor add.
    #[test]
    fn the_mac_table_swaps_ctrl_and_cmd_and_excludes_control_click() {
        let mac = Platform::Macos;
        assert!(click_adds_cursor(evt(false, false, true, false), mac));
        // Control+click is the secondary-click gesture on macOS — never
        // a cursor add here.
        assert!(!click_adds_cursor(evt(false, true, false, false), mac));
        assert!(click_adds_cursor(evt(true, false, false, false), mac));
        assert!(!click_adds_cursor(evt(false, false, false, true), mac));
        // ⌘⌥ and Ctrl+Alt both resolve to "add a cursor" (Alt decides).
        assert!(click_adds_cursor(evt(true, false, true, false), mac));
        assert!(click_adds_cursor(evt(true, true, false, false), mac));
        assert!(!click_adds_cursor(evt(false, false, false, false), mac));
    }

    /// A bare Linux/other platform is the non-mac branch too (the
    /// `browser.mac ? metaKey : ctrlKey` default the restore keeps).
    #[test]
    fn every_non_mac_platform_takes_the_ctrl_branch() {
        for platform in [Platform::Windows, Platform::Linux, Platform::Unknown] {
            assert!(click_adds_cursor(evt(false, true, false, false), platform), "{platform:?}");
            assert!(!click_adds_cursor(evt(false, false, true, false), platform), "{platform:?}");
        }
    }
}
