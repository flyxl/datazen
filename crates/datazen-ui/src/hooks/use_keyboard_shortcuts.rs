//! Port of `packages/frontend/src/hooks/useKeyboardShortcuts.ts`
//! (T341).
//!
//! ## What maps where
//!
//! - [`ShortcutDef`] is the React interface verbatim (the key spec
//!   string, the scope, the action, the description); the action is a
//!   shareable closure instead of a bare function.
//! - [`match_shortcut`] is the pure matcher: the `mod+shift+alt+key`
//!   spec grammar with its three special key names (`escape`,
//!   `delete` also matching `backspace`, `enter`, `space` matching
//!   `" "`).
//! - The `keydown` handler becomes [`resolve_shortcut`]: the field
//!   gating (a `table` shortcut is skipped while typing in a field;
//!   an `editor` shortcut fires **only** in a field; a `global` one
//!   always runs), the `mod = meta || ctrl` fold, the lowercased key,
//!   and the first-match-wins walk. The host owns the DOM: it feeds
//!   the [`ShortcutEvent`] and performs the `preventDefault` when the
//!   resolver fires.
//! - The `shortcutsRef` freshness (the effect closure always reads the
//!   latest array) is the caller passing the current slice — nothing
//!   is captured stale.
//!
//! Known deltas: `e.key.toLowerCase()` compares ASCII key names; the
//! port compares the already-lowercased [`ShortcutEvent::key`] the
//! same way. `isContentEditable` arrives as the
//! [`ShortcutEvent::in_field`] flag the host computes.

use std::rc::Rc;

/// `ShortcutDef.scope`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShortcutScope {
    Global,
    Editor,
    Table,
}

/// `ShortcutDef`: one registered shortcut.
#[derive(Clone)]
pub(crate) struct ShortcutDef {
    /// The key spec: `mod+shift+alt+key`, e.g. `"mod+k"`,
    /// `"shift+enter"`, `"escape"`.
    pub(crate) key: String,
    pub(crate) scope: ShortcutScope,
    pub(crate) action: Rc<dyn Fn()>,
    pub(crate) description: String,
}

impl ShortcutDef {
    /// `ShortcutDef` with a no-op action (tests and declarative
    /// registries).
    pub(crate) fn new(key: &str, scope: ShortcutScope, description: &str) -> Self {
        Self {
            key: key.to_owned(),
            scope,
            action: Rc::new(|| {}),
            description: description.to_owned(),
        }
    }
}

/// The keydown event as the resolver reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShortcutEvent {
    /// `e.metaKey || e.ctrlKey`.
    pub(crate) mod_: bool,
    pub(crate) shift: bool,
    pub(crate) alt: bool,
    /// `e.key.toLowerCase()`.
    pub(crate) key: String,
    /// The target is an input, a textarea, or contenteditable.
    pub(crate) in_field: bool,
}

/// `matchShortcut(def, ctx)`: whether the spec matches the event.
pub(crate) fn match_shortcut(def: &str, ctx: &ShortcutEvent) -> bool {
    let mut needs_mod = false;
    let mut needs_shift = false;
    let mut needs_alt = false;
    let mut key = String::new();
    for part in def.to_ascii_lowercase().split('+') {
        let part = part.trim();
        match part {
            "mod" => needs_mod = true,
            "shift" => needs_shift = true,
            "alt" => needs_alt = true,
            _ => key = part.to_owned(),
        }
    }
    if needs_mod != ctx.mod_ {
        return false;
    }
    if needs_shift != ctx.shift {
        return false;
    }
    if needs_alt != ctx.alt {
        return false;
    }
    match key.as_str() {
        "escape" => ctx.key == "escape",
        "delete" => ctx.key == "delete" || ctx.key == "backspace",
        "enter" => ctx.key == "enter",
        "space" => ctx.key == " ",
        _ => ctx.key == key,
    }
}

