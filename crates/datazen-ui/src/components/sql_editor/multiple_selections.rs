//! Port of `components/sql-editor/paste/multipleSelections.ts` (T320,
//! split) — the multi-cursor compartment's binding inventory.
//!
//! The React file is a CodeMirror 6 extension factory: it wires engine
//! facets (`allowMultipleSelections`, `drawSelection`,
//! `rectangularSelection`, `clickAddsSelectionRange`), one raw keydown
//! handler and two keymaps. The engine wiring has no Rune counterpart
//! — the native editor implements the commands itself — so this port
//! is the **contract**: every binding with its chord, platform
//! variant, precedence tier and preventDefault decision, the Mod-D
//! direct handler's predicate, and the rectangular-drag filter. The
//! native key dispatch answers from this table; nothing here renders.
//!
//! ## What maps where
//!
//! - `EditorState.allowMultipleSelections.of(true)` → the constant
//!   [`ALLOW_MULTIPLE_SELECTIONS`] (the feature is ungated: no
//!   compartment ever holds it false).
//! - `drawSelection()` → [`MULTIPLE_SELECTIONS_FACETS`]'s
//!   [`Facet::DrawSelection`]: secondary cursors have to be *drawn*,
//!   not just selected — with two ranges and no draw layer the second
//!   cursor is logically present and visually absent (the measured
//!   regression the React comment records). The native editor's
//!   cursor painting is the facet's counterpart.
//! - `rectangularSelection({ eventFilter })` → the
//!   [`Facet::RectangularSelection`] flag +
//!   [`rectangular_drag_predicate`]: macOS Option+drag,
//!   Shift+Option+drag and Cmd+Option+drag on a Trackpad, primary
//!   button only.
//! - `EditorView.clickAddsSelectionRange.of(clickAddsCursor)` → the
//!   [`Facet::ClickAddsSelectionRange`] flag; the predicate itself is
//!   T302's ([`crate::components::sql_editor::multi_cursor_pointer`]).
//! - The raw `domEventHandlers.keydown` (the WKWebView guard for
//!   Mod+D) → [`mod_d_direct_handler`]: Meta/Ctrl without Alt on the
//!   `d`/`D`/`KeyD` spellings, answered with
//!   preventDefault+stopPropagation only when
//!   `selectNextOccurrence` handled it.
//! - The plain `keymap.of([...])` → [`multiple_selections_bindings`]
//!   at [`BindingPrecedence::Default`]: the Mod-d family (all four
//!   spellings — `Shift-Mod-d` is the non-US-layout safety net, not
//!   dead weight), the non-shift add-cursor chord, and the copy-line
//!   moves (free on Windows/Linux at normal precedence; on macOS
//!   `Cmd+Shift+Up` stays the core "select to document start"
//!   gesture, which is why the four-modifier fallback exists).
//! - `Prec.high(keymap.of([...]))` → the same table's
//!   [`BindingPrecedence::High`] trio: `Shift-Alt-ArrowUp/Down`
//!   (dead at default precedence — `defaultKeymap` owns the chord on
//!   every platform and is registered first) and `Escape`
//!   ([`MultiCursorCommand::ExitMultiCursor`], T283's decision).
//!
//! ## The Escape precedence table (the file header's inventory)
//!
//! | # | binding | source | precedence |
//! |-: |---|---|---|
//! | 1 | `clearSnippet` | autocomplete snippetKeymap | highest |
//! | 2 | `closeCompletion` | autocomplete completionKeymap | highest |
//! | 3 | **`exitMultiCursor`** | **this module** | **high** |
//! | 4 | `simplifySelection` | commands defaultKeymap | default |
//! | 5 | `closeSearchPanel` | search searchKeymap | default, panel-scoped |
//!
//! Rows 1–2 answer only when a snippet or completion is live (both
//! return `false` otherwise), so the two-step dismissal survives; row
//! 5 is scope-gated to the panel. `Prec.high` — not `highest` — on
//! purpose: highest would put multi-cursor exit above the panel
//! dismissals. A Pro binding at `Prec.highest` that *declines* falls
//! through to row 3, so the privileged seam keeps its hand-back.
//!
//! Known deltas: `preventDefault` on the Escape binding is
//! deliberately absent (CodeMirror applies it even when `run` returns
//! false, which would swallow the one-cursor decline); the model
//! carries that as [`KeyBinding::prevent_default`] = `false` and the
//! test pins it.

/// The commands the inventory registers (CodeMirror imports; the
/// native editor implements the behaviours).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MultiCursorCommand {
    /// `selectNextOccurrence` (`@codemirror/search`): an empty cursor
    /// expands to the word, the next press adds the next occurrence.
    SelectNextOccurrence,
    /// `addCursorAbove` (`@codemirror/commands`).
    AddCursorAbove,
    /// `addCursorBelow`.
    AddCursorBelow,
    /// `copyLineUp` — selection only stays one range, the line is
    /// duplicated.
    CopyLineUp,
    /// `copyLineDown`.
    CopyLineDown,
    /// `exitMultiCursor` (T283's decision module).
    ExitMultiCursor,
}

