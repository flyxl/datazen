//! T132 options layer: the shared export-dialog vocabulary, ported from
//! `packages/frontend/src/lib/exportDialogShared.ts` (the parts the grid
//! `DataExportDialog` consumes) plus the `ExportScope` half of
//! `packages/frontend/src/lib/exportData.ts` (the type module is otherwise
//! retired — its implementation moved to the data fetchers, which are a host
//! concern).
//!
//! What lives here is pure: the format vocabulary in React's
//! `ARRAY_EXPORT_FORMAT_OPTIONS` order, the DBR-06 availability rules of
//! `buildExportFormatOptions`, the scope labels of `buildExportScopeOptions`,
//! and the de-duplicated disabled-format reasons the dialog paints under the
//! format select. Nothing reads app state; the dialog's VM feeds the inputs.
//!
//! ## Deliberate deviations from the React module
//!
//! * The kit `Select` has no per-option disabled state (a whole-control
//!   `disabled` only). The availability still rules: the dialog's choose
//!   handler ignores a disabled pick — the same defensive rule React applies
//!   to keyboard input on the raw select (`handleFormatChange` drops a
//!   disabled arrival) — and the disabled options' reasons stay on screen the
//!   whole time the range makes them unavailable, which is the DBR-06
//!   behaviour that matters. What is lost is only the grey-out of the row
//!   itself inside the listbox.
//! * `buildExportFormatOptions` takes the caller's option list in React
//!   (`ARRAY_` or `OBJECT_`); the grid dialog is the only Rune caller, so the
//!   list is baked in here ([`ARRAY_EXPORT_FORMATS`]) and the signature drops
//!   the parameter. The connection-level dialog, when it ports, can add its
//!   own entry point.
//! * `buildExportScopeOptions` resolves labels eagerly (`t()` plus
//!   `replace`), so it returns owned `String`s, not React's template
//!   callbacks.
//!
//! The full `lib/exportDialogShared.ts` port task (if the coordinator files
//! one) can lift these items verbatim: they are `pub` and stateless.

use crate::i18n::t;
use crate::components::connection::batch_export_dialog::DataFormat;

/// React's `ExportScope` union (`lib/exportData.ts`): what rows an export
/// writes.
#[allow(dead_code)] // wired with the DataExportDialog view (same task)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ExportScope {
    /// The rows currently loaded on the page.
    #[default]
    CurrentPage,
    /// The rows the caller marked selected.
    Selected,
    /// Every row of the table, streamed by the backend.
    EntireTable,
    /// The rows the active filters select, streamed by the backend (DB-15).
    FilteredTable,
}

#[allow(dead_code)] // wired with the DataExportDialog view (same task)
impl ExportScope {
    /// The stored value behind the option (React's `option.value`).
    pub fn value(self) -> &'static str {
        match self {
            Self::CurrentPage => "current_page",
            Self::Selected => "selected",
            Self::EntireTable => "entire_table",
            Self::FilteredTable => "filtered_table",
        }
    }

    /// React's `value as ExportScope` (the select answers with the value).
    pub fn from_value(value: &str) -> Self {
        match value {
            "selected" => Self::Selected,
            "entire_table" => Self::EntireTable,
            "filtered_table" => Self::FilteredTable,
            _ => Self::CurrentPage,
        }
    }

    /// The streaming scopes run through the backend exporter (React's
    /// `isStreamingScope`).
    pub fn is_streaming(self) -> bool {
        matches!(self, Self::EntireTable | Self::FilteredTable)
    }
}

/// React's `ARRAY_EXPORT_FORMAT_OPTIONS`: the DataTable export formats, in
/// order. Item 3d: SQL UPDATE is offered — both export dialogs pass the
/// primary key when known, and the Rust exporter falls back to the first
/// column otherwise (the old frontend rule).
#[allow(dead_code)] // wired with the DataExportDialog view (same task)
pub const ARRAY_EXPORT_FORMATS: [DataFormat; 8] = [
    DataFormat::Csv,
    DataFormat::Tsv,
    DataFormat::Json,
    DataFormat::Markdown,
    DataFormat::Xlsx,
    DataFormat::Parquet,
    DataFormat::SqlInsert,
    DataFormat::SqlUpdate,
];

