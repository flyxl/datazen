//! Configuration steps of the import wizard (React `ImportDialog.tsx` lines
//! 448–847): file picker + format options, parse diagnostics, target table +
//! mapping table/warnings, and the conflict-strategy / commit-mode settings.
//! Preview, progress and result live in [`super::preview`].

use rune::{col, icon, row, text, tok, El, TextCase};
use rune_kit::{Button, ButtonVariant, ControlSize};

use super::{
    emit, make_select, ImportAction, ImportActionFn, ImportDialogVm, ImportVmFn, MappingMode,
    OpenSelect, ROOT, SAMPLE_ROWS,
};
use crate::components::connection::connection_import_fields::import_file_display_name;
use crate::i18n::t;
use crate::icons::{ICON_LOADER2, ICON_TABLE, ICON_X};
use crate::ui::copyable::CopyableError;
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::toggle::{Checkbox, RadioGroup, RadioItem};

/// React's `IMPORT_ENCODINGS` (`lib/importData.ts`).
const ENCODINGS: [&str; 6] = ["utf-8", "gbk", "gb18030", "big5", "shift_jis", "latin1"];

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

/// File row + the format-specific option row. Only one format cluster mounts,
/// exactly as React's `file && format === '…'` guards do.
pub(crate) fn build_file_and_format<S: 'static>(
    vm: &ImportDialogVm,
    on_action: &Option<ImportActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let k = kit(dark);
    let busy = matches!(vm.phase, super::WizardPhase::Running);
    let file_name = vm
        .file
        .as_ref()
        .map(|file| import_file_display_name(&file.name).to_owned())
        .unwrap_or_else(|| t("import.selectFile").to_owned());
    let mut pick = Button::new(file_name)
        .variant(ButtonVariant::Neutral)
        .control_size(ControlSize::Small)
        .icon(&ICON_TABLE)
        .disabled(busy)
        .on_press(emit(on_action, ImportAction::PickFile));
    if !vm.parse_error_message.is_empty() && vm.file.is_none() {
        pick = pick.help(vm.parse_error_message.clone());
    }
    let mut file_row = row()
        .key(format!("{ROOT}-file-row"))
        .w_full()
        .gap(8.0)
        .flex_wrap()
        .center_y()
        .child(pick.build(k).key(format!("{ROOT}-select-file")));

    if let Some(format) = vm.format() {
        file_row = file_row.child(
            text(format.as_str())
                .key(format!("{ROOT}-format"))
                .font(tok::type_::LABEL)
                .color(k.c.text_weak)
                .text_case(TextCase::Uppercase),
        );
        file_row = file_row.child(
            Button::new(t("import.replaceFile"))
                .variant(ButtonVariant::Neutral)
                .control_size(ControlSize::Small)
                .disabled(busy)
                .on_press(emit(on_action, ImportAction::PickFile))
                .build(k)
                .key(format!("{ROOT}-replace-file")),
        );
    }

    let options = match vm.format() {
        Some(super::ImportFormat::Csv) => csv_options(vm, on_action, dark, layer),
        Some(super::ImportFormat::Json) => encoding_only(vm, on_action, dark, layer),
        Some(super::ImportFormat::Xlsx) => sheet_only(vm, on_action, dark, layer),
        None => row().key(format!("{ROOT}-no-format")),
    };
    col()
        .key(format!("{ROOT}-file-section"))
        .w_full()
        .min_w(0.0)
        .gap(8.0)
        .child(file_row)
        .child(options)
}

