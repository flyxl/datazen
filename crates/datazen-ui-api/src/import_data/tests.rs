use serde_json::{json, Map, Value};

use super::*;
use crate::DatabaseRegistry;

fn csv_options() -> CsvParseOptions {
    CsvParseOptions::default()
}

fn record(pairs: &[(&str, Value)]) -> Map<String, Value> {
    let mut map = Map::new();
    for (key, value) in pairs {
        map.insert((*key).to_owned(), value.clone());
    }
    map
}

#[test]
fn literals_and_format_detection_are_pinned() {
    assert_eq!(ImportFormat::Csv.as_str(), "csv");
    assert_eq!(ImportFormat::Json.as_str(), "json");
    assert_eq!(ImportFormat::Xlsx.as_str(), "xlsx");
    assert_eq!(
        ImportParseErrorKind::UnterminatedQuote.as_str(),
        "unterminated-quote"
    );
    assert_eq!(ImportParseErrorKind::InvalidJson.as_str(), "invalid-json");
    assert_eq!(ImportParseErrorKind::JsonShape.as_str(), "json-shape");
    assert_eq!(ImportParseErrorKind::EmptyFile.as_str(), "empty-file");
    assert_eq!(
        ImportParseErrorKind::UnsupportedSheet.as_str(),
        "unsupported-sheet"
    );
    assert_eq!(ImportEmptyFieldPolicy::Null.as_str(), "null");
    assert_eq!(ImportEmptyFieldPolicy::Empty.as_str(), "empty");
    assert_eq!(IMPORT_ENCODINGS.len(), 6);
    assert_eq!(detect_import_format("DATA.JSON"), Some(ImportFormat::Json));
    assert_eq!(detect_import_format("x.Csv"), Some(ImportFormat::Csv));
    assert_eq!(detect_import_format("a.xlsx"), Some(ImportFormat::Xlsx));
    assert_eq!(detect_import_format("a.txt"), None);
}

#[test]
fn delimiter_sniffing_ignores_quotes_and_stops_at_the_first_line() {
    assert_eq!(detect_csv_delimiter("a,b,c\n1;2;3"), ',');
    assert_eq!(detect_csv_delimiter("a\tb\n1,2"), '\t');
    assert_eq!(detect_csv_delimiter("a;b\n1,2"), ';');
    assert_eq!(detect_csv_delimiter("\"a,b\";c"), ';');
    assert_eq!(detect_csv_delimiter("plain"), ',');
}

#[test]
fn csv_parsing_uses_the_header_and_preserves_physical_lines() {
    let result = parse_csv_import("name,age\r\nalice,30\r\nbob,40", &csv_options()).unwrap();
    assert_eq!(result.columns, ["name", "age"]);
    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0]["name"], json!("alice"));
    assert_eq!(result.rows[0]["age"], json!("30"));
    assert_eq!(result.row_origins, [2, 3]);
    assert!(result.diagnostics.is_empty());
}

#[test]
fn csv_quoted_newlines_and_crlf_are_field_content() {
    let result = parse_csv_import("name,note\nalice,\"a\r\nb\"\n", &csv_options()).unwrap();
    assert_eq!(result.rows[0]["note"], json!("a\nb"));
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.row_origins, [2]);
}

#[test]
fn csv_blank_lines_and_trailing_breaks_do_not_add_rows() {
    let result = parse_csv_import("a,b\n\n1,2\n   \n3,4\n", &csv_options()).unwrap();
    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.row_origins, [3, 5]);
    assert_eq!(
        parse_csv_import("a,b\n", &csv_options())
            .unwrap()
            .rows
            .len(),
        0
    );
    assert!(parse_csv_import("", &csv_options())
        .unwrap()
        .columns
        .is_empty());
}

#[test]
fn csv_empty_field_policy_distinguishes_unquoted_empty_from_quoted() {
    let result = parse_csv_import("a,b\n,\"\"", &csv_options()).unwrap();
    assert!(result.rows[0]["a"].is_null());
    assert_eq!(result.rows[0]["b"], json!(""));
    let empty_policy = CsvParseOptions {
        empty_unquoted_as: ImportEmptyFieldPolicy::Empty,
        ..csv_options()
    };
    let result = parse_csv_import("a,b\n,", &empty_policy).unwrap();
    assert_eq!(result.rows[0]["a"], json!(""));
    assert_eq!(result.rows[0]["b"], json!(""));
}

