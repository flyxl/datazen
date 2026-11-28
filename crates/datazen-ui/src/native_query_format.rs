//! Mounted Query Format: submit only the owned draft text to a local provider,
//! and deliver one editor edit only to that exact still-current draft.
use std::time::Duration;

use datazen_ui_api::{
    native_sql_format::{NativeSqlFormatContext, NativeSqlFormatError, NativeSqlFormatRequest},
    NativeQueryDatabase, TabKind,
};
use rune::{Chord, KeyEvent, Tx};

use crate::{
    query_editor_tabs::QueryEditorViewport,
    sql_editor::format_draft::{FormatDraftSnapshot, FormatSourceError},
    state_def::AppState,
};

const EDITOR: &str = "sql-editor-scroll";
const POLL_EVERY: Duration = Duration::from_millis(25);
const UI_DEADLINE_SECONDS: f64 = 3.0;

#[derive(Default)]
pub(crate) struct NativeQueryFormat {
    next_generation: u64,
    pending: Option<Pending>,
}

struct Pending {
    generation: u64,
    owner: QueryEditorViewport,
    target: NativeQueryDatabase,
    context: NativeSqlFormatContext,
    draft: FormatDraftSnapshot,
    query_generation: u32,
    deadline: f64,
}

impl AppState {
    fn native_format_target(&self, context: &NativeSqlFormatContext) -> Option<NativeQueryDatabase> {
        let id = self.snippet_edit_owner()?;
        if self.connect_loading.is_some()
            || self.sql.goto_lease.is_some()
            || self.query_snippet_menu_owner().is_some()
            || !self.query_editor_matches_connection()
            || self.desktop_connection.as_deref() != Some(context.default_target.connection_id.as_str())
            || self.navigator_value_search_session_generation != Some(context.session_generation)
        {
            return None;
        }
        if let Some(explicit) = self.query_database_targets.get(&id) {
            let picker = self.desktop.native_query_database_context()?;
            if explicit.connection_id != context.default_target.connection_id
                || picker.current.connection_id != explicit.connection_id
                || !picker.choices.contains(&explicit.database)
            {
                return None; // a stale target must never fall back
            }
            return Some(explicit.clone());
        }
        Some(context.default_target.clone())
    }

    /// The real toolbar retains its old honest unsupported response with no
    /// local formatter, rather than claiming any engine in shared/demo builds.
    pub(crate) fn format_query(&mut self, tx: &mut Tx) {
        let Some(context) = self.desktop.native_sql_format_context() else {
            self.query_action_notice("Format is not available in this host");
            return;
        };
        let _ = self.begin_query_format(context, tx, true);
    }

    pub(crate) fn format_query_shortcut(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        if !self.owns_query_document_shortcut(tx) {
            return false;
        }
        let Some(context) = self.desktop.native_sql_format_context() else { return false };
        let Some(chord) = context.shortcut.as_deref().and_then(parse_format_chord) else { return false };
        if !chord.matches_tx(ev, tx) {
            return false;
        }
        if tx.key_repeat() {
            return true;
        }
        self.begin_query_format(context, tx, false)
    }

    fn begin_query_format(&mut self, context: NativeSqlFormatContext, tx: &mut Tx, toolbar: bool) -> bool {
        let Some(target) = self.native_format_target(&context) else { return false };
        let Some(owner) = self.query_editor_viewport().filter(|o| o.owns_current_query(self)) else {
            return false;
        };
        if toolbar {
            // React's imperative Format action returns focus to its editor.
            // Do this at invocation, never from a later worker completion.
            self.result_focused = false;
            self.sql.focus();
            tx.focus(EDITOR, tx.by_keys());
        }

        let (draft, sql) = match self.sql.capture_native_format() {
            Ok(captured) => captured,
            Err(FormatSourceError::Limit) => {
                self.cancel_native_query_format();
                self.query_action_notice("This SQL is too large to format.");
                return true;
            }
            Err(FormatSourceError::BlankOrInvalid) => return false,
        };
        self.cancel_native_query_format();
        let Some(generation) = self.native_query_format.next_generation.checked_add(1) else {
            return false;
        };
        self.native_query_format.next_generation = generation;
        let request = NativeSqlFormatRequest {
            generation,
            session_generation: context.session_generation,
            target: target.clone(),
            sql,
            options: context.engine_options(),
        };
        if !self.desktop.begin_native_sql_format(request) {
            self.query_action_notice("The local SQL formatter could not start.");
            return true;
        }
        self.query_notice = None;
        self.native_query_format.pending = Some(Pending {
            generation,
            owner,
            target,
            context,
            draft,
            query_generation: self.query_generation,
            deadline: tx.clock() + UI_DEADLINE_SECONDS,
        });
        tx.wake_after(POLL_EVERY);
        true
    }