fn csv_options<S: 'static>(
    vm: &ImportDialogVm,
    on_action: &Option<ImportActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let k = kit(dark);
    let encodings = strings(&ENCODINGS);
    let delimiters = strings(&["auto", ",", ";", "\t"]);
    let delimiter_labels = vec![
        t("import.delimiter.auto").to_owned(),
        t("import.delimiter.comma").to_owned(),
        t("import.delimiter.semicolon").to_owned(),
        t("import.delimiter.tab").to_owned(),
    ];
    let quotes = strings(&["\"", "'"]);
    let empties = strings(&["null", "empty"]);
    let empty_labels = vec![
        t("import.emptyUnquotedAs.null").to_owned(),
        t("import.emptyUnquotedAs.empty").to_owned(),
    ];

    let mut options = row()
        .key(format!("{ROOT}-csv-options"))
        .w_full()
        .gap(8.0)
        .flex_wrap()
        .center_y();

    options = options.child(labelled(
        t("import.encoding"),
        make_select(
            &format!("{ROOT}-encoding"),
            &encodings,
            &encodings,
            &vm.encoding,
            t("import.encoding"),
            112.0,
            vm.open_select == OpenSelect::Encoding,
            OpenSelect::Encoding,
            on_action,
            |value| ImportAction::SetEncoding(value.to_owned()),
            dark,
            layer,
        ),
    ));
    options = options.child(labelled(
        t("import.delimiter"),
        make_select(
            &format!("{ROOT}-delimiter"),
            &delimiter_labels,
            &delimiters,
            match vm.delimiter {
                super::Delimiter::Auto => "auto",
                super::Delimiter::Comma => ",",
                super::Delimiter::Semicolon => ";",
                super::Delimiter::Tab => "\t",
            },
            t("import.delimiter"),
            112.0,
            vm.open_select == OpenSelect::Delimiter,
            OpenSelect::Delimiter,
            on_action,
            |value| ImportAction::SetDelimiter(match value {
                "," => super::Delimiter::Comma,
                ";" => super::Delimiter::Semicolon,
                "\t" => super::Delimiter::Tab,
                _ => super::Delimiter::Auto,
            }),
            dark,
            layer,
        ),
    ));
    options = options.child(labelled(
        t("import.quoteChar"),
        make_select(
            &format!("{ROOT}-quote"),
            &quotes,
            &quotes,
            &vm.quote_char,
            t("import.quoteChar"),
            64.0,
            vm.open_select == OpenSelect::Quote,
            OpenSelect::Quote,
            on_action,
            |value| ImportAction::SetQuoteChar(value.to_owned()),
            dark,
            layer,
        ),
    ));
    options = options.child(labelled(
        t("import.emptyUnquotedAs"),
        make_select(
            &format!("{ROOT}-empty"),
            &empty_labels,
            &empties,
            match vm.empty_unquoted_as {
                super::EmptyUnquotedAs::Null => "null",
                super::EmptyUnquotedAs::Empty => "empty",
            },
            t("import.emptyUnquotedAs"),
            160.0,
            vm.open_select == OpenSelect::Empty,
            OpenSelect::Empty,
            on_action,
            |value| ImportAction::SetEmptyUnquotedAs(match value {
                "empty" => super::EmptyUnquotedAs::Empty,
                _ => super::EmptyUnquotedAs::Null,
            }),
            dark,
            layer,
        ),
    ));

    let first_row = vm.first_row_is_header;
    let mut header = Checkbox::new(format!("{ROOT}-first-row-header"), first_row)
        .label(t("import.firstRowIsHeader"));
    if let Some(callback) = on_action.clone() {
        header = header.on_toggle(move |s: &mut S| {
            callback(s, ImportAction::SetFirstRowIsHeader(!first_row))
        });
    }
    options.child(header.build(k))
}

fn encoding_only<S: 'static>(
    vm: &ImportDialogVm,
    on_action: &Option<ImportActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let encodings = strings(&ENCODINGS);
    row()
        .key(format!("{ROOT}-json-options"))
        .w_full()
        .gap(8.0)
        .center_y()
        .child(labelled(
            t("import.encoding"),
            make_select(
                &format!("{ROOT}-encoding"),
                &encodings,
                &encodings,
                &vm.encoding,
                t("import.encoding"),
                112.0,
                vm.open_select == OpenSelect::Encoding,
                OpenSelect::Encoding,
                on_action,
                |value| ImportAction::SetEncoding(value.to_owned()),
                dark,
                layer,
            ),
        ))
}

