//! Incremental SQL statement splitter shared by restore pipelines.
//!
//! Drivers may use [`SqlStatementScanner`] as-is, configure it, or ignore it
//! and override the whole restore pipeline.

/// Decode file bytes into UTF-8 text across read boundaries.
#[derive(Debug, Default)]
pub struct Utf8ChunkDecoder {
    pending: Vec<u8>,
}

impl Utf8ChunkDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<String, String> {
        self.pending.extend_from_slice(bytes);
        match std::str::from_utf8(&self.pending) {
            Ok(s) => {
                let out = s.to_owned();
                self.pending.clear();
                Ok(out)
            }
            Err(err) => {
                let valid = err.valid_up_to();
                if valid == 0 {
                    if self.pending.len() < 4 && err.error_len().is_none() {
                        return Ok(String::new());
                    }
                    return Err("invalid UTF-8 in SQL file".into());
                }
                let out = std::str::from_utf8(&self.pending[..valid])
                    .expect("valid_up_to is valid UTF-8")
                    .to_owned();
                self.pending.drain(..valid);
                Ok(out)
            }
        }
    }

    pub fn finish(&mut self) -> Result<String, String> {
        if self.pending.is_empty() {
            return Ok(String::new());
        }
        match std::str::from_utf8(&self.pending) {
            Ok(s) => {
                let out = s.to_owned();
                self.pending.clear();
                Ok(out)
            }
            Err(_) => Err("truncated UTF-8 in SQL file".into()),
        }
    }
}

#[derive(Debug, Clone)]
enum ScanMode {
    Normal,
    SingleQuote,
    DoubleQuote,
    Dollar { tag: String },
    LineComment,
    BlockComment { depth: u32 },
}

/// Incremental splitter: feed chunks, receive only complete statements.
#[derive(Debug, Clone)]
pub struct SqlStatementScanner {
    buf: String,
    start: usize,
    i: usize,
    delimiter: String,
    mode: ScanMode,
    recognize_delimiter: bool,
}

impl Default for SqlStatementScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl SqlStatementScanner {
    pub fn new() -> Self {
        Self {
            buf: String::new(),
            start: 0,
            i: 0,
            delimiter: ";".into(),
            mode: ScanMode::Normal,
            recognize_delimiter: true,
        }
    }

