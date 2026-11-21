pub type ExportDataFormat = crate::BatchExportDataFormat;
pub type ExportDataScope = crate::ExportScope;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_alias_pins_all_eight_wire_literals() {
        let cases = [
            (ExportDataFormat::Csv, "csv"),
            (ExportDataFormat::Tsv, "tsv"),
            (ExportDataFormat::Json, "json"),
            (ExportDataFormat::Markdown, "markdown"),
            (ExportDataFormat::Xlsx, "xlsx"),
            (ExportDataFormat::Parquet, "parquet"),
            (ExportDataFormat::SqlInsert, "sql_insert"),
            (ExportDataFormat::SqlUpdate, "sql_update"),
        ];
        for (value, literal) in cases {
            assert_eq!(value.as_str(), literal);
            assert_eq!(value.to_string(), literal);
        }
    }

    #[test]
    fn scope_alias_pins_all_four_wire_literals_and_provided_split() {
        let cases = [
            (ExportDataScope::CurrentPage, "current_page", true),
            (ExportDataScope::Selected, "selected", true),
            (ExportDataScope::EntireTable, "entire_table", false),
            (ExportDataScope::FilteredTable, "filtered_table", false),
        ];
        for (value, literal, provided) in cases {
            assert_eq!(value.as_str(), literal);
            assert_eq!(value.is_provided(), provided);
        }
    }

    #[test]
    fn aliases_are_identical_to_the_canonical_definitions() {
        let format: ExportDataFormat = crate::BatchExportDataFormat::Csv;
        let scope: ExportDataScope = crate::ExportScope::CurrentPage;
        assert_eq!(format.as_str(), "csv");
        assert_eq!(scope.as_str(), "current_page");
        let _: crate::BatchExportDataFormat = format;
        let _: crate::ExportScope = scope;
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _: Option<crate::ExportDataFormat> = None;
        let _: Option<crate::ExportDataScope> = None;
    }
}