fn sheet_only<S: 'static>(
    vm: &ImportDialogVm,
    on_action: &Option<ImportActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let Some(parsed) = vm.parsed.as_ref() else {
        return row().key(format!("{ROOT}-no-sheets"));
    };
    if parsed.sheet_names.is_empty() {
        return row().key(format!("{ROOT}-no-sheets"));
    }
    let current = vm
        .sheet_name
        .clone()
        .or_else(|| parsed.sheet_names.first().cloned())
        .unwrap_or_default();
    row()
        .key(format!("{ROOT}-xlsx-options"))
        .w_full()
        .gap(8.0)
        .center_y()
        .child(labelled(
            t("import.sheet"),
            make_select(
                &format!("{ROOT}-sheet"),
                &parsed.sheet_names,
                &parsed.sheet_names,
                &current,
                t("import.sheet"),
                176.0,
                vm.open_select == OpenSelect::Sheet,
                OpenSelect::Sheet,
                on_action,
                |value| ImportAction::SetSheetName(value.to_owned()),
                dark,
                layer,
            ),
        ))
}

fn labelled(label: &str, control: El) -> El {
    row().gap(6.0).center_y().child(text(label).wrap()).child(control)
}

/// Target table + match mode, target-column diagnostics, the mapping table
/// and all four informational/warning lines.
pub(crate) fn build_target_and_mapping<S: 'static>(
    vm: &ImportDialogVm,
    on_action: &Option<ImportActionFn<S>>,
    vm_edit: &Option<ImportVmFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let k = kit(dark);
    let has_columns = vm.parsed.as_ref().is_some_and(|parsed| !parsed.columns.is_empty());
    if !has_columns {
        return row().key(format!("{ROOT}-no-target"));
    }

    let mut section = col()
        .key(format!("{ROOT}-target-section"))
        .w_full()
        .min_w(0.0)
        .gap(8.0)
        .child(build_target_controls(vm, on_action, vm_edit, dark, layer));

    if vm.targets.loading {
        section = section.child(
            row()
                .key(format!("{ROOT}-loading-targets"))
                .gap(8.0)
                .center_y()
                .child(icon(&ICON_LOADER2, 12.0))
                .child(text(t("import.loadingTargets")).color(k.c.text_weak)),
        );
    }
    if !vm.targets.error.is_empty() {
        section = section.child(
            CopyableError::new(
                format!("{ROOT}-load-targets-error"),
                t("import.loadTargetsFailed").replace("{message}", &vm.targets.error),
            )
            .build::<S>(k),
        );
    }

    if let (Some(plan), Some(targets)) = (vm.mapping_plan(), vm.targets.columns.as_ref()) {
        section = section.child(build_mapping_table(vm, &plan, targets, on_action, dark, layer));
        section = section.child(
            text(
                t("import.summary")
                    .replace("{sources}", &vm.parsed.as_ref().map_or(0, |p| p.columns.len()).to_string())
                    .replace("{mapped}", &plan.mapped_count().to_string())
                    .replace("{skipped}", &plan.skipped_count().to_string())
                    .replace("{unwritten}", &plan.untargeted.len().to_string()),
            )
            .key("import-summary")
            .wrap(),
        );
        if !plan.conflicts.is_empty() {
            let detail = plan
                .conflicts
                .iter()
                .map(|conflict| format!("{} ← {}", conflict.target, conflict.sources.join(", ")))
                .collect::<Vec<_>>()
                .join("；");
            section = section.child(warning(
                "import-conflict-warning",
                format!(
                    "{} {}",
                    t("import.conflictWarning").replace("{count}", &plan.conflicts.len().to_string()),
                    detail
                ),
                dark,
            ));
        }
    }

    let duplicates = vm.duplicates();
    if !duplicates.is_empty() {
        let columns = duplicates
            .iter()
            .map(|d| d.column.clone())
            .collect::<Vec<_>>()
            .join(", ");
        section = section.child(warning(
            "import-duplicate-warning",
            t("import.duplicateColumnWarning").replace("{columns}", &columns),
            dark,
        ));
    }
    let ragged = vm.ragged();
    if !ragged.is_empty() {
        section = section.child(warning(
            "import-ragged-warning",
            t("import.raggedRowWarning")
                .replace("{count}", &ragged.len().to_string())
                .replace("{line}", &ragged.first().map_or(0, |d| d.line).to_string()),
            dark,
        ));
    }
    if let Some(plan) = vm.mapping_plan() {
        if !plan.unmapped.is_empty() {
            section = section.child(
                text(t("import.unmappedColumns").replace("{columns}", &plan.unmapped.join(", ")))
                    .wrap(),
            );
        }
    }
    section
}