    /// MySQL-style `DELIMITER` meta-commands (disable for PostgreSQL dumps).
    pub fn recognize_delimiter_commands(mut self, enabled: bool) -> Self {
        self.recognize_delimiter = enabled;
        self
    }

    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        if chunk.is_empty() {
            return Vec::new();
        }
        self.buf.push_str(chunk);
        self.drain(false)
    }

    pub fn finish(&mut self) -> Vec<String> {
        let mut out = self.drain(true);
        let tail = self.buf[self.start..].trim();
        if !tail.is_empty() {
            out.push(tail.to_string());
        }
        self.buf.clear();
        self.start = 0;
        self.i = 0;
        self.mode = ScanMode::Normal;
        self.delimiter = ";".into();
        out
    }

    fn drain(&mut self, eof: bool) -> Vec<String> {
        let mut stmts = Vec::new();
        loop {
            match self.step(eof) {
                Step::NeedMore => break,
                Step::Yield(stmt) => {
                    if !stmt.is_empty() {
                        stmts.push(stmt);
                    }
                }
                Step::Continue => {}
            }
        }
        self.compact();
        stmts
    }

    fn compact(&mut self) {
        if self.start == 0 {
            return;
        }
        self.buf.drain(..self.start);
        self.i = self.i.saturating_sub(self.start);
        self.start = 0;
    }

    fn step(&mut self, eof: bool) -> Step {
        let bytes = self.buf.as_bytes();
        let len = bytes.len();
        if self.i > len {
            self.i = len;
        }
        if self.i >= len {
            return Step::NeedMore;
        }

        match self.mode {
            ScanMode::Normal => self.step_normal(eof),
            ScanMode::SingleQuote => {
                if bytes[self.i] == b'\'' {
                    self.i += 1;
                    if self.i < len && bytes[self.i] == b'\'' {
                        self.i += 1;
                    } else {
                        self.mode = ScanMode::Normal;
                    }
                } else {
                    self.i += 1;
                }
                Step::Continue
            }
            ScanMode::DoubleQuote => {
                if bytes[self.i] == b'"' {
                    self.i += 1;
                    if self.i < len && bytes[self.i] == b'"' {
                        self.i += 1;
                    } else {
                        self.mode = ScanMode::Normal;
                    }
                } else {
                    self.i += 1;
                }
                Step::Continue
            }
            ScanMode::Dollar { ref tag } => {
                let tag = tag.clone();
                if bytes[self.i] == b'$' && self.buf[self.i..].starts_with(&tag) {
                    self.i += tag.len();
                    self.mode = ScanMode::Normal;
                } else if !eof && remaining_is_prefix(&self.buf[self.i..], &tag) {
                    return Step::NeedMore;
                } else {
                    self.i += 1;
                }
                Step::Continue
            }
            ScanMode::LineComment => {
                if bytes[self.i] == b'\n' {
                    self.mode = ScanMode::Normal;
                }
                self.i += 1;
                Step::Continue
            }
            ScanMode::BlockComment { depth } => {
                if self.i + 1 >= len {
                    return if eof {
                        self.i = len;
                        Step::NeedMore
                    } else {
                        Step::NeedMore
                    };
                }
                if bytes[self.i] == b'/' && bytes[self.i + 1] == b'*' {
                    self.mode = ScanMode::BlockComment { depth: depth + 1 };
                    self.i += 2;
                } else if bytes[self.i] == b'*' && bytes[self.i + 1] == b'/' {
                    self.i += 2;
                    self.mode = if depth <= 1 {
                        ScanMode::Normal
                    } else {
                        ScanMode::BlockComment { depth: depth - 1 }
                    };
                } else {
                    self.i += 1;
                }
                Step::Continue
            }
        }
    }

    fn step_normal(&mut self, eof: bool) -> Step {
        let bytes = self.buf.as_bytes();
        let len = bytes.len();
        let rest = &self.buf[self.i..];

        if self.recognize_delimiter && self.buf[self.start..self.i].trim().is_empty() {
            match match_delimiter_command(&self.buf, self.i, eof) {
                DelimMatch::NeedMore => return Step::NeedMore,
                DelimMatch::Hit { delimiter, next } => {
                    self.delimiter = delimiter;
                    self.i = next;
                    self.start = next;
                    return Step::Continue;
                }
                DelimMatch::None => {}
            }
        }

        if rest.starts_with(&self.delimiter) {
            let fragment = self.buf[self.start..self.i].trim().to_string();
            self.i += self.delimiter.len();
            self.start = self.i;
            return Step::Yield(fragment);
        }
        if !eof && remaining_is_prefix(rest, &self.delimiter) {
            return Step::NeedMore;
        }

        match bytes[self.i] {
            b'\'' => {
                self.i += 1;
                self.mode = ScanMode::SingleQuote;
            }
            b'"' => {
                self.i += 1;
                self.mode = ScanMode::DoubleQuote;
            }
            b'$' => match find_dollar_tag(bytes, self.i) {
                Some(tag_end) => {
                    let tag = self.buf[self.i..tag_end].to_string();
                    self.i = tag_end;
                    self.mode = ScanMode::Dollar { tag };
                }
                None => {
                    if !eof && incomplete_dollar_opener(bytes, self.i) {
                        return Step::NeedMore;
                    }
                    self.i += 1;
                }
            },
            b'-' => {
                if self.i + 1 >= len {
                    return if eof {
                        self.i += 1;
                        Step::Continue
                    } else {
                        Step::NeedMore
                    };
                }
                if bytes[self.i + 1] == b'-' {
                    self.i += 2;
                    self.mode = ScanMode::LineComment;
                } else {
                    self.i += 1;
                }
            }
            b'/' => {
                if self.i + 1 >= len {
                    return if eof {
                        self.i += 1;
                        Step::Continue
                    } else {
                        Step::NeedMore
                    };
                }
                if bytes[self.i + 1] == b'*' {
                    self.i += 2;
                    self.mode = ScanMode::BlockComment { depth: 1 };
                } else {
                    self.i += 1;
                }
            }
            _ => {
                self.i += 1;
            }
        }
        Step::Continue
    }
}

