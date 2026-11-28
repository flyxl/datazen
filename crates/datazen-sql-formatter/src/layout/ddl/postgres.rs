//! Narrow PostgreSQL-only formatting. No SQL is run and no COPY client,
//! data stream, program, filename, or external resource is opened.

use super::{clause_name, make_clause, Atom, Cursor, Error, Kind, Kw, Node, Parser};

pub(super) fn schema_definition(body: &[Node<'_>]) -> bool {
    let mut name = Cursor::new(body);
    if name.word("IF") && !name.words(&["NOT", "EXISTS"]) {
        return false;
    }
    name.identifier() && (name.finished() || (name.word("AUTHORIZATION") && name.whole_identifier()))
}

#[derive(Clone, Copy)]
enum OptionsFor {
    Explain,
    Copy,
}

#[derive(Clone, Copy)]
enum Value {
    Boolean,
    ExplainFormat,
    CopyFormat,
    Delimiter,
}

fn option(flavor: OptionsFor, key: &str) -> Option<(u8, Value)> {
    use Value::*;
    Some(match (flavor, key) {
        (OptionsFor::Explain, "ANALYZE" | "ANALYSE") => (1, Boolean),
        (OptionsFor::Explain, "VERBOSE") => (2, Boolean),
        (OptionsFor::Explain, "COSTS") => (4, Boolean),
        (OptionsFor::Explain, "BUFFERS") => (8, Boolean),
        (OptionsFor::Explain, "TIMING") => (16, Boolean),
        (OptionsFor::Explain, "SUMMARY") => (32, Boolean),
        (OptionsFor::Explain, "FORMAT") => (64, ExplainFormat),
        (OptionsFor::Copy, "FORMAT") => (1, CopyFormat),
        (OptionsFor::Copy, "HEADER") => (2, Boolean),
        (OptionsFor::Copy, "DELIMITER") => (4, Delimiter),
        _ => return None,
    })
}

fn accepts(value: Value, atom: Atom<'_>) -> bool {
    if matches!(value, Value::Delimiter) {
        // A single printable ASCII byte in an ordinary string; do not
        // interpret SQL escapes or the server's string-literal settings.
        return atom.kind == Kind::String
            && matches!(atom.text.as_bytes(), [b'\'', byte, b'\'']
                if (b' '..=b'~').contains(byte) && !matches!(*byte, b'\'' | b'\\'));
    }
    if !matches!(atom.kind, Kind::Word | Kind::Keyword(Kw::Keyword | Kw::DataType)) {
        return false;
    }
    let name = clause_name(atom);
    match value {
        Value::Boolean => matches!(name.as_str(), "TRUE" | "FALSE" | "ON" | "OFF"),
        Value::ExplainFormat => matches!(name.as_str(), "TEXT" | "XML" | "JSON" | "YAML"),
        Value::CopyFormat => matches!(name.as_str(), "TEXT" | "CSV" | "BINARY"),
        Value::Delimiter => false,
    }
}

impl<'t, 'a> Parser<'t, 'a> {
    // Called immediately after consuming the recognized EXPLAIN keyword.
    pub(super) fn pg_explain_prefix(&mut self) -> Result<Vec<Node<'a>>, Error> {
        let mut prefix = self.pg_comments()?;
        if self.peek().is_some_and(|head| head.punctuation("(")) {
            prefix.push(self.pg_options(OptionsFor::Explain)?);
            prefix.extend(self.pg_comments()?);
        }
        // The caller must see a SELECT now. Unparenthesized ANALYZE,
        // implicit options, and other statement types are not accepted.
        Ok(prefix)
    }

    pub(super) fn pg_copy_stdout(&mut self, level: usize) -> Result<Vec<Node<'a>>, Error> {
        let copy = self.bump()?;
        let body = self.sequence(level, Atom::zone_boundary)?;
        if !copy_target(&body) || !self.next_clause("WITH") {
            return Err(Error::Unsupported);
        }
        let with = self.bump()?;
        let mut options = self.pg_comments()?;
        options.push(self.pg_options(OptionsFor::Copy)?);
        options.extend(self.pg_comments()?);
        Ok(vec![make_clause(copy, body, true), make_clause(with, options, false)])
    }

    fn pg_comments(&mut self) -> Result<Vec<Node<'a>>, Error> {
        let mut result = Vec::new();
        while let Some(next) = self.peek().filter(|a| a.comment()) {
            if next.kind == Kind::DisableComment {
                return Err(Error::Unsupported);
            }
            result.push(Node::Atom(self.bump()?));
        }
        Ok(result)
    }

    fn pg_options(&mut self, flavor: OptionsFor) -> Result<Node<'a>, Error> {
        self.expect_punctuation("(")?;
        let mut group = Vec::new();
        let mut seen = 0;
        loop {
            group.extend(self.pg_comments()?);
            let key = self.bump()?;
            let name = clause_name(key);
            let (flag, value) = option(flavor, &name).ok_or(Error::Unsupported)?;
            if !matches!(key.kind, Kind::Word | Kind::Keyword(Kw::Keyword | Kw::Clause)) || seen & flag != 0 {
                return Err(Error::Unsupported);
            }
            seen |= flag;
            let mut rhs = self.pg_comments()?;
            let atom = self.bump()?;
            if !accepts(value, atom) {
                return Err(Error::Unsupported);
            }
            rhs.push(Node::Atom(atom));
            rhs.extend(self.pg_comments()?);
            if key.kind == Kind::Keyword(Kw::Clause) {
                // Pinned PostgreSQL ANALYZE is an inline-body clause, even
                // inside options. This forces the same real multiline group.
                if !matches!(flavor, OptionsFor::Explain) || !matches!(name.as_str(), "ANALYZE" | "ANALYSE") {
                    return Err(Error::Unsupported);
                }
                group.push(make_clause(key, rhs, true));
            } else {
                group.push(Node::Atom(key));
                group.extend(rhs);
            }
            let separator = self.bump()?;
            if separator.punctuation(")") {
                break;
            }
            if !separator.punctuation(",") {
                return Err(Error::Unsupported);
            }
            group.push(Node::Atom(separator));
        }
        Ok(Node::Group(group))
    }
}

fn opaque(nodes: &[Node<'_>]) -> bool {
    nodes.iter().any(|node| match node {
        Node::Atom(atom) => atom.kind == Kind::DisableComment,
        Node::Group(body) => opaque(body),
        _ => false,
    })
}

fn copy_target(body: &[Node<'_>]) -> bool {
    if opaque(body) {
        return false;
    }
    let mut target = Cursor::new(body);
    if !target.identifier() {
        return false;
    }
    // PostgreSQL's optional same-database/catalog and schema qualification;
    // do not treat arbitrary expressions as a table target.
    for _ in 0..2 {
        if !target.operator(".") { break; }
        if !target.identifier() { return false; }
    }
    if let Some(columns) = target.group() {
        if !columns.split(super::is_comma).all(|column| Cursor::new(column).whole_identifier()) {
            return false;
        }
    }
    target.exact_words(&["TO", "STDOUT"])
}
