//! Native translation of the statement/expression layout from sql-formatter
//! 15.8.2 (MIT, ZeroTurnaround LLC, George Leslie-Waksman, inferrinizzard and
//! contributors). See the crate's third-party notices. Nothing is executed.
//!
//! This is a deliberately bounded grammar lane, not a SQL validator or AST
//! printer. Initially only the exercised SQLite/ANSI Query shapes are admitted.

use std::{borrow::Cow, time::Instant};

use crate::{
    lexer::{KeywordKind as Kw, Token, TokenKind as Kind},
    Dialect, Error, Indent, KeywordCase, LogicalOperatorNewline, Options,
};

mod statement;

use crate::dialect_layout::{is_always_dense, is_property_access, is_select_head};
use statement::Dml;

const MAX_DEPTH: usize = 48;
const INLINE_WIDTH: usize = 50;

#[derive(Clone, Copy)]
struct Atom<'a> {
    kind: Kind,
    text: &'a str,
    before: &'a str,
}

impl<'a> From<&Token<'a>> for Atom<'a> {
    fn from(token: &Token<'a>) -> Self {
        Self { kind: token.kind, text: token.text, before: token.whitespace_before }
    }
}

impl Atom<'_> {
    fn punctuation(self, text: &str) -> bool {
        self.kind == Kind::Punctuation && self.text == text
    }

    fn quoted_or_word(self) -> bool {
        matches!(self.kind, Kind::Word | Kind::QuotedIdentifier | Kind::Placeholder)
    }

    fn comment(self) -> bool {
        matches!(self.kind, Kind::LineComment | Kind::BlockComment | Kind::DisableComment)
    }

    fn case(self, text: &str) -> bool {
        self.kind == Kind::Keyword(Kw::Case) && self.text.eq_ignore_ascii_case(text)
    }

    fn zone_boundary(self) -> bool {
        matches!(self.kind, Kind::Keyword(Kw::Select | Kw::Clause | Kw::SetOperation | Kw::Limit))
            || self.punctuation(")") || self.punctuation(";")
    }
}

enum Node<'a> {
    Atom(Atom<'a>),
    Group(Vec<Node<'a>>),
    Call(Atom<'a>, Vec<Node<'a>>, Vec<Node<'a>>),
    Clause(Atom<'a>, Vec<Node<'a>>),
    InlineClause(Atom<'a>, Vec<Node<'a>>),
    Set(Atom<'a>, Vec<Node<'a>>),
    Limit(Atom<'a>, Vec<Node<'a>>, Vec<Node<'a>>),
    Between(Atom<'a>, Vec<Node<'a>>, Atom<'a>, Vec<Node<'a>>),
    Case(Atom<'a>, Vec<Node<'a>>, Vec<Branch<'a>>, Atom<'a>),
}

enum Branch<'a> {
    When(Atom<'a>, Vec<Node<'a>>, Atom<'a>, Vec<Node<'a>>),
    Else(Atom<'a>, Vec<Node<'a>>),
}

fn check(deadline: Instant) -> Result<(), Error> {
    if Instant::now() >= deadline { Err(Error::Deadline) } else { Ok(()) }
}

fn js_space(ch: char) -> bool {
    matches!(ch as u32, 0x09..=0x0d | 0x20 | 0xa0 | 0x1680 | 0x2000..=0x200a
        | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff)
}

fn normalized_space(text: &str) -> Cow<'_, str> {
    // Most tokens have no internal whitespace. Preserve source spelling and
    // use upstream ECMAScript whitespace only for keyword phrases.
    if !text.chars().any(js_space) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(text.split(js_space).filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" "))
}

fn is_physical_newline(whitespace: &str) -> bool {
    whitespace.contains(['\r', '\n'])
}

fn clause_name(head: Atom<'_>) -> String {
    normalized_space(head.text).to_ascii_uppercase()
}

struct Parser<'t, 'a> {
    tokens: &'t [Token<'a>],
    pos: usize,
    deadline: Instant,
    dialect: Dialect,
}

