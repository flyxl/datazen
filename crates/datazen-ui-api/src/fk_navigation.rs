use crate::{FilterCondition, FilterOperator, ForeignKeyInfo, Value};

pub const FK_NAV_MAX_DEPTH: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub struct GridViewport {
    pub scroll_top: f64,
    pub anchor_row: Option<f64>,
    pub anchor_column: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkGroupAffordance {
    pub local_columns: Vec<String>,
    pub target_columns: Vec<String>,
    pub target_label: String,
    pub constraint_name: String,
    pub reference_table: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkNavTargetIdentity {
    pub database: String,
    pub schema: Option<String>,
    pub table: String,
    pub columns: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkBreadcrumbModel {
    pub source_table: String,
    pub local_columns: Vec<String>,
    pub target_table: String,
    pub target_columns: Vec<String>,
}

pub type NavValue = Option<Value>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FkIndex {
    entries: Vec<(String, FkGroupAffordance)>,
}

impl FkIndex {
    pub fn get(&self, column: &str) -> Option<&FkGroupAffordance> {
        self.entries.iter().find(|(key, _)| key == column).map(|(_, value)| value)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[(String, FkGroupAffordance)] {
        &self.entries
    }
}

fn target_label_of(reference_table: &str, target_columns: &[String]) -> String {
    if target_columns.len() == 1 {
        format!("{reference_table}.{}", target_columns[0])
    } else {
        format!("{reference_table}.({})", target_columns.join(", "))
    }
}

pub fn index_foreign_keys(foreign_keys: &[ForeignKeyInfo]) -> FkIndex {
    let mut entries = Vec::new();
    for foreign_key in foreign_keys {
        let group = FkGroupAffordance {
            local_columns: foreign_key.columns.clone(),
            target_columns: foreign_key.referenced_columns.clone(),
            target_label: target_label_of(&foreign_key.referenced_table, &foreign_key.referenced_columns),
            constraint_name: foreign_key.name.clone(),
            reference_table: foreign_key.referenced_table.clone(),
        };
        for column in &foreign_key.columns {
            if entries.iter().any(|(key, _)| key == column) {
                continue;
            }
            entries.push((column.clone(), group.clone()));
        }
    }
    FkIndex { entries }
}

#[derive(Clone, Debug)]
pub struct FkNavInput {
    pub column_name: String,
    pub row: Option<std::collections::HashMap<String, Value>>,
    pub index: FkIndex,
    pub visible_columns: Vec<String>,
    pub editing: bool,
}

pub fn is_fk_cell_navigable(input: &FkNavInput) -> bool {
    if input.editing {
        return false;
    }
    let Some(group) = input.index.get(&input.column_name) else {
        return false;
    };
    let Some(row) = input.row.as_ref() else {
        return false;
    };
    for column in &group.local_columns {
        match row.get(column) {
            None | Some(Value::Null) => return false,
            Some(_) => (),
        }
    }
    let first_visible = input.visible_columns.iter().find(|column| group.local_columns.contains(column));
    match first_visible {
        Some(column) => column == &input.column_name,
        None => false,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FkFilterTarget {
    pub columns: Vec<String>,
}

pub fn build_fk_filters(target: &FkFilterTarget, values: &[NavValue]) -> Option<Vec<FilterCondition>> {
    if target.columns.is_empty() {
        return None;
    }
    if target.columns.len() != values.len() {
        return None;
    }
    let mut filters = Vec::with_capacity(target.columns.len());
    for (column, value) in target.columns.iter().zip(values.iter()) {
        let value = value.clone()?;
        if value == Value::Null {
            return None;
        }
        filters.push(FilterCondition {
            column: column.clone(),
            operator: FilterOperator::Eq,
            value: Some(value),
            connector: None,
        });
    }
    Some(filters)
}

pub fn fk_nav_key(target: &FkNavTargetIdentity, values: &[NavValue]) -> String {
    let normalized: Vec<Value> = values.iter().map(|value| value.clone().unwrap_or(Value::Null)).collect();
    serde_json::json!([
        target.database.clone(),
        target.schema.clone(),
        target.table.clone(),
        target.columns.clone(),
        normalized,
    ])
    .to_string()
}

pub fn find_foreign_key<'a>(
    foreign_keys: &'a [ForeignKeyInfo],
    columns: &[String],
    constraint_name: Option<&str>,
) -> Option<&'a ForeignKeyInfo> {
    if let Some(name) = constraint_name.filter(|name| !name.is_empty()) {
        if let Some(named) = foreign_keys.iter().find(|foreign_key| foreign_key.name == name) {
            return Some(named);
        }
    }
    foreign_keys.iter().find(|foreign_key| {
        foreign_key.columns.len() == columns.len()
            && foreign_key.columns.iter().zip(columns.iter()).all(|(left, right)| left == right)
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorInput {
    pub scroll_top: f64,
    pub row_height: f64,
    pub viewport_height: f64,
    pub active_row: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnchorResult {
    pub anchor_row: f64,
    pub anchor_visible: bool,
}

pub fn pick_anchor_row(input: AnchorInput) -> AnchorResult {
    let row_height = input.row_height.max(1.0);
    let first_visible = (input.scroll_top / row_height).floor().max(0.0);
    let visible_rows = (input.viewport_height / row_height).floor().max(1.0);
    let last_visible = first_visible + visible_rows - 1.0;
    if let Some(active_row) = input.active_row {
        if active_row >= first_visible && active_row <= last_visible {
            return AnchorResult { anchor_row: active_row, anchor_visible: true };
        }
    }
    AnchorResult { anchor_row: first_visible, anchor_visible: false }
}

pub fn scroll_top_for_anchor_row(anchor_row: f64, row_height: f64) -> f64 {
    if !anchor_row.is_finite() || !row_height.is_finite() {
        return 0.0;
    }
    (anchor_row * row_height).max(0.0)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::ForeignKeyInfo;

    use super::*;

    fn fk(name: &str, columns: &[&str], target: &str, referenced: &[&str]) -> ForeignKeyInfo {
        ForeignKeyInfo {
            name: name.to_owned(),
            columns: columns.iter().map(|value| (*value).to_owned()).collect(),
            referenced_table: target.to_owned(),
            referenced_columns: referenced.iter().map(|value| (*value).to_owned()).collect(),
            on_update: None,
            on_delete: None,
            deferrability: None,
        }
    }

    fn row(values: &[(&str, Value)]) -> HashMap<String, Value> {
        values.iter().map(|(key, value)| ((*key).to_owned(), value.clone())).collect()
    }

    #[test]
    fn max_depth_is_pinned() {
        assert_eq!(FK_NAV_MAX_DEPTH, 32);
    }

    #[test]
    fn index_flattens_groups_and_first_constraint_wins() {
        let keys = vec![
            fk("fk_a", &["region", "id"], "sales.regions", &["code", "pk"]),
            fk("fk_b", &["region"], "other.regions", &["code"]),
        ];
        let index = index_foreign_keys(&keys);
        assert_eq!(index.len(), 2);
        assert_eq!(index.get("region").expect("present").constraint_name, "fk_a");
        assert_eq!(index.get("id").expect("present").constraint_name, "fk_a");
        assert_eq!(index.get("region").expect("present").local_columns, vec!["region", "id"]);
        assert_eq!(index.get("id").expect("present").target_columns, vec!["code", "pk"]);
        assert_eq!(index.get("id").expect("present").reference_table, "sales.regions");
    }

    #[test]
    fn target_label_uses_single_or_composite_form() {
        let single = index_foreign_keys(&[fk("f", &["a"], "customers", &["id"])]);
        assert_eq!(single.get("a").expect("present").target_label, "customers.id");
        let composite = index_foreign_keys(&[fk("f", &["a", "b"], "sales.customers", &["x", "y"])]);
        assert_eq!(composite.get("a").expect("present").target_label, "sales.customers.(x, y)");
    }

    #[test]
    fn navigability_requires_members_and_the_first_visible_column() {
        let index = index_foreign_keys(&[fk("f", &["a", "b"], "t", &["x", "y"])]);
        let full = row(&[("a", Value::from(1)), ("b", Value::from(2))]);
        let input = |column: &str, row: Option<HashMap<String, Value>>, visible: &[&str], editing: bool| FkNavInput {
            column_name: column.to_owned(),
            row,
            index: index.clone(),
            visible_columns: visible.iter().map(|value| (*value).to_owned()).collect(),
            editing,
        };
        assert!(is_fk_cell_navigable(&input("a", Some(full.clone()), &["a", "b"], false)));
        assert!(!is_fk_cell_navigable(&input("b", Some(full.clone()), &["a", "b"], false)));
        assert!(!is_fk_cell_navigable(&input("a", Some(full.clone()), &["a", "b"], true)));
        assert!(!is_fk_cell_navigable(&input("a", None, &["a", "b"], false)));
        assert!(!is_fk_cell_navigable(&input("c", Some(full.clone()), &["a", "b"], false)));
        assert!(is_fk_cell_navigable(&input("b", Some(full.clone()), &["b", "a"], false)), "the arrow follows display order");
        assert!(!is_fk_cell_navigable(&input("a", Some(full.clone()), &["b", "a"], false)));
        assert!(!is_fk_cell_navigable(&input("a", Some(full.clone()), &["z"], false)));
    }

    #[test]
    fn a_null_member_makes_the_whole_group_unlocatable() {
        let index = index_foreign_keys(&[fk("f", &["a", "b"], "t", &["x", "y"])]);
        let input = FkNavInput {
            column_name: "a".to_owned(),
            row: Some(row(&[("a", Value::from(1)), ("b", Value::Null)])),
            index,
            visible_columns: vec!["a".to_owned(), "b".to_owned()],
            editing: false,
        };
        assert!(!is_fk_cell_navigable(&input));
    }

    #[test]
    fn filters_require_nonempty_aligned_nonnull_values() {
        let target = FkFilterTarget { columns: vec!["x".to_owned(), "y".to_owned()] };
        let values = vec![Some(Value::from(1)), Some(Value::from("s"))];
        let filters = build_fk_filters(&target, &values).expect("filters");
        assert_eq!(filters.len(), 2);
        assert_eq!(filters[0].column, "x");
        assert_eq!(filters[0].operator, FilterOperator::Eq);
        assert_eq!(filters[0].value, Some(Value::from(1)));
        assert_eq!(filters[0].connector, None);
        assert!(build_fk_filters(&FkFilterTarget::default(), &[]).is_none());
        assert!(build_fk_filters(&target, &[Some(Value::from(1))]).is_none());
        assert!(build_fk_filters(&target, &[Some(Value::from(1)), None]).is_none());
        assert!(build_fk_filters(&target, &[Some(Value::from(1)), Some(Value::Null)]).is_none());
    }

    #[test]
    fn nav_key_includes_schema_and_normalizes_undefined() {
        let target = FkNavTargetIdentity {
            database: "db".to_owned(),
            schema: Some("public".to_owned()),
            table: "orders".to_owned(),
            columns: vec!["id".to_owned()],
        };
        let key = fk_nav_key(&target, &[Some(Value::from(7))]);
        let other_schema = FkNavTargetIdentity { schema: Some("audit".to_owned()), ..target.clone() };
        assert_ne!(key, fk_nav_key(&other_schema, &[Some(Value::from(7))]));
        assert_eq!(fk_nav_key(&target, &[None]), fk_nav_key(&target, &[Some(Value::Null)]));
        assert_eq!(key, fk_nav_key(&target, &[Some(Value::from(7))]));
        assert_eq!(
            key,
            "[\"db\",\"public\",\"orders\",[\"id\"],[7]]"
        );
    }

    #[test]
    fn find_foreign_key_prefers_name_then_positional_columns() {
        let keys = vec![
            fk("f1", &["a"], "t1", &["x"]),
            fk("f2", &["b", "c"], "t2", &["y", "z"]),
        ];
        let names = |values: &[&str]| values.iter().map(|value| (*value).to_owned()).collect::<Vec<_>>();
        let found = find_foreign_key(&keys, &names(&["a"]), Some("f2")).expect("named wins");
        assert_eq!(found.name, "f2");
        let found = find_foreign_key(&keys, &names(&["b", "c"]), None).expect("positional");
        assert_eq!(found.name, "f2");
        assert!(find_foreign_key(&keys, &names(&["c", "b"]), None).is_none());
        assert!(find_foreign_key(&keys, &names(&["a"]), Some("")).is_some());
        assert!(find_foreign_key(&keys, &names(&["a"]), Some("missing")).is_some());
        assert!(find_foreign_key(&keys, &names(&["missing"]), None).is_none());
    }

    #[test]
    fn anchor_prefers_a_visible_active_row() {
        let result = pick_anchor_row(AnchorInput {
            scroll_top: 100.0,
            row_height: 20.0,
            viewport_height: 100.0,
            active_row: Some(7.0),
        });
        assert_eq!(result, AnchorResult { anchor_row: 7.0, anchor_visible: true });
    }

    #[test]
    fn anchor_falls_back_when_active_is_offscreen_or_row_height_is_invalid() {
        let result = pick_anchor_row(AnchorInput {
            scroll_top: 100.0,
            row_height: 20.0,
            viewport_height: 100.0,
            active_row: Some(2.0),
        });
        assert_eq!(result, AnchorResult { anchor_row: 5.0, anchor_visible: false });
        let result = pick_anchor_row(AnchorInput {
            scroll_top: 100.0,
            row_height: 0.0,
            viewport_height: 0.0,
            active_row: None,
        });
        assert_eq!(result, AnchorResult { anchor_row: 100.0, anchor_visible: false });
    }

    #[test]
    fn scroll_top_for_anchor_is_clamped_and_rejects_nonfinite() {
        assert_eq!(scroll_top_for_anchor_row(5.0, 20.0), 100.0);
        assert_eq!(scroll_top_for_anchor_row(-2.0, 20.0), 0.0);
        assert_eq!(scroll_top_for_anchor_row(f64::NAN, 20.0), 0.0);
        assert_eq!(scroll_top_for_anchor_row(5.0, f64::INFINITY), 0.0);
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        assert_eq!(crate::FK_NAV_MAX_DEPTH, 32);
        let _ = crate::index_foreign_keys;
        let _ = crate::is_fk_cell_navigable;
        let _ = crate::build_fk_filters;
        let _ = crate::fk_nav_key;
        let _ = crate::find_foreign_key;
        let _ = crate::pick_anchor_row;
        let _ = crate::scroll_top_for_anchor_row;
        let _: Option<crate::FkIndex> = None;
        let _: Option<crate::FkNavInput> = None;
        let _: Option<crate::FkFilterTarget> = None;
        let _: Option<crate::FkNavTargetIdentity> = None;
        let _: Option<crate::FkBreadcrumbModel> = None;
        let _: Option<crate::GridViewport> = None;
        let _: Option<crate::AnchorInput> = None;
        let _: Option<crate::AnchorResult> = None;
        let _: Option<crate::NavValue> = None;
    }
}
