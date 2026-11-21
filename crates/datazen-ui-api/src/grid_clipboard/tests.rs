use serde_json::{json, Map, Value};

use super::*;

fn coord(row: i64, column: &str) -> ClipboardCellCoord {
    ClipboardCellCoord {
        row_index: row,
        column_name: column.to_owned(),
    }
}

fn rows(values: &[&[&str]]) -> Vec<Vec<Value>> {
    values
        .iter()
        .map(|row| {
            row.iter()
                .map(|value| Value::String((*value).to_owned()))
                .collect()
        })
        .collect()
}

fn record(pairs: &[(&str, Value)]) -> Map<String, Value> {
    let mut map = Map::new();
    for (key, value) in pairs {
        map.insert((*key).to_owned(), value.clone());
    }
    map
}

#[test]
fn limits_and_literals_are_pinned() {
    assert_eq!(GRID_PASTE_MAX_BYTES, 10 * 1024 * 1024);
    assert_eq!(GRID_PASTE_MAX_CELLS, 100_000);
    assert_eq!(GRID_PASTE_CONFIRM_CELLS, 2_000);
    assert_eq!(GRID_PASTE_CHUNK_CELLS, 5_000);
    assert_eq!(PasteParseErrorCode::Empty.as_str(), "grid.paste.empty");
    assert_eq!(
        PasteParseErrorCode::TooLarge.as_str(),
        "grid.paste.tooLarge"
    );
    assert_eq!(
        PasteParseErrorCode::RaggedRow.as_str(),
        "grid.paste.raggedRow"
    );
    assert_eq!(
        PasteProblemCode::UnclosedQuote.as_str(),
        "grid.paste.unclosedQuote"
    );
    assert_eq!(ClipboardDelimiter::Tab.as_str(), "\t");
    assert_eq!(ClipboardDelimiter::Comma.as_char(), ',');
    assert_eq!(ClipboardDelimiter::Semicolon.as_char(), ';');
}

#[test]
fn utf8_length_counts_code_points_exactly() {
    assert_eq!(utf8_byte_length(""), 0);
    assert_eq!(utf8_byte_length("abc"), 3);
    assert_eq!(utf8_byte_length("é中"), 5);
    assert_eq!(utf8_byte_length("🚀"), 4);
    assert_eq!(utf8_byte_length("a🚀b"), 6);
}

#[test]
fn delimiter_sniffing_prefers_tab_then_comma_then_semicolon_outside_quotes() {
    assert_eq!(
        sniff_clipboard_delimiter("a\tb,c;d"),
        ClipboardDelimiter::Tab
    );
    assert_eq!(
        sniff_clipboard_delimiter("a,b;c"),
        ClipboardDelimiter::Comma
    );
    assert_eq!(
        sniff_clipboard_delimiter("a;b"),
        ClipboardDelimiter::Semicolon
    );
    assert_eq!(sniff_clipboard_delimiter("plain"), ClipboardDelimiter::Tab);
    assert_eq!(
        sniff_clipboard_delimiter("\"a,b\",c"),
        ClipboardDelimiter::Comma
    );
    assert_eq!(
        sniff_clipboard_delimiter("\"a\tb\";c"),
        ClipboardDelimiter::Semicolon
    );
}

#[test]
fn parsing_strips_quotes_and_restores_doubled_quotes() {
    let result = parse_clipboard_matrix("a,\"b,c\",\"say \"\"hi\"\"\"");
    let ClipboardParseResult::Ok { matrix, problems } = result else {
        panic!("must parse")
    };
    assert!(problems.is_empty());
    assert_eq!(matrix.cells, vec![vec!["a", "b,c", "say \"hi\""]]);
    assert_eq!(matrix.quoted, vec![vec![false, true, true]]);
    assert_eq!(matrix.delimiter, ClipboardDelimiter::Comma);
    assert!(!matrix.multiline);
    assert_eq!(
        matrix.byte_length,
        utf8_byte_length("a,\"b,c\",\"say \"\"hi\"\"\"")
    );
}

