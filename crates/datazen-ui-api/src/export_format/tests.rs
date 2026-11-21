use serde_json::{json, Value};

use crate::ExportDataFormat;

use super::*;

fn options() -> FormatRowsOptions {
    FormatRowsOptions {
        table_name: "users".to_owned(),
        columns: vec![ExportColumn::named("id"), ExportColumn::named("name")],
        rows: vec![
            vec![Value::Number(1.into()), Value::String("alice".to_owned())],
            vec![Value::Number(2.into()), Value::String("bob".to_owned())],
        ],
        database_type: None,
        primary_key: None,
    }
}

fn text(format: ExportDataFormat) -> TextExportResult {
    format_rows_as_text(&options(), format).unwrap()
}

#[test]
fn extensions_filenames_and_mime_types_are_pinned() {
    let cases = [
        (ExportDataFormat::Csv, "csv", "text/csv"),
        (ExportDataFormat::Tsv, "tsv", "text/tab-separated-values"),
        (ExportDataFormat::Json, "json", "application/json"),
        (ExportDataFormat::Markdown, "md", "text/markdown"),
        (ExportDataFormat::SqlInsert, "sql", "text/sql"),
        (ExportDataFormat::SqlUpdate, "sql", "text/sql"),
    ];
    for (format, extension, mime) in cases {
        assert_eq!(format_extension(format), extension);
        assert_eq!(text(format).extension, extension);
        assert_eq!(text(format).mime_type, mime);
        assert_eq!(
            build_provided_export_filename("orders", format),
            format!("orders.{extension}")
        );
    }
    assert_eq!(format_extension(ExportDataFormat::Xlsx), "xlsx");
    assert_eq!(format_extension(ExportDataFormat::Parquet), "parquet");
    assert_eq!(text_mime_type(ExportDataFormat::Xlsx), None);
    assert_eq!(text_mime_type(ExportDataFormat::Parquet), None);
    assert_eq!(
        build_provided_export_filename("orders", ExportDataFormat::Xlsx),
        "orders.xlsx"
    );
}

#[test]
fn binary_formats_are_rejected_by_the_text_path() {
    for format in [ExportDataFormat::Xlsx, ExportDataFormat::Parquet] {
        assert_eq!(
            format_rows_as_text(&options(), format).unwrap_err(),
            ExportFormatError::BinaryFormatNotText(format)
        );
    }
    assert_eq!(
        ExportFormatError::NoColumns.as_str(),
        "export needs at least one column"
    );
}

#[test]
fn csv_formula_guard_quotes_only_bare_formula_prefixes() {
    assert_eq!(apply_csv_formula_guard("=SUM(A1)"), "\"=SUM(A1)\"");
    assert_eq!(apply_csv_formula_guard("+1"), "\"+1\"");
    assert_eq!(apply_csv_formula_guard("-5"), "\"-5\"");
    assert_eq!(apply_csv_formula_guard("@cmd"), "\"@cmd\"");
    assert_eq!(apply_csv_formula_guard("\"a,b\""), "\"a,b\"");
    assert_eq!(apply_csv_formula_guard("plain"), "plain");
    assert_eq!(apply_csv_formula_guard(" =SUM(A1)"), " =SUM(A1)");
}

#[test]
fn csv_output_escapes_cells_and_applies_the_guard() {
    let mut source = options();
    source.rows[0][1] = Value::String("=cmd".to_owned());
    source.rows[1][1] = Value::String("a,b".to_owned());
    let result = format_rows_as_text(&source, ExportDataFormat::Csv).unwrap();
    assert_eq!(result.content, "id,name\n1,\"=cmd\"\n2,\"a,b\"");
}

#[test]
fn csv_missing_cells_are_empty_and_extras_are_ignored() {
    let mut source = options();
    source.rows[0] = vec![Value::Number(9.into())];
    source.rows[1].push(Value::String("extra".to_owned()));
    let content = format_rows_as_text(&source, ExportDataFormat::Csv)
        .unwrap()
        .content;
    assert_eq!(content, "id,name\n9,\n2,bob");
}

