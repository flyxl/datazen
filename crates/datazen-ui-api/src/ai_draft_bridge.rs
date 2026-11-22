#[cfg(test)]
mod tests;

pub const AI_CHAT_DRAFT_FOCUS: bool = true;

pub const AI_DRAFT_BRIDGE_SURFACE: [&str; 8] = [
    "AiChatDraftSource",
    "AiChatDraftRequest",
    "ContentViewCallbacks",
    "OpenRelationSubTab",
    "TableContextInput",
    "TableSqlActionSpec",
    "QueryOpenSource",
    "QueryOpenFocus",
];

pub const AI_DRAFT_BRIDGE_GAP: [&str; 4] = [
    "React callback threading through ContentView/PanelContentRenderer/QueryPanel",
    "AI chat textarea write and onDraftConsumed acknowledgement",
    "crypto.randomUUID request ids",
    "conflict replace/append/cancel UI for a non-empty textarea",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AiChatDraftSource {
    QueryError,
}

impl AiChatDraftSource {
    pub fn as_str(self) -> &'static str {
        match self {
            AiChatDraftSource::QueryError => "query-error",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiChatDraftRequest {
    pub request_id: String,
    pub source: AiChatDraftSource,
    pub panel_id: String,
    pub connection_id: String,
    pub db_session_id: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub context_fingerprint: String,
    pub content: String,
    pub focus: bool,
}

impl AiChatDraftRequest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_id: String,
        source: AiChatDraftSource,
        panel_id: String,
        connection_id: String,
        db_session_id: String,
        database: Option<String>,
        schema: Option<String>,
        context_fingerprint: String,
        content: String,
    ) -> Self {
        Self {
            request_id,
            source,
            panel_id,
            connection_id,
            db_session_id,
            database,
            schema,
            context_fingerprint,
            content,
            focus: AI_CHAT_DRAFT_FOCUS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenRelationSubTab {
    Data,
    Structure,
    Ddl,
}

impl OpenRelationSubTab {
    pub fn as_str(self) -> &'static str {
        match self {
            OpenRelationSubTab::Data => "data",
            OpenRelationSubTab::Structure => "structure",
            OpenRelationSubTab::Ddl => "ddl",
        }
    }
}

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
pub enum TableSqlActionKind {
    OpenData,
    Select,
    Insert,
    Update,
    Ddl,
}

impl TableSqlActionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TableSqlActionKind::OpenData => "openData",
            TableSqlActionKind::Select => "select",
            TableSqlActionKind::Insert => "insert",
            TableSqlActionKind::Update => "update",
            TableSqlActionKind::Ddl => "ddl",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableContextInput {
    pub connection_id: String,
    pub db_session_id: String,
    pub database_type: String,
    pub database: String,
    pub schema: Option<String>,
    pub table_name: Option<String>,
    pub table_schema: Option<String>,
    pub view_name: Option<String>,
    pub view_schema: Option<String>,
    pub object_type: Option<String>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableSqlActionSpec {
    pub kind: TableSqlActionKind,
    pub source: Option<QueryOpenSource>,
    pub initial_sql: Option<String>,
    pub focus: Option<QueryOpenFocus>,
}

pub struct ContentViewCallbacks<'a> {
    pub open_relation: &'a dyn Fn(&str, Option<&str>, &str, Option<OpenRelationSubTab>, Option<&str>),
    pub open_ai_chat_draft: &'a dyn Fn(&AiChatDraftRequest),
    pub open_sql_editor_for_table: Option<&'a dyn Fn(&TableContextInput, &TableSqlActionSpec)>,
}