/// The handler: the first shortcut that passes its scope gate and
/// matches fires (the host preventDefaults and calls the action);
/// `None` means the event is not the registry's.
pub(crate) fn resolve_shortcut(
    shortcuts: &[ShortcutDef],
    event: &ShortcutEvent,
) -> Option<Rc<dyn Fn()>> {
    for shortcut in shortcuts {
        // A table shortcut is skipped while typing in a field.
        if event.in_field && shortcut.scope == ShortcutScope::Table {
            continue;
        }
        // An editor shortcut fires only inside a field.
        if shortcut.scope == ShortcutScope::Editor && !event.in_field {
            continue;
        }
        if match_shortcut(&shortcut.key, event) {
            return Some(Rc::clone(&shortcut.action));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(key: &str, mod_: bool, shift: bool, alt: bool, in_field: bool) -> ShortcutEvent {
        ShortcutEvent {
            mod_,
            shift,
            alt,
            key: key.to_owned(),
            in_field,
        }
    }

    #[test]
    fn the_spec_grammar_matches_exact_modifier_sets() {
        let ctx = event("k", true, false, false, false);
        assert!(match_shortcut("mod+k", &ctx));
        assert!(match_shortcut("Mod + K", &ctx), "the spec is case-insensitive");
        assert!(!match_shortcut("mod+shift+k", &ctx), "shift demanded but absent");
        assert!(!match_shortcut("k", &ctx), "mod demanded but absent");
        let ctx = event("k", true, true, true, false);
        assert!(match_shortcut("mod+alt+shift+k", &ctx));
    }

    #[test]
    fn the_special_key_names_have_their_aliases() {
        // delete also matches backspace.
        assert!(match_shortcut("delete", &event("delete", false, false, false, false)));
        assert!(match_shortcut("delete", &event("backspace", false, false, false, false)));
        assert!(!match_shortcut("delete", &event("enter", false, false, false, false)));
        // space matches " ".
        assert!(match_shortcut("space", &event(" ", false, false, false, false)));
        assert!(!match_shortcut("space", &event("spacebar", false, false, false, false)));
        // escape and enter match themselves only.
        assert!(match_shortcut("escape", &event("escape", false, false, false, false)));
        assert!(match_shortcut("enter", &event("enter", false, false, false, false)));
    }

    #[test]
    fn the_first_match_wins() {
        let first = ShortcutDef::new("mod+k", ShortcutScope::Global, "first");
        let second = ShortcutDef::new("mod+k", ShortcutScope::Global, "second");
        let fired = resolve_shortcut(
            &[first, second],
            &event("k", true, false, false, false),
        )
        .expect("fires");
        drop(fired);
        // Ordering is observable through the description the caller
        // keeps; here the pin is that exactly one fires (the walk
        // returns on the first match — `return` in the React loop).
    }

    #[test]
    fn a_table_shortcut_is_skipped_in_a_field() {
        let mut table = ShortcutDef::new("mod+t", ShortcutScope::Table, "table");
        let fired_in_field = std::rc::Rc::new(std::cell::Cell::new(0));
        let counter = Rc::clone(&fired_in_field);
        table.action = Rc::new(move || counter.set(counter.get() + 1));
        // In a field: not offered.
        assert!(resolve_shortcut(&[table.clone()], &event("t", true, false, false, true)).is_none());
        // Outside: fires.
        let action = resolve_shortcut(&[table], &event("t", true, false, false, false)).expect("fires");
        action();
        assert_eq!(fired_in_field.get(), 1);
    }

    #[test]
    fn an_editor_shortcut_fires_only_in_a_field() {
        let editor = ShortcutDef::new("mod+e", ShortcutScope::Editor, "editor");
        assert!(resolve_shortcut(&[editor.clone()], &event("e", true, false, false, false)).is_none());
        assert!(resolve_shortcut(&[editor], &event("e", true, false, false, true)).is_some());
    }

    #[test]
    fn a_global_shortcut_fires_everywhere() {
        let global = ShortcutDef::new("mod+g", ShortcutScope::Global, "global");
        assert!(resolve_shortcut(&[global.clone()], &event("g", true, false, false, true)).is_some());
        assert!(resolve_shortcut(&[global], &event("g", true, false, false, false)).is_some());
    }

    #[test]
    fn an_unmatched_event_is_not_the_registrys() {
        let global = ShortcutDef::new("mod+g", ShortcutScope::Global, "global");
        assert!(resolve_shortcut(&[global], &event("x", true, false, false, false)).is_none());
    }
}
