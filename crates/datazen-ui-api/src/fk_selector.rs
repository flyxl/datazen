use serde_json::Value;

pub const FK_SELECTOR_CANDIDATE_LIMIT: usize = 50;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkCandidateSource {
    pub local_column: String,
    pub constraint_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkCandidateGroup {
    pub local_columns: Vec<String>,
    pub constraint_name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FkCandidateSourceIndex {
    entries: Vec<(String, FkCandidateSource)>,
}

impl FkCandidateSourceIndex {
    pub fn get(&self, column: &str) -> Option<&FkCandidateSource> {
        self.entries.iter().find(|(key, _)| key == column).map(|(_, value)| value)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[(String, FkCandidateSource)] {
        &self.entries
    }
}

pub fn fk_group_is_pickable(local_columns: &[String]) -> bool {
    local_columns.len() == 1
}

pub fn index_fk_candidate_sources(groups: &[(String, FkCandidateGroup)]) -> FkCandidateSourceIndex {
    let mut entries: Vec<(String, FkCandidateSource)> = Vec::new();
    for (column_name, group) in groups {
        if !fk_group_is_pickable(&group.local_columns) {
            continue;
        }
        let source = FkCandidateSource {
            local_column: group.local_columns[0].clone(),
            constraint_name: if group.constraint_name.is_empty() {
                None
            } else {
                Some(group.constraint_name.clone())
            },
        };
        if let Some(existing) = entries.iter_mut().find(|(key, _)| key == column_name) {
            existing.1 = source;
        } else {
            entries.push((column_name.clone(), source));
        }
    }
    FkCandidateSourceIndex { entries }
}

fn js_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => "null".to_owned(),
        Value::Array(items) => items
            .iter()
            .map(|item| if item.is_null() { String::new() } else { js_string(item) })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}

pub fn format_fk_candidate(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        value => Some(js_string(value)),
    }
}

pub fn candidates_to_picker_options(rows: &[Vec<Option<Value>>]) -> Vec<String> {
    let mut options = Vec::new();
    for row in rows {
        if let Some(formatted) = format_fk_candidate(row.first().and_then(|value| value.as_ref())) {
            options.push(formatted);
        }
    }
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_limit_is_pinned() {
        assert_eq!(FK_SELECTOR_CANDIDATE_LIMIT, 50);
    }

    #[test]
    fn only_single_column_groups_are_pickable() {
        assert!(fk_group_is_pickable(&["id".to_owned()]));
        assert!(!fk_group_is_pickable(&[]));
        assert!(!fk_group_is_pickable(&["a".to_owned(), "b".to_owned()]));
    }

    #[test]
    fn index_skips_composites_and_maps_empty_constraint_to_none() {
        let groups = vec![
            ("composite".to_owned(), FkCandidateGroup {
                local_columns: vec!["a".to_owned(), "b".to_owned()],
                constraint_name: "fk_c".to_owned(),
            }),
            ("single".to_owned(), FkCandidateGroup {
                local_columns: vec!["id".to_owned()],
                constraint_name: String::new(),
            }),
            ("named".to_owned(), FkCandidateGroup {
                local_columns: vec!["code".to_owned()],
                constraint_name: "fk_n".to_owned(),
            }),
        ];
        let index = index_fk_candidate_sources(&groups);
        assert_eq!(index.len(), 2);
        assert!(index.get("composite").is_none());
        assert_eq!(index.get("single").expect("present").local_column, "id");
        assert_eq!(index.get("single").expect("present").constraint_name, None);
        assert_eq!(index.get("named").expect("present").constraint_name.as_deref(), Some("fk_n"));
    }

    #[test]
    fn duplicate_column_uses_the_last_source() {
        let groups = vec![
            ("col".to_owned(), FkCandidateGroup {
                local_columns: vec!["first".to_owned()],
                constraint_name: "a".to_owned(),
            }),
            ("col".to_owned(), FkCandidateGroup {
                local_columns: vec!["second".to_owned()],
                constraint_name: "b".to_owned(),
            }),
        ];
        let index = index_fk_candidate_sources(&groups);
        assert_eq!(index.len(), 1);
        assert_eq!(index.get("col").expect("present").local_column, "second");
    }

    #[test]
    fn candidate_formatting_covers_null_scalars_and_nonscalars() {
        assert_eq!(format_fk_candidate(None), None);
        assert_eq!(format_fk_candidate(Some(&Value::Null)), None);
        assert_eq!(format_fk_candidate(Some(&Value::String("s".into()))).as_deref(), Some("s"));
        assert_eq!(format_fk_candidate(Some(&Value::from(42))).as_deref(), Some("42"));
        assert_eq!(format_fk_candidate(Some(&Value::Bool(true))).as_deref(), Some("true"));
        assert_eq!(format_fk_candidate(Some(&Value::Bool(false))).as_deref(), Some("false"));
        assert_eq!(format_fk_candidate(Some(&serde_json::json!({"a": 1}))).as_deref(), Some("[object Object]"));
    }

    #[test]
    fn options_keep_order_and_duplicates_but_skip_unpickable_rows() {
        let rows = vec![
            vec![Some(Value::String("a".into()))],
            vec![Some(Value::Null)],
            vec![None],
            vec![Some(Value::String("a".into()))],
            vec![Some(Value::from(2)), Some(Value::String("ignored".into()))],
        ];
        assert_eq!(candidates_to_picker_options(&rows), vec!["a", "a", "2"]);
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        assert_eq!(crate::FK_SELECTOR_CANDIDATE_LIMIT, 50);
        let _ = crate::fk_group_is_pickable;
        let _ = crate::index_fk_candidate_sources;
        let _ = crate::format_fk_candidate;
        let _ = crate::candidates_to_picker_options;
        let _: Option<crate::FkCandidateSource> = None;
        let _: Option<crate::FkCandidateGroup> = None;
        let _: Option<crate::FkCandidateSourceIndex> = None;
    }
}
