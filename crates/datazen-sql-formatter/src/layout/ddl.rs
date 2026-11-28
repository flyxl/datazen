//! Closed, formatter-only routes for common single-object SQL definitions.
//! A recognized command name is not permission to lay out its unknown body.

use super::{clause_name, is_comment, make_clause, require_content, Atom, Dialect, Error, Kind, Kw, Node, Parser};

mod postgres;

pub(super) struct Command<'a> {
    pub(super) nodes: Vec<Node<'a>>,
    pub(super) query: Option<Query>,
}

#[derive(Clone, Copy)]
pub(super) enum Query {
    Explain,
    View { check_option: bool },
}

impl<'t, 'a> Parser<'t, 'a> {
    pub(super) fn root_ddl(&mut self, level: usize) -> Result<Option<Command<'a>>, Error> {
        let head = self.peek().ok_or(Error::Unsupported)?;
        if !matches!(head.kind, Kind::Word | Kind::Keyword(Kw::Clause | Kw::Keyword)) {
            return Ok(None);
        }
        let name = clause_name(head);
        let mut result = Command { nodes: Vec::new(), query: None };
        match name.as_str() {
            "DELIMITER" if mysql_family(self.dialect) => return Err(Error::Unsupported),
            "CREATE SCHEMA" if self.dialect == Dialect::PostgreSql => {
                let head = self.bump()?;
                let body = self.sequence(level, Atom::zone_boundary)?;
                if !postgres::schema_definition(&body) {
                    return Err(Error::Unsupported);
                }
                result.nodes.push(make_clause(head, body, true));
            }
            "COPY" if self.dialect == Dialect::PostgreSql => {
                result.nodes = self.pg_copy_stdout(level)?;
            }
            "CREATE TABLE" | "CREATE TABLE IF NOT EXISTS"
            | "CREATE TEMP TABLE" | "CREATE TEMP TABLE IF NOT EXISTS"
            | "CREATE TEMPORARY TABLE" | "CREATE TEMPORARY TABLE IF NOT EXISTS" => {
                let head = self.bump()?;
                let body = self.sequence(level, Atom::zone_boundary)?;
                if !table_definition(&body, self.dialect) {
                    return Err(Error::Unsupported);
                }
                result.nodes.push(make_clause(head, body, true));
            }
            "CREATE INDEX" | "CREATE UNIQUE INDEX" | "CREATE" => {
                let head = self.bump()?;
                let body = self.sequence(level, Atom::zone_boundary)?;
                if !index_definition(&name, &body) {
                    return Err(Error::Unsupported);
                }
                result.nodes.push(make_clause(head, body, true));
                if matches!(self.dialect, Dialect::Sqlite | Dialect::PostgreSql) && self.next_clause("WHERE") {
                    let head = self.bump()?;
                    let predicate = self.sequence(level, Atom::zone_boundary)?;
                    require_content(&predicate)?;
                    result.nodes.push(make_clause(head, predicate, false));
                }
            }
            "CREATE VIEW" | "CREATE VIEW IF NOT EXISTS" | "CREATE OR REPLACE VIEW"
            | "CREATE OR ALTER VIEW" | "CREATE TEMP VIEW" | "CREATE TEMPORARY VIEW" => {
                let head = self.bump()?;
                let body = self.sequence(level, Atom::zone_boundary)?;
                if !view_target(&body) || !self.next_select() {
                    return Err(Error::Unsupported);
                }
                result.nodes.push(make_clause(head, body, true));
                result.query = Some(Query::View { check_option: self.dialect == Dialect::MariaDb });
            }
            "DROP" | "DROP TABLE" | "DROP TABLE IF EXISTS" | "DROP INDEX" | "DROP INDEX IF EXISTS"
            | "DROP VIEW" | "DROP VIEW IF EXISTS" => {
                let head = self.bump()?;
                let body = self.sequence(level, Atom::zone_boundary)?;
                if !drop_target(&name, &body, self.dialect) {
                    return Err(Error::Unsupported);
                }
                result.nodes.push(make_clause(head, body, true));
            }
            "ALTER TABLE" => {
                let head = self.bump()?;
                let body = self.sequence(level, Atom::zone_boundary)?;
                if !Cursor::new(&body).whole_name() {
                    return Err(Error::Unsupported);
                }
                result.nodes.push(make_clause(head, body, true));
                self.alter_actions(level, &mut result.nodes)?;
            }
            "EXPLAIN" if self.dialect != Dialect::TransactSql => {
                let head = self.bump()?;
                let body = if self.dialect == Dialect::PostgreSql {
                    self.pg_explain_prefix()?
                } else {
                    let body = self.sequence(level, Atom::zone_boundary)?;
                    let mut validate = Cursor::new(&body);
                    if !validate.finished() && !(self.dialect == Dialect::Sqlite
                        && validate.words(&["QUERY", "PLAN"]) && validate.finished())
                    {
                        return Err(Error::Unsupported);
                    }
                    body
                };
                if !self.next_select() {
                    return Err(Error::Unsupported);
                }
                result.nodes.push(make_clause(head, body, true));
                result.query = Some(Query::Explain);
            }
            "BEGIN" | "BEGIN TRANSACTION" | "START TRANSACTION" | "COMMIT" | "ROLLBACK"
            | "ROLLBACK TO" | "ROLLBACK TO SAVEPOINT" | "SAVEPOINT" | "RELEASE SAVEPOINT"
            | "RELEASE" | "SAVE" => {
                let head = self.bump()?;
                let body = self.sequence(level, Atom::zone_boundary)?;
                if mysql_family(self.dialect) && matches!(name.as_str(), "ROLLBACK" | "ROLLBACK TO")
                    && Cursor::new(&body).exact_words(if name == "ROLLBACK" { &["TO"] } else { &[] })
                    && self.next_clause("SAVEPOINT")
                {
                    // This is two original phrase tokens in the pinned MariaDB
                    // tokenizer. Preserve its newline, and require the target.
                    result.nodes.push(make_clause(head, body, true));
                    let head = self.bump()?;
                    let target = self.sequence(level, Atom::zone_boundary)?;
                    if !Cursor::new(&target).whole_identifier() {
                        return Err(Error::Unsupported);
                    }
                    result.nodes.push(make_clause(head, target, true));
                } else {
                    if !transaction(&name, &body, self.dialect) {
                        return Err(Error::Unsupported);
                    }
                    result.nodes.push(make_clause(head, body, true));
                }
            }
            _ => return Ok(None),
        }
        if result.query.is_none() && self.peek().is_some_and(|a| !a.punctuation(";")) {
            return Err(Error::Unsupported);
        }
        Ok(Some(result))
    }

    fn next_clause(&self, name: &str) -> bool {
        self.peek().is_some_and(|a| a.kind == Kind::Keyword(Kw::Clause) && clause_name(a) == name)
    }

    fn next_select(&self) -> bool {
        self.peek().is_some_and(|a| a.kind == Kind::Keyword(Kw::Select))
    }

    fn alter_actions(&mut self, level: usize, into: &mut Vec<Node<'a>>) -> Result<(), Error> {
        let mut count = 0;
        loop {
            let action = self.peek().filter(|a| a.kind == Kind::Keyword(Kw::Clause)).ok_or(Error::Unsupported)?;
            if count != 0 && !mysql_family(self.dialect) {
                return Err(Error::Unsupported);
            }
            let name = clause_name(self.bump()?);
            let body = self.sequence(level, Atom::zone_boundary)?;
            let mut parts = Cursor::new(&body);
            let more = parts.remove_last_comma();
            let mut next_default = false;
            let valid = match name.as_str() {
                "ADD" | "ADD COLUMN" => {
                    if mysql_family(self.dialect) && name == "ADD" && parts.word("INDEX") {
                        parts.identifier() && parts.group().is_some_and(nonempty_expression) && parts.finished()
                    } else {
                        column(parts)
                    }
                }
                "DROP COLUMN" => parts.whole_identifier(),
                "RENAME COLUMN" => parts.identifier() && parts.word("TO") && parts.whole_identifier(),
                "RENAME TO" => parts.whole_name(),
                "ALTER COLUMN" if self.dialect == Dialect::PostgreSql => {
                    next_default = true;
                    parts.whole_identifier()
                }
                _ => false,
            };
            if !valid || (more && (next_default || !mysql_family(self.dialect))) {
                return Err(Error::Unsupported);
            }
            into.push(make_clause(action, body, true));
            count += 1;
            if next_default {
                if !self.next_clause("SET DEFAULT") {
                    return Err(Error::Unsupported);
                }
                let head = self.bump()?;
                let body = self.sequence(level, Atom::zone_boundary)?;
                if !nonempty_expression(&body) {
                    return Err(Error::Unsupported);
                }
                into.push(make_clause(head, body, true));
            }
            if !more {
                return Ok(());
            }
        }
    }
}

