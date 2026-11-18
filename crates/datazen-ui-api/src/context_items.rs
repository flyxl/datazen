//! AI-chat context splitting, ported from
//! `packages/frontend/src/lib/contextItems.ts`.
//!
//! `splitContextItems` is the bridge the AI panels use when handing a chat
//! request over IPC: tables travel as table ids, files and directories
//! travel as paths. The `ContextItem`/`ContextKind` vocabulary comes from
//! `packages/frontend/src/types/index.ts` (duplicated in
//! `types/connection.ts`); this module is its Rust home so the later
//! `types/` tasks (tasklist T213/T130) reuse it instead of redefining it.
//!
//! The kinds are a closed set in Rust: React's third branch (an unknown kind
//! is skipped) cannot occur, so the match is exhaustive with no ignore arm.

/// What a context entry is: a table, a file, or a directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextKind {
    File,
    Dir,
    Table,
}

/// One item the assistant may read: a table, a file, or a directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextItem {
    pub kind: ContextKind,
    /// What `splitContextItems` keys tables by.
    pub id: String,
    pub name: String,
    /// What files and directories travel as; absent entries fall back to `id`.
    pub path: Option<String>,
    /// Carried for the picker's display contract; the split ignores it.
    pub database: Option<String>,
}

/// The split a chat request sends: table ids, then file/dir paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SplitContextItems {
    pub context_files: Vec<String>,
    pub context_tables: Vec<String>,
}

/// Split context items into the table ids and file/dir paths the request
/// carries. Order inside each list follows the input order.
pub fn split_context_items(items: &[ContextItem]) -> SplitContextItems {
    let mut context_files = Vec::new();
    let mut context_tables = Vec::new();
    for it in items {
        match it.kind {
            ContextKind::Table => context_tables.push(it.id.clone()),
            ContextKind::File | ContextKind::Dir => {
                context_files.push(it.path.clone().unwrap_or_else(|| it.id.clone()));
            }
        }
    }
    SplitContextItems { context_files, context_tables }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: ContextKind, id: &str, path: Option<&str>) -> ContextItem {
        ContextItem {
            kind,
            id: id.into(),
            name: id.into(),
            path: path.map(str::to_owned),
            database: None,
        }
    }

    #[test]
    fn splits_tables_and_files() {
        // The React test: one table, one file with a path.
        let r = split_context_items(&[
            item(ContextKind::Table, "users", None),
            item(ContextKind::File, "a.sql", Some("a.sql")),
        ]);
        assert_eq!(r.context_tables, vec!["users".to_owned()]);
        assert_eq!(r.context_files, vec!["a.sql".to_owned()]);
    }

    #[test]
    fn an_entry_without_a_path_falls_back_to_its_id() {
        let r = split_context_items(&[
            item(ContextKind::Dir, "/tmp/project", None),
            item(ContextKind::File, "file:1", Some("b.sql")),
        ]);
        // A directory with no path and a file whose path beats its id.
        assert_eq!(r.context_files, vec!["/tmp/project".to_owned(), "b.sql".to_owned()]);
        assert!(r.context_tables.is_empty());
    }

    #[test]
    fn interleaved_input_keeps_each_list_in_input_order() {
        let r = split_context_items(&[
            item(ContextKind::File, "one.sql", Some("one.sql")),
            item(ContextKind::Table, "t1", None),
            item(ContextKind::Dir, "d", None),
            item(ContextKind::Table, "t2", None),
        ]);
        assert_eq!(r.context_files, vec!["one.sql".to_owned(), "d".to_owned()]);
        assert_eq!(r.context_tables, vec!["t1".to_owned(), "t2".to_owned()]);
    }

    #[test]
    fn no_items_split_into_nothing() {
        let r = split_context_items(&[]);
        assert_eq!(
            r,
            SplitContextItems { context_files: Vec::new(), context_tables: Vec::new() }
        );
    }
}