#[test]
fn csv_missing_cells_use_the_policy_and_report_ragged_rows() {
    let result = parse_csv_import("a,b,c\n1", &csv_options()).unwrap();
    assert!(result.rows[0]["b"].is_null());
    assert!(result.rows[0]["c"].is_null());
    assert_eq!(
        result.diagnostics,
        vec![ImportDiagnostic::RaggedRow {
            line: 2,
            expected: 3,
            got: 1
        }]
    );
}

#[test]
fn csv_duplicate_columns_are_diagnosed_without_renaming() {
    let result = parse_csv_import("Name,name\n1,2", &csv_options()).unwrap();
    assert_eq!(result.columns, ["Name", "name"]);
    assert_eq!(
        result.diagnostics,
        vec![ImportDiagnostic::DuplicateColumn {
            column: "name".to_owned()
        }]
    );
    let no_header = CsvParseOptions {
        first_row_is_header: false,
        ..csv_options()
    };
    assert_eq!(
        parse_csv_import("a;b", &no_header).unwrap().columns,
        ["column_1", "column_2"]
    );
}

#[test]
fn csv_unterminated_quote_and_bad_options_are_fatal() {
    let error = parse_csv_import("a,b\n1,\"oops", &csv_options()).unwrap_err();
    assert_eq!(error.kind, ImportParseErrorKind::UnterminatedQuote);
    assert_eq!(error.line, Some(2));

    let bad_quote = CsvParseOptions {
        quote_char: "''".to_owned(),
        ..csv_options()
    };
    let error = parse_csv_import("a,b", &bad_quote).unwrap_err();
    assert_eq!(error.kind, ImportParseErrorKind::UnterminatedQuote);
    assert_eq!(error.message, "quoteChar must be a single character");

    let bad_delim = CsvParseOptions {
        delimiter: "||".to_owned(),
        ..csv_options()
    };
    let error = parse_csv_import("a,b", &bad_delim).unwrap_err();
    assert_eq!(error.kind, ImportParseErrorKind::EmptyFile);
    assert_eq!(error.message, "delimiter must be a single character");
}

#[test]
fn json_import_accepts_arrays_and_single_objects() {
    let result = parse_json_import("[{\"a\":1},{\"b\":2,\"a\":3}]").unwrap();
    assert_eq!(result.columns, ["a", "b"]);
    assert_eq!(result.rows[0]["b"], Value::Null);
    assert_eq!(result.rows[1]["a"], json!(3));
    assert_eq!(result.row_origins, [0, 0]);
    let single = parse_json_import("{\"only\":true}").unwrap();
    assert_eq!(single.columns, ["only"]);
    assert_eq!(single.rows.len(), 1);
}

#[test]
fn json_import_rejects_bad_json_and_bad_shapes() {
    let error = parse_json_import("not json").unwrap_err();
    assert_eq!(error.kind, ImportParseErrorKind::InvalidJson);
    let error = parse_json_import("42").unwrap_err();
    assert_eq!(error.kind, ImportParseErrorKind::JsonShape);
    assert_eq!(error.message, "JSON payload must be an object or an array");
    let error = parse_json_import("[1,2]").unwrap_err();
    assert_eq!(error.kind, ImportParseErrorKind::JsonShape);
    assert_eq!(error.message, "JSON array items must all be objects");
    assert!(parse_json_import("[]").unwrap().rows.is_empty());
}

#[test]
fn xlsx_import_uses_the_injected_reader_and_sheet_choice() {
    let reader = |_: &str| -> Result<XlsxWorkbook, ImportParseError> {
        Ok(XlsxWorkbook {
            sheet_names: vec!["First".to_owned(), "Second".to_owned()],
            sheets: std::collections::HashMap::from([(
                "Second".to_owned(),
                vec![record(&[("a", json!(1))])],
            )]),
        })
    };
    let options = XlsxParseOptions {
        sheet_name: Some("Second".to_owned()),
    };
    let result = parse_xlsx_import("ignored", &options, &reader).unwrap();
    assert_eq!(result.chosen_sheet.as_deref(), Some("Second"));
    assert_eq!(result.sheet_names, ["First", "Second"]);
    assert_eq!(result.rows[0]["a"], json!(1));
    assert_eq!(result.row_origins, [0]);

    let bad = XlsxParseOptions {
        sheet_name: Some("Missing".to_owned()),
    };
    let error = parse_xlsx_import("ignored", &bad, &reader).unwrap_err();
    assert_eq!(error.kind, ImportParseErrorKind::UnsupportedSheet);
    assert_eq!(error.message, "No worksheet named \"Missing\"");

    let first = parse_xlsx("ignored", &reader).unwrap();
    assert_eq!(first.rows.len(), 0);
}

