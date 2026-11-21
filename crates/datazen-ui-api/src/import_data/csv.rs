use serde_json::{Map, Value};

use super::types::{
    CsvParseOptions, ImportDiagnostic, ImportEmptyFieldPolicy, ImportParseError,
    ImportParseErrorKind, ImportParseResult,
};

struct ScannedCell {
    value: String,
    quoted: bool,
}

struct ScannedRecord {
    cells: Vec<ScannedCell>,
    line: usize,
}

struct Scanned {
    records: Vec<ScannedRecord>,
    unterminated_quote_at_line: Option<usize>,
}

fn scan_delimited(text: &str, delimiter: char, quote_char: char) -> Scanned {
    let chars: Vec<char> = text.chars().collect();
    let mut records = Vec::new();
    let mut cells: Vec<ScannedCell> = Vec::new();
    let mut value = String::new();
    let mut quoted = false;
    let mut in_quotes = false;
    let mut value_started = false;
    let mut line = 1usize;
    let mut record_line = 1usize;
    let mut last_was_row_break = true;
    let mut index = 0usize;

    let end_cell = |cells: &mut Vec<ScannedCell>,
                    value: &mut String,
                    quoted: &mut bool,
                    value_started: &mut bool| {
        cells.push(ScannedCell {
            value: std::mem::take(value),
            quoted: *quoted,
        });
        *quoted = false;
        *value_started = false;
    };

    while index < chars.len() {
        let ch = chars[index];
        if in_quotes {
            if ch == quote_char {
                if chars.get(index + 1) == Some(&quote_char) {
                    value.push(quote_char);
                    index += 2;
                    continue;
                }
                in_quotes = false;
                index += 1;
                continue;
            }
            if ch == '\r' || ch == '\n' {
                value.push('\n');
                line += 1;
                index += if ch == '\r' && chars.get(index + 1) == Some(&'\n') {
                    2
                } else {
                    1
                };
                continue;
            }
            value.push(ch);
            index += 1;
            continue;
        }
        if ch == quote_char && !value_started {
            in_quotes = true;
            quoted = true;
            value_started = true;
            last_was_row_break = false;
            index += 1;
            continue;
        }
        if ch == delimiter {
            end_cell(&mut cells, &mut value, &mut quoted, &mut value_started);
            last_was_row_break = false;
            index += 1;
            continue;
        }
        if ch == '\n' || ch == '\r' {
            end_cell(&mut cells, &mut value, &mut quoted, &mut value_started);
            let blank = cells.len() == 1 && !cells[0].quoted && cells[0].value.trim().is_empty();
            if !blank {
                records.push(ScannedRecord {
                    cells: std::mem::take(&mut cells),
                    line: record_line,
                });
            } else {
                cells.clear();
            }
            line += 1;
            record_line = line;
            last_was_row_break = true;
            index += if ch == '\r' && chars.get(index + 1) == Some(&'\n') {
                2
            } else {
                1
            };
            continue;
        }
        value.push(ch);
        value_started = true;
        last_was_row_break = false;
        index += 1;
    }

    if in_quotes {
        return Scanned {
            records,
            unterminated_quote_at_line: Some(record_line),
        };
    }
    if !last_was_row_break {
        end_cell(&mut cells, &mut value, &mut quoted, &mut value_started);
        let blank = cells.len() == 1 && !cells[0].quoted && cells[0].value.trim().is_empty();
        if !blank {
            records.push(ScannedRecord {
                cells,
                line: record_line,
            });
        }
    }
    Scanned {
        records,
        unterminated_quote_at_line: None,
    }
}

pub fn detect_csv_delimiter(text: &str) -> char {
    let chars: Vec<char> = text.chars().collect();
    let mut in_quotes = false;
    let mut counts = [(',', 0usize), ('\t', 0usize), (';', 0usize)];
    for ch in chars.into_iter().take(8192) {
        if ch == '"' {
            in_quotes = !in_quotes;
            continue;
        }
        if in_quotes {
            continue;
        }
        if ch == '\n' || ch == '\r' {
            break;
        }
        if let Some(entry) = counts.iter_mut().find(|entry| entry.0 == ch) {
            entry.1 += 1;
        }
    }
    let mut best = counts[0];
    for entry in counts.iter().skip(1) {
        if entry.1 > best.1 {
            best = *entry;
        }
    }
    if best.1 > 0 {
        best.0
    } else {
        ','
    }
}

