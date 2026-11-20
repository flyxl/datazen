//! Port of `packages/frontend/src/windows/connection/navigator/DatabaseValueSearchDialog.tsx`.
//!
//! The scan itself is host-owned (`ValueSearchDriver` + IPC); this view owns the
//! cockpit: value/mode, scope and type selectors, the schema-qualified picker,
//! cost/progress/answer surfaces and the locate intent. Everything observable
//! is a [`ValueSearchVm`] value plus [`ValueSearchHostPort`] callbacks — no fake
//! IPC, promise, timer or thread.
//!
//! ## Keys
//!
//! React `data-testid`s are preserved on the dialog and every control/row
//! (`value-search-input`, `-mode`, `-start`, `-scope`, `-types`, `-results`,
//! `value-search-result-{key}`, `value-search-locate-{key}`, …).
//!
//! Leftover L1 (removal stage): the driver meta for `valueSearchExtraTypes`
//! and the scan result DTOs are local mirrors; feed them from the shared driver
//! registry / backend-client types when they land, then drop the allows.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use rune::{col, icon, row, text, El, TextEdit, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::{ICON_EXTERNAL_LINK, ICON_SEARCH, ICON_X};
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::select::Select;
use crate::ui::toggle::Checkbox;

pub(crate) const DIALOG_KEY: &str = "database-value-search-dialog";

mod value_search_model;
pub(crate) use value_search_model::*;
type PickFn<S> = Rc<dyn Fn(&mut S, &str) + 'static>;

/// Kit select wrapper with per-key open state; the built trigger is renamed to
/// the React `data-testid` so callers see the exact key.
fn select_el<S: 'static>(
    key: &str,
    labels: &[String],
    values: &[&str],
    current: &str,
    accessible_name: &str,
    width: f32,
    disabled: bool,
    vm: &ValueSearchVm,
    dark: bool,
    layer: &mut Vec<El>,
    host: &ValueSearchHostPort<S>,
    pick: PickFn<S>,
) -> El {
    let k = kit(dark);
    let mut select = Select::new(format!("{key}-picker"), labels)
        .values(values)
        .current(Some(current))
        .label(accessible_name)
        .width(width)
        .disabled(disabled);
    if !disabled {
        let opened = vm.select_open_values.get(key).copied().unwrap_or(false);
        if let Some(accessor) = host.select_open.clone() {
            let owned = key.to_owned();
            let a = accessor.clone();
            select = select.open(opened).on_toggle(move |s: &mut S| *a(s, &owned) = true);
            let a = accessor.clone();
            let owned = key.to_owned();
            select = select.on_close(move |s: &mut S| *a(s, &owned) = false);
            let a = accessor;
            let owned = key.to_owned();
            select = select.on_choose(move |s: &mut S, chosen: &str, _tx: &mut Tx| {
                pick(s, chosen);
                *a(s, &owned) = false;
            });
        } else {
            select = select.on_toggle(|_s: &mut S| {});
            select = select.on_choose(move |s: &mut S, chosen: &str, _tx: &mut Tx| pick(s, chosen));
        }
    }
    select.build(&k, layer).key(key)
}

fn button<S: 'static>(
    key: &str,
    label: String,
    dark: bool,
    disabled: bool,
    handler: Option<Rc<dyn Fn(&mut S) + 'static>>,
) -> El {
    let k = kit(dark);
    let mut el = Button::new(label)
        .variant(if key == "value-search-start" {
            ButtonVariant::Contrast
        } else {
            ButtonVariant::Ghost
        })
        .control_size(ControlSize::Small)
        .disabled(disabled);
    if let Some(handler) = handler {
        el = el.on_press(move |s: &mut S| handler(s));
    }
    el.build(k).key(key)
}