#[test]
fn tsv_replaces_whitespace_and_keeps_a_header() {
    let mut source = options();
    source.rows[0][1] = Value::String("a\tb\nc\rd".to_owned());
    source.rows[1] = vec![Value::Number(2.into())];
    let content = format_rows_as_text(&source, ExportDataFormat::Tsv)
        .unwrap()
        .content;
    assert_eq!(content, "id\tname\n1\ta b cd\n2\t");
}

#[test]
fn json_output_is_pretty_records_with_null_padding() {
    let mut source = options();
    source.rows[1] = vec![Value::Number(2.into())];
    let content = format_rows_as_text(&source, ExportDataFormat::Json)
        .unwrap()
        .content;
    assert_eq!(
        content,
        serde_json::to_string_pretty(&json!([
            {"id": 1, "name": "alice"},
            {"id": 2, "name": null}
        ]))
        .unwrap()
    );
}

#[test]
fn markdown_uses_the_shared_serializer() {
    let content = text(ExportDataFormat::Markdown).content;
    assert!(content.contains("| id | name |"));
    assert!(content.contains("| --- | --- |"));
    assert!(content.contains("| 1 | alice |"));
}

#[test]
fn sql_insert_wraps_batches_in_a_transaction() {
    let sql = text(ExportDataFormat::SqlInsert).content;
    assert_eq!(
        sql,
        "BEGIN;\nINSERT INTO \"users\" (\"id\", \"name\") VALUES\n  (1, 'alice'),\n  (2, 'bob');\nCOMMIT;"
    );
    let empty = FormatRowsOptions {
        rows: Vec::new(),
        ..options()
    };
    assert_eq!(
        format_rows_as_text(&empty, ExportDataFormat::SqlInsert)
            .unwrap()
            .content,
        "BEGIN;\nCOMMIT;"
    );
    assert_eq!(SQL_INSERT_BATCH_SIZE, 500);
    let batched = format_sql_insert_script(
        "t",
        &["id".to_owned()],
        &[vec![json!(1)], vec![json!(2)], vec![json!(3)]],
        None,
        2,
    );
    assert!(batched.contains("  (1),\n  (2);"));
    assert!(batched.contains("  (3);"));
    assert_eq!(batched.matches("INSERT INTO").count(), 2);
}

#[test]
fn sqlserver_and_mysql_dialects_are_distinct() {
    let mut source = options();
    source.database_type = Some("sqlserver".to_owned());
    assert_eq!(
        format_rows_as_text(&source, ExportDataFormat::SqlInsert).unwrap().content,
        "BEGIN TRANSACTION;\nINSERT INTO [users] ([id], [name]) VALUES\n  (1, 'alice'),\n  (2, 'bob');\nCOMMIT;"
    );
    source.database_type = Some("mysql".to_owned());
    source.rows[0][1] = Value::String("O'Reilly".to_owned());
    assert_eq!(
        format_rows_as_text(&source, ExportDataFormat::SqlInsert).unwrap().content,
        "BEGIN;\nINSERT INTO `users` (`id`, `name`) VALUES\n  (1, 'O''Reilly'),\n  (2, 'bob');\nCOMMIT;"
    );
    source.database_type = Some("redis".to_owned());
    assert_eq!(sql_quote_char(Some("redis")), "");
    assert_eq!(escape_sql_ident("col", Some("redis")), "col");
    source.database_type = Some("kiwi".to_owned());
    assert_eq!(sql_quote_char(Some("kiwi")), "`");
    assert_eq!(escape_sql_ident("kiwi", Some("kiwi")), "`kiwi`");
}

