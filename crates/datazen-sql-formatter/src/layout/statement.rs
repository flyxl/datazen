//! Explicitly admitted DML forms. The renderer still formats the same
//! upstream clause/expression stream. A grammar arm does not enable a dialect.

use super::{clause_name, is_comment, Dialect, Error, Kind, Kw, Node};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Dml {
    InsertPending,
    InsertValues,
    InsertSelect,
    UpsertSetPending,
    ConflictDone,
    ConflictSet,
    DuplicateKeySet,
    UpdatePending,
    UpdateSet,
    UpdateFrom,
    UpdateWhere,
    Delete,
    AliasDeletePending,
    AliasDeleteFrom,
    Where,
    Returned,
}

impl Dml {
    pub(super) fn start(name: &str, dialect: Dialect, body: &[Node<'_>]) -> Result<Option<(Self, bool)>, Error> {
        let sqlite = dialect == Dialect::Sqlite;
        let start = match name {
            "INSERT INTO" => Some((Self::InsertPending, false)),
            "REPLACE INTO" | "INSERT OR ABORT INTO" | "INSERT OR FAIL INTO"
            | "INSERT OR IGNORE INTO" | "INSERT OR REPLACE INTO" | "INSERT OR ROLLBACK INTO"
                if sqlite => Some((Self::InsertPending, false)),
            "UPDATE" => Some((Self::UpdatePending, true)),
            "UPDATE OR ABORT" | "UPDATE OR FAIL" | "UPDATE OR IGNORE"
            | "UPDATE OR REPLACE" | "UPDATE OR ROLLBACK"
                if sqlite => Some((Self::UpdatePending, true)),
            "DELETE FROM" => Some((Self::Delete, true)),
            _ => None,
        };
        if start.is_some() && !matches!(body.iter().find(|node| !is_comment(node)),
            Some(Node::Atom(a)) if matches!(a.kind, Kind::Word | Kind::QuotedIdentifier))
        {
            return Err(Error::Unsupported);
        }
        Ok(start)
    }

    /// The pinned MySQL-family tokenizer treats the first DELETE as an
    /// ordinary keyword in `DELETE target FROM source [JOIN …]`.
    pub(super) fn alias_delete(dialect: Dialect, body: &[Node<'_>]) -> Result<Self, Error> {
        let mut target = body.iter().filter(|node| !is_comment(node));
        if is_mysql_family(dialect) && target.next().is_some_and(identifier) && target.next().is_none() {
            Ok(Self::AliasDeletePending)
        } else {
            Err(Error::Unsupported)
        }
    }

    pub(super) fn begin_select(&mut self) -> Result<(), Error> {
        match self {
            Self::InsertPending => { *self = Self::InsertSelect; Ok(()) }
            Self::InsertSelect => Ok(()),
            _ => Err(Error::Unsupported),
        }
    }

    pub(super) fn query_source(self) -> bool { self == Self::InsertSelect }

    pub(super) fn complete(self) -> bool {
        !matches!(self, Self::InsertPending | Self::UpdatePending | Self::UpsertSetPending | Self::AliasDeletePending)
    }

    /// None lets an INSERT's SELECT body use the normal SELECT clause rules.
    /// Some(bool) identifies a DML-only clause and whether its body is inline.
    pub(super) fn clause(&mut self, name: &str, body: &[Node<'_>], dialect: Dialect) -> Result<Option<bool>, Error> {
        let sqlite = dialect == Dialect::Sqlite;
        let (next, inline) = match (*self, name) {
            (Self::InsertPending, "VALUES") if value_tuples(body) => (Self::InsertValues, false),
            (Self::InsertValues | Self::InsertSelect, "ON CONFLICT") if sqlite => {
                // These ACTUAL pinned phrase tokens determine which clause
                // is legal next. Do not accept an unrecognized fallback.
                let mut tail = body.iter().rev().filter(|node| !is_comment(node));
                let last = tail.next().and_then(atom_name);
                let next = match last.as_deref() {
                    Some("DO UPDATE") => Self::UpsertSetPending,
                    Some("DO NOTHING") => Self::ConflictDone,
                    Some("NOTHING") if tail.next().and_then(atom_name).as_deref() == Some("DO") => Self::ConflictDone,
                    _ => return Err(Error::Unsupported),
                };
                (next, true)
            }
            (Self::InsertValues | Self::InsertSelect, "ON DUPLICATE KEY UPDATE")
                if is_mysql_family(dialect) && assignments(body) => (Self::DuplicateKeySet, false),
            (Self::InsertValues | Self::InsertSelect, "ON DUPLICATE KEY UPDATE") =>
                return Err(Error::Unsupported),
            (Self::AliasDeletePending, "FROM") if body.iter().find(|node| !is_comment(node)).is_some_and(identifier) =>
                (Self::AliasDeleteFrom, false),
            (Self::UpdatePending | Self::UpsertSetPending, "SET") if body.iter().any(|node| {
                matches!(node, Node::Atom(a) if a.kind == Kind::Operator && a.text == "=")
            }) => (if *self == Self::UpdatePending { Self::UpdateSet } else { Self::ConflictSet }, false),
            (Self::UpdateSet, "FROM") if dialect == Dialect::PostgreSql => (Self::UpdateFrom, false),
            (Self::UpdateSet | Self::UpdateFrom, "WHERE") => (Self::UpdateWhere, false),
            (Self::Delete | Self::AliasDeleteFrom | Self::ConflictSet, "WHERE") => (Self::Where, false),
            (Self::UpdateSet | Self::UpdateFrom | Self::UpdateWhere, "RETURNING")
                if matches!(dialect, Dialect::Sqlite | Dialect::PostgreSql) => (Self::Returned, false),
            (Self::InsertValues | Self::InsertSelect | Self::DuplicateKeySet, "RETURNING")
                if dialect == Dialect::MariaDb => (Self::Returned, false),
            (Self::InsertValues | Self::InsertSelect | Self::ConflictDone | Self::ConflictSet
                | Self::Delete | Self::Where, "RETURNING") if sqlite => (Self::Returned, false),
            (Self::InsertSelect, _) => return Ok(None),
            _ => return Err(Error::Unsupported),
        };
        *self = next;
        Ok(Some(inline))
    }
}

pub(super) fn is_mysql_family(dialect: Dialect) -> bool {
    matches!(dialect, Dialect::MySql | Dialect::MariaDb)
}

fn identifier(node: &Node<'_>) -> bool {
    matches!(node, Node::Atom(a) if matches!(a.kind, Kind::Word | Kind::QuotedIdentifier))
}

fn operator(node: &Node<'_>, text: &str) -> bool {
    matches!(node, Node::Atom(a) if a.kind == Kind::Operator && a.text == text)
}

/// Recognize only explicit assignments to a (possibly qualified) column.
/// Do not infer a target from an arbitrary expression or an opaque span.
fn assignments(body: &[Node<'_>]) -> bool {
    body.split(|node| matches!(node, Node::Atom(a) if a.punctuation(","))).all(|part| {
        let mut nodes = part.iter().enumerate().filter(|(_, node)| !is_comment(node));
        if !nodes.next().is_some_and(|(_, node)| identifier(node)) { return false; }
        loop {
            match nodes.next() {
                Some((at, node)) if operator(node, "=") => return super::require_content(&part[at + 1..]).is_ok(),
                Some((_, node)) if operator(node, ".") && nodes.next().is_some_and(|(_, node)| identifier(node)) => {}
                _ => return false,
            }
        }
    })
}

fn atom_name(node: &Node<'_>) -> Option<String> {
    match node {
        Node::Atom(a) => Some(clause_name(*a)),
        _ => None,
    }
}

fn value_tuples(body: &[Node<'_>]) -> bool {
    let mut group = true;
    for node in body.iter().filter(|node| !is_comment(node)) {
        if group {
            if !matches!(node, Node::Group(nodes) if !nodes.is_empty()
                && !nodes.iter().any(|node| matches!(node, Node::Clause(..) | Node::Set(..) | Node::Limit(..)))) {
                return false;
            }
        } else if !matches!(node, Node::Atom(a) if a.punctuation(",")) {
            return false;
        }
        group = !group;
    }
    !group
}

/// PRAGMA is an ordinary reserved word in SQLite 15.8.2, not a clause.
/// Support only a name, name(value), or name = value (plus qualification).
pub(super) fn is_pragma_body(nodes: &[Node<'_>]) -> bool {
    fn name(node: &Node<'_>) -> bool {
        matches!(node, Node::Atom(a) if matches!(a.kind, Kind::Word | Kind::QuotedIdentifier | Kind::Keyword(Kw::Keyword)))
    }
    fn value(node: &Node<'_>) -> bool {
        name(node) || matches!(node, Node::Atom(a) if matches!(a.kind, Kind::Number | Kind::String))
    }
    let nodes: Vec<_> = nodes.iter().filter(|node| !is_comment(node)).collect();
    let Some((first, mut tail)) = nodes.split_first() else { return false };
    if !name(first) { return false; }
    if matches!(tail.first(), Some(Node::Atom(a)) if a.kind == Kind::Operator && a.text == ".") {
        if !tail.get(1).is_some_and(|node| name(node)) { return false; }
        tail = &tail[2..];
    }
    match tail {
        [] => true,
        [Node::Atom(a), arg] if a.kind == Kind::Operator && a.text == "=" => value(arg),
        [Node::Group(args)] => {
            let mut args = args.iter().filter(|node| !is_comment(node));
            args.next().is_some_and(value) && args.next().is_none()
        }
        _ => false,
    }
}
