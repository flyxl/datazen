//! Port of `packages/frontend/src/components/sql-editor/snippets/
//! builtinSnippets.ts` (T347).
//!
//! Built-in high-frequency SQL snippet library. Dialect-neutral on
//! purpose: anything dialect-specific belongs in a driver package,
//! not here.
//!
//! Every template ends with an empty terminal tabstop. CodeMirror
//! deactivates a snippet session as soon as the cursor reaches the
//! *last* field, so without a trailing stop the final meaningful
//! placeholder would end the session and Shift-Tab could no longer
//! walk backwards — the audit below pins that invariant for every
//! entry, plus the React journey suite's own pins (the `join`
//! template's repeated `${1:table_name}` placeholder).
//!
//! `Object.freeze` is a JS immutability gesture; the Rust face is a
//! shared `&'static [SqlSnippetItem]` (built once), which is already
//! immutable to its readers.
//!
//! Known deltas: none — the seven entries are the source's verbatim.

use crate::components::sql_editor::snippets::types::SqlSnippetItem;

/// The built-in snippet library, verbatim.
// consumers: the completion source (T359) and the settings card's
// import/export collision check; delete this allow when they land.
#[allow(dead_code)]
pub(crate) fn builtin_sql_snippets() -> &'static [SqlSnippetItem] {
    fn item(id: &str, prefix: &str, description_key: &str, template: &str) -> SqlSnippetItem {
        SqlSnippetItem {
            id: id.to_owned(),
            prefix: prefix.to_owned(),
            description_key: description_key.to_owned(),
            template: template.to_owned(),
        }
    }

    static BUILTIN: std::sync::OnceLock<Vec<SqlSnippetItem>> = std::sync::OnceLock::new();
    BUILTIN.get_or_init(|| {
        vec![
            item(
                "select-all",
                "sel*",
                "query.editor.snippet.selectAll",
                "SELECT *\nFROM ${1:table_name}\nWHERE ${2:condition};${3}",
            ),
            item(
                "select-columns",
                "selc",
                "query.editor.snippet.selectColumns",
                "SELECT ${2:columns}\nFROM ${1:table_name};${3}",
            ),
            item(
                "insert",
                "ins",
                "query.editor.snippet.insert",
                "INSERT INTO ${1:table_name} (${2:columns})\nVALUES (${3:values});${4}",
            ),
            item(
                "update",
                "upd",
                "query.editor.snippet.update",
                "UPDATE ${1:table_name}\nSET ${2:column} = ${3:value}\nWHERE ${4:condition};${5}",
            ),
            item(
                "delete",
                "del",
                "query.editor.snippet.delete",
                "DELETE FROM ${1:table_name}\nWHERE ${2:condition};${3}",
            ),
            item(
                "join",
                "join",
                "query.editor.snippet.join",
                "JOIN ${1:table_name} ON ${1:table_name}.${2:id} = ${3:other_table}.${4:fk_id}${5}",
            ),
            item(
                "count",
                "count",
                "query.editor.snippet.count",
                "SELECT COUNT(1) AS total\nFROM ${1:table_name};${2}",
            ),
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `${n}` — an empty terminal tabstop (no `:default`).
    fn ends_with_terminal_tabstop(template: &str) -> bool {
        let Some(open) = template.rfind("${") else { return false };
        let tail = &template[open..];
        tail.len() > 3
            && tail.starts_with("${")
            && tail.ends_with('}')
            && tail[2..tail.len() - 1].chars().all(|c| c.is_ascii_digit())
    }

    #[test]
    fn the_library_is_the_source_verbatim() {
        let all = builtin_sql_snippets();
        assert_eq!(all.len(), 7, "the React library has exactly seven entries");
        let prefixes: Vec<&str> = all.iter().map(|s| s.prefix.as_str()).collect();
        assert_eq!(prefixes, ["sel*", "selc", "ins", "upd", "del", "join", "count"]);
        let ids: Vec<&str> = all.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            ["select-all", "select-columns", "insert", "update", "delete", "join", "count"]
        );
        // One spot check per template body (the journey suite expands
        // `sel*` and `join`; the others are pinned by prefix+template).
        assert_eq!(
            all[0].template,
            "SELECT *\nFROM ${1:table_name}\nWHERE ${2:condition};${3}"
        );
        assert_eq!(all[1].template, "SELECT ${2:columns}\nFROM ${1:table_name};${3}");
        assert_eq!(
            all[5].template,
            "JOIN ${1:table_name} ON ${1:table_name}.${2:id} = ${3:other_table}.${4:fk_id}${5}"
        );
    }

    #[test]
    fn every_template_ends_with_an_empty_terminal_tabstop() {
        for snippet in builtin_sql_snippets() {
            assert!(
                ends_with_terminal_tabstop(&snippet.template),
                "`{}` must end with an empty terminal tabstop so Shift-Tab can walk back",
                snippet.prefix
            );
        }
    }

    #[test]
    fn ids_and_prefixes_are_unique() {
        let all = builtin_sql_snippets();
        let mut ids: Vec<&str> = all.iter().map(|s| s.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "ids are the import/export keys");
        let mut prefixes: Vec<&str> = all.iter().map(|s| s.prefix.as_str()).collect();
        prefixes.sort_unstable();
        prefixes.dedup();
        assert_eq!(prefixes.len(), all.len(), "two builtins must not claim one prefix");
    }

    #[test]
    fn the_join_template_links_its_repeated_placeholder() {
        // The journey suite's link pin: `join` reuses ${1:table_name}
        // twice — one edit updates both instances.
        let join = builtin_sql_snippets()
            .iter()
            .find(|s| s.prefix == "join")
            .expect("join builtin");
        assert_eq!(join.template.matches("${1:table_name}").count(), 2);
    }

    #[test]
    fn description_keys_are_i18n_dotted_keys() {
        for snippet in builtin_sql_snippets() {
            assert!(
                snippet.description_key.starts_with("query.editor.snippet."),
                "`{}` description key must be the i18n dotted path",
                snippet.id
            );
        }
    }
}
