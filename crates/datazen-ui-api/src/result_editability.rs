use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridReadOnlyReason {
    UnknownOrigin,
    AmbiguousOrigin,
    MultiTable,
    Aggregate,
    Distinct,
    SetOperation,
    DerivedSource,
    View,
    NoPrimaryKey,
    KeyNotSelected,
    SessionReadOnly,
    KeyValueNull,
    DuplicateIdentity,
    RowNotFound,
}

impl GridReadOnlyReason {
    pub fn as_str(self) -> &'static str {
        match self {
            GridReadOnlyReason::UnknownOrigin => "unknownOrigin",
            GridReadOnlyReason::AmbiguousOrigin => "ambiguousOrigin",
            GridReadOnlyReason::MultiTable => "multiTable",
            GridReadOnlyReason::Aggregate => "aggregate",
            GridReadOnlyReason::Distinct => "distinct",
            GridReadOnlyReason::SetOperation => "setOperation",
            GridReadOnlyReason::DerivedSource => "derivedSource",
            GridReadOnlyReason::View => "view",
            GridReadOnlyReason::NoPrimaryKey => "noPrimaryKey",
            GridReadOnlyReason::KeyNotSelected => "keyNotSelected",
            GridReadOnlyReason::SessionReadOnly => "sessionReadOnly",
            GridReadOnlyReason::KeyValueNull => "keyValueNull",
            GridReadOnlyReason::DuplicateIdentity => "duplicateIdentity",
            GridReadOnlyReason::RowNotFound => "rowNotFound",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IdentityColumn {
    pub column: String,
    pub source_column: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultTableRef {
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub name: String,
    pub aliased: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ServerEditability {
    pub writable: bool,
    pub identity_columns: Vec<IdentityColumn>,
    pub editable_columns: Vec<IdentityColumn>,
    pub table: Option<ResultTableRef>,
    pub reason: Option<GridReadOnlyReason>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GridEditability {
    Writable {
        identity_columns: Vec<IdentityColumn>,
        editable_columns: Vec<IdentityColumn>,
        table: Option<ResultTableRef>,
    },
    ReadOnly {
        reason: GridReadOnlyReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridColumn {
    pub name: String,
}

pub const RESULT_ORIGIN_WRITE_BACK_ENABLED: bool = false;

fn read_only(reason: GridReadOnlyReason) -> GridEditability {
    GridEditability::ReadOnly { reason }
}

fn reason_or_unknown(reason: Option<GridReadOnlyReason>) -> GridReadOnlyReason {
    match reason {
        Some(reason) => reason,
        None => GridReadOnlyReason::UnknownOrigin,
    }
}

fn values_equal(left: Option<&Value>, right: Option<&Value>) -> bool {
    let null = Value::Null;
    let left = match left {
        Some(value) => value,
        None => &null,
    };
    let right = match right {
        Some(value) => value,
        None => &null,
    };
    left.to_string() == right.to_string()
}

fn column_index(columns: &[GridColumn], name: &str) -> Option<usize> {
    columns.iter().position(|column| column.name == name)
}

pub fn resolve_row_addressability(
    identity_columns: &[IdentityColumn],
    rows: &[Vec<Option<Value>>],
    columns: &[GridColumn],
    row_index: i64,
) -> Option<GridReadOnlyReason> {
    if row_index < 0 || row_index as usize >= rows.len() {
        return Some(GridReadOnlyReason::RowNotFound);
    }
    if identity_columns.is_empty() {
        return None;
    }
    let index = row_index as usize;
    let row = &rows[index];
    let mut values: Vec<&Value> = Vec::with_capacity(identity_columns.len());
    for identity in identity_columns {
        let column = match column_index(columns, &identity.column) {
            Some(column) => column,
            None => return Some(GridReadOnlyReason::RowNotFound),
        };
        let value = match row.get(column) {
            Some(Some(value)) if value != &Value::Null => value,
            _ => return Some(GridReadOnlyReason::KeyValueNull),
        };
        values.push(value);
    }
    for other in 0..rows.len() {
        if other == index {
            continue;
        }
        let candidate = &rows[other];
        let mut same = true;
        for (position, identity) in identity_columns.iter().enumerate() {
            let column = match column_index(columns, &identity.column) {
                Some(column) => column,
                None => {
                    same = false;
                    break;
                }
            };
            let left = match values.get(position) {
                Some(value) => *value,
                None => {
                    same = false;
                    break;
                }
            };
            let right = match candidate.get(column) {
                Some(Some(value)) => Some(value),
                _ => None,
            };
            if !values_equal(Some(left), right) {
                same = false;
                break;
            }
        }
        if same {
            return Some(GridReadOnlyReason::DuplicateIdentity);
        }
    }
    None
}

pub fn resolve_grid_editability(
    server: &ServerEditability,
    rows: &[Vec<Option<Value>>],
    columns: &[GridColumn],
    row_index: i64,
) -> GridEditability {
    if !server.writable {
        return read_only(reason_or_unknown(server.reason));
    }
    let row_reason = resolve_row_addressability(&server.identity_columns, rows, columns, row_index);
    if let Some(reason) = row_reason {
        return read_only(reason);
    }
    GridEditability::Writable {
        identity_columns: server.identity_columns.clone(),
        editable_columns: server.editable_columns.clone(),
        table: server.table.clone(),
    }
}

pub fn origin_channel_writable(server: &ServerEditability) -> bool {
    RESULT_ORIGIN_WRITE_BACK_ENABLED && server.writable
}

pub fn resolve_result_write_back(legacy_verified: bool, server: &ServerEditability) -> bool {
    if !server.writable && server.reason == Some(GridReadOnlyReason::SessionReadOnly) {
        return false;
    }
    if legacy_verified {
        return true;
    }
    origin_channel_writable(server)
}

pub fn resolve_result_editability(
    legacy_verified: bool,
    server: &ServerEditability,
    rows: &[Vec<Option<Value>>],
    columns: &[GridColumn],
    row_index: i64,
) -> GridEditability {
    let row_reason = resolve_row_addressability(&server.identity_columns, rows, columns, row_index);
    if resolve_result_write_back(legacy_verified, server) {
        if let Some(reason) = row_reason {
            return read_only(reason);
        }
        return GridEditability::Writable {
            identity_columns: server.identity_columns.clone(),
            editable_columns: server.editable_columns.clone(),
            table: server.table.clone(),
        };
    }
    read_only(reason_or_unknown(server.reason))
}

pub fn grid_read_only_message_key(reason: GridReadOnlyReason) -> String {
    format!("query.gridReadOnly.{}", reason.as_str())
}

pub fn is_column_editable(server: &ServerEditability, column_name: &str) -> bool {
    if !server.writable {
        return false;
    }
    server.editable_columns.iter().any(|column| column.column == column_name)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn identity(name: &str) -> IdentityColumn {
        IdentityColumn { column: name.to_owned(), source_column: name.to_owned() }
    }

    fn columns(names: &[&str]) -> Vec<GridColumn> {
        names.iter().map(|name| GridColumn { name: (*name).to_owned() }).collect()
    }

    fn server(writable: bool, identities: &[&str], editable: &[&str]) -> ServerEditability {
        ServerEditability {
            writable,
            identity_columns: identities.iter().map(|name| identity(name)).collect(),
            editable_columns: editable.iter().map(|name| identity(name)).collect(),
            table: None,
            reason: None,
        }
    }

    #[test]
    fn reasons_and_message_keys_are_pinned() {
        assert_eq!(GridReadOnlyReason::SessionReadOnly.as_str(), "sessionReadOnly");
        assert_eq!(GridReadOnlyReason::KeyValueNull.as_str(), "keyValueNull");
        assert_eq!(GridReadOnlyReason::DuplicateIdentity.as_str(), "duplicateIdentity");
        assert_eq!(GridReadOnlyReason::RowNotFound.as_str(), "rowNotFound");
        assert_eq!(grid_read_only_message_key(GridReadOnlyReason::KeyValueNull), "query.gridReadOnly.keyValueNull");
        assert!(!RESULT_ORIGIN_WRITE_BACK_ENABLED);
    }

    #[test]
    fn out_of_range_and_missing_identity_column_are_row_not_found() {
        let cols = columns(&["id", "name"]);
        let rows = vec![vec![Some(json!(1)), Some(json!("a"))]];
        assert_eq!(resolve_row_addressability(&[identity("id")], &rows, &cols, -1), Some(GridReadOnlyReason::RowNotFound));
        assert_eq!(resolve_row_addressability(&[identity("id")], &rows, &cols, 1), Some(GridReadOnlyReason::RowNotFound));
        assert_eq!(resolve_row_addressability(&[identity("missing")], &rows, &cols, 0), Some(GridReadOnlyReason::RowNotFound));
    }

    #[test]
    fn no_known_identity_columns_is_not_a_refusal() {
        let rows = vec![vec![Some(json!(1))]];
        assert_eq!(resolve_row_addressability(&[], &rows, &columns(&["id"]), 0), None);
    }

    #[test]
    fn null_key_values_are_unaddressable() {
        let cols = columns(&["id"]);
        let rows = vec![
            vec![Some(Value::Null)],
            vec![None],
        ];
        assert_eq!(resolve_row_addressability(&[identity("id")], &rows, &cols, 0), Some(GridReadOnlyReason::KeyValueNull));
        assert_eq!(resolve_row_addressability(&[identity("id")], &rows, &cols, 1), Some(GridReadOnlyReason::KeyValueNull));
    }

    #[test]
    fn duplicate_identity_refuses_both_rows() {
        let cols = columns(&["id"]);
        let rows = vec![vec![Some(json!(1))], vec![Some(json!(1))], vec![Some(json!(2))]];
        assert_eq!(resolve_row_addressability(&[identity("id")], &rows, &cols, 0), Some(GridReadOnlyReason::DuplicateIdentity));
        assert_eq!(resolve_row_addressability(&[identity("id")], &rows, &cols, 1), Some(GridReadOnlyReason::DuplicateIdentity));
        assert_eq!(resolve_row_addressability(&[identity("id")], &rows, &cols, 2), None);
    }

    #[test]
    fn composite_keys_compare_positionally_with_json_equality() {
        let cols = columns(&["a", "b"]);
        let rows = vec![
            vec![Some(json!(1)), Some(json!("x"))],
            vec![Some(json!(1)), Some(json!("y"))],
        ];
        let keys = [identity("a"), identity("b")];
        assert_eq!(resolve_row_addressability(&keys, &rows, &cols, 0), None);
        let rows = vec![
            vec![Some(json!(1)), Some(json!("x"))],
            vec![Some(json!(1)), Some(json!("x"))],
        ];
        assert_eq!(resolve_row_addressability(&keys, &rows, &cols, 0), Some(GridReadOnlyReason::DuplicateIdentity));
    }

    #[test]
    fn server_refusal_is_preserved_without_inventing_a_reason() {
        let mut refused = server(false, &[], &[]);
        refused.reason = Some(GridReadOnlyReason::View);
        let rows = vec![vec![Some(json!(1))]];
        let verdict = resolve_grid_editability(&refused, &rows, &columns(&["id"]), 0);
        assert_eq!(verdict, GridEditability::ReadOnly { reason: GridReadOnlyReason::View });

        refused.reason = None;
        let verdict = resolve_grid_editability(&refused, &rows, &columns(&["id"]), 0);
        assert_eq!(verdict, GridEditability::ReadOnly { reason: GridReadOnlyReason::UnknownOrigin });
    }

    #[test]
    fn a_writable_server_verdict_still_checks_the_page() {
        let writable = server(true, &["id"], &["name"]);
        let rows = vec![vec![Some(Value::Null), Some(json!("a"))]];
        let verdict = resolve_grid_editability(&writable, &rows, &columns(&["id", "name"]), 0);
        assert_eq!(verdict, GridEditability::ReadOnly { reason: GridReadOnlyReason::KeyValueNull });

        let rows = vec![vec![Some(json!(1)), Some(json!("a"))]];
        let verdict = resolve_grid_editability(&writable, &rows, &columns(&["id", "name"]), 0);
        assert_eq!(verdict, GridEditability::Writable {
            identity_columns: vec![identity("id")],
            editable_columns: vec![identity("name")],
            table: None,
        });
    }

    #[test]
    fn write_back_gate_refuses_session_read_only_above_all() {
        let mut read_only_conn = server(false, &[], &[]);
        read_only_conn.reason = Some(GridReadOnlyReason::SessionReadOnly);
        assert!(!resolve_result_write_back(true, &read_only_conn));
        assert!(!resolve_result_write_back(false, &read_only_conn));
        assert!(!origin_channel_writable(&read_only_conn), "the origin channel is closed");

        let writable = server(true, &[], &[]);
        assert!(resolve_result_write_back(true, &writable));
        assert!(!resolve_result_write_back(false, &writable), "legacy false cannot be granted while the channel is closed");
    }

    #[test]
    fn final_editability_computes_page_facts_even_for_refusals() {
        let mut refused = server(false, &["id"], &[]);
        refused.reason = Some(GridReadOnlyReason::View);
        let rows = vec![vec![Some(Value::Null)]];
        let verdict = resolve_result_editability(false, &refused, &rows, &columns(&["id"]), 0);
        assert_eq!(verdict, GridEditability::ReadOnly { reason: GridReadOnlyReason::View });

        let writable = server(true, &["id"], &["name"]);
        let verdict = resolve_result_editability(true, &writable, &rows, &columns(&["id"]), 0);
        assert_eq!(verdict, GridEditability::ReadOnly { reason: GridReadOnlyReason::KeyValueNull });

        let rows = vec![vec![Some(json!(7))]];
        let verdict = resolve_result_editability(true, &writable, &rows, &columns(&["id"]), 0);
        assert!(matches!(verdict, GridEditability::Writable { .. }));
    }

    #[test]
    fn column_editability_uses_the_editable_list_only() {
        let writable = server(true, &["id"], &["name"]);
        assert!(is_column_editable(&writable, "name"));
        assert!(!is_column_editable(&writable, "id"));
        assert!(!is_column_editable(&writable, "missing"));
        let refused = server(false, &["id"], &["name"]);
        assert!(!is_column_editable(&refused, "name"));
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::resolve_row_addressability;
        let _ = crate::resolve_grid_editability;
        let _ = crate::origin_channel_writable;
        let _ = crate::resolve_result_write_back;
        let _ = crate::resolve_result_editability;
        let _ = crate::grid_read_only_message_key;
        let _ = crate::is_column_editable;
        let _: Option<crate::GridReadOnlyReason> = None;
        let _: Option<crate::IdentityColumn> = None;
        let _: Option<crate::ResultTableRef> = None;
        let _: Option<crate::ServerEditability> = None;
        let _: Option<crate::GridEditability> = None;
        let _: Option<crate::GridColumn> = None;
    }
}
