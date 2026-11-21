use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerializeFormat {
    Tsv,
    Csv,
    Json,
    Plain,
    Html,
    Markdown,
}

fn cell_to_text(value: &Value, null_literal: &str) -> String {
    match value {
        Value::Null => null_literal.to_owned(),
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Array(_) | Value::Object(_) => {
            serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
        }
    }
}

fn table_width(rows: &[Vec<Value>], column_names: &[String]) -> usize {
    if !column_names.is_empty() {
        return column_names.len();
    }
    rows.iter().map(Vec::len).max().unwrap_or(0)
}

fn escape_delimited(text: &str, delimiter: &str) -> String {
    if text.contains(delimiter) || text.contains('"') || text.contains('\n') || text.contains('\r')
    {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

fn escape_html_cell(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_markdown_cell_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "<br>")
}

pub fn row_to_named_record(column_names: &[String], row: &[Value]) -> Map<String, Value> {
    let mut record = Map::new();
    for (index, name) in column_names.iter().enumerate() {
        if !name.is_empty() {
            record.insert(name.clone(), row.get(index).cloned().unwrap_or(Value::Null));
        }
    }
    record
}

pub fn serialize_cells(
    rows: &[Vec<Value>],
    format: SerializeFormat,
    column_names: &[String],
    include_header: bool,
) -> String {
    const NULL_LITERAL: &str = "";
    if format == SerializeFormat::Json {
        let payload: Vec<Value> = rows
            .iter()
            .map(|row| Value::Object(row_to_named_record(column_names, row)))
            .collect();
        return serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "[]".to_owned());
    }

    let width = table_width(rows, column_names);
    if format == SerializeFormat::Plain {
        let mut lines = Vec::new();
        if include_header && !column_names.is_empty() {
            lines.push(column_names.join("\t"));
        }
        for row in rows {
            let cells: Vec<String> = (0..width)
                .map(|index| cell_to_text(row.get(index).unwrap_or(&Value::Null), NULL_LITERAL))
                .collect();
            lines.push(cells.join("\t"));
        }
        return lines.join("\n");
    }

    if format == SerializeFormat::Html {
        let mut lines = vec!["<table>".to_owned()];
        if !column_names.is_empty() {
            let head: String = column_names
                .iter()
                .map(|name| format!("<th>{}</th>", escape_html_cell(name)))
                .collect();
            lines.push(format!("<thead><tr>{head}</tr></thead>"));
        }
        if !rows.is_empty() {
            lines.push("<tbody>".to_owned());
            for row in rows {
                let cells: String = (0..width)
                    .map(|index| {
                        let text =
                            cell_to_text(row.get(index).unwrap_or(&Value::Null), NULL_LITERAL);
                        format!("<td>{}</td>", escape_html_cell(&text))
                    })
                    .collect();
                lines.push(format!("<tr>{cells}</tr>"));
            }
            lines.push("</tbody>".to_owned());
        }
        lines.push("</table>".to_owned());
        return lines.join("\n");
    }

    if format == SerializeFormat::Markdown {
        let head: Vec<String> = if column_names.is_empty() {
            vec![String::new(); width]
        } else {
            column_names.to_vec()
        };
        let head_cells: Vec<String> = head
            .iter()
            .map(|name| escape_markdown_cell_text(name))
            .collect();
        let separator: Vec<&str> = head.iter().map(|_| "---").collect();
        let mut lines = vec![
            format!("| {} |", head_cells.join(" | ")),
            format!("| {} |", separator.join(" | ")),
        ];
        for row in rows {
            let cells: Vec<String> = (0..width)
                .map(|index| {
                    let text = cell_to_text(row.get(index).unwrap_or(&Value::Null), NULL_LITERAL);
                    escape_markdown_cell_text(&text)
                })
                .collect();
            lines.push(format!("| {} |", cells.join(" | ")));
        }
        return lines.join("\n");
    }

    let delimiter = if format == SerializeFormat::Csv {
        ","
    } else {
        "\t"
    };
    let mut lines = Vec::new();
    if include_header {
        let header: Vec<String> = (0..width)
            .map(|index| {
                escape_delimited(
                    column_names.get(index).map_or("", String::as_str),
                    delimiter,
                )
            })
            .collect();
        lines.push(header.join(delimiter));
    }
    for row in rows {
        let cells: Vec<String> = (0..width)
            .map(|index| {
                let text = cell_to_text(row.get(index).unwrap_or(&Value::Null), NULL_LITERAL);
                escape_delimited(&text, delimiter)
            })
            .collect();
        lines.push(cells.join(delimiter));
    }
    lines.join("\n")
}

pub fn serialize_data_table_rows_as_tsv(rows: &[Vec<Value>]) -> String {
    serialize_cells(rows, SerializeFormat::Tsv, &[], false)
}

pub fn serialize_data_table_rows_as_csv(column_names: &[String], rows: &[Vec<Value>]) -> String {
    serialize_cells(rows, SerializeFormat::Csv, column_names, true)
}

pub fn serialize_data_table_rows_as_json(column_names: &[String], rows: &[Vec<Value>]) -> String {
    serialize_cells(rows, SerializeFormat::Json, column_names, false)
}

pub fn serialize_data_table_column_values(column_index: i64, rows: &[Vec<Value>]) -> String {
    if column_index < 0 {
        return String::new();
    }
    let index = column_index as usize;
    let column: Vec<Vec<Value>> = rows
        .iter()
        .map(|row| vec![row.get(index).cloned().unwrap_or(Value::Null)])
        .collect();
    serialize_cells(&column, SerializeFormat::Tsv, &[String::new()], false)
}