fn build_target_controls<S: 'static>(
    vm: &ImportDialogVm,
    on_action: &Option<ImportActionFn<S>>,
    vm_edit: &Option<ImportVmFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let mut row = row()
        .key(format!("{ROOT}-target-controls"))
        .w_full()
        .gap(8.0)
        .flex_wrap()
        .center_y();
    row = row.child(labelled(t("import.targetTable"), {
        if vm.table_options.is_empty() {
            let mut field = TextField::new(format!("{ROOT}-target-table"), &vm.target_table_edit)
                .height(28.0)
                .width(224.0)
                .placeholder(t("import.tableNamePlaceholder"));
            if let Some(vm_edit) = vm_edit.clone() {
                field = field.on_edit(move |s: &mut S| &mut vm_edit(s).target_table_edit);
            } else {
                field = field.disabled(true);
            }
            field.build(dark)
        } else {
            let labels: Vec<String> = vm.table_options.iter().map(|o| o.label.clone()).collect();
            let values: Vec<String> = vm.table_options.iter().map(|o| o.value.clone()).collect();
            let current_label = vm
                .table_options
                .iter()
                .find(|o| o.value == vm.target_table)
                .map(|o| o.label.clone())
                .unwrap_or_default();
            make_select(
                &format!("{ROOT}-target-table"),
                &labels,
                &values,
                &vm.target_table,
                t("import.targetTable"),
                224.0,
                vm.open_select == OpenSelect::TargetTable,
                OpenSelect::TargetTable,
                on_action,
                |value| ImportAction::SetTargetTable(value.to_owned()),
                dark,
                layer,
            )
            .label(current_label)
        }
    }));
    let modes = strings(&["auto", "name", "position"]);
    let mode_labels = vec![
        t("import.matchMode.auto").to_owned(),
        t("import.matchMode.name").to_owned(),
        t("import.matchMode.position").to_owned(),
    ];
    row.child(labelled(
        t("import.matchMode"),
        make_select(
            &format!("{ROOT}-match-mode"),
            &mode_labels,
            &modes,
            match vm.mapping_mode {
                MappingMode::Auto => "auto",
                MappingMode::Name => "name",
                MappingMode::Position => "position",
            },
            t("import.matchMode"),
            176.0,
            vm.open_select == OpenSelect::MatchMode,
            OpenSelect::MatchMode,
            on_action,
            |value| ImportAction::SetMappingMode(match value {
                "name" => MappingMode::Name,
                "position" => MappingMode::Position,
                _ => MappingMode::Auto,
            }),
            dark,
            layer,
        ),
    ))
}

fn build_mapping_table<S: 'static>(
    vm: &ImportDialogVm,
    plan: &super::MappingPlan,
    targets: &[super::TargetColumn],
    on_action: &Option<ImportActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) -> El {
    let k = kit(dark);
    let mut table = col()
        .key("import-mapping-table")
        .w_full()
        .min_w(0.0)
        .border(1.0, k.c.border_muted)
        .radius(6.0);
    table = table.child(
        row()
            .key(format!("{ROOT}-mapping-header"))
            .w_full()
            .gap(8.0)
            .child(cell(t("import.sourceColumn"), true, dark))
            .child(cell(t("import.targetColumn"), true, dark))
            .child(cell(t("import.sampleValue"), true, dark)),
    );
    for entry in &plan.entries {
        let sample = vm
            .parsed
            .as_ref()
            .and_then(|parsed| {
                parsed
                    .columns
                    .iter()
                    .position(|name| name == &entry.source)
                    .map(|index| parsed.sample(index))
            })
            .unwrap_or_else(|| "—".to_owned());
        let none_label = t("import.dontImport").to_owned();
        let labels: Vec<String> = std::iter::once(none_label)
            .chain(targets.iter().map(|column| {
                format!("{} ({})", column.name, column.data_type)
            }))
            .collect();
        let mut values = vec!["__none__".to_owned()];
        values.extend(targets.iter().map(|column| column.name.clone()));
        let current = vm
            .override_target(&entry.source)
            .unwrap_or_else(|| entry.target.clone())
            .unwrap_or_else(|| "__none__".to_owned());
        let source = entry.source.clone();
        let select = make_select(
            &format!("{ROOT}-mapping-{}", entry.source),
            &labels,
            &values,
            &current,
            t("import.targetColumn"),
            192.0,
            matches!(&vm.open_select, OpenSelect::Override(name) if name == &source),
            OpenSelect::Override(source.clone()),
            on_action,
            move |value| ImportAction::SetOverride {
                source: source.clone(),
                target: if value == "__none__" { None } else { Some(value.to_owned()) },
            },
            dark,
            layer,
        );
        table = table.child(
            row()
                .key(format!("{ROOT}-mapping-row-{}", entry.source))
                .w_full()
                .gap(8.0)
                .center_y()
                .child(cell(entry.source.clone(), false, dark))
                .child(select)
                .child(cell(sample, false, dark)),
        );
    }
    table
}

