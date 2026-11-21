//! Host-injected actions for the navigator tree row: every click, context
//! request and drag affordance is a port call, never an IPC or store write.

use std::rc::Rc;

use super::aria::ContextMenuTarget;

/// Every intent a row can emit. Grouped in one enum so tests can observe the
/// full journey without faking a host store.
#[allow(dead_code)] // host port; consumer: the navigator host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RowAction {
    ToggleGroup(String),
    SelectConnection(String),
    OpenConnection(String),
    ToggleConnection { connection_id: String, section_group: String },
    ToggleDatabase { connection_id: String, db_session_id: String, db_name: String },
    ToggleSchema(String),
    ToggleCategory { key: String, cat_id: String, db_session_id: String, db_name: String },
    ToggleNamespace { key: String, db_session_id: String, segments: Vec<String> },
    SelectTable { name: String, schema: Option<String>, database: String },
    SelectKvDb { connection_id: String, db_name: String },
    NewConnection,
    ContextMenu(ContextMenuTarget),
    DragStart(String),
}

#[allow(dead_code)] // host port; consumer: the navigator host (not wired).
pub(crate) struct TreeRowHostPort<S> {
    pub on_action: Option<Rc<dyn Fn(&mut S, RowAction) + 'static>>,
}

impl<S> Default for TreeRowHostPort<S> {
    fn default() -> Self {
        Self { on_action: None }
    }
}

impl<S: 'static> TreeRowHostPort<S> {
    pub(crate) fn emit(&self, state: &mut S, action: RowAction) {
        if let Some(on_action) = &self.on_action {
            on_action(state, action);
        }
    }
}
