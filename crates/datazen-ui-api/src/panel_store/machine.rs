use std::collections::BTreeSet;

use super::focus::{
    is_pane_key_of_panel, pane_key, pane_keys_of_panel, resolve_next_active, sync_pane_focus,
    FocusPatch,
};
use super::host::PanelStoreHost;
use super::types::{ExecTable, PanelExecState, PanelState, StorePanel, StorePanelType};
use crate::{
    cancel_and_cleanup_exec, cancel_and_cleanup_pane_exec, ExecMap, PanelExecDeps, PanelType,
    QueryExecState, SessionController,
};

struct StoreDeps<'a> {
    host: &'a dyn PanelStoreHost,
}

struct EagerController {
    accepted: bool,
}

impl SessionController for EagerController {
    fn cancel(&self) -> Result<(), String> {
        if self.accepted {
            Ok(())
        } else {
            Err("cancel refused".to_owned())
        }
    }
}

impl PanelExecDeps for StoreDeps<'_> {
    fn can_cancel_pane(&self, panel: &crate::Panel) -> bool {
        self.host.can_cancel(&panel.id)
    }

    fn pane_key(&self, panel_id: &str, pane_id: &str) -> String {
        pane_key(panel_id, Some(pane_id))
    }

    fn pane_keys_of_panel(&self, exec_keys: &[String], panel_id: &str) -> Vec<String> {
        pane_keys_of_panel(exec_keys, panel_id)
    }

    fn registry_keys_of_panel(&self, panel_id: &str) -> Vec<String> {
        self.host.session_keys_of_panel(panel_id)
    }

    fn peek_session(&self, key: &str) -> Option<Box<dyn SessionController>> {
        if !self.host.has_session(key) {
            return None;
        }
        let panel_id = super::focus::panel_id_of_pane_key(key);
        let pane_id = super::focus::pane_id_of_pane_key(key);
        let accepted = self.host.request_cancel(panel_id, pane_id);
        Some(Box::new(EagerController { accepted }))
    }

    fn remove_session(&self, key: &str) -> Result<(), String> {
        self.host.remove_session(key)
    }
}

fn lean_panel(panel: &StorePanel) -> crate::Panel {
    crate::Panel {
        id: panel.id.clone(),
        connection_id: panel.connection_id.clone(),
        panel_type: if panel.panel_type == StorePanelType::Query {
            PanelType::Query
        } else {
            PanelType::Other
        },
    }
}

fn lean_exec(table: &ExecTable) -> ExecMap {
    ExecMap::from_entries(
        table
            .iter()
            .map(|(key, state)| {
                (
                    key.clone(),
                    QueryExecState {
                        running: state.running,
                        execution_id: state.execution_id.clone(),
                        cancel_state: state.cancel_state,
                        ..QueryExecState::empty()
                    },
                )
            })
            .collect(),
    )
}

fn apply_remaining(table: &mut ExecTable, remaining: ExecMap) {
    let kept = remaining.keys().into_iter().collect::<BTreeSet<_>>();
    table.retain(|key, _| kept.contains(key));
}

pub struct PanelStore {
    pub state: PanelState,
    next_panel_id: u64,
    pub(crate) in_flight: BTreeSet<String>,
}

impl PanelStore {
    pub fn new() -> Self {
        Self {
            state: PanelState::default(),
            next_panel_id: 0,
            in_flight: BTreeSet::new(),
        }
    }

    pub fn next_panel_id(&mut self, prefix: &str) -> String {
        self.next_panel_id += 1;
        format!("panel-{prefix}-{}", self.next_panel_id)
    }

    pub fn reset(&mut self) {
        self.state = PanelState::default();
        self.next_panel_id = 0;
        self.in_flight.clear();
    }

