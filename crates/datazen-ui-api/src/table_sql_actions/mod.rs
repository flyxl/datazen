#[cfg(test)]
mod tests;

use crate::{DatabaseRegistry, TableContextInput, TableSqlActionKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryOpenSource {
    TableAction,
    ObjectSearch,
    AiAction,
}

impl QueryOpenSource {
    pub fn as_str(self) -> &'static str {
        match self {
            QueryOpenSource::TableAction => "table-action",
            QueryOpenSource::ObjectSearch => "object-search",
            QueryOpenSource::AiAction => "ai-action",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryOpenFocus {
    Editor,
    Result,
}

impl QueryOpenFocus {
    pub fn as_str(self) -> &'static str {
        match self {
            QueryOpenFocus::Editor => "editor",
            QueryOpenFocus::Result => "result",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlActionExecution {
    DraftOnly,
}

impl SqlActionExecution {
    pub fn as_str(self) -> &'static str {
        match self {
            SqlActionExecution::DraftOnly => "draft-only",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableContext {
    pub connection_id: String,
    pub db_session_id: String,
    pub database_type: String,
    pub database: String,
    pub schema: Option<String>,
    pub table_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryOpenContext {
    pub connection_id: String,
    pub db_session_id: String,
    pub database_type: String,
    pub database: String,
    pub schema: Option<String>,
    pub table_name: String,
    pub source: QueryOpenSource,
    pub action: TableSqlActionKind,
    pub initial_sql: Option<String>,
    pub focus: QueryOpenFocus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableSqlActionSpec {
    pub kind: TableSqlActionKind,
    pub source: Option<QueryOpenSource>,
    pub initial_sql: Option<String>,
    pub focus: Option<QueryOpenFocus>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableSqlAction {
    pub id: TableSqlActionKind,
    pub kind: TableSqlActionKind,
    pub label: String,
    pub description: String,
    pub context: QueryOpenContext,
    pub query_open_context: QueryOpenContext,
    pub sql_template: Option<String>,
    pub execution: SqlActionExecution,
    pub requires_driver_generation: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum TableActionInput<'a> {
    Kind(TableSqlActionKind),
    Spec(&'a TableSqlActionSpec),
}

impl<'a> From<TableSqlActionKind> for TableActionInput<'a> {
    fn from(kind: TableSqlActionKind) -> Self {
        TableActionInput::Kind(kind)
    }
}

impl<'a> From<&'a TableSqlActionSpec> for TableActionInput<'a> {
    fn from(spec: &'a TableSqlActionSpec) -> Self {
        TableActionInput::Spec(spec)
    }
}

pub const TABLE_SQL_ACTION_KINDS: [TableSqlActionKind; 5] = [
    TableSqlActionKind::OpenData,
    TableSqlActionKind::Select,
    TableSqlActionKind::Insert,
    TableSqlActionKind::Update,
    TableSqlActionKind::Ddl,
];

fn action_copy(kind: TableSqlActionKind) -> (&'static str, &'static str, bool) {
    match kind {
        TableSqlActionKind::OpenData => (
            "Open Data",
            "Open the existing Table Panel data view.",
            false,
        ),
        TableSqlActionKind::Select => (
            "SELECT",
            "Open a read-only SELECT draft for the table.",
            false,
        ),
        TableSqlActionKind::Insert => (
            "INSERT",
            "Open an INSERT draft with driver-specific columns and values to be supplied.",
            true,
        ),
        TableSqlActionKind::Update => (
            "UPDATE",
            "Open an UPDATE draft with a driver-specific assignment and predicate.",
            true,
        ),
        TableSqlActionKind::Ddl => (
            "DDL",
            "Open the driver-generated DDL entry point for the table.",
            true,
        ),
    }
}

fn normalize_required(value: Option<&str>, field: &str) -> Result<String, String> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => Ok(value.to_owned()),
        None => Err(format!("Missing {field} for table action")),
    }
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn read_table_name(input: &TableContextInput) -> Result<String, String> {
    let object_type = input
        .object_type
        .as_deref()
        .map(str::trim)
        .map(|value| value.to_lowercase());
    if let Some(kind) = object_type.as_deref() {
        if !["table", "view", "column", "materializedview"].contains(&kind) {
            return Err(format!(
                "Object type {} cannot produce a table context",
                input.object_type.as_deref().unwrap_or("")
            ));
        }
    }
    let name = input
        .table_name
        .as_deref()
        .or(input.view_name.as_deref())
        .or(input.name.as_deref());
    normalize_required(name, "tableName")
}

pub fn build_table_context(input: &TableContextInput) -> Result<TableContext, String> {
    let schema = input
        .schema
        .as_deref()
        .or(input.table_schema.as_deref())
        .or(input.view_schema.as_deref());
    Ok(TableContext {
        connection_id: normalize_required(Some(&input.connection_id), "connectionId")?,
        db_session_id: normalize_required(Some(&input.db_session_id), "dbSessionId")?,
        database_type: input.database_type.clone(),
        database: normalize_optional(Some(&input.database)).unwrap_or_default(),
        schema: normalize_optional(schema),
        table_name: read_table_name(input)?,
    })
}

pub fn quote_table_identifier(context: &TableContext, registry: &DatabaseRegistry) -> String {
    [context.schema.as_deref(), Some(context.table_name.as_str())]
        .into_iter()
        .flatten()
        .filter(|segment| !segment.is_empty())
        .map(|segment| crate::escape_ident(segment, Some(&context.database_type), registry))
        .collect::<Vec<_>>()
        .join(".")
}

pub fn build_safe_table_sql_template(
    context: &TableContext,
    action: TableSqlActionKind,
    registry: &DatabaseRegistry,
) -> Option<String> {
    let quoted_table = quote_table_identifier(context, registry);
    match action {
        TableSqlActionKind::OpenData => None,
        TableSqlActionKind::Select => Some(format!("SELECT * FROM {quoted_table};")),
        TableSqlActionKind::Insert => Some(format!(
            "INSERT INTO {quoted_table} /* driver-generated columns and values */;"
        )),
        TableSqlActionKind::Update => Some(format!(
            "UPDATE {quoted_table} SET /* driver-generated assignments */ WHERE /* driver-generated predicate */;"
        )),
        TableSqlActionKind::Ddl => Some(format!(
            "/* Driver-generated DDL for {quoted_table}; use the existing DDL command. */"
        )),
    }
}

pub fn build_no_primary_key_edit_template(
    context: &TableContext,
    registry: &DatabaseRegistry,
) -> String {
    let quoted_table = quote_table_identifier(context, registry);
    [
        "-- This table has no primary key: the grid cannot safely address a row.",
        "-- Select the rows first, then write the UPDATE/DELETE against the same predicate.",
        &format!("SELECT * FROM {quoted_table} WHERE /* e.g. id = 123 */;"),
    ]
    .join("\n")
}

pub fn no_primary_key_edit_spec(
    input: &TableContextInput,
    registry: &DatabaseRegistry,
) -> Result<TableSqlActionSpec, String> {
    let sql = build_no_primary_key_edit_template(&build_table_context(input)?, registry);
    Ok(TableSqlActionSpec {
        kind: TableSqlActionKind::Select,
        source: Some(QueryOpenSource::TableAction),
        initial_sql: Some(sql),
        focus: Some(QueryOpenFocus::Editor),
    })
}

fn normalize_action(action: TableActionInput<'_>) -> TableSqlActionSpec {
    match action {
        TableActionInput::Kind(kind) => TableSqlActionSpec {
            kind,
            source: None,
            initial_sql: None,
            focus: None,
        },
        TableActionInput::Spec(spec) => spec.clone(),
    }
}

pub fn build_query_open_context(
    table_context: &TableContextInput,
    action: TableActionInput<'_>,
    registry: &DatabaseRegistry,
) -> Result<QueryOpenContext, String> {
    let context = build_table_context(table_context)?;
    let spec = normalize_action(action);
    let initial_sql = spec
        .initial_sql
        .clone()
        .or_else(|| build_safe_table_sql_template(&context, spec.kind, registry));
    let focus = spec.focus.unwrap_or_else(|| {
        if spec.kind == TableSqlActionKind::OpenData {
            QueryOpenFocus::Result
        } else {
            QueryOpenFocus::Editor
        }
    });
    Ok(QueryOpenContext {
        connection_id: context.connection_id,
        db_session_id: context.db_session_id,
        database_type: context.database_type,
        database: context.database,
        schema: context.schema,
        table_name: context.table_name,
        source: spec.source.unwrap_or(QueryOpenSource::TableAction),
        action: spec.kind,
        initial_sql,
        focus,
    })
}

pub fn build_table_sql_action(
    table_context: &TableContextInput,
    action: TableActionInput<'_>,
    registry: &DatabaseRegistry,
) -> Result<TableSqlAction, String> {
    let spec = normalize_action(action);
    let query_open_context =
        build_query_open_context(table_context, TableActionInput::Spec(&spec), registry)?;
    let (label, description, requires_driver_generation) = action_copy(spec.kind);
    Ok(TableSqlAction {
        id: spec.kind,
        kind: spec.kind,
        label: label.to_owned(),
        description: description.to_owned(),
        context: query_open_context.clone(),
        sql_template: query_open_context.initial_sql.clone(),
        query_open_context,
        execution: SqlActionExecution::DraftOnly,
        requires_driver_generation,
    })
}

pub fn build_table_sql_actions(
    table_context: &TableContextInput,
    actions: Option<&[TableSqlActionKind]>,
    registry: &DatabaseRegistry,
) -> Result<Vec<TableSqlAction>, String> {
    let kinds = actions.unwrap_or(&TABLE_SQL_ACTION_KINDS);
    kinds
        .iter()
        .map(|kind| build_table_sql_action(table_context, TableActionInput::Kind(*kind), registry))
        .collect()
}