#[test]
fn xlsx_import_reports_empty_and_missing_workbooks() {
    let empty = |_: &str| Ok::<_, ImportParseError>(XlsxWorkbook::default());
    let error = parse_xlsx("x", &empty).unwrap_err();
    assert_eq!(error.kind, ImportParseErrorKind::UnsupportedSheet);
    assert_eq!(error.message, "The workbook has no worksheets");

    let failing = |_: &str| {
        Err::<XlsxWorkbook, _>(ImportParseError::new(
            ImportParseErrorKind::EmptyFile,
            "unreadable",
        ))
    };
    assert_eq!(parse_xlsx("x", &failing).unwrap_err().message, "unreadable");
}

#[test]
fn decoding_uses_utf8_by_default_and_reports_gaps() {
    let decoder = Utf8TextDecoder;
    assert_eq!(
        decode_import_text(&[0x41, 0xF0, 0x9F, 0x9A, 0x80], "utf-8", &decoder).unwrap(),
        "A🚀"
    );
    assert_eq!(is_supported_encoding("gbk"), true);
    let error = decode_import_text(&[0x41], "gbk", &decoder).unwrap_err();
    assert_eq!(error.kind, ImportParseErrorKind::UnsupportedEncoding);
    let error = decode_import_text(&[0x41], "weird", &decoder).unwrap_err();
    assert_eq!(error.kind, ImportParseErrorKind::UnsupportedEncoding);
    let b64 = crate::bytes_to_base64(&[0x41, 0x42]);
    assert_eq!(decode_import_base64(&b64, "utf-8", &decoder).unwrap(), "AB");
    let error = decode_import_base64("!!!", "utf-8", &decoder).unwrap_err();
    assert_eq!(error.kind, ImportParseErrorKind::InvalidBase64);

    struct UpperDecoder;
    impl ImportTextDecoder for UpperDecoder {
        fn decode(&self, bytes: &[u8], _: &str) -> Result<String, ImportParseError> {
            Ok(String::from_utf8_lossy(bytes).to_uppercase())
        }
    }
    assert_eq!(
        decode_import_base64(&b64, "utf-8", &UpperDecoder).unwrap(),
        "AB"
    );
}

#[test]
fn insert_sql_uses_the_dialect_registry_and_empty_rows() {
    let registry = DatabaseRegistry::default();
    let data = ParsedData {
        columns: vec!["id".to_owned(), "name".to_owned()],
        rows: vec![
            record(&[("id", json!(1)), ("name", json!("O'Reilly"))]),
            record(&[("id", json!(2)), ("name", json!("b"))]),
        ],
    };
    let sql = generate_insert_sql("t", &data, None, &registry);
    assert_eq!(
        sql,
        "INSERT INTO \"t\" (\"id\", \"name\") VALUES (1, 'O''Reilly');\nINSERT INTO \"t\" (\"id\", \"name\") VALUES (2, 'b');"
    );
    assert_eq!(
        generate_insert_sql("t", &ParsedData::default(), None, &registry),
        ""
    );
    let mut mysql_registry = DatabaseRegistry::default();
    let mut mysql_meta = crate::database_types::DatabaseTypeMeta::default();
    mysql_meta.quote_char = "`".to_owned();
    mysql_registry.register_metas(vec![("mysql".to_owned(), mysql_meta)]);
    let mysql = generate_insert_sql("t", &data, Some("mysql"), &mysql_registry);
    assert!(mysql.contains("`t`"));
}

#[test]
fn parse_import_data_dispatches_by_format() {
    let data = parse_import_data("a\n1", ImportFormat::Csv, &csv_options()).unwrap();
    assert_eq!(data.columns, ["a"]);
    let data = parse_import_data("{\"a\":1}", ImportFormat::Json, &csv_options()).unwrap();
    assert_eq!(data.rows[0]["a"], json!(1));
    let error = parse_import_data("x", ImportFormat::Xlsx, &csv_options()).unwrap_err();
    assert_eq!(error.kind, ImportParseErrorKind::UnsupportedSheet);
}
