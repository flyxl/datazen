use serde_json::Value;

use crate::data_table_context_menu::{
    cell_to_text, escape_delimited, escape_html_cell, escape_markdown_cell_text,
    row_to_named_record, table_width,
};

use super::types::{ClipboardFormat, SerializeCellsOptions};

pub fn escape_csv_field(value: &Value) -> String {
    escape_delimited(&cell_to_text(value, ""), ",")
}

pub fn escape_markdown_field(value: &Value) -> String {
    escape_markdown_cell_text(&cell_to_text(value, ""))
}

pub fn escape_html_cell_text(text: &str) -> String {
    escape_html_cell(text)
}

pub fn escape_markdown_cell_text_value(text: &str) -> String {
    escape_markdown_cell_text(text)
}

pub fn serialize_clipboard_cells(rows: &[Vec<Value>], options: &SerializeCellsOptions) -> String {
    let null_literal = options.null_literal.as_str();
    if options.format == ClipboardFormat::Json {
        let payload: Vec<Value> = if options.json_objects {
            rows.iter()
                .map(|row| Value::Object(row_to_named_record(&options.column_names, row)))
                .collect()
        } else {
            rows.iter().cloned().map(Value::Array).collect()
        };
        return if options.json_indent == 0 {
            serde_json::to_string(&payload).unwrap_or_else(|_| "[]".to_owned())
        } else {
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "[]".to_owned())
        };
    }

    let width = table_width(rows, &options.column_names);
    let cells_of = |row: &[Value]| -> Vec<String> {
        (0..width)
            .map(|index| cell_to_text(row.get(index).unwrap_or(&Value::Null), null_literal))
            .collect()
    };

    if options.format == ClipboardFormat::Plain {
        let mut lines = Vec::new();
        if options.include_header == Some(true) && !options.column_names.is_empty() {
            lines.push(options.column_names.join("\t"));
        }
        for row in rows {
            lines.push(cells_of(row).join("\t"));
        }
        return lines.join("\n");
    }

    if options.format == ClipboardFormat::Html {
        let mut lines = vec!["<table>".to_owned()];
        if !options.column_names.is_empty() {
            let head: String = options
                .column_names
                .iter()
                .map(|name| format!("<th>{}</th>", escape_html_cell(name)))
                .collect();
            lines.push(format!("<thead><tr>{head}</tr></thead>"));
        }
        if !rows.is_empty() {
            lines.push("<tbody>".to_owned());
            for row in rows {
                let cells: String = cells_of(row)
                    .iter()
                    .map(|cell| format!("<td>{}</td>", escape_html_cell(cell)))
                    .collect();
                lines.push(format!("<tr>{cells}</tr>"));
            }
            lines.push("</tbody>".to_owned());
        }
        lines.push("</table>".to_owned());
        return lines.join("\n");
    }

    if options.format == ClipboardFormat::Markdown {
        let head: Vec<String> = if options.column_names.is_empty() {
            vec![String::new(); width]
        } else {
            options.column_names.clone()
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
            let cells: Vec<String> = cells_of(row)
                .iter()
                .map(|cell| escape_markdown_cell_text(cell))
                .collect();
            lines.push(format!("| {} |", cells.join(" | ")));
        }
        return lines.join("\n");
    }

    let delimiter = if options.format == ClipboardFormat::Csv {
        ","
    } else {
        "\t"
    };
    let include_header = options
        .include_header
        .unwrap_or(options.format == ClipboardFormat::Csv);
    let mut lines = Vec::new();
    if include_header {
        let header: Vec<String> = (0..width)
            .map(|index| {
                escape_delimited(
                    options.column_names.get(index).map_or("", String::as_str),
                    delimiter,
                )
            })
            .collect();
        lines.push(header.join(delimiter));
    }
    for row in rows {
        let cells: Vec<String> = cells_of(row)
            .iter()
            .map(|cell| escape_delimited(cell, delimiter))
            .collect();
        lines.push(cells.join(delimiter));
    }
    lines.join("\n")
}