fn cell(content: impl Into<String>, header: bool, dark: bool) -> El {
    let mut el = text(content).grow(1.0).min_w(0.0).wrap();
    if header {
        el = el.color(kit(dark).c.text_weak);
    }
    el
}

/// React's warning banners: same text/keys, kit warning tokens. The upstream
/// AlertTriangle glyph is not in the registry; a registry X stands in.
fn warning(key: &str, message: String, dark: bool) -> El {
    let k = kit(dark);
    row()
        .key(key)
        .w_full()
        .gap(8.0)
        .center_y()
        .border(1.0, k.c.warning.border)
        .radius(6.0)
        .child(icon(&ICON_X, 14.0).color(k.c.warning.icon))
        .child(text(message).key(format!("{key}-text")).color(k.c.warning.fg).wrap())
}

/// The two radio groups from React's execution-settings section. Only shown
/// while a parse with rows exists and target columns are loaded (config phase).
pub(crate) fn build_execution<S: 'static>(
    vm: &ImportDialogVm,
    on_action: &Option<ImportActionFn<S>>,
    dark: bool,
) -> El {
    let has_rows = vm.parsed.as_ref().is_some_and(|parsed| !parsed.rows.is_empty());
    if !has_rows || vm.phase != super::WizardPhase::Config || vm.targets.columns.is_none() {
        return row().key(format!("{ROOT}-no-execution"));
    }
    let k = kit(dark);
    let on_error = RadioGroup::new(format!("{ROOT}-conflict-strategy"))
        .label(t("import.conflictStrategy"))
        .item(
            RadioItem::new(t("import.conflictStrategy.stop"), vm.on_error == super::OnError::Stop)
                .on_select(press(on_action, ImportAction::SetOnError(super::OnError::Stop))),
        )
        .item(
            RadioItem::new(t("import.conflictStrategy.skip"), vm.on_error == super::OnError::Skip)
                .on_select(press(on_action, ImportAction::SetOnError(super::OnError::Skip))),
        );

    let commit = RadioGroup::new(format!("{ROOT}-commit-mode"))
        .label(t("import.commitMode"))
        .item(
            RadioItem::new(
                t("import.commitMode.transaction"),
                vm.commit_mode == super::CommitMode::Transaction,
            )
            .on_select(press(on_action, ImportAction::SetCommitMode(super::CommitMode::Transaction))),
        )
        .item(
            RadioItem::new(
                t("import.commitMode.autocommit"),
                vm.commit_mode == super::CommitMode::Autocommit,
            )
            .on_select(press(on_action, ImportAction::SetCommitMode(super::CommitMode::Autocommit))),
        );
    let note = if vm.commit_mode == super::CommitMode::Transaction {
        t("import.transactionNote")
    } else {
        t("import.autocommitNote")
    };
    col()
        .key(format!("{ROOT}-execution"))
        .w_full()
        .gap(8.0)
        .child(row().w_full().gap(16.0).flex_wrap().child(on_error.build(k)).child(commit.build(k)))
        .child(text(note).color(k.c.text_weak).wrap())
}

fn press<S: 'static>(on_action: &Option<ImportActionFn<S>>, action: ImportAction) -> impl Fn(&mut S) + Clone {
    emit(on_action, action)
}

// Keep the sample-row cap referenced from this module so a future drift in the
// React constant fails loudly instead of silently changing the sample cell.
const _: () = assert!(SAMPLE_ROWS == 5);
