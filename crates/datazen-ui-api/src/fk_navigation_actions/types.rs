use crate::{
    FilterCondition, ForeignKeyInfo, ForeignKeyTargetRef, ForeignKeyTargetResolution, GridViewport,
    ResolveForeignKeyTargetsRequest, Value,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableIdentity {
    pub database: String,
    pub schema: Option<String>,
    pub table: String,
}

impl TableIdentity {
    pub fn of_target(target: &ForeignKeyTargetRef) -> Self {
        Self {
            database: target.database.clone(),
            schema: target.schema.clone(),
            table: target.table.clone(),
        }
    }

    pub fn same(&self, other: &Self) -> bool {
        self.database == other.database && self.schema == other.schema && self.table == other.table
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkNavOrigin {
    pub source_panel_id: String,
    pub source_table: String,
    pub local_columns: Vec<String>,
    pub target_table: String,
    pub target_columns: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkNavEntry {
    pub id: String,
    pub panel_id: String,
    pub nav_key: String,
    pub origin: FkNavOrigin,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FkNavState {
    pub back: Vec<FkNavEntry>,
    pub by_nav_key: std::collections::BTreeMap<String, String>,
    pub rewinds: Vec<String>,
    pub next_entry_id: u64,
}

impl FkNavState {
    pub fn find_entry_id_by_nav_key(&self, nav_key: &str) -> Option<&str> {
        self.by_nav_key.get(nav_key).map(String::as_str)
    }

    pub fn push(&mut self, panel_id: &str, nav_key: &str, origin: FkNavOrigin) -> FkNavEntry {
        self.next_entry_id += 1;
        let entry = FkNavEntry {
            id: format!("fk-{}", self.next_entry_id),
            panel_id: panel_id.to_owned(),
            nav_key: nav_key.to_owned(),
            origin,
        };
        self.back.push(entry.clone());
        self.by_nav_key.insert(nav_key.to_owned(), entry.id.clone());
        entry
    }

    pub fn rewind_to(&mut self, entry_id: &str) {
        self.rewinds.push(entry_id.to_owned());
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelKind {
    Table,
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub id: String,
    pub kind: PanelKind,
    pub table_name: String,
    pub database: String,
    pub table_schema: Option<String>,
    pub connection_id: String,
    pub db_session_id: String,
    pub connection_name: String,
    pub database_type: Option<String>,
}

impl Panel {
    pub fn identity(&self) -> Option<TableIdentity> {
        if self.kind != PanelKind::Table {
            return None;
        }
        Some(TableIdentity {
            database: self.database.clone(),
            schema: self.table_schema.clone(),
            table: self.table_name.clone(),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PanelState {
    pub panels: Vec<Panel>,
    pub active_panel_id: Option<String>,
    pub added: Vec<Panel>,
    pub activated: Vec<String>,
    pub updated: Vec<(String, FkOrigin)>,
}

impl PanelState {
    pub fn set_active_panel(&mut self, panel_id: &str) {
        self.active_panel_id = Some(panel_id.to_owned());
        self.activated.push(panel_id.to_owned());
    }

    pub fn add_panel(&mut self, panel: Panel) {
        self.panels.push(panel.clone());
        self.added.push(panel);
    }

    pub fn update_panel(&mut self, panel_id: &str, origin: FkOrigin) {
        self.updated.push((panel_id.to_owned(), origin));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkOrigin {
    pub source_panel_id: String,
    pub source_table: String,
    pub local_columns: Vec<String>,
    pub target_table: String,
    pub target_columns: Vec<String>,
    pub nav_entry_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableChangeContext {
    pub connection_id: String,
    pub db_session_id: String,
    pub driver_type: Option<String>,
    pub database: String,
    pub schema: Option<String>,
    pub table: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FkActionEffects {
    pub recorded_viewports: Vec<(String, GridViewport)>,
    pub seeded_panels: Vec<(String, TableChangeContext, Vec<FilterCondition>)>,
    pub timeline: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OpenForeignKeyTargetInput {
    pub source_panel_id: String,
    pub source_table: String,
    pub db_session_id: String,
    pub database: String,
    pub schema: Option<String>,
    pub connection_id: Option<String>,
    pub driver_type: Option<String>,
    pub foreign_key: ForeignKeyInfo,
    pub values: Vec<Value>,
    pub source_viewport: Option<GridViewport>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkJumpError {
    pub code: String,
    pub raw: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkJumpOutcome {
    pub ok: bool,
    pub error: Option<FkJumpError>,
    pub panel_id: Option<String>,
    pub reused: bool,
    pub target_missing: bool,
}

impl FkJumpOutcome {
    pub fn refused(error: FkJumpError) -> Self {
        Self {
            ok: false,
            error: Some(error),
            panel_id: None,
            reused: false,
            target_missing: false,
        }
    }

    pub fn landed(panel_id: String, reused: bool, target_missing: bool) -> Self {
        Self {
            ok: true,
            error: None,
            panel_id: Some(panel_id),
            reused,
            target_missing,
        }
    }
}

pub const GRID_ERROR_CODES: &[&str] = &[
    "grid.commit.stalePlan",
    "grid.commit.affectedMismatch",
    "grid.commit.conflictingIntents",
    "grid.cell.noRowIdentity",
    "grid.cell.ambiguousIdentity",
    "grid.cell.readOnly",
    "grid.cell.invalidValue",
    "grid.insert.unknownColumn",
    "grid.insert.returningUnavailable",
    "grid.fk.shapeMismatch",
    "grid.fk.unknownConstraint",
    "grid.fk.valueNull",
    "grid.fk.targetUnresolved",
    "grid.fk.probeFailed",
    "grid.fk.noReferences",
    "grid.filter.unsupportedOperator",
    "grid.filter.incomplete",
    "grid.view.invalid",
    "grid.view.nameExists",
    "grid.result.originUnavailable",
    "grid.result.originAmbiguous",
    "grid.result.rowNotAddressable",
    "grid.result.multiRowNotAllowed",
    "grid.result.writeDenied",
    "grid.write.expressionUnsupported",
    "grid.paging.keysetUnavailable",
    "grid.paging.cursorInvalidated",
    "grid.paging.cursorTooDeep",
    "grid.paste.empty",
    "grid.paste.tooLarge",
    "grid.paste.raggedRow",
    "grid.paste.unclosedQuote",
    "grid.paste.outOfBounds",
    "grid.paste.noWritableCell",
    "grid.paste.notNullEmpty",
    "grid.paste.identityChanged",
    "grid.paste.crossTable",
    "grid.paste.columnMismatch",
    "grid.clipboard.writeFailed",
    "grid.clipboard.readFailed",
    "grid.pasteRows.notRowData",
    "grid.pasteRows.noWritableColumn",
    "grid.filter.rawWhereEmpty",
    "grid.filter.rawWhereTooLong",
    "grid.filter.rawWhereUnterminatedString",
    "grid.filter.rawWhereUnterminatedIdentifier",
    "grid.filter.rawWhereComment",
    "grid.filter.rawWhereMultiStatement",
    "grid.filter.rawWhereUnbalancedParenthesis",
    "grid.filter.rawWhereForbiddenKeyword",
    "grid.filter.rawWhereDollarSign",
    "grid.filter.rawWhereUnsupported",
];

pub fn classify_grid_error(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return "unknown".to_owned();
    }
    let mut best: Option<&str> = None;
    for code in GRID_ERROR_CODES {
        if *code == trimmed || trimmed.starts_with(&format!("{code}:")) {
            if is_none_or(best, |current: &str| code.len() > current.len()) {
                best = Some(code);
            }
        }
    }
    best.unwrap_or("unknown").to_owned()
}

fn is_none_or<T>(value: Option<T>, predicate: impl FnOnce(T) -> bool) -> bool {
    match value {
        None => true,
        Some(inner) => predicate(inner),
    }
}

pub fn host_error_message(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        raw.to_owned()
    } else {
        trimmed.to_owned()
    }
}

pub struct FkActionDeps {
    pub resolve_target: Box<dyn Fn(&ResolveForeignKeyTargetsRequest) -> Result<ForeignKeyTargetResolution, String>>,
    pub next_panel_id: Box<dyn Fn() -> String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_of_target_normalizes_null_schema() {
        let target = ForeignKeyTargetRef {
            database: "shop".to_owned(),
            schema: None,
            table: "orders".to_owned(),
            columns: vec!["id".to_owned()],
            data_types: vec![],
            primary_keys: vec![],
            unique: true,
            candidate_limit: 10,
        };
        assert_eq!(
            TableIdentity::of_target(&target),
            TableIdentity { database: "shop".to_owned(), schema: None, table: "orders".to_owned() }
        );
    }

    #[test]
    fn identity_comparison_requires_all_three_fields() {
        let base = TableIdentity { database: "a".to_owned(), schema: None, table: "t".to_owned() };
        assert!(base.same(&base.clone()));
        assert!(!base.same(&TableIdentity { database: "b".to_owned(), ..base.clone() }));
        assert!(!base.same(&TableIdentity { schema: Some("s".to_owned()), ..base.clone() }));
        assert!(!base.clone().same(&TableIdentity { table: "other".to_owned(), ..base }));
    }

    #[test]
    fn classification_is_anchored_and_requires_the_colon_separator() {
        assert_eq!(classify_grid_error(""), "unknown");
        assert_eq!(classify_grid_error("grid.fk.shapeMismatch"), "grid.fk.shapeMismatch");
        assert_eq!(classify_grid_error("  grid.fk.valueNull: bad  "), "grid.fk.valueNull");
        assert_eq!(classify_grid_error("grid.cell.readOnlyBecause: prose"), "unknown");
        assert_eq!(classify_grid_error("prose grid.fk.valueNull: prose"), "unknown");
        assert_eq!(classify_grid_error("grid.nope.reason: x"), "unknown");
    }

    #[test]
    fn host_error_message_trims_but_keeps_raw_when_blank() {
        assert_eq!(host_error_message("  boom  "), "boom");
        assert_eq!(host_error_message("   "), "   ");
    }

    #[test]
    fn nav_state_push_is_visible_by_key_and_in_back_order() {
        let mut nav = FkNavState::default();
        let origin = FkNavOrigin {
            source_panel_id: "p1".to_owned(),
            source_table: "a".to_owned(),
            local_columns: vec!["id".to_owned()],
            target_table: "b".to_owned(),
            target_columns: vec!["a_id".to_owned()],
        };
        let first = nav.push("p2", "key1", origin.clone());
        assert_eq!(first.id, "fk-1");
        assert_eq!(nav.find_entry_id_by_nav_key("key1"), Some("fk-1"));
        nav.push("p3", "key2", origin);
        assert_eq!(nav.back.len(), 2);
        assert_eq!(nav.back[1].id, "fk-2");
    }

    #[test]
    fn panel_identity_ignores_non_table_panels() {
        let panel = Panel {
            id: "p1".to_owned(),
            kind: PanelKind::Table,
            table_name: "t".to_owned(),
            database: "a".to_owned(),
            table_schema: None,
            connection_id: "c".to_owned(),
            db_session_id: "s".to_owned(),
            connection_name: String::new(),
            database_type: None,
        };
        assert!(panel.identity().is_some());
        let other = Panel { kind: PanelKind::Other, ..panel };
        assert!(other.identity().is_none());
    }
}