/// One format entry with its availability (React's `ExportFormatOption`).
/// An unavailable format stays listed, with its reason on screen — never
/// hidden, never silently reset (DBR-06, restored for the new architecture).
#[allow(dead_code)] // wired with the DataExportDialog view (same task)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportFormatOption {
    /// The format the entry offers.
    pub format: DataFormat,
    /// Offered but unavailable. The kit listbox cannot grey the row out, so
    /// the dialog refuses the pick instead (see the module docs).
    pub disabled: bool,
    /// i18n key of the explanation shown while this format is disabled.
    pub disabled_reason_key: Option<&'static str>,
}

/// Port of `buildExportFormatOptions` for the grid dialog's format list.
///
/// The rules, in React's order (the first match wins per entry, so a
/// key-less SQL UPDATE on a web streaming scope carries the no-key reason
/// while its neighbours carry the web reason — both explanations stay on
/// screen, exactly as DBR-06 showed):
///
/// 1. SQL UPDATE without a key column: its WHERE has nothing to bind, so it
///    is disabled with `export.formatDisabledNoKey` rather than silently
///    writing `WHERE <first column> = …`.
/// 2. Parquet off the desktop streaming scopes (PRD §4 F8): the only writer
///    is the Rust batch exporter, so the provided scopes and all of web are
///    greyed out with `export.formatDisabledParquetRustOnly`.
/// 3. Web streaming scopes: the web exporter assembles CSV only, so every
///    other format is disabled with `export.formatDisabledWebSqlScope`.
///
/// XLSX is never disabled (both roads serve it: SheetJS-equivalent in-memory
/// formatting for provided scopes, the Rust writer for SQL scopes).
#[allow(dead_code)] // wired with the DataExportDialog view (same task)
pub fn build_format_options(
    scope: ExportScope,
    is_desktop: bool,
    has_key_column: bool,
) -> Vec<ExportFormatOption> {
    let streaming = scope.is_streaming();
    ARRAY_EXPORT_FORMATS
        .iter()
        .map(|&format| {
            if format == DataFormat::SqlUpdate && !has_key_column {
                return ExportFormatOption {
                    format,
                    disabled: true,
                    disabled_reason_key: Some("export.formatDisabledNoKey"),
                };
            }
            if format == DataFormat::Parquet && (!is_desktop || !streaming) {
                return ExportFormatOption {
                    format,
                    disabled: true,
                    disabled_reason_key: Some("export.formatDisabledParquetRustOnly"),
                };
            }
            if !is_desktop && streaming && format != DataFormat::Csv {
                return ExportFormatOption {
                    format,
                    disabled: true,
                    disabled_reason_key: Some("export.formatDisabledWebSqlScope"),
                };
            }
            ExportFormatOption {
                format,
                disabled: false,
                disabled_reason_key: None,
            }
        })
        .collect()
}

/// The entry a scope/format lookup lands on, if the list has it.
#[allow(dead_code)] // wired with the DataExportDialog view (same task)
pub fn format_option<'a>(
    options: &'a [ExportFormatOption],
    format: DataFormat,
) -> Option<&'a ExportFormatOption> {
    options.iter().find(|o| o.format == format)
}

/// The reasons of the disabled formats, each shown once (React's
/// `disabledFormatReasons`: a `Set` over the disabled entries, in list
/// order).
#[allow(dead_code)] // wired with the DataExportDialog view (same task)
pub fn disabled_reason_keys(options: &[ExportFormatOption]) -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = Vec::new();
    for option in options {
        if let (true, Some(key)) = (option.disabled, option.disabled_reason_key) {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
    }
    keys
}

/// One scope entry of the range select (React's option object).
#[allow(dead_code)] // wired with the DataExportDialog view (same task)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportScopeOption {
    pub scope: ExportScope,
    /// The label with its counts baked in (React builds it from `t()`).
    pub label: String,
    /// React disables the "selected rows" option when nothing is selected;
    /// the pick is refused in the dialog (the kit cannot grey the row).
    pub disabled: bool,
}

