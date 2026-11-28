//! The read/open subset of the original object search on the connected Native catalog.
use std::{collections::HashMap, rc::Rc};

use datazen_ui_api::{Catalog, ConnectionProfile, TabKind};
use rune::{El, TextEdit};

use super::global_object_search::{self, GlobalObjectSearchVm, ObjectSearchAction,
    ObjectSearchResult, ObjectType, RelationInfo, SchemaIndexEntry};
use crate::{AppState, Section};

pub(crate) const OPEN_KEY: &str = "navigator-native-object-search";

#[derive(Clone, Debug, Eq, PartialEq)]
struct Scope {
    connection: String,
    // Set by the successful OpenConnection delivery for every live catalog.
    generation: u32,
}

impl Scope {
    // UI-only identity for the reusable view; it is never sent to a driver.
    fn view_identity(&self) -> String { format!("native:{}", self.generation) }
}

#[derive(Default)]
pub(crate) struct SearchState {
    scope: Option<Scope>,
    epoch: u64,
    query: TextEdit,
    index: Vec<SchemaIndexEntry>,
}

fn representable(value: &str) -> bool {
    !value.is_empty() && value.trim() == value
}

fn catalog_index(scope: &Scope, catalog: &Catalog, profile: Option<&ConnectionProfile>) -> Vec<SchemaIndexEntry> {
    let mut counts = HashMap::<&str, usize>::new();
    // Count *all* raw occurrences before filtering or applying the display
    // bound, including table/view coincidences and unseen schemas.
    for schema in catalog.databases.iter().flat_map(|db| &db.schemas) {
        for name in schema.tables.iter().chain(&schema.views) {
            *counts.entry(name.as_str()).or_default() += 1;
        }
    }
    let mut out = Vec::new();
    for database in &catalog.databases {
        for schema in &database.schemas {
            if !representable(&database.name) || !representable(&schema.name) {
                continue;
            }
            let relations = schema.tables.iter().map(|name| (name, "table"))
                .chain(schema.views.iter().map(|name| (name, "view")))
                .filter(|(name, _)| representable(name) && counts.get(name.as_str()) == Some(&1))
                .map(|(name, kind)| RelationInfo { name: name.clone(), schema: None, table_type: kind.into() })
                .collect::<Vec<_>>();
            if !relations.is_empty() {
                out.push(SchemaIndexEntry {
                    connection_id: scope.connection.clone(),
                    db_session_id: scope.view_identity(),
                    database_type: profile.map(|p| p.engine.clone()).unwrap_or_default(),
                    connection_name: profile.map(|p| p.name.clone()),
                    database: Some(database.name.clone()),
                    schema: Some(schema.name.clone()),
                    relations,
                    ..Default::default()
                });
            }
        }
    }
    out
}

fn unique_open_target(catalog: &Catalog, result: &ObjectSearchResult) -> bool {
    let mut matches = catalog.databases.iter().flat_map(|database| {
        database.schemas.iter().flat_map(move |schema| {
            schema.tables.iter().map(|name| (name, ObjectType::Table))
                .chain(schema.views.iter().map(|name| (name, ObjectType::View)))
                .filter(move |(name, _)| *name == &result.name)
                .map(move |(_, kind)| (database.name.as_str(), schema.name.as_str(), kind))
        })
    });
    matches.next().is_some_and(|(database, schema, kind)| {
        result.database.as_deref() == Some(database)
            && result.schema.as_deref() == Some(schema)
            && result.table_name.as_deref() == Some(result.name.as_str())
            && kind == result.object_type
    }) && matches.next().is_none()
}

impl AppState {
    fn native_object_search_scope(&self) -> Option<Scope> {
        if !matches!(self.active, Section::Connections | Section::Workspace)
            || self.connect_loading.is_some()
            || self.desktop_catalog.is_none()
            || self.native_kv.is_some()
            || (self.tabs.selected().is_some_and(|tab| tab.kind == TabKind::Table)
                && (self.grid.editing || self.grid.dirty_cells() != 0))
        {
            return None;
        }
        Some(Scope {
            connection: self.desktop_connection.clone()?,
            generation: self.navigator_value_search_session_generation?,
        })
    }

    pub(crate) fn native_object_search_is_available(&self) -> bool {
        self.native_object_search_scope().is_some()
    }

    pub(crate) fn native_object_search_is_current(&self) -> bool {
        self.native_object_search.scope.as_ref().is_some_and(|scope| {
            self.native_object_search_scope().as_ref() == Some(scope)
        })
    }

    pub(crate) fn open_native_object_search(&mut self) {
        let Some(scope) = self.native_object_search_scope() else { return };
        let Some(catalog) = self.desktop_catalog.as_ref() else { return };
        let profile = self.desktop_connections.as_ref().and_then(|rows| rows.iter()
            .find(|profile| profile.id == scope.connection));
        let index = catalog_index(&scope, catalog, profile);
        self.close_navigator_value_search();
        self.native_object_search.epoch = self.native_object_search.epoch.wrapping_add(1);
        self.native_object_search.scope = Some(scope);
        self.native_object_search.query = TextEdit::default();
        self.native_object_search.index = index;
    }

    pub(crate) fn close_native_object_search(&mut self) {
        if self.native_object_search.scope.take().is_some() {
            self.native_object_search.query = TextEdit::default();
            self.native_object_search.index.clear();
        }
    }

    fn native_object_search_action(&mut self, epoch: u64, action: ObjectSearchAction) {
        if epoch != self.native_object_search.epoch || !self.native_object_search_is_current() {
            return;
        }
        match action {
            ObjectSearchAction::Close => self.close_native_object_search(),
            ObjectSearchAction::ClearQuery => self.native_object_search.query = TextEdit::default(),
            ObjectSearchAction::OpenResult(result) => {
                let Some(scope) = self.native_object_search.scope.as_ref() else { return };
                if result.connection_id != scope.connection
                    || result.db_session_id != scope.view_identity()
                    || !self.desktop_catalog.as_ref().is_some_and(|catalog| unique_open_target(catalog, &result))
                {
                    return;
                }
                // Exactly the already-supported tree click, including the
                // host's independent bare-name namespace checks.
                if self.open_table_tab(&result.name).is_some() {
                    self.close_native_object_search();
                }
            }
            // No SQL drafts, writes, or fabricated DDL affordances here.
            ObjectSearchAction::OpenTableAction(_, _) => {}
        }
    }
}

pub(crate) fn mount(state: &AppState, dark: bool, layer: &mut Vec<El>) {
    if !state.native_object_search_is_current() { return; }
    let search = &state.native_object_search;
    let vm = GlobalObjectSearchVm { open: true, query: search.query.clone(), index: search.index.clone() };
    let epoch = search.epoch;
    let edit: Rc<dyn for<'a> Fn(&'a mut AppState) -> &'a mut TextEdit> =
        Rc::new(|state| &mut state.native_object_search.query);
    let action = Rc::new(move |state: &mut AppState, action| state.native_object_search_action(epoch, action));
    global_object_search::build_native(&vm, edit, action, dark, layer, epoch);
}

#[cfg(test)]
#[path = "native_object_search_tests.rs"]
mod tests;
