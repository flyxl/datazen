use super::types::{
    ClipboardDelimiter, ClipboardMatrix, ClipboardParseResult, PasteParseErrorCode,
    PasteParseRefusal, PasteProblem, PasteProblemCode, GRID_PASTE_MAX_BYTES, GRID_PASTE_MAX_CELLS,
};

pub fn utf8_byte_length(text: &str) -> usize {
    text.chars().map(char::len_utf8).sum()
}

pub fn sniff_clipboard_delimiter(text: &str) -> ClipboardDelimiter {
    let chars: Vec<char> = text.chars().collect();
    let (mut tabs, mut commas, mut semicolons) = (0usize, 0usize, 0usize);
    let mut in_quotes = false;
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if in_quotes {
            if ch == '"' {
                if chars.get(index + 1) == Some(&'"') {
                    index += 2;
                    continue;
                }
                in_quotes = false;
            }
            index += 1;
            continue;
        }
        if ch == '"' {
            in_quotes = true;
            index += 1;
            continue;
        }
        match ch {
            '\t' => tabs += 1,
            ',' => commas += 1,
            ';' => semicolons += 1,
            _ => {}
        }
        index += 1;
    }
    if tabs > 0 {
        return ClipboardDelimiter::Tab;
    }
    if commas > 0 {
        return ClipboardDelimiter::Comma;
    }
    if semicolons > 0 {
        return ClipboardDelimiter::Semicolon;
    }
    ClipboardDelimiter::Tab
}

struct ScannedBlock {
    cells: Vec<Vec<String>>,
    quoted: Vec<Vec<bool>>,
    unclosed_quote: bool,
}

fn scan_block(text: &str, delimiter: ClipboardDelimiter) -> ScannedBlock {
    let chars: Vec<char> = text.chars().collect();
    let separator = delimiter.as_char();
    let mut cells = Vec::new();
    let mut quoted = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut row_quoted: Vec<bool> = Vec::new();
    let mut field = String::new();
    let mut field_started = false;
    let mut field_quoted = false;
    let mut in_quotes = false;
    let mut last_was_row_break = false;
    let mut index = 0usize;

    let end_field = |row: &mut Vec<String>,
                     row_quoted: &mut Vec<bool>,
                     field: &mut String,
                     field_quoted: &mut bool,
                     field_started: &mut bool| {
        row.push(std::mem::take(field));
        row_quoted.push(*field_quoted);
        *field_quoted = false;
        *field_started = false;
    };

    while index < chars.len() {
        let ch = chars[index];
        if in_quotes {
            if ch == '"' {
                if chars.get(index + 1) == Some(&'"') {
                    field.push('"');
                    index += 2;
                    continue;
                }
                in_quotes = false;
                index += 1;
                continue;
            }
            if ch == '\r' {
                field.push('\n');
                index += if chars.get(index + 1) == Some(&'\n') {
                    2
                } else {
                    1
                };
                continue;
            }
            field.push(ch);
            last_was_row_break = false;
            index += 1;
            continue;
        }
        if ch == '"' && !field_started {
            in_quotes = true;
            field_quoted = true;
            field_started = true;
            last_was_row_break = false;
            index += 1;
            continue;
        }
        if ch == separator {
            end_field(
                &mut row,
                &mut row_quoted,
                &mut field,
                &mut field_quoted,
                &mut field_started,
            );
            last_was_row_break = false;
            index += 1;
            continue;
        }
        if ch == '\n' {
            end_field(
                &mut row,
                &mut row_quoted,
                &mut field,
                &mut field_quoted,
                &mut field_started,
            );
            cells.push(std::mem::take(&mut row));
            quoted.push(std::mem::take(&mut row_quoted));
            last_was_row_break = true;
            index += 1;
            continue;
        }
        if ch == '\r' {
            end_field(
                &mut row,
                &mut row_quoted,
                &mut field,
                &mut field_quoted,
                &mut field_started,
            );
            cells.push(std::mem::take(&mut row));
            quoted.push(std::mem::take(&mut row_quoted));
            last_was_row_break = true;
            index += if chars.get(index + 1) == Some(&'\n') {
                2
            } else {
                1
            };
            continue;
        }
        field.push(ch);
        field_started = true;
        last_was_row_break = false;
        index += 1;
    }

    let unclosed_quote = in_quotes;
    if !last_was_row_break {
        end_field(
            &mut row,
            &mut row_quoted,
            &mut field,
            &mut field_quoted,
            &mut field_started,
        );
        cells.push(row);
        quoted.push(row_quoted);
    }
    ScannedBlock {
        cells,
        quoted,
        unclosed_quote,
    }
}

pub fn parse_clipboard_matrix(text: &str) -> ClipboardParseResult {
    let byte_length = utf8_byte_length(text);
    let has_delimiter = text.chars().any(|ch| matches!(ch, '\t' | ',' | ';' | '"'));
    if !has_delimiter && text.trim().is_empty() {
        return ClipboardParseResult::Refused(PasteParseRefusal {
            code: PasteParseErrorCode::Empty,
            cells: 0,
            byte_length,
            row: None,
            expected: None,
            actual: None,
        });
    }
    if byte_length > GRID_PASTE_MAX_BYTES {
        return ClipboardParseResult::Refused(PasteParseRefusal {
            code: PasteParseErrorCode::TooLarge,
            cells: 0,
            byte_length,
            row: None,
            expected: None,
            actual: None,
        });
    }

    let delimiter = sniff_clipboard_delimiter(text);
    let block = scan_block(text, delimiter);
    let width = block.cells.first().map_or(0, Vec::len);
    let cell_count = block.cells.len().saturating_mul(width);
    if let Some(ragged_row) = block.cells.iter().position(|row| row.len() != width) {
        return ClipboardParseResult::Refused(PasteParseRefusal {
            code: PasteParseErrorCode::RaggedRow,
            cells: cell_count,
            byte_length,
            row: Some(ragged_row),
            expected: Some(width),
            actual: Some(block.cells[ragged_row].len()),
        });
    }
    if cell_count > GRID_PASTE_MAX_CELLS {
        return ClipboardParseResult::Refused(PasteParseRefusal {
            code: PasteParseErrorCode::TooLarge,
            cells: cell_count,
            byte_length,
            row: None,
            expected: None,
            actual: None,
        });
    }

    let mut problems = Vec::new();
    if block.unclosed_quote {
        problems.push(PasteProblem {
            code: PasteProblemCode::UnclosedQuote,
            row: None,
            column: None,
        });
    }
    ClipboardParseResult::Ok {
        matrix: ClipboardMatrix {
            cells: block.cells,
            quoted: block.quoted,
            delimiter,
            multiline: text.contains('\n') || text.contains('\r'),
            byte_length,
        },
        problems,
    }
}