#[test]
fn literal_escaping_covers_both_scanners() {
    assert_eq!(sql_quote_char(None), "\"");
    assert_eq!(escape_sql_ident("a\"b", None), "\"a\"\"b\"");
    assert_eq!(escape_sql_ident("a]b", Some("sqlserver")), "[a]]b]");
    assert_eq!(default_escape_sql_literal(&Value::Null), "NULL");
    assert_eq!(default_escape_sql_literal(&json!(3)), "3");
    assert_eq!(default_escape_sql_literal(&json!(true)), "TRUE");
    assert_eq!(default_escape_sql_literal(&json!("x'y")), "'x''y'");
    assert_eq!(
        mysql_escape_sql_literal(&json!("a\\b\"c'd")),
        "'a\\\\b\\\"c''d'"
    );
    assert_eq!(
        escape_sql_literal(&json!("a\\b"), Some("mysql")),
        "'a\\\\b'"
    );
    assert_eq!(escape_sql_literal(&json!("a\\b"), Some("kiwi")), "'a\\b'");
}

#[test]
fn sql_update_chooses_the_key_and_omits_it_from_set() {
    let mut source = options();
    source.primary_key = Some("name".to_owned());
    let content = format_rows_as_text(&source, ExportDataFormat::SqlUpdate)
        .unwrap()
        .content;
    assert_eq!(
        content,
        "BEGIN;\nUPDATE \"users\" SET \"id\" = 1 WHERE \"name\" = 'alice';\nUPDATE \"users\" SET \"id\" = 2 WHERE \"name\" = 'bob';\nCOMMIT;"
    );
    source.primary_key = None;
    source.columns[1].is_primary_key = true;
    assert_eq!(
        update_key_column(&["id".to_owned(), "name".to_owned()], &source.columns, None).as_deref(),
        Some("name")
    );
    source.columns = vec![ExportColumn::named("only")];
    assert_eq!(
        format_rows_as_text(&source, ExportDataFormat::SqlUpdate).unwrap_err(),
        ExportFormatError::SqlUpdateNeedsNonKeyColumn
    );
}

#[test]
fn xlsx_path_is_an_explicit_gap_and_cell_mapping_is_pure() {
    assert_eq!(
        format_rows_as_xlsx(&options(), Some(ExportDataFormat::Xlsx)).unwrap_err(),
        ExportFormatError::XlsxWriterUnavailable
    );
    assert_eq!(
        format_rows_as_xlsx(&options(), Some(ExportDataFormat::Csv)).unwrap_err(),
        ExportFormatError::XlsxFormatRejected(ExportDataFormat::Csv)
    );
    assert_eq!(
        XLSX_MIME_TYPE,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    );
    let rows = xlsx_sheet_rows(&options()).unwrap();
    assert_eq!(rows[0]["id"], XlsxCellValue::Number("1".to_owned()));
    assert_eq!(rows[0]["name"], XlsxCellValue::Text("alice".to_owned()));
    assert_eq!(to_xlsx_cell(&Value::Null), XlsxCellValue::Blank);
    assert_eq!(to_xlsx_cell(&json!(true)), XlsxCellValue::Bool(true));
    assert_eq!(
        to_xlsx_cell(&json!({"a": 1})),
        XlsxCellValue::ObjectJson("{\"a\":1}".to_owned())
    );
    assert_eq!(
        bytes_to_xlsx_cell(&[0, 15, 255]),
        XlsxCellValue::BytesHex("000fff".to_owned())
    );
}

#[test]
fn empty_columns_are_rejected_everywhere() {
    let empty = FormatRowsOptions {
        columns: Vec::new(),
        ..options()
    };
    for format in [
        ExportDataFormat::Csv,
        ExportDataFormat::SqlInsert,
        ExportDataFormat::SqlUpdate,
    ] {
        assert_eq!(
            format_rows_as_text(&empty, format).unwrap_err(),
            ExportFormatError::NoColumns
        );
    }
    assert_eq!(
        xlsx_sheet_rows(&empty).unwrap_err(),
        ExportFormatError::NoColumns
    );
}
