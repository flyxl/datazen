use crate::{BatchExportDataFormat, ExportScope};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisabledReasonKey {
    NoKey,
    WebSqlScope,
    ParquetRustOnly,
}

impl DisabledReasonKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoKey => "export.formatDisabledNoKey",
            Self::WebSqlScope => "export.formatDisabledWebSqlScope",
            Self::ParquetRustOnly => "export.formatDisabledParquetRustOnly",
        }
    }
}

pub type FormatOptionSpec = (BatchExportDataFormat, &'static str);

pub const SHARED_EXPORT_FORMAT_OPTIONS: [FormatOptionSpec; 6] = [
    (BatchExportDataFormat::Csv, "CSV"),
    (BatchExportDataFormat::Json, "JSON"),
    (BatchExportDataFormat::Markdown, "Markdown"),
    (BatchExportDataFormat::Xlsx, "Excel (XLSX)"),
    (BatchExportDataFormat::Parquet, "Parquet"),
    (BatchExportDataFormat::SqlInsert, "SQL INSERT"),
];

pub const OBJECT_EXPORT_FORMAT_OPTIONS: [FormatOptionSpec; 7] = [
    (BatchExportDataFormat::Csv, "CSV"),
    (BatchExportDataFormat::Json, "JSON"),
    (BatchExportDataFormat::Markdown, "Markdown"),
    (BatchExportDataFormat::Xlsx, "Excel (XLSX)"),
    (BatchExportDataFormat::Parquet, "Parquet"),
    (BatchExportDataFormat::SqlInsert, "SQL INSERT"),
    (BatchExportDataFormat::SqlUpdate, "SQL UPDATE"),
];

pub const ARRAY_EXPORT_FORMAT_OPTIONS: [FormatOptionSpec; 8] = [
    (BatchExportDataFormat::Csv, "CSV"),
    (BatchExportDataFormat::Tsv, "TSV"),
    (BatchExportDataFormat::Json, "JSON"),
    (BatchExportDataFormat::Markdown, "Markdown"),
    (BatchExportDataFormat::Xlsx, "Excel (XLSX)"),
    (BatchExportDataFormat::Parquet, "Parquet"),
    (BatchExportDataFormat::SqlInsert, "SQL INSERT"),
    (BatchExportDataFormat::SqlUpdate, "SQL UPDATE"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportFormatOption {
    pub value: BatchExportDataFormat,
    pub label: String,
    pub disabled: bool,
    pub disabled_reason_key: Option<DisabledReasonKey>,
}

#[derive(Clone, Copy, Debug)]
pub struct BuildExportFormatOptions<'a> {
    pub options: &'a [FormatOptionSpec],
    pub scope: ExportScope,
    pub is_desktop: bool,
    pub has_key_column: bool,
}

fn is_streaming_scope(scope: ExportScope) -> bool {
    matches!(scope, ExportScope::EntireTable | ExportScope::FilteredTable)
}

pub fn build_export_format_options(opts: &BuildExportFormatOptions<'_>) -> Vec<ExportFormatOption> {
    let streaming = is_streaming_scope(opts.scope);
    opts.options
        .iter()
        .map(|(value, label)| {
            let mut out = ExportFormatOption {
                value: *value,
                label: (*label).to_owned(),
                disabled: false,
                disabled_reason_key: None,
            };
            if *value == BatchExportDataFormat::SqlUpdate && !opts.has_key_column {
                out.disabled = true;
                out.disabled_reason_key = Some(DisabledReasonKey::NoKey);
                return out;
            }
            if *value == BatchExportDataFormat::Parquet && (!opts.is_desktop || !streaming) {
                out.disabled = true;
                out.disabled_reason_key = Some(DisabledReasonKey::ParquetRustOnly);
                return out;
            }
            if !opts.is_desktop && streaming && *value != BatchExportDataFormat::Csv {
                out.disabled = true;
                out.disabled_reason_key = Some(DisabledReasonKey::WebSqlScope);
            }
            out
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeLabels {
    pub current_page: String,
    pub selected_rows: String,
    pub entire_table: String,
    pub filtered_table: String,
    pub rows: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EntireTableAvailability {
    #[default]
    Absent,
    Present { total_rows: Option<i64> },
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BuildExportScopeOptions {
    pub row_count: i64,
    pub selected_count: i64,
    pub entire_table: EntireTableAvailability,
    pub filtered_table: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportScopeOption {
    pub value: ExportScope,
    pub label: String,
    pub disabled: bool,
}

pub fn build_export_scope_options(
    labels: &ScopeLabels,
    opts: &BuildExportScopeOptions,
) -> Vec<ExportScopeOption> {
    let mut options = vec![
        ExportScopeOption {
            value: ExportScope::CurrentPage,
            label: format!("{} ({} {})", labels.current_page, opts.row_count, labels.rows),
            disabled: false,
        },
        ExportScopeOption {
            value: ExportScope::Selected,
            label: format!("{} ({} {})", labels.selected_rows, opts.selected_count, labels.rows),
            disabled: opts.selected_count == 0,
        },
    ];
    if opts.filtered_table {
        options.push(ExportScopeOption {
            value: ExportScope::FilteredTable,
            label: labels.filtered_table.clone(),
            disabled: false,
        });
    }
    if let EntireTableAvailability::Present { total_rows } = opts.entire_table {
        let label = match total_rows {
            Some(total) if total >= 0 => {
                format!("{} ({} {})", labels.entire_table, total, labels.rows)
            }
            _ => labels.entire_table.clone(),
        };
        options.push(ExportScopeOption {
            value: ExportScope::EntireTable,
            label,
            disabled: false,
        });
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> ScopeLabels {
        ScopeLabels {
            current_page: "Current page".to_owned(),
            selected_rows: "Selected rows".to_owned(),
            entire_table: "Entire table".to_owned(),
            filtered_table: "Filtered results".to_owned(),
            rows: "rows".to_owned(),
        }
    }

    fn format_opts(options: &[FormatOptionSpec], scope: ExportScope, desktop: bool, key: bool) -> Vec<ExportFormatOption> {
        build_export_format_options(&BuildExportFormatOptions {
            options,
            scope,
            is_desktop: desktop,
            has_key_column: key,
        })
    }

    #[test]
    fn reason_keys_pin_the_i18n_literals() {
        assert_eq!(DisabledReasonKey::NoKey.as_str(), "export.formatDisabledNoKey");
        assert_eq!(DisabledReasonKey::WebSqlScope.as_str(), "export.formatDisabledWebSqlScope");
        assert_eq!(DisabledReasonKey::ParquetRustOnly.as_str(), "export.formatDisabledParquetRustOnly");
    }

    #[test]
    fn the_three_catalogs_keep_their_exact_order_and_labels() {
        assert_eq!(SHARED_EXPORT_FORMAT_OPTIONS.len(), 6);
        assert_eq!(OBJECT_EXPORT_FORMAT_OPTIONS.len(), 7);
        assert_eq!(ARRAY_EXPORT_FORMAT_OPTIONS.len(), 8);
        assert_eq!(
            SHARED_EXPORT_FORMAT_OPTIONS.iter().map(|(value, label)| (*value, *label)).collect::<Vec<_>>(),
            vec![
                (BatchExportDataFormat::Csv, "CSV"),
                (BatchExportDataFormat::Json, "JSON"),
                (BatchExportDataFormat::Markdown, "Markdown"),
                (BatchExportDataFormat::Xlsx, "Excel (XLSX)"),
                (BatchExportDataFormat::Parquet, "Parquet"),
                (BatchExportDataFormat::SqlInsert, "SQL INSERT"),
            ]
        );
        assert_eq!(OBJECT_EXPORT_FORMAT_OPTIONS[6], (BatchExportDataFormat::SqlUpdate, "SQL UPDATE"));
        assert_eq!(ARRAY_EXPORT_FORMAT_OPTIONS[1], (BatchExportDataFormat::Tsv, "TSV"));
        assert_eq!(ARRAY_EXPORT_FORMAT_OPTIONS[7], (BatchExportDataFormat::SqlUpdate, "SQL UPDATE"));
    }

    #[test]
    fn desktop_streaming_with_a_key_enables_every_option() {
        let options = format_opts(&ARRAY_EXPORT_FORMAT_OPTIONS, ExportScope::EntireTable, true, true);
        assert_eq!(options.len(), 8);
        assert!(options.iter().all(|option| !option.disabled && option.disabled_reason_key.is_none()));
    }

    #[test]
    fn a_key_column_rule_disables_sql_update_even_on_desktop() {
        let options = format_opts(&OBJECT_EXPORT_FORMAT_OPTIONS, ExportScope::CurrentPage, true, false);
        let update = options.iter().find(|option| option.value == BatchExportDataFormat::SqlUpdate);
        assert_eq!(update.map(|option| option.disabled_reason_key), Some(Some(DisabledReasonKey::NoKey)));
    }

    #[test]
    fn parquet_is_enabled_only_on_desktop_streaming_scopes() {
        for scope in [ExportScope::EntireTable, ExportScope::FilteredTable] {
            let options = format_opts(&ARRAY_EXPORT_FORMAT_OPTIONS, scope, true, true);
            assert!(!options.iter().find(|option| option.value == BatchExportDataFormat::Parquet).map(|option| option.disabled).unwrap_or(true));
        }
        for scope in [ExportScope::CurrentPage, ExportScope::Selected] {
            let options = format_opts(&ARRAY_EXPORT_FORMAT_OPTIONS, scope, true, true);
            assert_eq!(
                options.iter().find(|option| option.value == BatchExportDataFormat::Parquet).and_then(|option| option.disabled_reason_key),
                Some(DisabledReasonKey::ParquetRustOnly)
            );
        }
        let web = format_opts(&ARRAY_EXPORT_FORMAT_OPTIONS, ExportScope::EntireTable, false, true);
        assert_eq!(
            web.iter().find(|option| option.value == BatchExportDataFormat::Parquet).and_then(|option| option.disabled_reason_key),
            Some(DisabledReasonKey::ParquetRustOnly)
        );
    }

    #[test]
    fn web_streaming_keeps_csv_and_disables_everything_else() {
        let options = format_opts(&ARRAY_EXPORT_FORMAT_OPTIONS, ExportScope::FilteredTable, false, true);
        let csv = options.iter().find(|option| option.value == BatchExportDataFormat::Csv);
        assert_eq!(csv.map(|option| option.disabled), Some(false));
        for option in options.iter().filter(|option| option.value != BatchExportDataFormat::Csv && option.value != BatchExportDataFormat::Parquet && option.value != BatchExportDataFormat::SqlUpdate) {
            assert_eq!(option.disabled_reason_key, Some(DisabledReasonKey::WebSqlScope), "{}", option.label);
        }
    }

    #[test]
    fn the_no_key_reason_wins_over_the_web_streaming_reason() {
        let options = format_opts(&OBJECT_EXPORT_FORMAT_OPTIONS, ExportScope::EntireTable, false, false);
        let update = options.iter().find(|option| option.value == BatchExportDataFormat::SqlUpdate);
        assert_eq!(update.and_then(|option| option.disabled_reason_key), Some(DisabledReasonKey::NoKey));
    }

    #[test]
    fn provided_scopes_on_web_keep_non_parquet_formats_enabled() {
        let options = format_opts(&ARRAY_EXPORT_FORMAT_OPTIONS, ExportScope::CurrentPage, false, true);
        let json = options.iter().find(|option| option.value == BatchExportDataFormat::Json);
        assert_eq!(json.map(|option| option.disabled), Some(false));
    }

    #[test]
    fn the_two_navigation_scopes_are_always_present() {
        let options = build_export_scope_options(&labels(), &BuildExportScopeOptions::default());
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].label, "Current page (0 rows)");
        assert_eq!(options[1].label, "Selected rows (0 rows)");
        assert!(options[1].disabled);
    }

    #[test]
    fn selected_rows_are_enabled_when_at_least_one_is_selected() {
        let options = build_export_scope_options(&labels(), &BuildExportScopeOptions { selected_count: 3, ..Default::default() });
        assert_eq!(options[1].label, "Selected rows (3 rows)");
        assert!(!options[1].disabled);
    }

    #[test]
    fn filtered_and_entire_scopes_are_added_in_order_when_offered() {
        let options = build_export_scope_options(&labels(), &BuildExportScopeOptions {
            row_count: 10,
            selected_count: 2,
            filtered_table: true,
            entire_table: EntireTableAvailability::Present { total_rows: Some(77) },
        });
        assert_eq!(options.len(), 4);
        assert_eq!(options[2].value, ExportScope::FilteredTable);
        assert_eq!(options[2].label, "Filtered results");
        assert_eq!(options[3].value, ExportScope::EntireTable);
        assert_eq!(options[3].label, "Entire table (77 rows)");
    }

    #[test]
    fn each_optional_scope_can_appear_alone() {
        let filtered = build_export_scope_options(&labels(), &BuildExportScopeOptions { filtered_table: true, ..Default::default() });
        assert_eq!(filtered.len(), 3);
        let entire = build_export_scope_options(&labels(), &BuildExportScopeOptions { entire_table: EntireTableAvailability::Present { total_rows: None }, ..Default::default() });
        assert_eq!(entire.len(), 3);
        assert_eq!(entire[2].label, "Entire table");
    }

    #[test]
    fn an_negative_unknown_total_has_no_count_in_the_label() {
        let options = build_export_scope_options(&labels(), &BuildExportScopeOptions {
            entire_table: EntireTableAvailability::Present { total_rows: Some(-1) },
            ..Default::default()
        });
        assert_eq!(options[2].label, "Entire table");
    }
}