impl<'a> Parser<'_, 'a> {
    fn peek(&self) -> Option<Atom<'a>> {
        self.tokens.get(self.pos).map(Atom::from)
    }

    fn bump(&mut self) -> Result<Atom<'a>, Error> {
        check(self.deadline)?;
        let token = self.peek().ok_or(Error::Unsupported)?;
        self.pos += 1;
        Ok(token)
    }

    fn expect_punctuation(&mut self, text: &str) -> Result<(), Error> {
        if !self.bump()?.punctuation(text) {
            return Err(Error::Unsupported);
        }
        Ok(())
    }

    fn region(&mut self, level: usize, statement: bool) -> Result<Vec<Node<'a>>, Error> {
        if level > MAX_DEPTH {
            return Err(Error::Unsupported);
        }
        let mut nodes = Vec::new();
        let mut root_starter = false;
        let mut saw_select = false;
        let mut compound_waiting = false;
        let mut seen_data = false;
        let mut saw_frame = false;
        let mut query_clause = 0;
        let mut dml: Option<Dml> = None;
        let mut cte_pending = false;
        let mut after_go = false;
        let after_semicolon = statement && self.pos.checked_sub(1).and_then(|i| self.tokens.get(i))
            .is_some_and(|token| Atom::from(token).punctuation(";"));

        while let Some(head) = self.peek() {
            if head.punctuation(")") || head.punctuation(";") {
                break;
            }
            if self.dialect == Dialect::TransactSql && head.kind == Kind::Keyword(Kw::Clause)
                && head.text.eq_ignore_ascii_case("GO")
            {
                // GO is a client batch boundary, not a SQL semicolon.
                // Admit only a real delimiter on its own physical line with
                // no repeat count, trailing semicolon, or same-line payload.
                let complete = (saw_select || dml.is_some_and(Dml::complete)) && !compound_waiting;
                if !statement || !is_physical_newline(head.before)
                    || !(complete || (after_semicolon && !root_starter && !after_go))
                {
                    return Err(Error::Unsupported);
                }
                let go = self.bump()?;
                if self.peek().is_some_and(|next| next.punctuation(";") || !is_physical_newline(next.before)) {
                    return Err(Error::Unsupported);
                }
                nodes.push(Node::InlineClause(go, Vec::new()));
                root_starter = false;
                saw_select = false;
                seen_data = false;
                query_clause = 0;
                dml = None;
                cte_pending = false;
                after_go = true;
                continue;
            }
            if statement && !seen_data && !root_starter && statement::is_mysql_family(self.dialect)
                && head.kind == Kind::Keyword(Kw::Keyword) && head.text.eq_ignore_ascii_case("DELETE")
            {
                let head = self.bump()?;
                let target = self.sequence(level, Atom::zone_boundary)?;
                dml = Some(Dml::alias_delete(self.dialect, &target)?);
                nodes.push(Node::InlineClause(head, target));
                root_starter = true;
                seen_data = true;
                continue;
            }
            if statement && !seen_data && !root_starter && self.dialect == Dialect::Sqlite
                && head.kind == Kind::Keyword(Kw::Keyword) && head.text.eq_ignore_ascii_case("PRAGMA")
            {
                nodes.push(Node::Atom(self.bump()?));
                let body = self.sequence(level, Atom::zone_boundary)?;
                if !statement::is_pragma_body(&body) || self.peek().is_some_and(|a| !a.punctuation(";")) {
                    return Err(Error::Unsupported);
                }
                nodes.extend(body);
                return Ok(nodes);
            }
            match head.kind {
                Kind::Keyword(Kw::Select) => {
                    if let Some(dml) = &mut dml { dml.begin_select()?; }
                    let head = self.bump()?;
                    let name = clause_name(head);
                    if !is_select_head(self.dialect, &name)
                        || (saw_select && !compound_waiting) || saw_frame
                        || (!root_starter && seen_data)
                    {
                        return Err(Error::Unsupported);
                    }
                    let expressions = self.sequence(level, Atom::zone_boundary)?;
                    require_content(&expressions)?;
                    nodes.push(Node::Clause(head, expressions));
                    root_starter = true;
                    cte_pending = false;
                    saw_select = true;
                    compound_waiting = false;
                    query_clause = 0;
                    seen_data = true;
                }
                Kind::Keyword(Kw::Clause) => {
                    let head = self.bump()?;
                    let name = clause_name(head);
                    let expressions = self.sequence(level, Atom::zone_boundary)?;
                    require_content(&expressions)?;
                    let pg_cte_update = self.dialect == Dialect::PostgreSql && cte_pending && name == "UPDATE";
                    if statement && ((!seen_data && !root_starter) || pg_cte_update) {
                        if let Some((started, inline)) = Dml::start(&name, self.dialect, &expressions)? {
                            dml = Some(started);
                            cte_pending = false;
                            root_starter = true;
                            seen_data = true;
                            nodes.push(make_clause(head, expressions, inline));
                            continue;
                        }
                    }
                    if let Some(dml) = &mut dml {
                        if let Some(inline) = dml.clause(&name, &expressions, self.dialect)? {
                            nodes.push(make_clause(head, expressions, inline));
                            continue;
                        }
                    }
                    match name.as_str() {
                        "WITH" | "WITH RECURSIVE" if !seen_data => {
                            root_starter = true;
                            cte_pending = true;
                        }
                        name if saw_select && !compound_waiting => {
                            let order = match name {
                                "FROM" => 1,
                                "WHERE" => 2,
                                "GROUP BY" | "GROUP BY ALL" | "GROUP BY DISTINCT" => 3,
                                "HAVING" => 4,
                                "WINDOW" => 5,
                                "ORDER BY" => 6,
                                "OFFSET" => 8,
                                "FETCH FIRST" | "FETCH NEXT" => 9,
                                _ => return Err(Error::Unsupported),
                            };
                            if order <= query_clause { return Err(Error::Unsupported); }
                            query_clause = order;
                        }
                        "PARTITION BY" | "ORDER BY" if !statement && !root_starter => {
                            // OVER (...) and named WINDOW specifications.
                            // They render clauses without a SELECT starter.
                            saw_frame = true;
                        }
                        _ => return Err(Error::Unsupported),
                    }
                    if name.starts_with("WITH") && !expressions.iter().any(|node| {
                        matches!(node, Node::Group(nodes) if contains_select(nodes))
                    }) { return Err(Error::Unsupported); }
                    nodes.push(Node::Clause(head, expressions));
                    seen_data = true;
                }
                Kind::Keyword(Kw::Limit) => {
                    if !saw_select || compound_waiting || query_clause >= 7 || !dml.is_none_or(Dml::query_source) {
                        return Err(Error::Unsupported);
                    }
                    query_clause = 7;
                    let head = self.bump()?;
                    let mut comments = Vec::new();
                    while self.peek().is_some_and(Atom::comment) {
                        comments.push(Node::Atom(self.bump()?));
                    }
                    let expressions = self.sequence(level, Atom::zone_boundary)?;
                    require_content(&expressions)?;
                    let mut commas = expressions.iter().enumerate().filter_map(|(i, node)| {
                        matches!(node, Node::Atom(atom) if atom.punctuation(",")).then_some(i)
                    });
                    if let Some(comma) = commas.next() {
                        if commas.next().is_some() {
                            return Err(Error::Unsupported);
                        }
                        require_content(&expressions[..comma])?;
                        require_content(&expressions[comma + 1..])?;
                    }
                    nodes.push(Node::Limit(head, comments, expressions));
                    seen_data = true;
                }
                Kind::Keyword(Kw::SetOperation) => {
                    if !saw_select || compound_waiting || !dml.is_none_or(Dml::query_source) {
                        return Err(Error::Unsupported);
                    }
                    let op = self.bump()?;
                    let tail = self.sequence(level, Atom::zone_boundary)?;
                    // Preserve a compound with an explicitly parenthesized
                    // query; other loose material is outside this grammar.
                    compound_waiting = !tail.iter().any(|node| {
                        matches!(node, Node::Group(nodes) if contains_select(nodes))
                    });
                    if compound_waiting && tail.iter().any(|node| !is_comment(node)) {
                        return Err(Error::Unsupported);
                    }
                    nodes.push(Node::Set(op, tail));
                }
                _ => {
                    // Except for leading comments, a statement never starts
                    // with an arbitrary expression that looks like another
                    // SQL command. Expressions inside parentheses may.
                    if statement && !root_starter && !head.comment() {
                        return Err(Error::Unsupported);
                    }
                    let node = self.expression(level)?;
                    seen_data |= !head.comment();
                    nodes.push(node);
                }
            }
        }
        if compound_waiting || dml.is_some_and(|kind| !kind.complete())
            || (dml.is_none() && root_starter && !saw_select)
        {
            return Err(Error::Unsupported);
        }
        Ok(nodes)
    }

    fn sequence(&mut self, level: usize, stop: impl Fn(Atom<'a>) -> bool) -> Result<Vec<Node<'a>>, Error> {
        let mut result = Vec::new();
        while let Some(a) = self.peek() {
            if a.punctuation(")") || a.punctuation(";") || stop(a) {
                break;
            }
            result.push(self.expression(level)?);
        }
        Ok(result)
    }

    fn group(&mut self, level: usize) -> Result<Vec<Node<'a>>, Error> {
        self.expect_punctuation("(")?;
        let body = self.region(level + 1, false)?;
        self.expect_punctuation(")")?;
        Ok(body)
    }

    fn expression(&mut self, level: usize) -> Result<Node<'a>, Error> {
        if level > MAX_DEPTH {
            return Err(Error::Unsupported);
        }
        let head = self.peek().ok_or(Error::Unsupported)?;
        if head.punctuation("(") {
            return Ok(Node::Group(self.group(level)?));
        }
        let head = self.bump()?;
        match head.kind {
            Kind::Keyword(Kw::Function | Kw::ParameterizedDataType) => {
                let mut comments = Vec::new();
                while self.peek().is_some_and(Atom::comment) {
                    comments.push(Node::Atom(self.bump()?));
                }
                Ok(Node::Call(head, comments, self.group(level)?))
            }
            Kind::Keyword(Kw::Between) => {
                let lower = self.sequence(level + 1, |a| {
                    a.zone_boundary() || a.punctuation(",") || a.kind == Kind::Keyword(Kw::LogicalOperator)
                })?;
                require_content(&lower)?;
                let and = self.bump()?;
                if and.kind != Kind::Keyword(Kw::LogicalOperator) || !and.text.eq_ignore_ascii_case("AND") {
                    return Err(Error::Unsupported);
                }
                let mut upper = Vec::new();
                while self.peek().is_some_and(Atom::comment) {
                    upper.push(Node::Atom(self.bump()?));
                }
                if self.peek().is_none_or(|a| {
                    a.zone_boundary() || a.punctuation(",")
                        || a.kind == Kind::Keyword(Kw::LogicalOperator) || a.case("END")
                        || a.case("WHEN") || a.case("THEN") || a.case("ELSE")
                }) {
                    return Err(Error::Unsupported);
                }
                upper.push(self.expression(level + 1)?);
                Ok(Node::Between(head, lower, and, upper))
            }
            Kind::Keyword(Kw::Case) if head.case("CASE") => self.case_expression(head, level + 1),
            Kind::Keyword(Kw::Case | Kw::Select | Kw::Clause | Kw::SetOperation | Kw::Limit) => Err(Error::Unsupported),
            Kind::Punctuation if !head.punctuation(",") => Err(Error::Unsupported),
            Kind::Operator if is_property_access(self.dialect, head.text) => {
                // The upstream token passes already demoted adjacent
                // property keywords. Refuse a missing/non-property RHS.
                let following = self.tokens[self.pos..].iter().map(Atom::from).find(|a| !a.comment());
                if following.is_none_or(|a| !a.quoted_or_word() && !(a.kind == Kind::Operator && a.text == "*")) {
                    return Err(Error::Unsupported);
                }
                Ok(Node::Atom(head))
            }
            _ => Ok(Node::Atom(head)),
        }
    }

    fn case_expression(&mut self, head: Atom<'a>, level: usize) -> Result<Node<'a>, Error> {
        let stop = |a: Atom<'a>| a.zone_boundary() || a.case("WHEN") || a.case("THEN")
            || a.case("ELSE") || a.case("END") || a.punctuation(",");
        let subject = self.sequence(level, stop)?;
        let mut branches = Vec::new();
        let mut else_seen = false;
        while let Some(token) = self.peek() {
            if !else_seen && token.case("WHEN") {
                let when = self.bump()?;
                let condition = self.sequence(level, stop)?;
                require_case_operand(&condition)?;
                let next = self.peek();
                if !next.is_some_and(|a| a.case("THEN")) {
                    if next.is_none_or(|a| a.zone_boundary() || a.case("WHEN")
                        || a.case("ELSE") || a.case("END"))
                    {
                        return Err(Error::Syntax);
                    }
                    return Err(Error::Unsupported);
                }
                let then = self.bump()?;
                let result = self.sequence(level, stop)?;
                require_case_operand(&result)?;
                branches.push(Branch::When(when, condition, then, result));
            } else if !else_seen && !branches.is_empty() && token.case("ELSE") {
                let other = self.bump()?;
                let result = self.sequence(level, stop)?;
                require_case_operand(&result)?;
                branches.push(Branch::Else(other, result));
                else_seen = true;
            } else {
                break;
            }
        }
        let next = self.peek();
        if !next.is_some_and(|a| a.case("END")) {
            return Err(if next.is_none_or(Atom::zone_boundary) {
                Error::Syntax
            } else {
                // Some alternate branch orders are formatted upstream.
                // Keep outside this subset instead of calling them malformed.
                Error::Unsupported
            });
        }
        if branches.is_empty() {
            return Err(Error::Unsupported);
        }
        Ok(Node::Case(head, subject, branches, self.bump()?))
    }
}

