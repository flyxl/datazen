use std::collections::BTreeMap;

use super::types::{ActiveConnectionEntry, ContentViewConnectionContext, SavedConnection};

fn build_context(
    connection_id: &str,
    db_session_id: &str,
    saved: &SavedConnection,
) -> ContentViewConnectionContext {
    ContentViewConnectionContext {
        connection_id: connection_id.to_owned(),
        db_session_id: db_session_id.to_owned(),
        connection_name: saved.name.clone(),
        database_type: saved.database_type.clone(),
    }
}

pub fn resolve_connection_context(
    db_session_id: &str,
    active_connections: &BTreeMap<String, ActiveConnectionEntry>,
    saved_connections: &[SavedConnection],
) -> Option<ContentViewConnectionContext> {
    let entry = active_connections
        .values()
        .find(|entry| entry.db_session_id == db_session_id)?;
    if entry.db_session_id.is_empty() {
        return None;
    }
    let saved = saved_connections.iter().find(|saved| saved.id == entry.connection_id)?;
    Some(build_context(&entry.connection_id, &entry.db_session_id, saved))
}

pub fn resolve_connection_context_by_connection(
    connection_id: &str,
    active_connections: &BTreeMap<String, ActiveConnectionEntry>,
    saved_connections: &[SavedConnection],
) -> Option<ContentViewConnectionContext> {
    let entry = active_connections.get(connection_id)?;
    if entry.db_session_id.is_empty() {
        return None;
    }
    let saved = saved_connections.iter().find(|saved| saved.id == connection_id)?;
    Some(build_context(connection_id, &entry.db_session_id, saved))
}
