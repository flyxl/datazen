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
        if let Some(pos) = s.find(keyword) {
            let after = &s[pos + keyword.len()..];
            if let Some(paren_start) = after.find('(') {
                let inner = &after[paren_start + 1..];
                if let Some(paren_end) = inner.find(')') {
                    let list_str = &inner[..paren_end];
                    return list_str
                        .split(',')
                        .filter_map(|part| {
                            let t = part.trim();
                            if t.starts_with('`') && t.ends_with('`') && t.len() >= 2 {
                                Some(t[1..t.len() - 1].to_string())
                            } else {
                                None
                            }
                        })
                        .collect();
                }
            }
        }
        Vec::new()
    }

    pub(crate) fn extract_rule(s: &str, keyword: &str) -> String {
        if let Some(pos) = s.find(keyword) {
            let after = s[pos + keyword.len()..].trim_start();
            let rule = after
                .split(|c: char| c == ',' || c == ')' || c == '\n')
                .next()
                .unwrap_or("")
                .trim();
            if !rule.is_empty() {
                return rule.to_uppercase();
            }
        }
        "RESTRICT".to_string()
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