/// Port of `buildExportScopeOptions`: the range options with their live
/// counts, in React's order — page, selected, filtered, entire.
///
/// `entire` is `Some(total)` when the entire-table scope is offered (the
/// caller passed `{ totalRows }`), carrying the server's row count when it
/// reported one (`totalRows` may legitimately be unknown: `None` renders the
/// plain label without counts, React's `total == null` branch); `None` omits
/// the scope. `filtered` offers the DB-15 scope — the caller passes it only
/// when a filtered statement builder exists, so the scope never appears for a
/// table whose filtered read cannot be built.
#[allow(dead_code)] // wired with the DataExportDialog view (same task)
pub fn build_scope_options(
    row_count: usize,
    selected_count: usize,
    entire: Option<Option<u64>>,
    filtered: bool,
) -> Vec<ExportScopeOption> {
    let rows = t("common.rows");
    let mut options = vec![
        ExportScopeOption {
            scope: ExportScope::CurrentPage,
            label: format!(
                "{} ({row_count} {rows})",
                t("export.currentPage"),
            ),
            disabled: false,
        },
        ExportScopeOption {
            scope: ExportScope::Selected,
            label: format!("{} ({selected_count} {rows})", t("export.selectedRows")),
            disabled: selected_count == 0,
        },
    ];
    if filtered {
        options.push(ExportScopeOption {
            scope: ExportScope::FilteredTable,
            label: t("export.filteredTable").to_owned(),
            disabled: false,
        });
    }
    if let Some(total) = entire {
        options.push(ExportScopeOption {
            scope: ExportScope::EntireTable,
            label: match total {
                // React renders counts only for a non-negative known total.
                Some(total) => format!("{} ({total} {rows})", t("export.entireTable")),
                None => t("export.entireTable").to_owned(),
            },
            disabled: false,
        });
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(format: DataFormat, disabled: bool, key: Option<&'static str>) -> ExportFormatOption {
        ExportFormatOption {
            format,
            disabled,
            disabled_reason_key: key,
        }
    }

    #[test]
    fn the_format_vocabulary_keeps_the_react_array_order() {
        let values: Vec<&str> = ARRAY_EXPORT_FORMATS.iter().map(|f| f.value()).collect();
        assert_eq!(
            values,
            [
                "csv", "tsv", "json", "markdown", "xlsx", "parquet", "sql_insert", "sql_update"
            ]
        );
    }

    #[test]
    fn desktop_current_page_offers_every_format_but_parquet() {
        // The provided scopes have no frontend Parquet writer (PRD §4 F8),
        // so Parquet is the one greyed-out entry even here.
        let options = build_format_options(ExportScope::CurrentPage, true, true);
        assert_eq!(options.len(), 8);
        for entry in &options {
            let expected = entry.format == DataFormat::Parquet;
            assert_eq!(entry.disabled, expected, "{:?}", entry.format);
            if expected {
                assert_eq!(
                    entry.disabled_reason_key,
                    Some("export.formatDisabledParquetRustOnly")
                );
            }
        }
        // The desktop streaming scope, by contrast, serves Parquet: nothing
        // but a key-less SQL UPDATE is greyed there.
        let streaming = build_format_options(ExportScope::EntireTable, true, true);
        assert!(streaming.iter().all(|o| !o.disabled));
        assert!(disabled_reason_keys(&streaming).is_empty());
    }

    #[test]
    fn sql_update_without_a_key_column_is_disabled_with_its_reason() {
        let options = build_format_options(ExportScope::CurrentPage, true, false);
        let sql_update = format_option(&options, DataFormat::SqlUpdate);
        assert_eq!(
            sql_update,
            Some(&option(
                DataFormat::SqlUpdate,
                true,
                Some("export.formatDisabledNoKey")
            ))
        );
        // The no-key rule is specific: it wins even on a web streaming scope,
        // where the other non-CSV formats carry the web reason instead.
        let web = build_format_options(ExportScope::EntireTable, false, false);
        let web_update = format_option(&web, DataFormat::SqlUpdate).copied().unwrap_or_else(
            || ExportFormatOption {
                format: DataFormat::SqlUpdate,
                disabled: false,
                disabled_reason_key: None,
            },
        );
        assert_eq!(web_update.disabled_reason_key, Some("export.formatDisabledNoKey"));
    }

    #[test]
    fn parquet_is_rust_only_and_greyed_off_the_desktop_streaming_scopes() {
        // The desktop streaming scope serves it.
        let streaming = build_format_options(ExportScope::EntireTable, true, true);
        assert_eq!(
            format_option(&streaming, DataFormat::Parquet).map(|o| o.disabled),
            Some(false)
        );
        // A provided scope does not, even on desktop.
        let provided = build_format_options(ExportScope::CurrentPage, true, true);
        assert_eq!(
            format_option(&provided, DataFormat::Parquet).map(|o| o.disabled),
            Some(true)
        );
        // Neither does web.
        let web = build_format_options(ExportScope::EntireTable, false, true);
        assert_eq!(
            format_option(&web, DataFormat::Parquet).map(|o| o.disabled_reason_key),
            Some(Some("export.formatDisabledParquetRustOnly"))
        );
    }

    #[test]
    fn web_streaming_scopes_serve_csv_only() {
        let options = build_format_options(ExportScope::FilteredTable, false, true);
        for entry in &options {
            let expected = entry.format == DataFormat::Csv;
            assert_eq!(entry.disabled, !expected, "{:?}", entry.format);
            if expected {
                continue;
            }
            // Parquet's Rust-only rule (rule 2) wins over the web rule
            // (rule 3), the same first-match order React evaluates in.
            let expected_reason = if entry.format == DataFormat::Parquet {
                "export.formatDisabledParquetRustOnly"
            } else {
                "export.formatDisabledWebSqlScope"
            };
            assert_eq!(entry.disabled_reason_key, Some(expected_reason));
        }
    }

    #[test]
    fn xlsx_is_never_disabled_except_on_web_streaming_scopes() {
        // No rule targets XLSX itself: desktop serves it everywhere, and web
        // serves it on the provided scopes. Only web's CSV-only streaming
        // scopes (rule 3) sweep it up with every other non-CSV format.
        for scope in [ExportScope::CurrentPage, ExportScope::Selected] {
            for is_desktop in [true, false] {
                let options = build_format_options(scope, is_desktop, true);
                assert_eq!(
                    format_option(&options, DataFormat::Xlsx).map(|o| o.disabled),
                    Some(false)
                );
            }
        }
        for scope in [ExportScope::EntireTable, ExportScope::FilteredTable] {
            let options = build_format_options(scope, true, true);
            assert_eq!(
                format_option(&options, DataFormat::Xlsx).map(|o| o.disabled),
                Some(false)
            );
        }
        let web_streaming = build_format_options(ExportScope::EntireTable, false, true);
        assert_eq!(
            format_option(&web_streaming, DataFormat::Xlsx)
                .map(|o| o.disabled_reason_key),
            Some(Some("export.formatDisabledWebSqlScope"))
        );
    }

    #[test]
    fn the_reasons_are_deduplicated_in_list_order() {
        let options = vec![
            option(DataFormat::Csv, false, None),
            option(DataFormat::Parquet, true, Some("export.formatDisabledParquetRustOnly")),
            option(DataFormat::SqlInsert, true, Some("export.formatDisabledWebSqlScope")),
            option(DataFormat::SqlUpdate, true, Some("export.formatDisabledParquetRustOnly")),
        ];
        assert_eq!(
            disabled_reason_keys(&options),
            vec!["export.formatDisabledParquetRustOnly", "export.formatDisabledWebSqlScope"]
        );
    }

    #[test]
    fn scope_values_round_trip_through_the_wire_names() {
        for scope in [
            ExportScope::CurrentPage,
            ExportScope::Selected,
            ExportScope::EntireTable,
            ExportScope::FilteredTable,
        ] {
            assert_eq!(ExportScope::from_value(scope.value()), scope);
        }
        // The unknown branch falls back to the page scope, like the default.
        assert_eq!(ExportScope::from_value("nope"), ExportScope::CurrentPage);
    }

    #[test]
    fn the_page_scope_is_first_and_carries_its_live_count() {
        let options = build_scope_options(25, 3, None, false);
        assert_eq!(options.len(), 2);
        assert!(options[0].label.contains("25"));
        assert!(options[0].label.contains(t("export.currentPage")));
    }

    #[test]
    fn a_zero_selection_disables_the_selected_scope_but_keeps_it_listed() {
        let options = build_scope_options(25, 0, None, false);
        assert_eq!(options[1].scope, ExportScope::Selected);
        assert!(options[1].disabled);
        assert!(options[1].label.contains("0"));
    }

    #[test]
    fn the_filtered_and_entire_scopes_appear_only_when_offered() {
        // Neither offered.
        let base = build_scope_options(10, 2, None, false);
        assert_eq!(
            base.iter().map(|o| o.scope).collect::<Vec<_>>(),
            vec![ExportScope::CurrentPage, ExportScope::Selected]
        );
        // Both offered, with a known total.
        let both = build_scope_options(10, 2, Some(Some(1_000_000)), true);
        assert_eq!(
            both.iter().map(|o| o.scope).collect::<Vec<_>>(),
            vec![
                ExportScope::CurrentPage,
                ExportScope::Selected,
                ExportScope::FilteredTable,
                ExportScope::EntireTable
            ]
        );
        assert!(both[3].label.contains("1000000"));
        // An unknown total renders the plain entire-table label.
        let unknown = build_scope_options(10, 2, Some(None), true);
        assert_eq!(unknown[3].label, t("export.entireTable"));
    }
}
