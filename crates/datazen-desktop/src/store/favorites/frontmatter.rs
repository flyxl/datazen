//! Favorites file format: SQL with a leading `--` comment front-matter block.
//!
//! A favorite is a plain `.sql` file that stays runnable as-is:
//!
//! ```sql
//! -- title: Nightly order reconciliation
//! -- connectionId: 7f3a...
//! -- createdAt: 2026-08-18T10:00:00Z
//! -- updatedAt: 2026-08-18T10:00:00Z
//! SELECT * FROM orders WHERE ...
//! ```
//!
//! Using SQL line comments for metadata is the point: the file is a
//! self-describing artifact that `psql`, an editor, or `git diff` can all read
//! without a DataZen-specific decoder, and the metadata can never make the
//! statement unparseable.
//!
//! ## Grammar (deliberately forgiving)
//!
//! Front-matter is the *contiguous run of `-- key: value` lines at the very
//! top of the file*. The run ends at the first line that is not one. A file
//! that starts with any other comment, or with SQL, is stored verbatim and
//! contributes zero metadata — hand-written SQL must load, not be mangled.
//!
//! Values are escaped so a metadata value can never terminate its own line and
//! spill into the statement body.
//!
//! ## The one rule about whitespace
//!
//! `parse` normalizes nothing — a hand-written file is never rewritten just
//! because it was read. `render` guarantees exactly one trailing newline on a
//! non-empty file, because a file that does not end in one is a text file that
//! every line-based tool complains about. So a statement that came from the SQL
//! editor as `SELECT 1` reads back as `SELECT 1\n`, and that is the whole of
//! the difference.

/// Line comment that introduces every metadata line.
const COMMENT_PREFIX: &str = "--";

/// Parsed front-matter: insertion-ordered, so a file renders back byte-for-byte
/// and a key a future build writes keeps its place at the top of the block.
///
/// A `BTreeMap` would be shorter but reorders the keys alphabetically, which
/// turns every re-save into a diff and buries `title` under `connectionId`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrontMatter {
    fields: Vec<(String, String)>,
}

/// A favorite file split into its metadata block and its SQL body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFile {
    pub front_matter: FrontMatter,
    /// The statement text with the front-matter block removed, byte-for-byte.
    pub body: String,
}

impl FrontMatter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Value of `key`, or `None` when the key is absent.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Value of `key` as a non-empty string, or `None` when absent or empty.
    pub fn get_non_empty(&self, key: &str) -> Option<&str> {
        self.get(key).filter(|v| !v.trim().is_empty())
    }

    /// Set `key`, keeping its original position when it is already present.
    pub fn set(&mut self, key: &str, value: &str) {
        match self.fields.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value.to_string(),
            None => self.fields.push((key.to_string(), value.to_string())),
        }
    }

    /// Set only when `value` has content, so an absent optional key stays
    /// absent instead of being written as an empty line.
    pub fn set_some(&mut self, key: &str, value: Option<&str>) {
        if let Some(v) = value.filter(|v| !v.trim().is_empty()) {
            self.set(key, v);
        } else {
            self.remove(key);
        }
    }

    pub fn remove(&mut self, key: &str) {
        self.fields.retain(|(k, _)| k != key);
    }

    /// Every key in file order, including keys DataZen does not recognize.
    ///
    /// Test-only: the read path asks for the six fields it knows by name, and
    /// nothing in the app iterates the header.
    #[cfg(test)]
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.fields.iter().map(|(k, _)| k.as_str())
    }
}

impl ParsedFile {
    /// Split `content` into front-matter and SQL body.
    pub fn parse(content: &str) -> Self {
        // A UTF-8 BOM is legal at the start of a hand-edited file and would
        // otherwise make the first line look like SQL.
        let content = content.strip_prefix('\u{feff}').unwrap_or(content);
        let mut front_matter = FrontMatter::new();
        let mut consumed_lines = 0usize;

        for line in content.split_inclusive('\n') {
            match parse_metadata_line(line) {
                Some((key, value)) => front_matter.set(&key, &value),
                // End of the front-matter block. Everything from here on —
                // including this line — is the statement body.
                None => break,
            }
            consumed_lines += 1;
        }

        let body = strip_first_lines(content, consumed_lines);
        Self { front_matter, body }
    }

    /// Serialize back to a runnable `.sql` file.
    ///
    /// `parse` never normalizes anything, so this is the only place that can:
    /// a rendered file always ends in a newline, which is what git and every
    /// line-based tool expect. A body that already ends in one is untouched.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (key, value) in &self.front_matter.fields {
            out.push_str(COMMENT_PREFIX);
            out.push(' ');
            out.push_str(key);
            out.push_str(": ");
            out.push_str(&escape_value(value));
            out.push('\n');
        }
        out.push_str(&self.body);
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }
}