    pub fn add_panel(&mut self, panel: StorePanel, activate: bool, pane_id: Option<&str>) {
        let key = pane_key(&panel.id, pane_id);
        if panel.panel_type == StorePanelType::Query {
            self.state.query_exec.insert(key, PanelExecState::empty());
        }
        let mut panels = self.state.panels.clone();
        panels.push(panel.clone());
        let active = if activate {
            Some(panel.id.as_str())
        } else {
            self.state.active_panel_id.as_deref()
        };
        self.state.panels = panels.clone();
        self.apply_focus(sync_pane_focus(
            &self.state.focused_pane_id_by_panel,
            &panels,
            active,
            None,
        ));
    }

    fn apply_focus(&mut self, patch: FocusPatch) {
        self.state.focused_pane_id_by_panel = patch.focused_pane_id_by_panel;
        self.state.focused_pane_id = patch.focused_pane_id;
        self.state.active_panel_id = patch.active_panel_id;
    }

    pub fn remove_panel(
        &mut self,
        host: &dyn PanelStoreHost,
        panel_id: &str,
    ) -> Result<(), String> {
        let panel = match self
            .state
            .panels
            .iter()
            .find(|entry| entry.id == panel_id)
            .cloned()
        {
            Some(panel) => panel,
            None => return Ok(()),
        };
        let mut pane_ids = pane_keys_of_panel(
            &self.state.query_exec.keys().cloned().collect::<Vec<_>>(),
            panel_id,
        );
        for key in host.session_keys_of_panel(panel_id) {
            if !pane_ids.contains(&key) {
                pane_ids.push(key);
            }
        }
        if !host.confirm_close(panel_id, &pane_ids) {
            return Ok(());
        }
        for key in &pane_ids {
            if host.has_session(key) {
                if host.remove_session(key).is_err() {
                    host.mark_blocked(key);
                    return Ok(());
                }
            }
        }
        let deps = StoreDeps { host };
        let remaining = cancel_and_cleanup_exec(
            &deps,
            &[lean_panel(&panel)],
            &lean_exec(&self.state.query_exec),
        );
        apply_remaining(&mut self.state.query_exec, remaining);
        let panels = self.state.panels.clone();
        let next_panels = panels
            .iter()
            .filter(|entry| entry.id != panel_id)
            .cloned()
            .collect::<Vec<_>>();
        let active = resolve_next_active(&panels, panel_id, self.state.active_panel_id.as_deref());
        self.apply_focus(sync_pane_focus(
            &self.state.focused_pane_id_by_panel,
            &next_panels,
            active.as_deref(),
            None,
        ));
        self.state.panels = next_panels;
        Ok(())
    }

    fn remove_many<F>(&mut self, host: &dyn PanelStoreHost, predicate: F)
    where
        F: Fn(&StorePanel) -> bool,
    {
        let ids = self
            .state
            .panels
            .iter()
            .filter(|panel| predicate(panel))
            .map(|panel| panel.id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            let _ = self.remove_panel(host, &id);
        }
    }

    pub fn remove_all_for_connection(&mut self, host: &dyn PanelStoreHost, connection_id: &str) {
        self.remove_many(host, |panel| panel.connection_id == connection_id);
    }

    pub fn remove_panels_for_relation(
        &mut self,
        host: &dyn PanelStoreHost,
        connection_id: &str,
        table: &str,
        database: Option<&str>,
    ) {
        self.remove_many(host, |panel| {
            super::focus::matches_relation(panel, connection_id, table, database)
        });
    }

    pub fn remove_panels_for_database(
        &mut self,
        host: &dyn PanelStoreHost,
        connection_id: &str,
        database: &str,
        session_database: Option<&str>,
    ) {
        self.remove_many(host, |panel| {
            super::focus::matches_database(panel, connection_id, database, session_database)
        });
    }

    pub fn set_active_panel(&mut self, panel_id: Option<&str>) {
        let panels = self.state.panels.clone();
        self.apply_focus(sync_pane_focus(
            &self.state.focused_pane_id_by_panel,
            &panels,
            panel_id,
            None,
        ));
    }

    pub fn update_panel(&mut self, panel_id: &str, patch: super::types::PanelPatch) {
        for panel in &mut self.state.panels {
            if panel.id == panel_id {
                patch.apply(panel);
            }
        }
    }