enum Step {
    Continue,
    NeedMore,
    Yield(String),
}

enum DelimMatch {
    None,
    NeedMore,
    Hit { delimiter: String, next: usize },
}

fn remaining_is_prefix(remaining: &str, token: &str) -> bool {
    !remaining.is_empty() && token.starts_with(remaining) && remaining.len() < token.len()
}

/// Parse a MySQL client `DELIMITER` meta-command at `i`.
fn match_delimiter_command(input: &str, i: usize, eof: bool) -> DelimMatch {
    let rest = match input.get(i..) {
        Some(r) => r,
        None => return DelimMatch::None,
    };
    const KW: &str = "delimiter";
    if rest.len() < KW.len() {
        if !eof && KW.starts_with(&rest.to_ascii_lowercase()) {
            return DelimMatch::NeedMore;
        }
        return DelimMatch::None;
    }
    if !rest[..KW.len()].eq_ignore_ascii_case(KW) {
        return DelimMatch::None;
    }
    let after = match rest.get(KW.len()..) {
        Some(a) => a,
        None => {
            return if eof {
                DelimMatch::None
            } else {
                DelimMatch::NeedMore
            };
        }
    };
    let Some(next_ch) = after.chars().next() else {
        return if eof {
            DelimMatch::None
        } else {
            DelimMatch::NeedMore
        };
    };
    if !next_ch.is_whitespace() {
        return DelimMatch::None;
    }
    let mut j = i + KW.len();
    while j < input.len() {
        let b = input.as_bytes()[j];
        if b == b'\n' {
            return DelimMatch::Hit {
                delimiter: ";".into(),
                next: j + 1,
            };
        }
        if !b.is_ascii_whitespace() {
            break;
        }
        j += 1;
    }
    if j >= input.len() {
        return if eof {
            DelimMatch::None
        } else {
            DelimMatch::NeedMore
        };
    }
    let delim_start = j;
    while j < input.len() {
        let b = input.as_bytes()[j];
        if b.is_ascii_whitespace() {
            break;
        }
        j += 1;
    }
    let new_delim = input[delim_start..j].to_string();
    while j < input.len() && input.as_bytes()[j] != b'\n' {
        j += 1;
    }
    if j >= input.len() && !eof {
        return DelimMatch::NeedMore;
    }
    if j < input.len() {
        j += 1;
    }
    DelimMatch::Hit {
        delimiter: if new_delim.is_empty() {
            ";".into()
        } else {
            new_delim
        },
        next: j,
    }
}

/// Try to match a `$tag$` dollar-quote opener starting at position `pos`.
/// Returns `Some(end)` where `end` is the byte index past the closing `$`.
pub fn find_dollar_tag(bytes: &[u8], pos: usize) -> Option<usize> {
    if pos >= bytes.len() || bytes[pos] != b'$' {
        return None;
    }
    let mut j = pos + 1;
    while j < bytes.len() {
        if bytes[j] == b'$' {
            return Some(j + 1);
        }
        if !bytes[j].is_ascii_alphanumeric() && bytes[j] != b'_' {
            return None;
        }
        j += 1;
    }
    None
}

fn incomplete_dollar_opener(bytes: &[u8], pos: usize) -> bool {
    if pos >= bytes.len() || bytes[pos] != b'$' {
        return false;
    }
    let mut j = pos + 1;
    while j < bytes.len() {
        if bytes[j] == b'$' {
            return false;
        }
        if !bytes[j].is_ascii_alphanumeric() && bytes[j] != b'_' {
            return false;
        }
        j += 1;
    }
    true
}

/// Split SQL text into individual statements, respecting quotes, comments,
/// dollar-quotes, and MySQL `DELIMITER` meta-commands.
pub fn split_sql_statements(input: &str) -> Vec<String> {
    let mut scanner = SqlStatementScanner::new();
    let mut stmts = scanner.push(input);
    stmts.extend(scanner.finish());
    stmts
}

