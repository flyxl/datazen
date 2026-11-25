//! The Variables and Details tabs: two-column key/value tables with exact
//! localized titles and data-driven rows. React renders them as `<table>`
//! bodies; Rune uses keyed rows so tests can observe every row without viewport
//! coordinates.

use rune::{col, icon, row, text, El};

use crate::i18n::t;
use crate::icons::ICON_TABLE;
use crate::ui::kit::kit;

use super::format::{detail_keys, format_value, label_for_key};
use super::ServerStatusVm;

/// React `detailRows`: ordered/labeled key/value pairs from the status record.
#[allow(dead_code)] // pure projection; consumed by the details table and tests.
pub fn detail_rows(vm: &ServerStatusVm) -> Vec<(String, String)> {
    let status = match &vm.status {
        Some(status) => status,
        None => return Vec::new(),
    };
    detail_keys(status)
        .into_iter()
        .map(|key| {
            let value = status.get(&key).cloned().unwrap_or_default();
            (label_for_key(&key), format_value(&key, &value))
        })
        .collect()
}

/// Build the Variables tab: the table when variables exist, else the empty note.
#[allow(dead_code)] // view part; remove when the connection host wires the panel.
pub fn variables(vm: &ServerStatusVm, dark: bool) -> El {
    let k = kit(dark);
    if vm.variables.is_empty() {
        return col()
            .key("server-status-empty-variables")
            .w_full()
            .grow(1.0)
            .center()
            .child(text(t("serverStatus.emptyVariables")).font(k.ty.text_small).color(k.c.text_muted));
    }
    let mut table = col()
        .key("server-status-variables-table")
        .w_full()
        .min_w(0.0)
        .gap(2.0)
        .child(
            row().key("server-status-variables-head").w_full().gap(6.0).center_y()
                .child(icon(&ICON_TABLE, 14.0).color(k.c.text_muted))
                .child(text(t("serverStatus.statusVarsTitle")).font(k.ty.text_small).color(k.c.text_base))
                .child(text(vm.variables.len().to_string()).font(k.ty.text_xsmall).color(k.c.text_muted)),
        );
    for variable in &vm.variables {
        table = table.child(
            row()
                .key(format!("server-status-variable-{}", variable.name))
                .w_full()
                .gap(10.0)
                .center_y()
                .child(
                    text(variable.name.clone())
                        .key(format!("server-status-variable-name-{}", variable.name))
                        .font(k.ty.mono)
                        .color(k.c.text_muted),
                )
                .child(
                    text(variable.value.clone().unwrap_or_else(|| "—".to_owned()))
                        .key(format!("server-status-variable-value-{}", variable.name))
                        .font(k.ty.mono)
                        .color(k.c.text_base),
                ),
        );
    }
    table
}

/// Build the Details tab: one row per visible status field.
#[allow(dead_code)] // view part; remove when the connection host wires the panel.
pub fn details(vm: &ServerStatusVm, dark: bool) -> El {
    let k = kit(dark);
    let mut table = col()
        .key("server-status-details-table")
        .w_full()
        .min_w(0.0)
        .gap(2.0)
        .child(
            row().key("server-status-details-head").w_full().gap(6.0).center_y()
                .child(icon(&ICON_TABLE, 14.0).color(k.c.text_muted))
                .child(text(t("serverStatus.detailTitle")).font(k.ty.text_small).color(k.c.text_base)),
        );
    for (index, (label, value)) in detail_rows(vm).into_iter().enumerate() {
        table = table.child(
            row()
                .key(format!("server-status-detail-{index}"))
                .w_full()
                .gap(10.0)
                .center_y()
                .child(text(label).font(k.ty.text_small).color(k.c.text_muted))
                .child(text(value).key(format!("server-status-detail-value-{index}")).font(k.ty.mono).color(k.c.text_base)),
        );
    }
    table
}