    pub(crate) fn cancel_native_query_format(&mut self) {
        if let Some(pending) = self.native_query_format.pending.take() {
            self.desktop.cancel_native_sql_format(pending.generation);
        }
    }

    fn format_is_current(&self, pending: &Pending, tx: &Tx) -> bool {
        self.owns_query_document_shortcut(tx)
            && self.tabs.selected().is_some_and(|tab| tab.kind == TabKind::Query)
            && pending.owner.owns_current_query(self)
            && self.query_generation == pending.query_generation
            && self.sql.owns_native_format(&pending.draft)
            && self.native_format_target(&pending.context).as_ref() == Some(&pending.target)
            && self.desktop.native_sql_format_context().as_ref() == Some(&pending.context)
    }

    pub(crate) fn poll_native_query_format(&mut self, now: f64, tx: &mut Tx) -> bool {
        if let Some(pending) = self.native_query_format.pending.as_ref() {
            if !self.format_is_current(pending, tx) {
                self.cancel_native_query_format();
            } else if now >= pending.deadline {
                self.cancel_native_query_format();
                self.query_action_notice("SQL formatting timed out.");
                return true;
            }
        }
        // Drain a small bounded burst, including late answers made obsolete
        // by cancellation, without blocking the frame or consuming another
        // feature's deliveries.
        for _ in 0..8 {
            let Some(delivery) = self.desktop.poll_native_sql_format() else { break };
            if !self.native_query_format.pending.as_ref()
                .is_some_and(|pending| pending.generation == delivery.generation)
            {
                continue;
            }
            let Some(pending) = self.native_query_format.pending.take() else { continue };
            if !self.format_is_current(&pending, tx) {
                return false;
            }
            match delivery.result {
                Ok(formatted) => {
                    if self.sql.apply_native_format(&pending.draft, formatted) {
                        tx.scroll_into_view_in(pending.owner.anchor, "sql-caret");
                        return true;
                    }
                    return false;
                }
                // CodeMirror's parse refusal is deliberately silent and
                // cannot change a draft (or clear its redo).
                Err(NativeSqlFormatError::Rejected | NativeSqlFormatError::Cancelled) => return false,
                Err(error) => {
                    self.query_action_notice(match error {
                        NativeSqlFormatError::Limit => "This SQL is too large to format.",
                        NativeSqlFormatError::Timeout => "SQL formatting timed out.",
                        _ => "The local SQL formatter is unavailable.",
                    });
                    return true;
                }
            }
        }
        if self.native_query_format.pending.is_some() {
            tx.wake_after(POLL_EVERY);
        }
        false
    }
}

fn parse_format_chord(input: &str) -> Option<Chord> {
    // CodeMirror names modifiers with minus. splitn with a terminal key
    // preserves "Mod--" (minus) instead of turning it into a different key.
    let mut rest = input.trim();
    let mut parts = Vec::new();
    while let Some((modifier, tail)) = rest.split_once('-') {
        if !["mod", "ctrl", "alt", "shift", "meta", "cmd"].iter()
            .any(|known| modifier.eq_ignore_ascii_case(known))
        {
            break;
        }
        parts.push(modifier);
        rest = tail;
    }
    if rest.is_empty() {
        return None;
    }
    if rest == "+" {
        // Rune parses a trailing literal plus as two separators.
        if parts.is_empty() { return Chord::parse("+").ok(); }
        return Chord::parse(&format!("{}++", parts.join("+"))).ok();
    }
    parts.push(rest);
    Chord::parse(&parts.join("+")).ok()
}
