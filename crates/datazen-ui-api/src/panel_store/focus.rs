use std::collections::{BTreeMap, BTreeSet};

use super::types::{ExecTable, StorePanel, StorePanelType};

pub const DEFAULT_PANE_ID: &str = "main";
pub const PANE_SEPARATOR: &str = "::";

#[derive(Clone, Debug, PartialEq)]
pub struct FocusPatch {
    pub focused_pane_id_by_panel: BTreeMap<String, String>,
    pub focused_pane_id: Option<String>,
    pub active_panel_id: Option<String>,
}

pub fn pane_key(panel_id: &str, pane_id: Option<&str>) -> String {
    match pane_id {
        None | Some(DEFAULT_PANE_ID) => panel_id.to_owned(),
        Some(pane_id) => format!("{panel_id}{PANE_SEPARATOR}{pane_id}"),
    }
}

pub fn panel_id_of_pane_key(key: &str) -> &str {
    match key.find(PANE_SEPARATOR) {
        None => key,
        Some(at) => &key[..at],
    }
}

pub fn pane_id_of_pane_key(key: &str) -> &str {
    match key.find(PANE_SEPARATOR) {
        None => DEFAULT_PANE_ID,
        Some(at) => &key[at + PANE_SEPARATOR.len()..],
    }
}

pub fn is_pane_key_of_panel(key: &str, panel_id: &str) -> bool {
    panel_id_of_pane_key(key) == panel_id
}

pub fn pane_keys_of_panel(keys: &[String], panel_id: &str) -> Vec<String> {
    keys.iter()
        .filter(|key| is_pane_key_of_panel(key, panel_id))
        .cloned()
        .collect()
}

pub fn pane_args(pane_id: Option<&str>) -> Vec<&str> {
    match pane_id {
        Some(pane_id) if pane_id != DEFAULT_PANE_ID => vec![pane_id],
        _ => Vec::new(),
    }
}

pub fn resolve_focused_pane_id(focused_pane_id: Option<&str>) -> &str {
    focused_pane_id.unwrap_or(DEFAULT_PANE_ID)
}

pub fn resolve_next_active(
    panels: &[StorePanel],
    removed_id: &str,
    current_active_id: Option<&str>,
) -> Option<String> {
    if current_active_id != Some(removed_id) {
        return current_active_id.map(str::to_owned);
    }
    let index = panels.iter().position(|panel| panel.id == removed_id)?;
    let remaining = panels
        .iter()
        .filter(|panel| panel.id != removed_id)
        .collect::<Vec<_>>();
    if remaining.is_empty() {
        return None;
    }
    let next = remaining[index.min(remaining.len() - 1)];
    Some(next.id.clone())
}

pub fn sync_pane_focus(
    previous: &BTreeMap<String, String>,
    panels: &[StorePanel],
    active_panel_id: Option<&str>,
    focus: Option<(&str, Option<&str>)>,
) -> FocusPatch {
    let live = panels
        .iter()
        .map(|panel| panel.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut next = BTreeMap::new();
    for (panel_id, pane_id) in previous {
        if live.contains(panel_id.as_str()) && focus.map(|(id, _)| id) != Some(panel_id.as_str()) {
            next.insert(panel_id.clone(), pane_id.clone());
        }
    }
    if let Some((panel_id, Some(pane_id))) = focus {
        next.insert(panel_id.to_owned(), pane_id.to_owned());
    }
    let focused = active_panel_id.and_then(|id| next.get(id).cloned());
    FocusPatch {
        focused_pane_id_by_panel: next,
        focused_pane_id: focused,
        active_panel_id: active_panel_id.map(str::to_owned),
    }
}

fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|text| !text.is_empty())
}

pub fn matches_relation(
    panel: &StorePanel,
    connection_id: &str,
    table: &str,
    database: Option<&str>,
) -> bool {
    if panel.connection_id != connection_id {
        return false;
    }
    let target_db = trimmed(database);
    match panel.panel_type {
        StorePanelType::Table => {
            if panel.table_name.as_deref() != Some(table) {
                return false;
            }
            let panel_db = trimmed(panel.database.as_deref());
            target_db.is_none() || panel_db.is_none() || panel_db == target_db
        }
        StorePanelType::View => {
            if panel.view_name.as_deref() != Some(table) {
                return false;
            }
            let panel_db = trimmed(panel.database.as_deref());
            target_db.is_none() || panel_db.is_none() || panel_db == target_db
        }
        _ => false,
    }
}

pub fn matches_database(
    panel: &StorePanel,
    connection_id: &str,
    database: &str,
    session_database: Option<&str>,
) -> bool {
    if panel.connection_id != connection_id {
        return false;
    }
    let target = database.trim();
    if target.is_empty() {
        return false;
    }
    let session = trimmed(session_database);
    match panel.panel_type {
        StorePanelType::Table | StorePanelType::View => match trimmed(panel.database.as_deref()) {
            Some(panel_db) => panel_db == target,
            None => session == Some(target),
        },
        StorePanelType::Query | StorePanelType::CreateTable => {
            trimmed(panel.database.as_deref()) == Some(target)
        }
        StorePanelType::RedisDb => panel.db_name.as_deref() == Some(target),
        _ => false,
    }
}

pub fn exec_keys_of_pane(exec: &ExecTable, panel_id: &str) -> Vec<String> {
    exec.keys()
        .filter(|key| is_pane_key_of_panel(key, panel_id))
        .cloned()
        .collect()
}