/// The precedence tier a binding mounts at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingPrecedence {
    /// `Prec.high`: lifted above `defaultKeymap` only — never above
    /// the editor's own custom shortcuts or the autocomplete
    /// dismissals.
    High,
    /// Plain `keymap.of(...)`: registration order decides, and
    /// `defaultKeymap` (registered first in `editorExtensions.ts`)
    /// wins every conflict.
    Default,
}

/// One keymap binding: the chord, its macOS variant, the command, the
/// precedence tier and the preventDefault decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyBinding {
    /// The `key:` spelling (`Mod-` is Cmd on macOS, Ctrl elsewhere).
    pub(crate) key: &'static str,
    /// The `mac:` variant, when the chord differs.
    pub(crate) mac: Option<&'static str>,
    pub(crate) command: MultiCursorCommand,
    pub(crate) precedence: BindingPrecedence,
    /// Whether the binding declares `preventDefault`.
    pub(crate) prevent_default: bool,
}

/// `EditorState.allowMultipleSelections.of(true)` — the feature flag
/// the compartment exists to hold (never false).
pub(crate) const ALLOW_MULTIPLE_SELECTIONS: bool = true;

/// The engine facets the extension wires (beyond the keymaps).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Facet {
    /// `drawSelection()`: the cursor/selection decoration layer — a
    /// second cursor must be *visible* (one `.cm-cursor-secondary`
    /// per extra range), or the multi-cursor state is an invisible
    /// accident.
    DrawSelection,
    /// `rectangularSelection({ eventFilter })`: Alt/Option+drag column
    /// selection, Trackpad-compatible.
    RectangularSelection,
    /// `EditorView.clickAddsSelectionRange.of(clickAddsCursor)` —
    /// REPLACES the engine default, so the host owns the whole chord
    /// table (T302's predicate).
    ClickAddsSelectionRange,
}

/// The facet list the extension holds, in mount order.
pub(crate) const MULTIPLE_SELECTIONS_FACETS: [Facet; 3] = [
    Facet::DrawSelection,
    Facet::RectangularSelection,
    Facet::ClickAddsSelectionRange,
];

/// The full binding inventory: the plain keymap (default precedence)
/// followed by the `Prec.high` trio. The order inside one tier is the
/// registration order the engine's conflict resolution uses.
pub(crate) fn multiple_selections_bindings() -> Vec<KeyBinding> {
    let default_tier = [
        // The Mod-d family: `runHandlers` tries the shift-less name
        // first for character keys, so `Mod-d` answers the unshifted
        // press and `Mod-D` the shifted one; `Shift-Mod-d/-D` are the
        // non-US-layout keyCode fallbacks (base[68] === "d"), not
        // dead weight.
        ("Mod-d", None, MultiCursorCommand::SelectNextOccurrence),
        ("Mod-D", None, MultiCursorCommand::SelectNextOccurrence),
        ("Shift-Mod-d", None, MultiCursorCommand::SelectNextOccurrence),
        ("Shift-Mod-D", None, MultiCursorCommand::SelectNextOccurrence),
        // The non-shift add-cursor chord (Option+Cmd+Up/Down on macOS).
        (
            "Alt-Mod-ArrowUp",
            Some("Alt-Cmd-ArrowUp"),
            MultiCursorCommand::AddCursorAbove,
        ),
        (
            "Alt-Mod-ArrowDown",
            Some("Alt-Cmd-ArrowDown"),
            MultiCursorCommand::AddCursorBelow,
        ),
        // Copy line keeps a home now that Option+Shift+Up/Down is
        // multi-cursor: free on Windows/Linux at normal precedence.
        // On macOS `Cmd+Shift+Up` is standardKeymap's
        // select-to-document-start and is left alone — hence the
        // mac-only four-modifier fallbacks in the high tier's wake.
        ("Mod-Shift-ArrowUp", None, MultiCursorCommand::CopyLineUp),
        ("Mod-Shift-ArrowDown", None, MultiCursorCommand::CopyLineDown),
        (
            "Alt-Shift-Mod-ArrowUp",
            Some("Alt-Shift-Cmd-ArrowUp"),
            MultiCursorCommand::CopyLineUp,
        ),
        (
            "Alt-Shift-Mod-ArrowDown",
            Some("Alt-Shift-Cmd-ArrowDown"),
            MultiCursorCommand::CopyLineDown,
        ),
    ];
    let high_tier = [
        // The actual fix for the dead Shift-Alt-Arrow* keys:
        // `@codemirror/commands` binds them to copyLineUp/Down with NO
        // platform variant, so defaultKeymap (registered first) wins
        // on every platform — `Prec.high` lifts ONLY these above it.
        ("Shift-Alt-ArrowUp", None, MultiCursorCommand::AddCursorAbove),
        ("Shift-Alt-ArrowDown", None, MultiCursorCommand::AddCursorBelow),
        // Escape joins them: the only competitor is defaultKeymap's
        // `simplifySelection` (registered first, default precedence).
        // NO preventDefault — see the module header: declaring it
        // would swallow the one-cursor decline.
        ("Escape", None, MultiCursorCommand::ExitMultiCursor),
    ];
    let mut bindings = default_tier
        .into_iter()
        .map(|(key, mac, command)| KeyBinding {
            key,
            mac,
            command,
            precedence: BindingPrecedence::Default,
            prevent_default: true,
        })
        .collect::<Vec<_>>();
    bindings.extend(high_tier.into_iter().map(|(key, mac, command)| KeyBinding {
        key,
        mac,
        command,
        precedence: BindingPrecedence::High,
        // The Escape binding deliberately omits it.
        prevent_default: key != "Escape",
    }));
    bindings
}

