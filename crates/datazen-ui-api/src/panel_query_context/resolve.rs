use std::collections::BTreeMap;

use super::namespace::{
    build_path_hierarchy_database_pin, namespace_child_names, namespace_roots_from,
    path_hierarchy_connection_root, path_hierarchy_relative_namespace_path, SqlNamespace,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueryPanelContext {
    pub db_session_id: String,
    pub database_type: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub namespace_path: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemaStateForPanel {
    pub databases: Vec<String>,
    pub path_aliases: BTreeMap<String, String>,
    pub namespace_tree: SqlNamespace,
    pub current_database: Option<String>,
    pub current_schema: Option<String>,
}

fn is_bare_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_bare_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

fn parse_name_part(chars: &[char], index: usize) -> Option<(String, usize)> {
    let ch = *chars.get(index)?;
    match ch {
        '`' | '"' | '[' => {
            let closing = match ch {
                '`' => '`',
                '"' => '"',
                _ => ']',
            };
            let mut cursor = index + 1;
            while cursor < chars.len() && chars[cursor] != closing {
                cursor += 1;
            }
            if cursor >= chars.len() || cursor == index + 1 {
                return None;
            }
            let text: String = chars[index + 1..cursor].iter().collect();
            Some((text, cursor + 1))
        }
        other if is_bare_start(other) => {
            let mut cursor = index;
            while cursor < chars.len() && is_bare_continue(chars[cursor]) {
                cursor += 1;
            }
            let text: String = chars[index..cursor].iter().collect();
            Some((text, cursor))
        }
        _ => None,
    }
}

fn qualified_name_parts(sql: &str) -> Option<Vec<String>> {
    let chars: Vec<char> = sql.chars().collect();
    for index in 0..chars.len() {
        let (first, mut end) = match parse_name_part(&chars, index) {
            Some(value) => value,
            None => continue,
        };
        let mut parts = vec![first];
        loop {
            let mut cursor = end;
            while cursor < chars.len() && chars[cursor].is_whitespace() {
                cursor += 1;
            }
            if cursor >= chars.len() || chars[cursor] != '.' {
                break;
            }
            cursor += 1;
            while cursor < chars.len() && chars[cursor].is_whitespace() {
                cursor += 1;
            }
            match parse_name_part(&chars, cursor) {
                Some((part, next)) => {
                    parts.push(part);
                    end = next;
                }
                None => break,
            }
        }
        if parts.len() >= 2 {
            return Some(parts);
        }
    }
    None
}

fn strip_quote(part: &str) -> String {
    part.trim()
        .trim_start_matches(|ch| ch == '`' || ch == '"' || ch == '[')
        .trim_end_matches(|ch| ch == '`' || ch == '"' || ch == ']')
        .to_owned()
}

fn sql_qualified_namespace_path(
    sql: Option<&str>,
    databases: &[String],
    namespace_tree: &SqlNamespace,
    path_aliases: &BTreeMap<String, String>,
) -> Option<Vec<String>> {
    let sql = sql?;
    if sql.is_empty() {
        return None;
    }
    let matched = qualified_name_parts(sql)?;
    let parts: Vec<String> = matched.iter().map(|part| strip_quote(part)).collect();
    let head = parts.first()?;
    let rest = &parts[1..];
    let roots = namespace_roots_from(namespace_tree, path_aliases, databases);
    let head_is_known = roots.contains(head)
        || databases
            .iter()
            .any(|database| database.split(':').nth(1) == Some(head.as_str()));
    if !head_is_known || rest.is_empty() {
        return None;
    }
    let mut result = vec![head.clone()];
    result.extend(rest[..rest.len() - 1].iter().cloned());
    Some(result)
}

pub fn panel_target_database(
    panel: &QueryPanelContext,
    schema_state: &SchemaStateForPanel,
    path_hierarchy: bool,
    sql: Option<&str>,
) -> Option<String> {
    if path_hierarchy {
        let databases = &schema_state.databases;
        let aliases = &schema_state.path_aliases;
        let tree = &schema_state.namespace_tree;
        let root_names = namespace_child_names(tree, &[]);
        let selected =
            path_hierarchy_relative_namespace_path(databases, tree, &panel.namespace_path);
        let from_sql = sql_qualified_namespace_path(sql, databases, tree, aliases);
        let extends_selection = selected.is_empty()
            || (from_sql
                .as_ref()
                .is_some_and(|path| path.len() > selected.len())
                && from_sql.as_ref().is_some_and(|path| {
                    selected
                        .iter()
                        .zip(path.iter())
                        .all(|(left, right)| left == right)
                }));
        let relative = if extends_selection
            && from_sql
                .as_ref()
                .is_some_and(|path| path.len() > selected.len())
        {
            from_sql.unwrap_or_default()
        } else {
            selected
        };
        let root = path_hierarchy_connection_root(
            databases,
            panel.database.as_deref(),
            schema_state.current_database.as_deref(),
            aliases,
            &root_names,
        );
        if root.is_none() && relative.is_empty() {
            return None;
        }
        let root = match root {
            Some(root) => root,
            None => return Some(relative.join("/")),
        };
        return Some(build_path_hierarchy_database_pin(&root, &relative));
    }
    if let Some(database) = panel
        .database
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return Some(database.to_owned());
    }
    schema_state.current_database.clone()
}

pub fn panel_target_schema(
    panel: &QueryPanelContext,
    schema_state: &SchemaStateForPanel,
) -> Option<String> {
    if let Some(schema) = panel
        .schema
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return Some(schema.to_owned());
    }
    schema_state.current_schema.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> SqlNamespace {
        SqlNamespace::branch(&[
            (
                "hive",
                SqlNamespace::branch(&[("snap", SqlNamespace::leaf())]),
            ),
            ("audit", SqlNamespace::leaf()),
        ])
    }

    fn panel() -> QueryPanelContext {
        QueryPanelContext {
            db_session_id: "s1".to_owned(),
            database_type: "superset".to_owned(),
            database: Some("hive".to_owned()),
            schema: None,
            namespace_path: Vec::new(),
        }
    }

    fn schema_state() -> SchemaStateForPanel {
        let mut aliases = BTreeMap::new();
        aliases.insert("hive".to_owned(), "558".to_owned());
        SchemaStateForPanel {
            databases: vec!["558:hive".to_owned()],
            path_aliases: aliases,
            namespace_tree: tree(),
            current_database: None,
            current_schema: None,
        }
    }

    #[test]
    fn qualified_names_accept_quotes_brackets_and_spaced_dots() {
        let databases = ["hive".to_owned()];
        let aliases = BTreeMap::new();
        let cases = [
            "select * from hive.snap.orders",
            "select * from `hive` . \"snap\" . [orders]",
        ];
        for sql in cases {
            assert_eq!(
                sql_qualified_namespace_path(Some(sql), &databases, &tree(), &aliases),
                Some(vec!["hive".to_owned(), "snap".to_owned()]),
                "{sql}"
            );
        }
        assert_eq!(
            sql_qualified_namespace_path(None, &databases, &tree(), &aliases),
            None
        );
        assert_eq!(
            sql_qualified_namespace_path(Some("select 1"), &databases, &tree(), &aliases),
            None
        );
    }

    #[test]
    fn unknown_head_or_unbracketed_id_is_not_a_namespace_hint() {
        let databases = ["hive".to_owned()];
        let aliases = BTreeMap::new();
        assert_eq!(
            sql_qualified_namespace_path(
                Some("select * from other.snap.t"),
                &databases,
                &tree(),
                &aliases
            ),
            None
        );
        assert_eq!(
            sql_qualified_namespace_path(
                Some("select * from orders"),
                &databases,
                &tree(),
                &aliases
            ),
            None
        );
    }

    #[test]
    fn non_path_hierarchy_uses_panel_then_current_database() {
        let panel = panel();
        let mut state = schema_state();
        state.current_database = Some("558:hive".to_owned());
        assert_eq!(
            panel_target_database(&panel, &state, false, None).as_deref(),
            Some("hive")
        );
        let mut no_database = panel.clone();
        no_database.database = Some("  ".to_owned());
        assert_eq!(
            panel_target_database(&no_database, &state, false, None).as_deref(),
            Some("558:hive")
        );
        let mut empty = SchemaStateForPanel::default();
        empty.current_database = Some("fallback".to_owned());
        assert_eq!(
            panel_target_database(&no_database, &empty, false, None).as_deref(),
            Some("fallback")
        );
    }

    #[test]
    fn path_hierarchy_sql_extends_the_explicit_selection() {
        let panel = panel();
        let state = schema_state();
        let pin =
            panel_target_database(&panel, &state, true, Some("select * from hive.snap.orders"));
        assert_eq!(pin.as_deref(), Some("558:hive/hive/snap"));
    }

    #[test]
    fn path_hierarchy_relative_path_without_root_is_a_plain_relative_path() {
        let panel = panel();
        let mut state = schema_state();
        state.databases.clear();
        state.current_database = None;
        let mut item = panel.clone();
        item.database = None;
        item.namespace_path = vec!["audit".to_owned()];
        let result = panel_target_database(&item, &state, true, None);
        assert_eq!(result.as_deref(), Some("audit"));
    }

    #[test]
    fn schema_uses_panel_then_current_schema() {
        let mut item = panel();
        item.schema = Some(" public ".to_owned());
        let mut state = schema_state();
        state.current_schema = Some("fallback".to_owned());
        assert_eq!(
            panel_target_schema(&item, &state).as_deref(),
            Some(" public ")
        );
        item.schema = None;
        assert_eq!(
            panel_target_schema(&item, &state).as_deref(),
            Some("fallback")
        );
    }
}