    pub fn close_others(&mut self, host: &dyn PanelStoreHost, panel_id: &str) {
        self.remove_many(host, |panel| panel.id != panel_id);
    }

    pub fn close_all(&mut self, host: &dyn PanelStoreHost) {
        self.remove_many(host, |_| true);
    }

    pub fn close_to_the_right(&mut self, host: &dyn PanelStoreHost, panel_id: &str) {
        let ids = self
            .state
            .panels
            .iter()
            .position(|panel| panel.id == panel_id)
            .map(|index| {
                self.state.panels[index + 1..]
                    .iter()
                    .map(|panel| panel.id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for id in ids {
            let _ = self.remove_panel(host, &id);
        }
    }

    pub fn close_to_the_left(&mut self, host: &dyn PanelStoreHost, panel_id: &str) {
        let ids = self
            .state
            .panels
            .iter()
            .position(|panel| panel.id == panel_id)
            .map(|index| {
                self.state.panels[..index]
                    .iter()
                    .map(|panel| panel.id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for id in ids {
            let _ = self.remove_panel(host, &id);
        }
    }

    pub fn set_focused_pane(&mut self, pane_id: Option<&str>, panel_id: Option<&str>) {
        let target = match panel_id.or(self.state.active_panel_id.as_deref()) {
            Some(target) => target.to_owned(),
            None => return,
        };
        let panels = self.state.panels.clone();
        let active = self.state.active_panel_id.clone();
        self.apply_focus(sync_pane_focus(
            &self.state.focused_pane_id_by_panel,
            &panels,
            active.as_deref(),
            Some((&target, pane_id)),
        ));
    }

    pub fn open_pane(&mut self, panel_id: &str, pane_id: &str) {
        let panel = match self.state.panels.iter().find(|entry| entry.id == panel_id) {
            Some(panel) => panel.clone(),
            None => return,
        };
        if panel.panel_type != StorePanelType::Query || pane_id.is_empty() {
            return;
        }
        let key = pane_key(panel_id, Some(pane_id));
        let panels = self.state.panels.clone();
        let patch = sync_pane_focus(
            &self.state.focused_pane_id_by_panel,
            &panels,
            Some(panel_id),
            Some((panel_id, Some(pane_id))),
        );
        self.apply_focus(patch);
        self.state
            .query_exec
            .entry(key)
            .or_insert_with(PanelExecState::empty);
    }

    pub fn close_pane(
        &mut self,
        host: &dyn PanelStoreHost,
        panel_id: &str,
        pane_id: &str,
    ) -> Result<(), String> {
        let panel = match self
            .state
            .panels
            .iter()
            .find(|entry| entry.id == panel_id)
            .cloned()
        {
            Some(panel) => panel,
            None => return Ok(()),
        };
        if panel.panel_type != StorePanelType::Query {
            return Ok(());
        }
        let key = pane_key(panel_id, Some(pane_id));
        if !host.confirm_close(panel_id, &[key.clone()]) {
            return Ok(());
        }
        if host.has_session(&key) {
            host.remove_session(&key)
                .map_err(|_| host.close_blocked_message())?;
        }
        let deps = StoreDeps { host };
        let remaining = cancel_and_cleanup_pane_exec(
            &deps,
            &crate::Panel {
                id: panel_id.to_owned(),
                connection_id: panel.connection_id.clone(),
                panel_type: PanelType::Query,
            },
            pane_id,
            &lean_exec(&self.state.query_exec),
        );
        apply_remaining(&mut self.state.query_exec, remaining);
        let clears = self
            .state
            .focused_pane_id_by_panel
            .get(panel_id)
            .map(|value| value == pane_id)
            .unwrap_or(false);
        let panels = self.state.panels.clone();
        let active = self.state.active_panel_id.clone();
        self.apply_focus(sync_pane_focus(
            &self.state.focused_pane_id_by_panel,
            &panels,
            active.as_deref(),
            if clears { Some((panel_id, None)) } else { None },
        ));
        Ok(())
    }
}
