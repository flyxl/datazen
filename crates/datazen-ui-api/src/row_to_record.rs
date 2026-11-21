use serde_json::{Map, Value};

pub fn row_to_record<S: AsRef<str>>(
    row: Option<&[Option<Value>]>,
    columns: &[S],
) -> Option<Map<String, Value>> {
    let cells = row?;
    let mut record = Map::new();
    for (index, column) in columns.iter().enumerate() {
        let value = cells.get(index).cloned().flatten().unwrap_or(Value::Null);
        record.insert(column.as_ref().to_owned(), value);
    }
    Some(record)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn null_row_yields_none() {
        let columns = ["a", "b"];
        assert!(row_to_record(None, &columns).is_none());
    }

    #[test]
    fn maps_cells_to_column_names() {
        let columns = ["pid", "user", "state"];
        let row = [Some(json!(42)), Some(json!("alice")), Some(json!("active"))];
        let record = row_to_record(Some(&row), &columns).unwrap();
        assert_eq!(record["pid"], json!(42));
        assert_eq!(record["user"], json!("alice"));
        assert_eq!(record["state"], json!("active"));
        assert_eq!(record.len(), 3);
    }

    #[test]
    fn null_and_missing_cells_become_null() {
        let columns = ["a", "b", "c"];
        let row = [None, Some(json!(7))];
        let record = row_to_record(Some(&row), &columns).unwrap();
        assert_eq!(record["a"], Value::Null);
        assert_eq!(record["b"], json!(7));
        assert_eq!(record["c"], Value::Null);
    }

    #[test]
    fn extra_cells_beyond_columns_are_ignored() {
        let columns = ["a"];
        let row = [Some(json!(1)), Some(json!(2))];
        let record = row_to_record(Some(&row), &columns).unwrap();
        assert_eq!(record.len(), 1);
        assert_eq!(record["a"], json!(1));
    }

    #[test]
    fn empty_columns_yield_an_empty_record() {
        let columns: [&str; 0] = [];
        let row = [Some(json!(1))];
        let record = row_to_record(Some(&row), &columns).unwrap();
        assert!(record.is_empty());
    }

    #[test]
    fn duplicate_column_names_keep_the_last_cell() {
        let columns = ["a", "a"];
        let row = [Some(json!(1)), Some(json!(2))];
        let record = row_to_record(Some(&row), &columns).unwrap();
        assert_eq!(record.len(), 1);
        assert_eq!(record["a"], json!(2));
    }

    #[test]
    fn accepts_owned_string_column_names() {
        let columns = vec!["x".to_owned(), "y".to_owned()];
        let row = [Some(json!(true)), None];
        let record = row_to_record(Some(&row), &columns).unwrap();
        assert_eq!(record["x"], json!(true));
        assert_eq!(record["y"], Value::Null);
    }
}