fn mysql_family(dialect: Dialect) -> bool {
    matches!(dialect, Dialect::MySql | Dialect::MariaDb)
}

pub(super) fn with_check_option(nodes: &[Node<'_>]) -> bool {
    Cursor::new(nodes).exact_words(&["CHECK", "OPTION"])
}

struct Cursor<'n, 'a> {
    nodes: Vec<&'n Node<'a>>,
    pos: usize,
}

impl<'n, 'a> Cursor<'n, 'a> {
    fn new(nodes: &'n [Node<'a>]) -> Self {
        Self { nodes: nodes.iter().filter(|n| !is_comment(n)).collect(), pos: 0 }
    }

    fn finished(&self) -> bool { self.pos == self.nodes.len() }

    fn take_when(&mut self, f: impl FnOnce(&Node<'a>) -> bool) -> bool {
        if self.nodes.get(self.pos).is_some_and(|n| f(n)) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn word(&mut self, text: &str) -> bool {
        self.take_when(|n| matches!(n, Node::Atom(a) if matches!(a.kind, Kind::Word | Kind::Keyword(_)) && clause_name(*a) == text))
    }

    fn words(&mut self, words: &[&str]) -> bool {
        words.iter().all(|word| self.word(word))
    }

    fn exact_words(mut self, words: &[&str]) -> bool {
        self.words(words) && self.finished()
    }

    fn identifier(&mut self) -> bool {
        self.take_when(|n| matches!(n, Node::Atom(a) if matches!(a.kind, Kind::Word | Kind::QuotedIdentifier)))
    }

    fn operator(&mut self, text: &str) -> bool {
        self.take_when(|n| matches!(n, Node::Atom(a) if a.kind == Kind::Operator && a.text == text))
    }

    fn name(&mut self) -> bool {
        if !self.identifier() { return false; }
        while self.operator(".") {
            if !self.identifier() { return false; }
        }
        true
    }

    fn whole_name(&mut self) -> bool { self.name() && self.finished() }

    fn whole_identifier(&mut self) -> bool { self.identifier() && self.finished() }

    fn group(&mut self) -> Option<&'n [Node<'a>]> {
        let Node::Group(nodes) = *self.nodes.get(self.pos)? else { return None };
        self.pos += 1;
        Some(nodes)
    }

    fn remove_last_comma(&mut self) -> bool {
        if self.nodes.last().is_some_and(|n| is_comma(n)) {
            self.nodes.pop();
            true
        } else {
            false
        }
    }
}

fn is_comma(node: &Node<'_>) -> bool {
    matches!(node, Node::Atom(a) if a.punctuation(","))
}

fn nonempty_expression(nodes: &[Node<'_>]) -> bool {
    require_content(nodes).is_ok()
        && !nodes.iter().any(|n| matches!(n, Node::Clause(..) | Node::InlineClause(..) | Node::Set(..) | Node::Limit(..)))
}

fn column(mut nodes: Cursor<'_, '_>) -> bool {
    // Admit a real identifier and declared type (including custom types),
    // not arbitrary expressions, SELECT, or opaque structural material.
    if !nodes.identifier() { return false; }
    if !nodes.take_when(|n| matches!(n, Node::Atom(a)
        if matches!(a.kind, Kind::Word | Kind::QuotedIdentifier | Kind::Keyword(Kw::DataType)))
        && !matches!(n, Node::Atom(a) if matches!(clause_name(*a).as_str(),
            "DEFAULT" | "NULL" | "NOT" | "PRIMARY" | "UNIQUE" | "COLLATE" | "REFERENCES")))
        && !nodes.take_when(|n| matches!(n, Node::Call(head, _, _) if head.kind == Kind::Keyword(Kw::ParameterizedDataType)))
    {
        return false;
    }
    nodes.nodes[nodes.pos..].iter().all(|n| !matches!(n,
        Node::Clause(..) | Node::InlineClause(..) | Node::Set(..) | Node::Limit(..)
    ))
}

fn table_columns(nodes: &[Node<'_>]) -> bool {
    nodes.split(is_comma).all(|part| {
        let mut items = Cursor::new(part);
        // Known leading constraint keywords remain whole. Details are
        // expressions subsequently laid out without rewriting their values.
        if items.word("PRIMARY KEY") || items.word("UNIQUE") || items.word("CHECK")
            || items.word("FOREIGN KEY") || items.word("CONSTRAINT")
            || items.words(&["PRIMARY", "KEY"]) || items.words(&["FOREIGN", "KEY"])
        {
            let mut groups = items.nodes[items.pos..].iter().filter_map(|n| {
                if let Node::Group(body) = n { Some(body) } else { None }
            });
            groups.next().is_some_and(|group| nonempty_expression(group))
        } else {
            column(Cursor::new(part))
        }
    })
}

fn table_definition(nodes: &[Node<'_>], dialect: Dialect) -> bool {
    let mut spec = Cursor::new(nodes);
    if !spec.name() || !spec.group().is_some_and(table_columns) { return false; }
    if spec.finished() { return true; }
    if !mysql_family(dialect) { return false; }
    // Explicit, single-valued MySQL options; no arbitrary tail, AS SELECT,
    // command delimiter, query, or nested table options are guessed.
    while !spec.finished() {
        if spec.word("ENGINE") {
            if !spec.operator("=") || !spec.identifier() { return false; }
        } else if spec.word("DEFAULT") {
            if !(spec.word("CHARSET") || spec.words(&["CHARACTER", "SET"]))
                || !spec.operator("=") || !spec.identifier()
            {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}

fn index_definition(head: &str, nodes: &[Node<'_>]) -> bool {
    let mut parts = Cursor::new(nodes);
    if head == "CREATE" {
        parts.word("UNIQUE");
        if !parts.word("INDEX") { return false; }
    }
    if parts.word("IF") && !parts.words(&["NOT", "EXISTS"]) { return false; }
    parts.name() && parts.word("ON") && parts.name()
        && parts.group().is_some_and(nonempty_expression) && parts.finished()
}

fn view_target(nodes: &[Node<'_>]) -> bool {
    let mut parts = Cursor::new(nodes);
    parts.name() && parts.word("AS") && parts.finished()
}

fn drop_target(head: &str, nodes: &[Node<'_>], dialect: Dialect) -> bool {
    let mut parts = Cursor::new(nodes);
    if head == "DROP" && !(parts.word("INDEX") || parts.word("TABLE") || parts.word("VIEW")) {
        return false;
    }
    if !head.ends_with("IF EXISTS") && parts.word("IF") && !parts.word("EXISTS") {
        return false;
    }
    if !parts.name() { return false; }
    if dialect == Dialect::PostgreSql {
        if !parts.word("CASCADE") { parts.word("RESTRICT"); }
    }
    parts.finished()
}

fn transaction(head: &str, body: &[Node<'_>], dialect: Dialect) -> bool {
    let mut parts = Cursor::new(body);
    match head {
        "BEGIN" if dialect == Dialect::Sqlite => {
            if !parts.word("DEFERRED") && !parts.word("IMMEDIATE") { parts.word("EXCLUSIVE"); }
            parts.word("TRANSACTION");
            parts.finished()
        }
        "BEGIN" | "SAVE" | "COMMIT" | "ROLLBACK" if dialect == Dialect::TransactSql => {
            // Bare BEGIN is a procedural block, not a transaction.
            if parts.word("TRANSACTION") || parts.word("TRAN") {
                parts.finished() || parts.whole_identifier()
            } else {
                matches!(head, "COMMIT" | "ROLLBACK") && parts.finished()
            }
        }
        "BEGIN TRANSACTION" | "START TRANSACTION" => parts.finished(),
        "BEGIN" if dialect == Dialect::PostgreSql => parts.finished() || parts.exact_words(&["TRANSACTION"]),
        "SAVEPOINT" | "RELEASE SAVEPOINT" | "ROLLBACK TO SAVEPOINT" => parts.whole_identifier(),
        "RELEASE" if dialect == Dialect::Sqlite => {
            parts.word("SAVEPOINT");
            parts.whole_identifier()
        }
        "ROLLBACK" if dialect == Dialect::Sqlite => {
            parts.word("TRANSACTION");
            if parts.word("TO") {
                parts.word("SAVEPOINT");
                parts.whole_identifier()
            } else {
                parts.finished()
            }
        }
        "COMMIT" | "ROLLBACK" => parts.finished() || parts.exact_words(&["TRANSACTION"]),
        _ => false,
    }
}
