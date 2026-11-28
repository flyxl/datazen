//! Live Query actions shared by key dispatch and the footer's hints.

use datazen_ui_api::{KeymapAction, TabKind};
use rune::{Chord, ChordKey, ChordMod, KeyEvent, KeyPlatform, Tx};

use crate::{native_query_format::parse_format_chord, state_def::AppState, types::Section};

const NEW_QUERY_BINDING: &str = "Mod-n";
const NEW_QUERY: Chord = Chord::new(ChordKey::Char('n')).with(ChordMod::Primary);

impl AppState {
    pub(crate) fn has_query_shortcut_context(&self) -> bool {
        self.active == Section::Workspace && self.session_open() && self.native_kv.is_none()
    }

    /// Wire values the live shell can actually route. Disconnected Home
    /// intentionally cannot advertise a New Query shortcut.
    pub(crate) fn native_home_query_binding(&self, action: KeymapAction) -> Option<String> {
        match action {
            KeymapAction::NewQuery if self.has_query_shortcut_context() => Some(NEW_QUERY_BINDING.into()),
            KeymapAction::FormatSql => self.desktop.native_format_shortcut().filter(|value| {
                parse_format_chord(value).is_some_and(|chord| {
                    !self.has_query_shortcut_context() || !chord.same_keys(&NEW_QUERY, KeyPlatform::current())
                })
            }),
            _ => None,
        }
    }

    fn new_query_in_current_database(&mut self) -> bool {
        // Drivers with a single fixed database already route through the
        // current session. Where the live host has a database picker, pin the
        // current panel's database (or session default) at creation, just as
        // React handleNewQuery does. Don't copy a previous profile's scope.
        let mut target = None;
        if let Some(context) = self.desktop.native_query_database_context() {
            if self.desktop_connection.as_deref() != Some(context.current.connection_id.as_str()) {
                return false;
            }
            let mut current = context.current;
            if let Some(tab) = self.tabs.selected() {
                if tab.kind == TabKind::Query {
                    if let Some(active) = self.query_database_targets.get(&tab.id)
                        .filter(|active| active.connection_id == current.connection_id)
                    {
                        current.database.clone_from(&active.database);
                    }
                } else if let Some(database) = self.unique_table_database(&tab.title) {
                    current.database = database.to_owned();
                }
            }
            if !context.choices.contains(&current.database) {
                return false;
            }
            target = Some(current);
        }
        let Some(tab) = self.open_query_tab("Query") else { return false };
        if let Some(target) = target {
            self.query_database_targets.insert(tab, target);
        }
        true
    }

    fn unique_table_database(&self, table: &str) -> Option<&str> {
        // The current Native table tab only carries a bare name. Use a unique
        // catalog match, never guess the database for an ambiguous name.
        let mut databases = self.desktop_catalog.as_ref()?.databases.iter().filter(|db| {
            db.schemas.iter().any(|schema| schema.tables.iter().chain(&schema.views).any(|name| name == table))
        });
        let database = databases.next()?;
        databases.next().is_none().then_some(database.name.as_str())
    }
}

pub(super) fn new_query(state: &mut AppState, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if !state.has_query_shortcut_context() || !NEW_QUERY.matches_tx(ev, tx) {
        return false; // Linux/Windows retain their existing New Connection fallback.
    }
    // In the SQL section, a temporarily unavailable Query command must not
    // accidentally open the File menu's different New Connection action.
    if !tx.key_repeat()
        && state.connect_loading.is_none()
        && !state.connection_dialog_open
        && !state.import_dialog_open
        && state.running_close_request.is_none()
        && state.query_database_picker_tab.is_none()
        && !state.native_object_search_is_current()
        && !state.navigator_value_search_is_current()
        && state.active_cell_preview().is_none()
        && state.sql.composition().is_none()
        && !crate::sql_editor::goto_line::holds_focus(state, tx)
        && state.new_query_in_current_database()
    {
        state.titlebar_menu.open_menu = None;
        state.theme_toggle_open = false;
        state.sql.focus();
        tx.focus("sql-editor-scroll", tx.by_keys());
    }
    true
}