#[test]
fn parsing_handles_crlf_and_a_trailing_break_without_a_blank_row() {
    let result = parse_clipboard_matrix("a\tb\r\nc\td\r\n");
    let ClipboardParseResult::Ok { matrix, .. } = result else {
        panic!("must parse")
    };
    assert_eq!(matrix.cells.len(), 2);
    assert_eq!(matrix.cells[0], ["a", "b"]);
    assert_eq!(matrix.cells[1], ["c", "d"]);
    assert!(matrix.multiline);

    let single = parse_clipboard_matrix("a\n");
    let ClipboardParseResult::Ok { matrix, .. } = single else {
        panic!("must parse")
    };
    assert_eq!(matrix.cells.len(), 1);
}

#[test]
fn unclosed_quote_is_a_problem_not_a_refusal() {
    let result = parse_clipboard_matrix("a,\"unclosed,b\nc");
    let ClipboardParseResult::Ok { matrix, problems } = result else {
        panic!("must parse")
    };
    assert_eq!(matrix.cells.len(), 1);
    assert_eq!(matrix.cells[0].len(), 2);
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].code, PasteProblemCode::UnclosedQuote);
    assert_eq!(problems[0].row, None);
    assert_eq!(problems[0].column, None);
}

#[test]
fn empty_bytes_and_ragged_rows_are_structured_refusals() {
    let empty = parse_clipboard_matrix("  \n");
    let ClipboardParseResult::Refused(refusal) = empty else {
        panic!("must refuse")
    };
    assert_eq!(refusal.code, PasteParseErrorCode::Empty);
    assert_eq!(refusal.cells, 0);

    let delimiter_only = parse_clipboard_matrix("\t");
    let ClipboardParseResult::Ok { matrix, .. } = delimiter_only else {
        panic!("must parse")
    };
    assert_eq!(matrix.cells, vec![vec!["", ""]]);

    let ragged = parse_clipboard_matrix("a\tb\nc");
    let ClipboardParseResult::Refused(refusal) = ragged else {
        panic!("must refuse")
    };
    assert_eq!(refusal.code, PasteParseErrorCode::RaggedRow);
    assert_eq!(refusal.row, Some(1));
    assert_eq!(refusal.expected, Some(2));
    assert_eq!(refusal.actual, Some(1));

    let huge = format!("{}x", "a".repeat(GRID_PASTE_MAX_BYTES));
    let too_large = parse_clipboard_matrix(&huge);
    let ClipboardParseResult::Refused(refusal) = too_large else {
        panic!("must refuse")
    };
    assert_eq!(refusal.code, PasteParseErrorCode::TooLarge);
    assert_eq!(refusal.cells, 0);
}

#[test]
fn range_mapping_uses_render_order_and_rejects_unknown_columns() {
    let order = vec!["id".to_owned(), "name".to_owned(), "age".to_owned()];
    let forward = to_clipboard_range(&(coord(2, "name"), coord(0, "id")), &order).unwrap();
    assert_eq!(forward.top_row, 0);
    assert_eq!(forward.bottom_row, 2);
    assert_eq!(forward.left_column, "id");
    assert_eq!(forward.right_column, "name");
    assert!(to_clipboard_range(&(coord(0, "missing"), coord(1, "id")), &order).is_none());
    assert!(to_clipboard_range(&(coord(-1, "id"), coord(1, "id")), &order).is_none());
}

#[test]
fn selection_and_row_matrices_group_by_row_and_pad_with_null() {
    let data = vec![
        record(&[("id", json!(1)), ("name", json!("a"))]),
        record(&[("id", json!(2))]),
    ];
    let coords = [
        coord(1, "id"),
        coord(0, "name"),
        coord(0, "id"),
        coord(5, "id"),
    ];
    let matrix = build_selection_matrix(&data, &coords);
    assert_eq!(matrix.len(), 3);
    assert_eq!(matrix[0], [json!(2)]);
    assert_eq!(matrix[1], [json!("a"), json!(1)]);
    assert_eq!(matrix[2], [Value::Null]);

    let by_rows = build_rows_matrix(&data, &["id".to_owned(), "name".to_owned()], &[1, -3, 0]);
    assert_eq!(by_rows.len(), 2);
    assert_eq!(by_rows[0], [json!(1), json!("a")]);
    assert_eq!(by_rows[1], [json!(2), Value::Null]);
}

