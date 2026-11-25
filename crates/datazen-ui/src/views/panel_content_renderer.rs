//! Port of `packages/frontend/src/windows/connection/PanelContentRenderer.tsx`.
//!
//! React routes one active panel to a table/view/query/… sub-view, with a KV
//! branch chosen by driver metadata rather than the panel type literal. Rune
//! keeps the routing and per-slot empty/loading shape here; the heavy sub-views
//! and the KV driver view are host-owned through [`PanelHostPort`], so nothing
//! fake (IPC, store, timers) lives in this view.
//!
//! ## Keys
//!
//! React test ids `sub-tab-{id}` and `panel-context-path` are kept. Rune-side
//! `panel-slot-*` markers name the child slots for tests until each real view
//! is mounted by the host.
//!
//! Leftover L1 (removal stage): the driver-meta flags (isKeyValue/readOnly)
//! and the slot markers are stand-ins; feed them from the shared driver registry
//! and mount the real sub-views as they land, dropping the per-item allows.

use std::rc::Rc;

use rune::{col, row, text, El};

use crate::i18n::t;
use crate::ui::kit::kit;

mod kv_slot;
mod model;

pub use kv_slot::KvViewHostPort;
pub use model::{
    create_kv_slot_state, format_context_path, is_key_value_driver, is_read_only_driver, sub_tabs,
    view_sub_tabs, KvSlotAtom, KvSlotState, KvViewProps, PanelType, PanelVm, SubTabId,
};

use kv_slot::kv_view;

/// Every command the renderer asks the host to perform.
#[allow(dead_code)] // host port; consumer: the content-view host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PanelAction {
    SetSubTab { panel_id: String, sub_tab: SubTabId },
    ExitStructureEditing(String),
    EditTableStructure { panel_id: String, name: String },
    SelectTable { table: String, schema: Option<String>, database: String },
    OpenErDiagram { focus: Option<String>, database: Option<String> },
    ClosePanel(String),
    Refresh,
    /// A data-bearing panel (server status / processes) produced new cache data.
    UpdatePanelData { panel_id: String, data_key: &'static str },
}

#[allow(dead_code)] // host port; consumer: the content-view host (not wired).
pub struct PanelHostPort<S> {
    pub on_action: Option<Rc<dyn Fn(&mut S, PanelAction) + 'static>>,
    pub kv: KvViewHostPort<S>,
}

impl<S> Default for PanelHostPort<S> {
    fn default() -> Self {
        Self {
            on_action: None,
            kv: KvViewHostPort { on_view: None },
        }
    }
}

impl<S: 'static> PanelHostPort<S> {
    fn emit(&self, state: &mut S, action: PanelAction) {
        if let Some(on_action) = &self.on_action {
            on_action(state, action);
        }
    }
}