/// Build the dialog into `layer`. A closed dialog mounts nothing.
#[allow(dead_code)] // view entry point; consumer: the navigator host (not wired).
pub(crate) fn database_value_search_dialog<S: 'static>(
    vm: &ValueSearchVm,
    dark: bool,
    layer: &mut Vec<El>,
    host: ValueSearchHostPort<S>,
) {
    let k = kit(dark);
    let busy = is_busy(vm.progress.as_ref());
    let all_targets = value_search_targets(&vm.tables);
    let targets: Vec<SearchTarget> = match vm.scope {
        SearchScope::All => all_targets.clone(),
        SearchScope::Selected => all_targets
            .iter()
            .filter(|target| vm.selected_keys.contains(&target_key(target)))
            .cloned()
            .collect(),
    };

    let mut dialog = Dialog::new(DIALOG_KEY, t("valueSearch.title"))
        .open(vm.open)
        .description(t("valueSearch.description"))
        .width(576.0)
        .on_close({
            let on_close = host.on_close.clone();
            move |s: &mut S| {
                if let Some(on_close) = &on_close {
                    on_close(s);
                }
            }
        });

    // Search line: value + match mode + start/pause/resume/cancel.
    let mut value_field = TextField::new("value-search-input", &vm.value)
        .height(28.0)
        .pad_x(8.0)
        .grow()
        .placeholder(t("valueSearch.placeholder"))
        .disabled(busy);
    if let Some(accessor) = host.value_edit.clone() {
        value_field = value_field.on_edit(rune::edit_access(move |s: &mut S| accessor(s)));
    }
    let search_line = row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child(icon(&ICON_SEARCH, 14.0).color(k.c.text_muted))
        .child(value_field.build(dark));

    let mut mode_labels = Vec::new();
    let mut mode_values = Vec::new();
    for mode in MODES {
        mode_labels.push(t(mode.label_key()).to_owned());
        mode_values.push(mode.as_str());
    }
    let on_mode = host.on_mode.clone();
    let mode_pick: PickFn<S> = Rc::new(move |s: &mut S, chosen: &str| {
        if let Some(cb) = &on_mode {
            cb(s, mode_from_str(chosen));
        }
    });
    let mode_select = select_el(
        "value-search-mode",
        &mode_labels,
        &mode_values,
        t(vm.mode.label_key()),
        t("valueSearch.modeLabel"),
        110.0,
        busy,
        vm,
        dark,
        layer,
        &host,
        mode_pick,
    );
    let mut search_line = search_line.child(mode_select);
    let status = vm.progress.as_ref().map(|p| p.status);
    if !busy {
        let request = StartRequest {
            db_session_id: vm.db_session_id.clone(),
            database: vm.database.clone(),
            targets: targets.clone(),
            value: vm.value.text.trim().to_owned(),
            mode: vm.mode,
            cell_types: cell_types_for_scope(vm.type_scope),
        };
        let on_start = host.on_start.clone();
        search_line = search_line.child(button(
            "value-search-start",
            t("valueSearch.start").to_owned(),
            dark,
            vm.value.text.trim().is_empty() || targets.is_empty(),
            on_start.map(|cb| Rc::new(move |s: &mut S| cb(s, request.clone())) as Rc<dyn Fn(&mut S)>),
        ));
    }
    match status {
        Some(SearchStatus::Running) => {
            search_line = search_line.child(button(
                "value-search-pause",
                t("valueSearch.pause").to_owned(),
                dark,
                false,
                host.on_pause.clone(),
            ));
        }
        Some(SearchStatus::Paused) => {
            search_line = search_line.child(button(
                "value-search-resume",
                t("valueSearch.resume").to_owned(),
                dark,
                false,
                host.on_resume.clone(),
            ));
        }
        _ => {}
    }
    if busy {
        search_line = search_line.child(
            Button::new("")
                .icon(&ICON_X)
                .variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small)
                .build(k)
                .key("value-search-cancel")
                .on_press({
                    let on_cancel = host.on_cancel.clone();
                    move |s: &mut S| {
                        if let Some(cb) = &on_cancel {
                            cb(s);
                        }
                    }
                }),
        );
    }
    dialog = dialog.child(search_line);

    // Scope line: all/selected + searchable type scope.
    let scope_labels = [
        t("valueSearch.scope.all").replace("{count}", &all_targets.len().to_string()),
        t("valueSearch.scope.selected").replace("{count}", &vm.selected_keys.len().to_string()),
    ];
    let on_scope = host.on_scope.clone();
    let scope_pick: PickFn<S> = Rc::new(move |s: &mut S, chosen: &str| {
        if let Some(cb) = &on_scope {
            cb(s, if chosen == "selected" { SearchScope::Selected } else { SearchScope::All });
        }
    });
    let scope_select = select_el(
        "value-search-scope",
        &scope_labels,
        &["all", "selected"],
        if vm.scope == SearchScope::All { &scope_labels[0] } else { &scope_labels[1] },
        t("valueSearch.scopeLabel"),
        200.0,
        busy,
        vm,
        dark,
        layer,
        &host,
        scope_pick,
    );
    let type_scopes = type_scopes_for_driver(vm.driver_type.as_deref());
    let type_labels: Vec<String> = type_scopes
        .iter()
        .map(|scope| t(scope.label_key()).to_owned())
        .collect();
    let type_values: Vec<&str> = type_scopes.iter().map(|scope| scope.as_str()).collect();
    let on_type_scope = host.on_type_scope.clone();
    let type_pick: PickFn<S> = Rc::new(move |s: &mut S, chosen: &str| {
        if let Some(cb) = &on_type_scope {
            cb(s, type_scope_from_str(chosen));
        }
    });
    let type_select = select_el(
        "value-search-types",
        &type_labels,
        &type_values,
        t(vm.type_scope.label_key()),
        t("valueSearch.typesLabel"),
        160.0,
        busy,
        vm,
        dark,
        layer,
        &host,
        type_pick,
    );
    dialog = dialog.child(row().w_full().gap(8.0).child(scope_select).child(type_select));

    // Selected-scope table picker: schema-qualified, filterable.
    if vm.scope == SearchScope::Selected {
        let mut picker = col()
            .key("value-search-table-picker")
            .w_full()
            .gap(6.0)
            .border(1.0, k.c.border_base);
        let mut filter_field = TextField::new("value-search-table-filter", &vm.picker_filter)
            .height(24.0)
            .pad_x(8.0)
            .placeholder(t("valueSearch.pickerPlaceholder"))
            .disabled(busy);
        if let Some(accessor) = host.picker_filter_edit.clone() {
            filter_field = filter_field.on_edit(rune::edit_access(move |s: &mut S| accessor(s)));
        }
        picker = picker.child(filter_field.build(dark));
        let needle = vm.picker_filter.text.trim().to_lowercase();
        let rows: Vec<&SearchTarget> = all_targets
            .iter()
            .filter(|target| target_key(target).to_lowercase().contains(&needle))
            .collect();
        if rows.is_empty() {
            picker = picker.child(
                text(t("valueSearch.pickerEmpty"))
                    .key("value-search-picker-empty")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
            );
        }
        for target in rows {
            let key = target_key(target);
            let mut option = Checkbox::new(
                format!("value-search-table-option-{key}"),
                vm.selected_keys.contains(&key),
            )
            .label(key.clone())
            .disabled(busy);
            if let Some(on_toggle_target) = host.on_toggle_target.clone() {
                let toggle_key = key.clone();
                option = option.on_toggle(move |s: &mut S| on_toggle_target(s, toggle_key.clone()));
            }
            picker = picker.child(option.build(k));
        }
        picker = picker.child(
            text(
                t("valueSearch.pickerCount")
                    .replace("{selected}", &vm.selected_keys.len().to_string())
                    .replace("{total}", &all_targets.len().to_string()),
            )
            .key("value-search-picker-count")
            .font(k.ty.text_xsmall)
            .color(k.c.text_muted),
        );
        dialog = dialog.child(picker);
    }

    if vm.type_scope != TypeScope::Text {
        dialog = dialog.child(
            text(t("valueSearch.types.note"))
                .key("value-search-types-note")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    }
    dialog = dialog.child(
        text(
            t("valueSearch.costNote").replace("{count}", &targets.len().to_string()),
        )
        .key("value-search-cost-note")
        .font(k.ty.text_xsmall)
        .color(k.c.text_muted),
    );

    if let Some(progress) = vm.progress.as_ref() {
        if progress.status != SearchStatus::Idle {
            let line = match progress.status {
                SearchStatus::Running => progress.current.as_deref().map_or_else(
                    || t("valueSearch.status.running").to_owned(),
                    |table| t("valueSearch.scanning").replace("{table}", table),
                ),
                status => t(status.status_key()).to_owned(),
            };
            dialog = dialog.child(
                row()
                    .key("value-search-progress")
                    .w_full()
                    .gap(8.0)
                    .child(text(line).font(k.ty.text_xsmall).color(k.c.text_base))
                    .child(
                        text(format!("{} / {}", progress.completed, progress.total))
                            .key("value-search-progress-count")
                            .font(k.ty.text_xsmall)
                            .color(k.c.text_muted),
                    ),
            );
        }
        if progress.status == SearchStatus::Done {
            if let Some(ms) = progress.elapsed_ms {
                let seconds = (ms / 1000).max(1);
                dialog = dialog.child(
                    text(
                        t("valueSearch.doneNote")
                            .replace("{count}", &progress.total.to_string())
                            .replace("{seconds}", &seconds.to_string()),
                    )
                    .key("value-search-done-note")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
                );
            }
        }
    }

    let results = vm.progress.as_ref().map(|p| p.results.as_slice()).unwrap_or(&[]);
    let ranked = rank_tables(results, 3);
    if !ranked.is_empty() {
        let mut box_el = col()
            .key("value-search-most-likely")
            .w_full()
            .gap(4.0)
            .child(
                text(t("valueSearch.mostLikely"))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_accent),
            );
        for result in &ranked {
            box_el = box_el.child(result_row(result, "likely", vm, dark, layer, &host));
        }
        dialog = dialog.child(box_el);
    }
    let hits = with_hits(results);
    if !hits.is_empty() {
        let mut box_el = col().key("value-search-results").w_full().gap(2.0);
        for result in &hits {
            box_el = box_el.child(result_row(result, "hit", vm, dark, layer, &host));
        }
        dialog = dialog.child(box_el);
    }
    if vm.progress.as_ref().is_some_and(|p| p.status == SearchStatus::Done) && hits.is_empty() {
        dialog = dialog.child(
            text(t("valueSearch.noHits"))
                .key("value-search-empty")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    }
    let zero: Vec<&SearchResult> = results
        .iter()
        .filter(|result| result.matches == Some(0))
        .collect();
    if !zero.is_empty() {
        dialog = dialog.child(
            text(t("valueSearch.zeroHits").replace("{count}", &zero.len().to_string()))
                .key("value-search-zero-hits-summary")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    }
    let errors: Vec<&SearchResult> = results
        .iter()
        .filter(|result| result.matches.is_none())
        .collect();
    if !errors.is_empty() {
        dialog = dialog.child(
            text(t("valueSearch.errors").replace("{count}", &errors.len().to_string()))
                .key("value-search-errors-summary")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    }
    if all_targets.is_empty() {
        dialog = dialog.child(
            row()
                .gap(6.0)
                .child(icon(&ICON_SEARCH, 14.0).color(k.c.text_muted))
                .child(
                    text(t("valueSearch.noTables"))
                        .key("value-search-no-tables")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                ),
        );
    }
    dialog.build(k, layer);
}

fn result_row<S: 'static>(
    result: &SearchResult,
    prefix: &str,
    vm: &ValueSearchVm,
    dark: bool,
    _layer: &mut Vec<El>,
    host: &ValueSearchHostPort<S>,
) -> El {
    let k = kit(dark);
    let key = target_key(&SearchTarget {
        table: result.table.clone(),
        schema: result.schema.clone(),
    });
    let mut row_el = row()
        .key(format!("value-search-result-{key}"))
        .w_full()
        .gap(8.0)
        .center_y();
    let name = match result.schema.as_deref() {
        Some(schema) if !schema.is_empty() => format!("{schema}.{}", result.table),
        _ => result.table.clone(),
    };
    row_el = row_el
        .child(text(name).font(k.ty.text_xsmall).color(k.c.text_base))
        .child(
            text(result.matches.map_or_else(|| "-".to_owned(), |m| m.to_string()))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    let mut locate = Button::new(t("valueSearch.locate"))
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .icon(&ICON_EXTERNAL_LINK)
        .disabled(result.columns.is_empty());
    if let (Some(on_locate), Some(run)) = (host.on_locate.clone(), vm.last_run.clone()) {
        let intent = LocateIntent {
            table: result.table.clone(),
            schema: result.schema.clone(),
            database: vm.database.clone(),
            value: run.0,
            mode: run.1,
            columns: result.columns.clone(),
        };
        locate = locate.on_press(move |s: &mut S| on_locate(s, intent.clone()));
    }
    row_el.child(locate.build(k).key(format!("value-search-locate-{key}")))
}

// Leftover L1 (removal stage): bind the host ports to the navigator's scan
// driver and settings slice when that view lands, share the driver-meta and
// result DTOs with the backend-client types, and drop the per-item allows.