/// The raw `domEventHandlers.keydown` guard: the high-priority direct
/// handler for Mod+D / Cmd+D across macOS WKWebView and web. The
/// event's `key`/`code` spellings arrive verbatim (`d` or `D`, or the
/// physical `KeyD` for layouts that report another character).
pub(crate) fn mod_d_direct_handler(
    meta_key: bool,
    ctrl_key: bool,
    alt_key: bool,
    key: &str,
    code: &str,
) -> bool {
    (meta_key || ctrl_key) && !alt_key && (key == "d" || key == "D" || code == "KeyD")
}

/// `rectangularSelection`'s `eventFilter`: macOS Option+drag,
/// Shift+Option+drag and Cmd+Option+drag on a Trackpad, primary
/// button only. The predicate is the React expression verbatim — the
/// first disjunct subsumes the second (`e.altKey || (e.altKey &&
/// e.shiftKey)`), kept as written so the table stays readable; the
/// tests pin the *semantics* (Alt is the deciding modifier).
pub(crate) fn rectangular_drag_predicate(
    alt_key: bool,
    shift_key: bool,
    meta_key: bool,
    button: i16,
    buttons: u16,
) -> bool {
    (alt_key || (alt_key && shift_key) || (meta_key && alt_key)) && (button == 0 || buttons == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_binding_inventory_is_the_react_table() {
        let bindings = multiple_selections_bindings();
        assert_eq!(bindings.len(), 13, "10 default + 3 high");
        // The default tier, in registration order.
        let default_tier: Vec<&KeyBinding> = bindings
            .iter()
            .filter(|b| b.precedence == BindingPrecedence::Default)
            .collect();
        let keys: Vec<&str> = default_tier.iter().map(|b| b.key).collect();
        assert_eq!(
            keys,
            vec![
                "Mod-d",
                "Mod-D",
                "Shift-Mod-d",
                "Shift-Mod-D",
                "Alt-Mod-ArrowUp",
                "Alt-Mod-ArrowDown",
                "Mod-Shift-ArrowUp",
                "Mod-Shift-ArrowDown",
                "Alt-Shift-Mod-ArrowUp",
                "Alt-Shift-Mod-ArrowDown",
            ]
        );
        // The mac variants exist exactly where the chord differs.
        let by_key = |key: &str| {
            bindings
                .iter()
                .find(|b| b.key == key)
                .unwrap_or_else(|| panic!("{key} missing"))
        };
        assert_eq!(by_key("Alt-Mod-ArrowUp").mac, Some("Alt-Cmd-ArrowUp"));
        assert_eq!(by_key("Alt-Mod-ArrowDown").mac, Some("Alt-Cmd-ArrowDown"));
        assert_eq!(by_key("Alt-Shift-Mod-ArrowUp").mac, Some("Alt-Shift-Cmd-ArrowUp"));
        assert_eq!(by_key("Alt-Shift-Mod-ArrowDown").mac, Some("Alt-Shift-Cmd-ArrowDown"));
        assert_eq!(by_key("Mod-d").mac, None);
        assert_eq!(by_key("Mod-Shift-ArrowUp").mac, None, "no mac variant: the chord is left to standardKeymap on macOS");

        // The commands.
        assert_eq!(by_key("Mod-d").command, MultiCursorCommand::SelectNextOccurrence);
        assert_eq!(by_key("Alt-Mod-ArrowUp").command, MultiCursorCommand::AddCursorAbove);
        assert_eq!(by_key("Alt-Mod-ArrowDown").command, MultiCursorCommand::AddCursorBelow);
        assert_eq!(by_key("Mod-Shift-ArrowUp").command, MultiCursorCommand::CopyLineUp);
        assert_eq!(by_key("Mod-Shift-ArrowDown").command, MultiCursorCommand::CopyLineDown);
        assert_eq!(
            by_key("Alt-Shift-Mod-ArrowUp").command,
            MultiCursorCommand::CopyLineUp,
            "the mac fallback keeps copy-line alive"
        );
    }

    #[test]
    fn the_high_tier_is_exactly_the_trio() {
        let bindings = multiple_selections_bindings();
        let high: Vec<&KeyBinding> = bindings
            .iter()
            .filter(|b| b.precedence == BindingPrecedence::High)
            .collect();
        assert_eq!(high.len(), 3);
        assert_eq!(high[0].key, "Shift-Alt-ArrowUp");
        assert_eq!(high[0].command, MultiCursorCommand::AddCursorAbove);
        assert_eq!(high[1].key, "Shift-Alt-ArrowDown");
        assert_eq!(high[1].command, MultiCursorCommand::AddCursorBelow);
        assert_eq!(high[2].key, "Escape");
        assert_eq!(high[2].command, MultiCursorCommand::ExitMultiCursor);
    }

    #[test]
    fn every_binding_prevents_default_except_escape() {
        for binding in multiple_selections_bindings() {
            let expected = binding.key != "Escape";
            assert_eq!(
                binding.prevent_default, expected,
                "{}: preventDefault is {}",
                binding.key, expected
            );
        }
    }

    #[test]
    fn the_facet_flags_are_mounted() {
        assert!(ALLOW_MULTIPLE_SELECTIONS);
        assert_eq!(
            MULTIPLE_SELECTIONS_FACETS,
            [
                Facet::DrawSelection,
                Facet::RectangularSelection,
                Facet::ClickAddsSelectionRange,
            ]
        );
    }

    #[test]
    fn the_mod_d_guard_matches_every_spelling() {
        // Cmd+D on macOS.
        assert!(mod_d_direct_handler(true, false, false, "d", "KeyD"));
        // Ctrl+D on Windows/Linux.
        assert!(mod_d_direct_handler(false, true, false, "d", "KeyD"));
        // The shifted spelling.
        assert!(mod_d_direct_handler(true, false, false, "D", "KeyD"));
        // A non-US layout: the physical code carries the chord.
        assert!(mod_d_direct_handler(true, false, false, "đ", "KeyD"));
        // Alt is the rectangular chord, never Mod-D.
        assert!(!mod_d_direct_handler(true, false, true, "d", "KeyD"));
        // No modifier: an ordinary 'd'.
        assert!(!mod_d_direct_handler(false, false, false, "d", "KeyD"));
        // Another key entirely.
        assert!(!mod_d_direct_handler(true, false, false, "f", "KeyF"));
    }

    #[test]
    fn the_rectangular_filter_follows_the_table() {
        // Alt+drag, primary button: rectangular.
        assert!(rectangular_drag_predicate(true, false, false, 0, 1));
        // Shift+Alt+drag and Cmd+Alt+drag (the Trackpad chord).
        assert!(rectangular_drag_predicate(true, true, false, 0, 1));
        assert!(rectangular_drag_predicate(true, false, true, 0, 1));
        // No Alt: never rectangular.
        assert!(!rectangular_drag_predicate(false, false, false, 0, 1));
        assert!(!rectangular_drag_predicate(false, true, true, 0, 1));
        // Alt but a secondary button: not rectangular (that is the
        // context-menu gesture).
        assert!(!rectangular_drag_predicate(true, false, false, 2, 2));
        // `buttons` carries the primary bit during a drag.
        assert!(rectangular_drag_predicate(true, false, false, -1, 1));
    }

    #[test]
    fn shift_alt_arrow_wins_over_the_default_keymap() {
        // The regression the Prec.high trio fixes: defaultKeymap owns
        // `Shift-Alt-ArrowUp/Down` for copyLineUp/Down on EVERY
        // platform (no mac variant) and is registered first, so the
        // chord is dead at default precedence. The inventory answers
        // with the high-tier pair above it — selection-only commands,
        // the document untouched.
        let bindings = multiple_selections_bindings();
        let high_keys: Vec<&str> = bindings
            .iter()
            .filter(|b| b.precedence == BindingPrecedence::High)
            .map(|b| b.key)
            .collect();
        assert!(high_keys.contains(&"Shift-Alt-ArrowUp"));
        assert!(high_keys.contains(&"Shift-Alt-ArrowDown"));
        // And the copy-line moves live on at default precedence for
        // the platforms where the chord is free.
        assert!(bindings.iter().any(|b| b.key == "Mod-Shift-ArrowUp"
            && b.command == MultiCursorCommand::CopyLineUp
            && b.precedence == BindingPrecedence::Default));
    }
}
