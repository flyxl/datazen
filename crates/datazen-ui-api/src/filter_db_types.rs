pub trait DbTypeOption {
    fn value(&self) -> &str;
    fn label(&self) -> &str;
}

pub fn filter_db_types_by_query<T: DbTypeOption + Clone>(items: &[T], query: &str) -> Vec<T> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return items.to_vec();
    }
    items
        .iter()
        .filter(|item| {
            item.label().to_lowercase().contains(&query)
                || item.value().to_lowercase().contains(&query)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Item {
        value: String,
        label: String,
    }

    impl DbTypeOption for Item {
        fn value(&self) -> &str {
            &self.value
        }

        fn label(&self) -> &str {
            &self.label
        }
    }

    fn item(value: &str, label: &str) -> Item {
        Item { value: value.to_owned(), label: label.to_owned() }
    }

    fn items() -> Vec<Item> {
        vec![
            item("postgresql", "PostgreSQL"),
            item("mysql", "MySQL Server"),
            item("redis", "Redis"),
        ]
    }

    #[test]
    fn empty_or_whitespace_query_returns_everything() {
        let source = items();
        assert_eq!(filter_db_types_by_query(&source, ""), source);
        assert_eq!(filter_db_types_by_query(&source, "   "), source);
    }

    #[test]
    fn matches_label_case_insensitively() {
        let result = filter_db_types_by_query(&items(), "POSTGRES");
        assert_eq!(result, vec![item("postgresql", "PostgreSQL")]);
    }

    #[test]
    fn matches_value_case_insensitively() {
        let result = filter_db_types_by_query(&items(), "REDIS");
        assert_eq!(result, vec![item("redis", "Redis")]);
    }

    #[test]
    fn query_is_trimmed() {
        let result = filter_db_types_by_query(&items(), "  mysql ");
        assert_eq!(result, vec![item("mysql", "MySQL Server")]);
    }

    #[test]
    fn substring_on_label_or_value_is_enough() {
        let result = filter_db_types_by_query(&items(), "server");
        assert_eq!(result, vec![item("mysql", "MySQL Server")]);
        let result = filter_db_types_by_query(&items(), "sql");
        assert_eq!(
            result,
            vec![item("postgresql", "PostgreSQL"), item("mysql", "MySQL Server")]
        );
    }

    #[test]
    fn no_match_returns_empty_and_input_is_unchanged() {
        let source = items();
        let result = filter_db_types_by_query(&source, "zzz");
        assert!(result.is_empty());
        assert_eq!(source, items());
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::filter_db_types_by_query::<Item>;
        let _: Option<&dyn crate::DbTypeOption> = None;
    }
}
