use super::types::DroppedTable;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableSchemaLike {
    pub present: bool,
}

pub trait EditorHandle {
    fn get_document(&self) -> String;

    fn raw_insert(&mut self, text: &str, pos: usize);

    fn focus(&mut self);

    fn insert_at(&mut self, text: &str, pos: Option<usize>);
}

pub trait DropHost {
    fn format_table_identifier(&self, table: &str, db_type: &str, schema: Option<&str>) -> String;

    fn fetch_table_schema(
        &self,
        db_session_id: &str,
        connection_id: &str,
        table: &str,
        schema: Option<&str>,
        database: &str,
        db_type: &str,
    ) -> Option<TableSchemaLike>;

    fn generate_sql(
        &self,
        schema: Option<&TableSchemaLike>,
        db_type: &str,
        schema_prefix: Option<&str>,
        table_name: &str,
        table_ref_label: &str,
    ) -> String;
}

#[derive(Clone, Debug, PartialEq)]
pub struct DropOptions {
    pub connection_id: String,
    pub db_session_id: String,
    pub database_type: Option<String>,
    pub database: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DropEffect {
    RawInsert { text: String, pos: usize },
    Focus,
    InsertAt { text: String, pos: Option<usize> },
}

fn is_word(char: char) -> bool {
    char.is_ascii_alphanumeric() || char == '_'
}

pub fn drop_pane_key(panel_id: &str, pane_id: Option<&str>) -> String {
    match pane_id {
        None | Some("main") => panel_id.to_owned(),
        Some(pane_id) => format!("{panel_id}::{pane_id}"),
    }
}

fn trailing_join_or_from(trimmed: &str) -> bool {
    let upper = trimmed.to_ascii_uppercase();
    if upper.ends_with("JOIN") || upper.ends_with("FROM") {
        let start = upper.len() - 4;
        if start == 0 || !is_word(upper.chars().nth(start - 1).unwrap_or(' ')) {
            return true;
        }
    }
    let tokens = upper.split_whitespace().collect::<Vec<_>>();
    if let Some(last) = tokens.last() {
        if *last == "JOIN" || *last == "FROM" {
            return true;
        }
    }
    false
}

fn comma_after_from(trimmed: &str) -> bool {
    let upper = trimmed.to_ascii_uppercase();
    let Some(at) = upper.find("FROM") else {
        return false;
    };
    let before_ok = at == 0 || !is_word(upper.chars().nth(at - 1).unwrap_or(' '));
    let after_at = at + 4;
    let after_ok = upper
        .chars()
        .nth(after_at)
        .map(|char| !is_word(char))
        .unwrap_or(true);
    if !before_ok || !after_ok {
        return false;
    }
    let tail = &upper[after_at..];
    if !tail.trim_end().ends_with(',') {
        return false;
    }
    let last_comma = tail.rfind(',').unwrap_or(0);
    !tail[..last_comma].contains(',')
}

fn trailing_as(trimmed: &str) -> bool {
    let upper = trimmed.to_ascii_uppercase();
    if !upper.ends_with("AS") {
        return false;
    }
    let start = upper.len() - 2;
    start == 0 || !is_word(upper.chars().nth(start - 1).unwrap_or(' '))
}

pub fn detect_drop_context(doc: &str, pos: usize, qualified_name: &str) -> (String, bool) {
    if doc.trim().is_empty() {
        return (format!("SELECT * FROM {qualified_name} "), true);
    }
    let boundary = pos.min(doc.len());
    let before = &doc[..boundary];
    let after = &doc[boundary..];
    let trimmed = before.trim_end();
    if trailing_join_or_from(trimmed) || comma_after_from(trimmed) || trailing_as(trimmed) {
        return (format!(" {qualified_name} "), true);
    }
    let leading = if !before.is_empty()
        && !before
            .chars()
            .last()
            .map(char::is_whitespace)
            .unwrap_or(true)
    {
        " "
    } else {
        ""
    };
    let trailing = if !after.is_empty()
        && !after
            .chars()
            .next()
            .map(char::is_whitespace)
            .unwrap_or(true)
    {
        " "
    } else {
        ""
    };
    (format!("{leading}{qualified_name}{trailing}"), true)
}

pub fn create_query_drop_handler(
    options: &DropOptions,
    tables: &[DroppedTable],
    pos: Option<usize>,
    editor: &mut dyn EditorHandle,
    host: &dyn DropHost,
) -> Vec<DropEffect> {
    let db_type = if options.database_type.as_deref().unwrap_or("").is_empty() {
        "sqlite"
    } else {
        options.database_type.as_deref().unwrap_or("sqlite")
    };
    let target_session = if options.db_session_id.is_empty() {
        options.connection_id.as_str()
    } else {
        options.db_session_id.as_str()
    };
    let doc = editor.get_document();
    if !doc.trim().is_empty() {
        let drop_pos = pos.unwrap_or(doc.len());
        let mut effects = Vec::new();
        for table in tables {
            let display =
                host.format_table_identifier(&table.table_name, db_type, table.schema.as_deref());
            let (text, _) = detect_drop_context(&doc, drop_pos, &display);
            editor.raw_insert(&text, drop_pos);
            effects.push(DropEffect::RawInsert {
                text,
                pos: drop_pos,
            });
        }
        editor.focus();
        effects.push(DropEffect::Focus);
        return effects;
    }

    let mut generated = Vec::new();
    for table in tables {
        let table_ref = match &table.schema {
            Some(schema) => format!("{schema}.{}", table.table_name),
            None => table.table_name.clone(),
        };
        let schema = if options.database.is_empty() {
            None
        } else {
            host.fetch_table_schema(
                target_session,
                &options.connection_id,
                &table.table_name,
                table.schema.as_deref(),
                &options.database,
                db_type,
            )
        };
        generated.push(host.generate_sql(
            schema.as_ref(),
            db_type,
            table.schema.as_deref(),
            &table.table_name,
            &table_ref,
        ));
    }
    let combined = generated.join("\n\n");
    if combined.is_empty() {
        return Vec::new();
    }
    editor.insert_at(&combined, pos);
    vec![DropEffect::InsertAt {
        text: combined,
        pos,
    }]
}