#[test]
fn escape_helpers_cover_csv_html_and_markdown() {
    assert_eq!(escape_csv_field(&Value::Null), "");
    assert_eq!(escape_csv_field(&json!("a,b")), "\"a,b\"");
    assert_eq!(escape_csv_field(&json!("say \"hi\"")), "\"say \"\"hi\"\"\"");
    assert_eq!(escape_csv_field(&json!(7)), "7");
    assert_eq!(escape_markdown_field(&json!("a|b\nc")), "a\\|b<br>c");
    assert_eq!(escape_markdown_field(&json!("a\\b")), "a\\\\b");
    assert_eq!(escape_html_cell_text("a&b<c>d"), "a&amp;b&lt;c&gt;d");
    assert_eq!(escape_markdown_cell_text_value("a\r\nb\rc"), "a<br>b<br>c");
}

#[test]
fn delimited_serialization_uses_one_kernel_with_header_defaults() {
    let data = rows(&[&["a", "b\tc"], &["d", "e"]]);
    let tsv = serialize_clipboard_cells(
        &data,
        &SerializeCellsOptions::new(ClipboardFormat::Tsv, vec!["h1".to_owned(), "h2".to_owned()]),
    );
    assert_eq!(tsv, "a\t\"b\tc\"\nd\te");
    let csv = serialize_clipboard_cells(
        &data,
        &SerializeCellsOptions::new(ClipboardFormat::Csv, vec!["h1".to_owned(), "h2".to_owned()]),
    );
    assert_eq!(csv, "h1,h2\na,b\tc\nd,e");
    let plain = serialize_clipboard_cells(
        &data,
        &SerializeCellsOptions::new(ClipboardFormat::Plain, vec!["h1".to_owned()]),
    );
    assert_eq!(plain, "a\nd");
}

#[test]
fn json_serialization_supports_arrays_objects_and_indent() {
    let data = vec![vec![json!(1), Value::String("a".to_owned())]];
    let names = vec!["id".to_owned(), "name".to_owned()];
    let mut options = SerializeCellsOptions::new(ClipboardFormat::Json, names.clone());
    assert_eq!(serialize_clipboard_cells(&data, &options), "[[1,\"a\"]]");
    options.json_objects = true;
    assert_eq!(
        serialize_clipboard_cells(&data, &options),
        "[{\"id\":1,\"name\":\"a\"}]"
    );
    options.json_indent = 2;
    assert!(serialize_clipboard_cells(&data, &options).contains('\n'));
}

#[test]
fn html_and_markdown_serialization_always_write_a_header() {
    let data = rows(&[&["x", "y"]]);
    let html = serialize_clipboard_cells(
        &data,
        &SerializeCellsOptions::new(ClipboardFormat::Html, vec!["h".to_owned()]),
    );
    assert!(html.contains("<thead><tr><th>h</th></tr></thead>"));
    assert!(html.contains("<td>x</td>"));
    let markdown = serialize_clipboard_cells(
        &data,
        &SerializeCellsOptions::new(ClipboardFormat::Markdown, vec!["h".to_owned()]),
    );
    assert_eq!(markdown, "| h |\n| --- |\n| x |");
}

#[test]
fn null_literal_and_include_header_are_honored() {
    let data = vec![vec![Value::Null, Value::String(String::new())]];
    let mut options =
        SerializeCellsOptions::new(ClipboardFormat::Csv, vec!["a".to_owned(), "b".to_owned()]);
    options.null_literal = "NULL".to_owned();
    options.include_header = Some(false);
    assert_eq!(serialize_clipboard_cells(&data, &options), "NULL,");
}