fn auto_column_name(index: usize) -> String {
    format!("column_{}", index + 1)
}

pub fn dedupe_columns(raw: Vec<String>, diagnostics: &mut Vec<ImportDiagnostic>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    raw.into_iter()
        .enumerate()
        .map(|(index, name)| {
            let trimmed = name.trim();
            let candidate = if trimmed.is_empty() {
                auto_column_name(index)
            } else {
                trimmed.to_owned()
            };
            if !seen.insert(candidate.to_lowercase()) {
                diagnostics.push(ImportDiagnostic::DuplicateColumn {
                    column: candidate.clone(),
                });
            }
            candidate
        })
        .collect()
}

pub fn parse_csv_import(
    text: &str,
    options: &CsvParseOptions,
) -> Result<ImportParseResult, ImportParseError> {
    if options.quote_char.chars().count() != 1 {
        return Err(ImportParseError::new(
            ImportParseErrorKind::UnterminatedQuote,
            "quoteChar must be a single character",
        ));
    }
    let quote_char = options.quote_char.chars().next().unwrap_or('"');
    let effective_delimiter = if options.delimiter == "auto" {
        detect_csv_delimiter(text)
    } else {
        let mut chars = options.delimiter.chars();
        let first = chars.next();
        if first.is_none() || chars.next().is_some() {
            return Err(ImportParseError::new(
                ImportParseErrorKind::EmptyFile,
                "delimiter must be a single character",
            ));
        }
        first.unwrap_or(',')
    };

    let scanned = scan_delimited(text, effective_delimiter, quote_char);
    if let Some(line) = scanned.unterminated_quote_at_line {
        return Err(ImportParseError::at_line(
            ImportParseErrorKind::UnterminatedQuote,
            format!("Unterminated quoted field (started around line {line})"),
            line,
        ));
    }
    if scanned.records.is_empty() {
        return Ok(ImportParseResult::default());
    }

    let mut diagnostics = Vec::new();
    let header = if options.first_row_is_header {
        scanned.records.first()
    } else {
        None
    };
    let data = if options.first_row_is_header {
        &scanned.records[1..]
    } else {
        &scanned.records[..]
    };
    let raw_columns: Vec<String> = match header {
        Some(record) => record.cells.iter().map(|cell| cell.value.clone()).collect(),
        None => {
            let width = data
                .iter()
                .map(|record| record.cells.len())
                .max()
                .unwrap_or(0);
            (0..width).map(auto_column_name).collect()
        }
    };
    let columns = dedupe_columns(raw_columns, &mut diagnostics);

    let mut rows = Vec::new();
    let mut row_origins = Vec::new();
    for record in data {
        if record.cells.len() != columns.len() {
            diagnostics.push(ImportDiagnostic::RaggedRow {
                line: record.line,
                expected: columns.len(),
                got: record.cells.len(),
            });
        }
        let mut row = Map::new();
        for (index, column) in columns.iter().enumerate() {
            let cell = record.cells.get(index);
            let value = match cell {
                None => empty_value(options.empty_unquoted_as),
                Some(cell) if cell.value.is_empty() && !cell.quoted => {
                    empty_value(options.empty_unquoted_as)
                }
                Some(cell) => Value::String(cell.value.clone()),
            };
            row.insert(column.clone(), value);
        }
        rows.push(row);
        row_origins.push(record.line);
    }

    Ok(ImportParseResult {
        columns,
        rows,
        row_origins,
        diagnostics,
    })
}

fn empty_value(policy: ImportEmptyFieldPolicy) -> Value {
    match policy {
        ImportEmptyFieldPolicy::Null => Value::Null,
        ImportEmptyFieldPolicy::Empty => Value::String(String::new()),
    }
}
