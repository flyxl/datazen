pub fn table_matches_object_search(
    table_name: &str,
    query: &str,
    columns: Option<&[String]>,
) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    if table_name.to_lowercase().contains(&q) {
        return true;
    }
    if q.len() < 2 {
        return false;
    }
    match columns {
        Some(columns) if !columns.is_empty() => columns
            .iter()
            .any(|column| column.to_lowercase().contains(&q)),
        _ => false,
    }
}

pub fn matching_columns(query: &str, columns: Option<&[String]>) -> Vec<String> {
    let q = query.trim().to_lowercase();
    if q.len() < 2 {
        return Vec::new();
    }
    match columns {
        Some(columns) if !columns.is_empty() => columns
            .iter()
            .filter(|column| column.to_lowercase().contains(&q))
            .cloned()
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_matches_everything() {
        assert!(table_matches_object_search("users", "", None));
        assert!(table_matches_object_search("users", "   ", None));
    }

    #[test]
    fn table_name_match_is_case_insensitive() {
        assert!(table_matches_object_search("Users", "user", None));
        assert!(table_matches_object_search("users", " USERS ", None));
    }

    #[test]
    fn column_match_requires_a_two_character_query() {
        let columns = ["email".to_owned(), "id".to_owned()];
        assert!(!table_matches_object_search("users", "x", Some(&columns)));
        assert!(table_matches_object_search("users", "EM", Some(&columns)));
        assert!(!table_matches_object_search("users", "zz", Some(&columns)));
    }

    #[test]
    fn missing_or_empty_columns_cannot_match() {
        assert!(!table_matches_object_search("users", "em", None));
        let empty: [String; 0] = [];
        assert!(!table_matches_object_search("users", "em", Some(&empty)));
    }

    #[test]
    fn matching_columns_returns_originals_in_order() {
        let columns = [
            "Email".to_owned(),
            "id".to_owned(),
            "EMailBackup".to_owned(),
        ];
        assert_eq!(
            matching_columns(" em ", Some(&columns)),
            ["Email", "EMailBackup"]
        );
        assert!(matching_columns("e", Some(&columns)).is_empty());
        assert!(matching_columns("em", None).is_empty());
    }
}