fn sub_tab_bar<S: 'static>(
    panel: &PanelVm,
    database: Option<&str>,
    tabs: Vec<(SubTabId, String)>,
    dark: bool,
    host: &PanelHostPort<S>,
    state: &mut S,
) -> El {
    let k = kit(dark);
    let mut bar = row().key("panel-sub-tab-bar").w_full().h(32.0);
    let mut buttons = row().key("panel-sub-tab-buttons").grow(1.0).h_full();
    for (id, label) in tabs {
        let active = panel.sub_tab == id;
        let mut button = row()
            .key(format!("sub-tab-{}", id.as_str()))
            .h_full()
            .px(12.0)
            .label(label.clone())
            .focusable(true)
            .fill(if active { k.c.surface_base } else { k.c.surface_weak });
        if let Some(on_action) = host.on_action.clone() {
            let panel_id = panel.id.clone();
            button = button.on_press(move |s: &mut S| {
                on_action(s, PanelAction::SetSubTab { panel_id: panel_id.clone(), sub_tab: id })
            });
        }
        buttons = buttons.child(button.child(
            text(label)
                .font(k.ty.text_small)
                .color(if active { k.c.text_base } else { k.c.text_muted }),
        ));
    }
    bar = bar.child(buttons);
    let context = format_context_path(
        &panel.connection_name,
        database,
        panel.table_schema.as_deref(),
    );
    if !context.is_empty() {
        bar = bar.child(
            text(context)
                .key("panel-context-path")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    }
    bar
}

fn slot(key: &str, label: &str, dark: bool) -> El {
    let k = kit(dark);
    col()
        .key(key)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .gap(4.0)
        .child(
            text(label)
                .key(format!("{key}-label"))
                .font(k.ty.text_small)
                .color(k.c.text_base),
        )
}

/// Build the renderer. `None` means React's `return null` (no panel or an
/// ER diagram without a database) — nothing is mounted.
#[allow(dead_code)] // view entry point; consumer: the content-view host (not wired).
pub fn panel_content_renderer<S: 'static>(
    panel: Option<&PanelVm>,
    _focused_pane_id: Option<&str>,
    current_database: Option<&str>,
    last_table_schema: Option<&str>,
    dark: bool,
    slot_state: Option<KvSlotAtom>,
    host: &PanelHostPort<S>,
    state: &mut S,
) -> Option<El> {
    let panel = panel?;
    let k = kit(dark);

    // KV / document panels route by driver metadata, not by panel type.
    if is_key_value_driver(&panel.database_type) {
        return Some(kv_view(panel, dark, slot_state, &host.kv, state));
    }

    let database = panel
        .database
        .as_deref()
        .or(current_database)
        .unwrap_or("");
    let read_only = is_read_only_driver(&panel.database_type);

    let body = match panel.panel_type {
        PanelType::Table => {
            let tabs = sub_tabs(read_only, &|key| t(key).to_owned());
            let mut content = col().key("panel-slot-table").w_full().grow(1.0).gap(4.0);
            content = content.child(sub_tab_bar(panel, panel.database.as_deref().or(current_database), tabs, dark, host, state));
            match panel.sub_tab {
                SubTabId::Data => content = content.child(slot("panel-slot-data", "Table data", dark)),
                SubTabId::Structure if panel.structure_editing => {
                    content = content.child(slot("panel-slot-structure-editor", "Edit structure", dark));
                }
                SubTabId::Structure => {
                    content = content.child(slot("panel-slot-structure", "Structure", dark));
                }
                SubTabId::Indexes => {
                    content = content.child(slot("panel-slot-indexes", "Indexes", dark));
                }
                SubTabId::ForeignKeys => {
                    content = content.child(slot("panel-slot-foreign-keys", "Foreign keys", dark));
                }
                SubTabId::Ddl => content = content.child(slot("panel-slot-ddl", "DDL", dark)),
            }
            Some(content)
        }
        PanelType::View => {
            let tabs = view_sub_tabs(&|key| t(key).to_owned());
            let mut content = col().key("panel-slot-view").w_full().grow(1.0).gap(4.0);
            content = content.child(sub_tab_bar(panel, panel.database.as_deref().or(current_database), tabs, dark, host, state));
            match panel.sub_tab {
                SubTabId::Data => content = content.child(slot("panel-slot-data", "View data", dark)),
                SubTabId::Structure => {
                    content = content.child(slot("panel-slot-structure", "Structure", dark))
                }
                SubTabId::Ddl => content = content.child(slot("panel-slot-ddl", "DDL", dark)),
                _ => {}
            }
            Some(content)
        }
        PanelType::Query => Some(slot("panel-slot-query", "Query", dark)),
        PanelType::CreateTable => {
            if panel.multi_db_without_database && database.is_empty() {
                Some(
                    col()
                        .key("panel-slot-create-table-loading")
                        .w_full()
                        .grow(1.0)
                        .center()
                        .child(
                            text(t("common.loading"))
                                .font(k.ty.text_small)
                                .color(k.c.text_muted),
                        ),
                )
            } else {
                let label = format!(
                    "{} · {}",
                    t("common.default"),
                    panel.table_schema.as_deref().or(last_table_schema).unwrap_or("")
                );
                Some(slot("panel-slot-create-table", &label, dark))
            }
        }
        PanelType::ErDiagram => {
            if database.is_empty() {
                None
            } else {
                Some(slot("panel-slot-er-diagram", database, dark))
            }
        }
        PanelType::Objects => Some(slot("panel-slot-objects", database, dark)),
        PanelType::Privileges => Some(slot("panel-slot-privileges", database, dark)),
        PanelType::ServerStatus | PanelType::Processes => {
            let key = if panel.panel_type == PanelType::ServerStatus {
                "panel-slot-server-status"
            } else {
                "panel-slot-processes"
            };
            let mut el = slot(key, database, dark);
            let data_key = if panel.panel_type == PanelType::ServerStatus {
                "status"
            } else {
                "rows"
            };
            if panel.has_panel_data {
                if let Some(on_action) = host.on_action.clone() {
                    let panel_id = panel.id.clone();
                    el = el.on_press(move |s: &mut S| {
                        on_action(s, PanelAction::UpdatePanelData {
                            panel_id: panel_id.clone(),
                            data_key,
                        })
                    });
                }
            }
            Some(el)
        }
        PanelType::DbObject => Some(slot("panel-slot-db-object", &panel.table_name, dark)),
    }?;

    Some(col().key("panel-content-renderer").w_full().h_full().child(body))
}

// Leftover L1 (removal stage): mount the real sub-views and the KV driver view
// from the content-view host, replace the driver-meta stand-ins with the shared
// registry, and drop the per-item allows and slot markers.