fn make_clause<'a>(head: Atom<'a>, body: Vec<Node<'a>>, inline: bool) -> Node<'a> {
    if inline { Node::InlineClause(head, body) } else { Node::Clause(head, body) }
}

fn is_comment(node: &Node<'_>) -> bool {
    matches!(node, Node::Atom(atom) if atom.comment())
}

fn require_case_operand(nodes: &[Node<'_>]) -> Result<(), Error> {
    if nodes.iter().all(is_comment) {
        return Err(Error::Syntax);
    }
    require_content(nodes)
}

fn require_content(nodes: &[Node<'_>]) -> Result<(), Error> {
    let mut real = nodes.iter().filter(|node| !is_comment(node));
    let Some(first) = real.next() else { return Err(Error::Unsupported) };
    let last = real.next_back().unwrap_or(first);
    if [first, last].into_iter().any(|node| {
        matches!(node, Node::Atom(atom)
            if atom.punctuation(",") || matches!(atom.kind, Kind::Keyword(Kw::Join | Kw::LogicalOperator)))
    }) || matches!(last, Node::Atom(atom) if atom.kind == Kind::Operator && atom.text != "*") {
        return Err(Error::Unsupported);
    }
    let mut comma = false;
    for node in nodes.iter().filter(|node| !is_comment(node)) {
        let next = matches!(node, Node::Atom(atom) if atom.punctuation(","));
        if comma && next { return Err(Error::Unsupported); }
        comma = next;
    }
    Ok(())
}

fn contains_select(nodes: &[Node<'_>]) -> bool {
    nodes.iter().any(|node| matches!(node, Node::Clause(head, _) if head.kind == Kind::Keyword(Kw::Select)))
}

enum Part<'a> {
    Text(Cow<'a, str>),
    Space,
    NoSpace,
    NoNewline,
    Newline,
    MandatoryNewline,
    Indent,
    SingleIndent,
}

impl<'a> Part<'a> {
    fn text(s: &'a str) -> Self { Self::Text(Cow::Borrowed(s)) }

    fn horizontal(&self) -> bool { matches!(self, Self::Space | Self::SingleIndent) }
}

enum PaintError {
    Core(Error),
    NotInline,
}

impl From<Error> for PaintError {
    fn from(error: Error) -> Self { Self::Core(error) }
}

type PaintResult<T = ()> = Result<T, PaintError>;

#[derive(Clone, Copy, Default)]
struct Inline {
    units: usize,
    trailing_space: bool,
}

struct Layout<'a, 'o> {
    parts: Vec<Part<'a>>,
    bytes: usize,
    indentation: usize,
    indent: &'o str,
    inline: Option<Inline>,
    options: &'o Options,
    output_limit: usize,
    deadline: Instant,
}

impl<'a, 'o> Layout<'a, 'o> {
    fn new(options: &'o Options, indent: &'o str, output_limit: usize, deadline: Instant, inline: bool) -> Self {
        Self {
            parts: Vec::new(), bytes: 0, indentation: 0, indent, options,
            inline: inline.then_some(Inline::default()), output_limit, deadline,
        }
    }

    fn weight(&self, part: &Part<'_>) -> usize {
        match part {
            Part::Text(s) => s.len(),
            Part::SingleIndent => self.indent.len(),
            Part::Space | Part::Newline | Part::MandatoryNewline => 1,
            _ => 0,
        }
    }

    fn push(&mut self, part: Part<'a>) -> PaintResult {
        self.bytes = self.bytes.checked_add(self.weight(&part)).ok_or(Error::OutputLimit)?;
        // Transient indentation/trailing spaces can be removed by later
        // markers. The joined output receives the exact hard limit as well.
        let temporary = self.output_limit.saturating_add(MAX_DEPTH * 8 + 128);
        if self.bytes > temporary {
            return Err(Error::OutputLimit.into());
        }
        self.parts.push(part);
        Ok(())
    }

    fn trim(&mut self, allow_newline: bool) {
        while self.parts.last().is_some_and(|p| p.horizontal() || (allow_newline && matches!(p, Part::Newline))) {
            if let Some(part) = self.parts.pop() {
                self.bytes -= self.weight(&part);
            }
        }
    }

    fn add(&mut self, items: impl IntoIterator<Item = Part<'a>>) -> PaintResult {
        // Keep the upstream InlineLayout's entire-add width decision (e.g.
        // remove one pending space before accounting for a closing ')').
        for item in items {
            if let Some(inline) = self.inline.as_mut() {
                match &item {
                    Part::Text(s) => {
                        inline.units += s.encode_utf16().count();
                        inline.trailing_space = false;
                    }
                    Part::Newline | Part::MandatoryNewline => return Err(PaintError::NotInline),
                    Part::Indent | Part::SingleIndent | Part::Space if !inline.trailing_space => {
                        inline.units += 1;
                        inline.trailing_space = true;
                    }
                    Part::NoNewline | Part::NoSpace if inline.trailing_space => {
                        inline.units -= 1;
                        inline.trailing_space = false;
                    }
                    _ => {}
                }
            }
            match item {
                Part::NoSpace => self.trim(false),
                Part::NoNewline => self.trim(true),
                Part::Newline | Part::MandatoryNewline => {
                    self.trim(false);
                    match self.parts.last() {
                        None | Some(Part::MandatoryNewline) => {}
                        Some(Part::Newline) => {
                            self.parts.pop();
                            self.bytes -= 1;
                            self.push(item)?;
                        }
                        _ => self.push(item)?,
                    }
                }
                Part::Indent => {
                    for _ in 0..self.indentation {
                        self.push(Part::SingleIndent)?;
                    }
                }
                other => self.push(other)?,
            }
        }
        if self.inline.is_some_and(|p| p.units > INLINE_WIDTH) {
            return Err(PaintError::NotInline);
        }
        Ok(())
    }

    fn is_start_of_line(&self) -> bool {
        self.parts.iter().rev().find(|p| !matches!(p, Part::SingleIndent))
            .is_some_and(|p| matches!(p, Part::Newline | Part::MandatoryNewline))
    }

    fn keyword(&self, atom: Atom<'a>) -> Part<'a> {
        let raw = normalized_space(atom.text);
        Part::Text(match self.options.keyword_case {
            KeywordCase::Preserve => raw,
            KeywordCase::Upper => Cow::Owned(raw.to_ascii_uppercase()),
            KeywordCase::Lower => Cow::Owned(raw.to_ascii_lowercase()),
        })
    }

    fn nodes(&mut self, nodes: &[Node<'a>]) -> PaintResult {
        for node in nodes {
            check(self.deadline)?;
            self.node(node)?;
        }
        Ok(())
    }

    fn atom(&mut self, atom: Atom<'a>) -> PaintResult {
        use Part::*;
        match atom.kind {
            Kind::LineComment => {
                let comment = Part::text(atom.text);
                if atom.before.contains('\n') {
                    self.add([Newline, Indent, comment, MandatoryNewline, Indent])
                } else if !self.parts.is_empty() {
                    self.add([NoNewline, Space, comment, MandatoryNewline, Indent])
                } else {
                    self.add([comment, MandatoryNewline, Indent])
                }
            }
            Kind::BlockComment => self.block_comment(atom),
            // Unlike ordinary comments, the opaque protected span is
            // neither scanned, reindented, nor split into logical lines.
            Kind::DisableComment => self.add([Part::text(atom.text), Space]),
            Kind::Punctuation if atom.punctuation(",") => {
                if self.inline.is_some() {
                    self.add([NoSpace, Part::text(","), Space])
                } else {
                    self.add([NoSpace, Part::text(","), Newline, Indent])
                }
            }
            Kind::Operator if is_property_access(self.options.dialect, atom.text) || is_always_dense(self.options.dialect, atom.text) => self.add([NoSpace, Part::text(atom.text)]),
            Kind::Operator if atom.text == ":" => self.add([NoSpace, Part::text(atom.text), Space]),
            Kind::Keyword(Kw::Join) => self.add([Newline, Indent, self.keyword(atom), Space]),
            Kind::Keyword(Kw::LogicalOperator) => match self.options.logical_operator_newline {
                LogicalOperatorNewline::Before => self.add([Newline, Indent, self.keyword(atom), Space]),
                LogicalOperatorNewline::After => self.add([self.keyword(atom), Newline, Indent]),
            },
            Kind::Keyword(Kw::DataType) => self.add([Text(normalized_space(atom.text)), Space]),
            Kind::Keyword(_) => self.add([self.keyword(atom), Space]),
            _ => self.add([Part::text(atom.text), Space]),
        }
    }

    fn block_comment(&mut self, atom: Atom<'a>) -> PaintResult {
        use Part::*;
        if !(atom.text.contains('\n') || atom.before.contains('\n') || self.is_start_of_line()) {
            return self.add([Part::text(atom.text), Space]);
        }
        let mut lines = atom.text.split('\n');
        let first = lines.next().unwrap_or_default();
        let last = lines.next_back();
        let is_doc = matches!(first, "/*" | "/**")
            && last.is_some_and(|line| line.trim_start_matches(js_space) == "*/")
            && lines.all(|line| line.trim_start_matches(js_space).starts_with('*'));
        for line in atom.text.split('\n') {
            check(self.deadline)?;
            let stripped = line.trim_start_matches(js_space);
            let shaped = if is_doc && stripped.starts_with('*') {
                Cow::Owned(format!(" {stripped}"))
            } else {
                Cow::Borrowed(stripped)
            };
            self.add([Newline, Indent, Text(shaped)])?;
        }
        self.add([Newline, Indent])
    }

    fn group(&mut self, body: &[Node<'a>]) -> PaintResult {
        use Part::*;
        let mut fast = Self::new(self.options, "", self.output_limit, self.deadline, true);
        match fast.nodes(body) {
            Ok(()) => {
                self.add([Part::text("(")])?;
                self.add(fast.parts)?;
                self.add([NoSpace, Part::text(")"), Space])
            }
            Err(PaintError::NotInline) if self.inline.is_some() => Err(PaintError::NotInline),
            Err(PaintError::NotInline) => {
                self.add([Part::text("("), Newline])?;
                self.indentation += 1;
                self.add([Indent])?;
                self.nodes(body)?;
                self.indentation -= 1;
                self.add([Newline, Indent, Part::text(")"), Space])
            }
            Err(other) => Err(other),
        }
    }

    fn node(&mut self, node: &Node<'a>) -> PaintResult {
        use Part::*;
        match node {
            Node::Atom(atom) => self.atom(*atom),
            Node::Group(body) => self.group(body),
            Node::Call(name, comments, body) => {
                // Function and datatype case are preserve in React's
                // formatter options, independently of the keyword case.
                self.add([Text(normalized_space(name.text))])?;
                self.nodes(comments)?;
                self.group(body)
            }
            Node::InlineClause(head, body) => {
                self.add([Newline, Indent, self.keyword(*head), Space])?;
                self.nodes(body)
            }
            Node::Clause(head, body) => {
                self.add([Newline, Indent, self.keyword(*head), Newline])?;
                self.indentation += 1;
                self.add([Indent])?;
                self.nodes(body)?;
                self.indentation -= 1;
                Ok(())
            }
            Node::Set(head, body) => {
                self.add([Newline, Indent, self.keyword(*head), Newline, Indent])?;
                self.nodes(body)
            }
            Node::Limit(head, comments, body) => {
                self.add([Newline, Indent, self.keyword(*head)])?;
                self.nodes(comments)?;
                self.indentation += 1;
                self.add([Newline, Indent])?;
                for part in body {
                    check(self.deadline)?;
                    if matches!(part, Node::Atom(a) if a.punctuation(",")) {
                        self.add([NoSpace, Part::text(","), Space])?;
                    } else {
                        self.node(part)?;
                    }
                }
                self.indentation -= 1;
                Ok(())
            }
            Node::Between(head, lower, and, upper) => {
                self.add([self.keyword(*head), Space])?;
                self.nodes(lower)?;
                self.add([NoSpace, Space, self.keyword(*and), Space])?;
                self.nodes(upper)?;
                self.add([Space])
            }
            Node::Case(head, subject, branches, end) => {
                self.add([self.keyword(*head), Space])?;
                self.indentation += 1;
                self.nodes(subject)?;
                for branch in branches {
                    check(self.deadline)?;
                    match branch {
                        Branch::When(when, condition, then, result) => {
                            self.add([Newline, Indent, self.keyword(*when), Space])?;
                            self.nodes(condition)?;
                            self.add([self.keyword(*then), Space])?;
                            self.nodes(result)?;
                        }
                        Branch::Else(other, result) => {
                            self.add([Newline, Indent, self.keyword(*other), Space])?;
                            self.nodes(result)?;
                        }
                    }
                }
                self.indentation -= 1;
                self.add([Newline, Indent, self.keyword(*end), Space])
            }
        }
    }

    fn append(self, into: &mut String) -> Result<(), Error> {
        for p in self.parts {
            check(self.deadline)?;
            match p {
                Part::Text(s) => into.push_str(&s),
                Part::Space => into.push(' '),
                Part::Newline | Part::MandatoryNewline => into.push('\n'),
                Part::SingleIndent => into.push_str(self.indent),
                _ => return Err(Error::Unsupported),
            }
            if into.len() > self.output_limit.saturating_add(MAX_DEPTH * 8 + 128) {
                return Err(Error::OutputLimit);
            }
        }
        Ok(())
    }
}

pub(crate) fn format_tokens(
    tokens: &[Token<'_>], options: &Options, output_limit: usize, deadline: Instant,
) -> Result<String, Error> {
    check(deadline)?;
    format_admitted_tokens(tokens, options, output_limit, deadline)
}

fn format_admitted_tokens(
    tokens: &[Token<'_>], options: &Options, output_limit: usize, deadline: Instant,
) -> Result<String, Error> {
    let indent = match options.indent {
        Indent::Spaces(2) => "  ",
        Indent::Spaces(4) => "    ",
        Indent::Tabs => "\t",
        _ => return Err(Error::InvalidOptions),
    };
    let mut parser = Parser { tokens, pos: 0, deadline, dialect: options.dialect };
    let mut output = String::new();
    while parser.pos < tokens.len() {
        check(deadline)?;
        let nodes = parser.region(0, true)?;
        if nodes.is_empty() {
            return Err(Error::Unsupported);
        }
        if !output.is_empty() {
            let gaps = usize::from(options.lines_between_queries) + 1;
            if output.len().checked_add(gaps).is_none_or(|size| size > output_limit) {
                return Err(Error::OutputLimit);
            }
            output.extend(std::iter::repeat_n('\n', gaps));
        }
        let mut layout = Layout::new(options, indent, output_limit, deadline, false);
        let painted = layout.nodes(&nodes).and_then(|_| {
            if parser.peek().is_some() {
                parser.expect_punctuation(";")?;
                layout.add([Part::NoNewline, Part::text(";")])?;
            }
            Ok(())
        });
        match painted {
            Ok(()) => layout.append(&mut output)?,
            Err(PaintError::Core(error)) => return Err(error),
            Err(PaintError::NotInline) => return Err(Error::Unsupported),
        }
    }
    output.truncate(output.trim_end_matches(js_space).len());
    if output.len() > output_limit {
        return Err(Error::OutputLimit);
    }
    check(deadline)?;
    Ok(output)
}

#[cfg(test)]
mod tests;
