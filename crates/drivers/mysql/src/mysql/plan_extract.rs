//! Grant, plan-metric, and explain-JSON extraction helpers.

use super::*;

impl MysqlDriver {
    pub(crate) fn grants_prove_server_wide_catalog_visibility(grants: &[String]) -> bool {
        let has_global_select = grants.iter().any(|grant| {
            let upper = grant.trim().to_ascii_uppercase();
            if !upper.starts_with("GRANT ") {
                return false;
            }
            let Some(on) = upper.find(" ON *.* TO ") else {
                return false;
            };
            upper[6..on]
                .split(',')
                .map(str::trim)
                .any(|privilege| matches!(privilege, "SELECT" | "ALL PRIVILEGES"))
        });
        has_global_select
            && !grants.iter().any(|grant| {
                grant
                    .trim_start()
                    .to_ascii_uppercase()
                    .starts_with("REVOKE ")
            })
    }

    /// Extract a parenthesized list of backtick-quoted identifiers after a keyword.
    pub(crate) fn extract_backtick_list_after(s: &str, keyword: &str) -> Vec<String> {
        let Some((_, mut rest)) = s.split_once(keyword) else {
            return Vec::new();
        };
        // REFERENCES can include quoted table names that themselves contain
        // parentheses. Only the first opening parenthesis outside a name
        // begins the column list.
        loop {
            match rest.find(['`', '(']).map(|index| &rest[index..]) {
                Some(input) if input.starts_with('(') => {
                    rest = &input[1..];
                    break;
                }
                Some(input) => {
                    let Some((_, tail)) = Self::take_backtick_name(input) else {
                        return Vec::new();
                    };
                    rest = tail;
                }
                None => return Vec::new(),
            }
        }
        let mut names = Vec::new();
        loop {
            let Some((name, tail)) = Self::take_backtick_name(rest.trim_start()) else {
                return Vec::new();
            };
            names.push(name);
            let tail = tail.trim_start();
            if tail.starts_with(')') {
                return names;
            }
            let Some(next) = tail.strip_prefix(',') else {
                return Vec::new();
            };
            rest = next;
        }
    }

    /// Decode one MySQL backtick-delimited identifier and return its tail.
    pub(crate) fn take_backtick_name(input: &str) -> Option<(String, &str)> {
        let body = input.strip_prefix('`')?;
        let mut offset = 0;
        while let Some(index) = body[offset..].find('`').map(|delta| offset + delta) {
            let tail = &body[index + 1..];
            if tail.starts_with('`') {
                offset = index + 2;
            } else {
                return Some((body[..index].replace("``", "`"), tail));
            }
        }
        None
    }

    pub(crate) fn extract_rule(s: &str, keyword: &str) -> String {
        let Some((_, rest)) = s.split_once(keyword) else {
            return "RESTRICT".to_owned();
        };
        let mut words = rest
            .split(|ch: char| ch.is_ascii_whitespace() || matches!(ch, ',' | ')'))
            .filter(|word| !word.is_empty());
        let first = words.next().unwrap_or_default();
        match (first, words.next()) {
            ("SET", Some("NULL")) => "SET NULL",
            ("SET", Some("DEFAULT")) => "SET DEFAULT",
            ("NO", Some("ACTION")) => "NO ACTION",
            ("CASCADE", _) => "CASCADE",
            _ => "RESTRICT",
        }
        .to_owned()
    }
    pub(crate) fn extract_mysql_plan_metrics(
        plan_json: &serde_json::Value,
    ) -> (Option<f64>, Option<i64>) {
        let query_cost = plan_json
            .get("query_block")
            .and_then(|block| block.get("cost_info"))
            .and_then(|info| info.get("query_cost"))
            .and_then(|value| {
                value
                    .as_f64()
                    .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
            });
        let estimated_rows = plan_json
            .get("query_block")
            .and_then(|block| block.get("table"))
            .and_then(|table| table.get("rows_examined_per_scan"))
            .and_then(|value| {
                value
                    .as_i64()
                    .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
            });
        (query_cost, estimated_rows)
    }
}