/// Returns true when `stmt` is empty or contains only SQL comments / whitespace.
pub fn is_comment_only_or_empty(stmt: &str) -> bool {
    let bytes = stmt.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= len {
            return true;
        }
        if i + 1 < len && bytes[i] == b'-' && bytes[i + 1] == b'-' {
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < len {
                if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_sql_respects_semicolon_in_single_quotes() {
        let stmts = split_sql_statements("SELECT 'a;b'; SELECT 1;");
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0], "SELECT 'a;b'");
        assert_eq!(stmts[1], "SELECT 1");
    }

    #[test]
    fn split_sql_respects_dollar_quoted_body_with_semicolon() {
        let stmts = split_sql_statements("SELECT $$foo;bar$$; SELECT 1;");
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0], "SELECT $$foo;bar$$");
        assert_eq!(stmts[1], "SELECT 1");
    }

    #[test]
    fn split_sql_respects_line_comment_with_semicolon() {
        let stmts = split_sql_statements("SELECT 1; -- trailing; comment\nSELECT 2;");
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0], "SELECT 1");
        assert_eq!(stmts[1], "-- trailing; comment\nSELECT 2");
    }

    #[test]
    fn split_sql_respects_block_comment_with_semicolon() {
        let stmts = split_sql_statements("SELECT 1; /* block; comment */ SELECT 2;");
        assert_eq!(stmts.len(), 2);
        assert_eq!(stmts[0], "SELECT 1");
        assert_eq!(stmts[1], "/* block; comment */ SELECT 2");
    }

    #[test]
    fn is_comment_only_or_empty_detects_comment_statements() {
        assert!(is_comment_only_or_empty("-- only a comment"));
        assert!(is_comment_only_or_empty("/* block */"));
        assert!(!is_comment_only_or_empty("SELECT 1"));
    }

    #[test]
    fn split_sql_respects_mysql_delimiter_around_routines() {
        let sql =
            "DELIMITER $$\nCREATE PROCEDURE foo()\nBEGIN\n  SELECT 1;\nEND$$\nDELIMITER ;\nSELECT 2;";
        let stmts = split_sql_statements(sql);
        assert_eq!(stmts.len(), 2);
        assert!(stmts[0].contains("CREATE PROCEDURE"));
        assert!(stmts[0].contains("SELECT 1;"));
        assert_eq!(stmts[1], "SELECT 2");
    }

    #[test]
    fn scanner_keeps_statements_intact_across_chunks() {
        let sql =
            "INSERT INTO t VALUES ('a;b'); CREATE FUNCTION f() RETURNS int AS $$ BEGIN RETURN 1; END; $$ LANGUAGE plpgsql;";
        let mut scanner = SqlStatementScanner::new();
        let mut got = Vec::new();
        for chunk in sql.as_bytes().chunks(7) {
            got.extend(scanner.push(std::str::from_utf8(chunk).unwrap()));
        }
        got.extend(scanner.finish());
        assert_eq!(got, split_sql_statements(sql));
        assert_eq!(got.len(), 2);
        assert!(got[0].contains("'a;b'"));
        assert!(got[1].contains("RETURN 1;"));
    }

    #[test]
    fn scanner_does_not_emit_until_delimiter() {
        let mut scanner = SqlStatementScanner::new();
        assert!(scanner.push("INSERT INTO t VALUES (1").is_empty());
        assert!(scanner.push(", 2").is_empty());
        let mid = scanner.push("); SELECT 3;");
        assert_eq!(mid, vec!["INSERT INTO t VALUES (1, 2)", "SELECT 3"]);
        assert!(scanner.finish().is_empty());
    }

    #[test]
    fn utf8_decoder_holds_incomplete_codepoint() {
        let mut dec = Utf8ChunkDecoder::new();
        let s = "你好";
        let bytes = s.as_bytes();
        assert!(dec.push(&bytes[..1]).unwrap().is_empty());
        let rest = dec.push(&bytes[1..]).unwrap();
        assert_eq!(rest, "你好");
    }
}
