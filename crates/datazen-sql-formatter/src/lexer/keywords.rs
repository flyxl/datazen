//! The original tokenizer checks rule groups in order and longest alternatives
//! inside each group. Immutable tables preserve both before any layout.

use super::{is_ws, skip_ws, KeywordKind};
use crate::Dialect;

type Candidate = (&'static str, KeywordKind, bool);
type Group = (&'static str, &'static [Candidate]);

mod mariadb;
mod mysql;
mod postgresql;
mod sql;
mod sqlite;
mod transactsql;

fn groups(dialect: Dialect) -> &'static [Group] {
    match dialect {
        Dialect::Sql => sql::GROUPS,
        Dialect::Sqlite => sqlite::GROUPS,
        Dialect::PostgreSql => postgresql::GROUPS,
        Dialect::MySql => mysql::GROUPS,
        Dialect::MariaDb => mariadb::GROUPS,
        Dialect::TransactSql => transactsql::GROUPS,
    }
}

fn js_ascii_fold(c: char) -> Option<u8> {
    // These are the two non-ASCII simple case folds that join ASCII in an
    // ECMAScript Unicode case-insensitive word boundary.
    match c {
        '\u{17f}' => Some(b'S'),
        '\u{212a}' => Some(b'K'),
        c if c.is_ascii() => Some((c as u8).to_ascii_uppercase()),
        _ => None,
    }
}

fn word_boundary(text: &str, dialect: Dialect, special: bool) -> bool {
    let Some(c) = text.chars().next() else { return true; };
    let is_word = js_ascii_fold(c).is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_');
    let explicit_ident_char = !special && match dialect {
        Dialect::PostgreSql | Dialect::MySql | Dialect::MariaDb => c == '$',
        Dialect::TransactSql => matches!(c, '#' | '@' | '$'),
        _ => false,
    };
    !is_word && !explicit_ident_char
}

fn match_alternative(text: &str, literal: &str, dialect: Dialect, special: bool) -> Option<usize> {
    let mut tail = text;
    for (index, part) in literal.split(' ').enumerate() {
        if index != 0 {
            if !tail.starts_with(is_ws) {
                return None;
            }
            tail = &tail[skip_ws(tail)..];
        }
        for expected in part.bytes() {
            let next = tail.chars().next()?;
            if js_ascii_fold(next) != Some(expected) {
                return None;
            }
            tail = &tail[next.len_utf8()..];
        }
    }
    word_boundary(tail, dialect, special).then_some(text.len() - tail.len())
}

pub(super) fn keyword(text: &str, dialect: Dialect) -> Option<(usize, KeywordKind)> {
    let mut key = [0; 64];
    let count = text.chars()
        .map_while(js_ascii_fold)
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'@' | b'#' | b'$'))
        .take(key.len() + 1)
        .enumerate()
        .try_fold(0, |_, (i, c)| {
            *key.get_mut(i)? = c;
            Some(i + 1)
        })?;
    if count == 0 { return None; }
    let key = std::str::from_utf8(&key[..count]).ok()?;
    let table = groups(dialect);
    let find = |key: &str| {
        let index = table.binary_search_by(|entry| entry.0.cmp(&key)).ok()?;
        table[index].1.iter().find_map(|&(alternative, kind, special)| {
            match_alternative(text, alternative, dialect, special).map(|matched| (matched, kind))
        })
    };
    find(key).or_else(|| {
        // The separately pinned CASE/END/BETWEEN/AND/etc. rules use a plain
        // JS word boundary; the dialect-specific extra identifier characters
        // are only excluded by the ordinary reserved-word rules.
        let extra = key.find(['#', '@', '$'])?;
        if extra == 0 { return None; }
        find(&key[..extra])
    })
}
