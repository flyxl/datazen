//! KV panel routing: the driver view is host-owned; this module builds its
//! props and forwards the exact `KvSlotAtom` identity (F-2 relay).

use std::rc::Rc;

use rune::{col, row, text, El};

use crate::ui::kit::kit;

use super::model::{KvSlotAtom, KvViewProps, PanelVm};

#[allow(dead_code)] // host contract; consumer: the KV workbench host (not wired).
pub(crate) struct KvViewHostPort<S> {
    pub on_view: Option<Rc<dyn Fn(&mut S, KvViewProps, Option<KvSlotAtom>) + 'static>>,
}

/// Build the KV view marker and hand the host the props + the very same atom.
#[allow(dead_code)] // view entry point; consumer: the panel host (not wired).
pub(crate) fn kv_view<S: 'static>(
    panel: &PanelVm,
    dark: bool,
    slot_state: Option<KvSlotAtom>,
    host: &KvViewHostPort<S>,
    state: &mut S,
) -> El {
    let k = kit(dark);
    let atom = slot_state.clone();
    let (has_state, selected, dirty) = match &atom {
        Some(atom) => {
            let borrowed = atom.borrow();
            (true, borrowed.selected_key.clone(), borrowed.dirty)
        }
        None => (false, String::new(), false),
    };
    let props = KvViewProps {
        panel_id: panel.id.clone(),
        db_session_id: panel.db_session_id.clone(),
        connection_id: panel.connection_id.clone(),
        connection_name: panel.connection_name.clone(),
        database_type: panel.database_type.clone(),
        initial_database: panel.kv_db_name.clone(),
        hide_sidebar: true,
        is_active: true,
        has_slot_state: has_state,
        selected_key: selected.clone(),
        dirty,
    };
    // The KV view mounts for every KV panel, relay or not — the atom is simply
    // absent when the host has no slot state to publish.
    if let Some(on_view) = host.on_view.clone() {
        on_view(state, props, atom.clone());
    }
    let mut view = col()
        .key("panel-kv-view")
        .w_full()
        .h_full()
        .gap(4.0)
        .child(
            text("KV")
                .key("panel-kv-view-title")
                .font(k.ty.text_small)
                .color(k.c.text_base),
        )
        .child(row().key("panel-kv-session").label(panel.db_session_id.clone()).a11y_hidden())
        .child(row().key("panel-kv-database").label(panel.kv_db_name.clone().unwrap_or_default()).a11y_hidden());
    if has_state {
        view = view
            .child(row().key("panel-kv-has-state").label("yes").a11y_hidden())
            .child(row().key("panel-kv-selected-key").label(selected).a11y_hidden())
            .child(row().key("panel-kv-dirty").label(dirty.to_string()).a11y_hidden());
    } else {
        view = view.child(row().key("panel-kv-has-state").label("no").a11y_hidden());
    }
    view
}