/// Parse one `-- key: value` line. `None` means "this line is not metadata",
/// which terminates the block.
fn parse_metadata_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let rest = trimmed.strip_prefix(COMMENT_PREFIX)?;
    // `--key: v` (no space) is accepted; `-- key: v` is the canonical form.
    let rest = rest.trim_start_matches([' ', '\t']);
    let (key, value) = rest.split_once(':')?;
    let key = key.trim();
    // A bare SQL comment (`-- just a note`) must end the block rather than be
    // swallowed as a key with an empty value.
    if key.is_empty() || !is_metadata_key(key) {
        return None;
    }
    Some((key.to_string(), unescape_value(value.trim())))
}

/// Metadata keys are plain identifiers; anything else is user prose.
fn is_metadata_key(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Escape a value so it can never break out of its `-- key: value` line.
fn escape_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

fn unescape_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            // A lone backslash is user text, not an escape: keep it verbatim
            // so a Windows path like `C:\tmp` round-trips unchanged.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Drop the first `count` lines, keeping the rest byte-for-byte.
fn strip_first_lines(content: &str, count: usize) -> String {
    if count == 0 {
        return content
            .strip_prefix('\u{feff}')
            .unwrap_or(content)
            .to_string();
    }
    let mut offset = 0usize;
    for _ in 0..count {
        match content[offset..].find('\n') {
            Some(i) => offset += i + 1,
            None => return String::new(),
        }
    }
    content[offset..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_canonical_layout() {
        // The order the plan's §2.6.2 documents. `parse` must not reorder it.
        let file = ParsedFile::parse(
            "-- title: Nightly reconciliation\n\
             -- connectionId: cfg-1\n\
             -- createdAt: 2026-08-18T10:00:00Z\n\
             SELECT 1;\n",
        );
        assert_eq!(
            file.front_matter.get("title"),
            Some("Nightly reconciliation")
        );
        assert_eq!(file.front_matter.get("connectionId"), Some("cfg-1"));
        assert_eq!(file.body, "SELECT 1;\n");
        assert_eq!(
            file.render(),
            "-- title: Nightly reconciliation\n\
             -- connectionId: cfg-1\n\
             -- createdAt: 2026-08-18T10:00:00Z\n\
             SELECT 1;\n"
        );
    }

    #[test]
    fn round_trips_through_render() {
        let src = "-- title: a\n-- keyword: recon\n-- custom: kept\nSELECT 1;\n";
        let parsed = ParsedFile::parse(src);
        assert_eq!(parsed.render(), src, "render must be byte-stable");
    }

    #[test]
    fn a_plain_sql_file_carries_no_metadata() {
        let file = ParsedFile::parse("SELECT 1;\n");
        assert!(file.front_matter.keys().next().is_none());
        assert_eq!(file.body, "SELECT 1;\n");
    }

    #[test]
    fn a_leading_prose_comment_ends_the_block() {
        // The user wrote a note, not metadata: nothing may be consumed.
        let src = "-- exports nightly\nSELECT 1;\n";
        let file = ParsedFile::parse(src);
        assert!(file.front_matter.keys().next().is_none());
        assert_eq!(file.body, src);
    }

    #[test]
    fn metadata_stops_at_the_first_non_metadata_line() {
        let src = "-- title: a\n-- just a note\nSELECT 1;\n";
        let file = ParsedFile::parse(src);
        assert_eq!(file.front_matter.get("title"), Some("a"));
        assert_eq!(file.body, "-- just a note\nSELECT 1;\n");
    }

    #[test]
    fn survives_a_truncated_front_matter_block() {
        // Crash between writing the temp file and the rename leaves a prefix
        // of the header with no statement. It must load, not error.
        let file = ParsedFile::parse("-- title: half written\n-- createdAt: 2026-08");
        assert_eq!(file.front_matter.get("title"), Some("half written"));
        assert_eq!(file.body, "");
    }

    #[test]
    fn an_empty_file_is_a_valid_empty_favorite() {
        let file = ParsedFile::parse("");
        assert!(file.front_matter.keys().next().is_none());
        assert_eq!(file.body, "");
    }

    #[test]
    fn a_metadata_only_file_keeps_an_empty_statement() {
        let file = ParsedFile::parse("-- title: nothing yet\n");
        assert_eq!(file.front_matter.get("title"), Some("nothing yet"));
        assert_eq!(file.body, "");
    }

    #[test]
    fn values_may_contain_colons() {
        let file =
            ParsedFile::parse("-- createdAt: 2026-08-18T10:00:00Z\n-- title: a: b\nSELECT 1;\n");
        assert_eq!(
            file.front_matter.get("createdAt"),
            Some("2026-08-18T10:00:00Z")
        );
        assert_eq!(file.front_matter.get("title"), Some("a: b"));
        assert_eq!(file.body, "SELECT 1;\n");
    }

    #[test]
    fn multiline_values_cannot_escape_their_line() {
        let original = ParsedFile {
            front_matter: {
                let mut fm = FrontMatter::new();
                fm.set("title", "first\nDROP TABLE users;");
                fm
            },
            body: "SELECT 1;\n".to_string(),
        };
        let rendered = original.render();
        let reparsed = ParsedFile::parse(&rendered);
        assert_eq!(
            reparsed.front_matter.get("title"),
            Some("first\nDROP TABLE users;")
        );
        assert_eq!(reparsed.body, "SELECT 1;\n", "injected text stays metadata");
        assert_eq!(
            rendered
                .lines()
                .filter(|l| l.starts_with(COMMENT_PREFIX))
                .count(),
            1,
            "the injected newline must not create a second comment line"
        );
    }

    #[test]
    fn backslashes_survive_without_being_eaten() {
        let mut fm = FrontMatter::new();
        fm.set("database", r"C:\tmp\db");
        let rendered = ParsedFile {
            front_matter: fm,
            body: "SELECT 1;".into(),
        }
        .render();
        // Doubled on the way out, single again on the way in. What matters is
        // the round trip: a Windows database path must come back byte-exact.
        assert_eq!(rendered, "-- database: C:\\\\tmp\\\\db\nSELECT 1;\n");
        assert_eq!(
            ParsedFile::parse(&rendered).front_matter.get("database"),
            Some(r"C:\tmp\db")
        );
    }

    #[test]
    fn a_literal_backslash_n_is_not_mistaken_for_a_newline() {
        // The one place the escaping could bite: a path whose last character is
        // `\n` as two real characters.
        let mut fm = FrontMatter::new();
        fm.set("database", r"share\new");
        let rendered = ParsedFile {
            front_matter: fm,
            body: "SELECT 1;".into(),
        }
        .render();
        assert_eq!(rendered, "-- database: share\\\\new\nSELECT 1;\n");
        assert_eq!(
            ParsedFile::parse(&rendered).front_matter.get("database"),
            Some(r"share\new")
        );
    }

    #[test]
    fn unknown_keys_are_preserved_in_file_order() {
        let src = "-- zeta: 1\n-- alpha: 2\n-- title: t\nSELECT 1;\n";
        let parsed = ParsedFile::parse(src);
        assert_eq!(
            parsed.front_matter.keys().collect::<Vec<_>>(),
            ["zeta", "alpha", "title"]
        );
        assert_eq!(parsed.render(), src, "order is the file's own, not sorted");
    }

    #[test]
    fn resetting_a_key_keeps_its_position() {
        let mut fm = FrontMatter::new();
        fm.set("title", "a");
        fm.set("connectionId", "c");
        fm.set("title", "b");
        let rendered = ParsedFile {
            front_matter: fm,
            body: "SELECT 1;".into(),
        }
        .render();
        assert_eq!(rendered, "-- title: b\n-- connectionId: c\nSELECT 1;\n");
    }

    #[test]
    fn crlf_line_endings_do_not_leak_into_values() {
        let file = ParsedFile::parse("-- title: windows\r\nSELECT 1;\r\n");
        assert_eq!(file.front_matter.get("title"), Some("windows"));
        assert_eq!(file.body, "SELECT 1;\r\n");
    }

    #[test]
    fn a_bom_does_not_hide_the_first_metadata_line() {
        let file = ParsedFile::parse("\u{feff}-- title: bom\nSELECT 1;\n");
        assert_eq!(file.front_matter.get("title"), Some("bom"));
        assert_eq!(file.body, "SELECT 1;\n");
    }

    #[test]
    fn empty_values_are_distinguishable_from_absent_keys() {
        let file = ParsedFile::parse("-- title:\nSELECT 1;\n");
        assert_eq!(file.front_matter.get("title"), Some(""));
        assert_eq!(file.front_matter.get_non_empty("title"), None);
        assert_eq!(file.front_matter.get("missing"), None);
    }

    #[test]
    fn a_statement_without_a_trailing_newline_still_lands_one() {
        // The editor hands over `SELECT 1` with no newline; the file it produces
        // has to be a well-formed text file.
        let file = ParsedFile::parse("-- title: a\nSELECT 1");
        assert_eq!(file.body, "SELECT 1");
        assert_eq!(file.render(), "-- title: a\nSELECT 1\n");
    }
}
