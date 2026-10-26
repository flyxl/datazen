//! 历史作用域、排序与分页词汇（原 `src-tauri/src/store/history_db.rs` 顶部）。

use serde::{Deserialize, Serialize};

use super::query::QueryHistoryEntry;

/// 单库保留上限（业务规则：追加后截断到该行数；由存储实现执行）。
pub const MAX_QUERY_HISTORY: usize = 1000;
/// Workflow 历史保留上限（词汇先随 history 归位；Workflow 功能族迁移时消费）。
pub const MAX_WORKFLOW_HISTORY: usize = 100;
/// 迁移运行历史保留上限。
pub const MAX_MIGRATION_RUN_HISTORY: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryScope {
    Query,
    Workflow,
    All,
}

impl HistoryScope {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "query" => Some(Self::Query),
            "workflow" => Some(Self::Workflow),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

/// Sort order for the paged query-history read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistoryOrder {
    /// Newest first (the historical default).
    #[default]
    Recent,
    /// Oldest first.
    Oldest,
    /// Slowest first — surfaces the statements that actually hurt.
    Slowest,
}

impl HistoryOrder {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "recent" => Some(Self::Recent),
            "oldest" => Some(Self::Oldest),
            "slowest" => Some(Self::Slowest),
            _ => None,
        }
    }
}

/// Filters for the paged query-history read.
///
/// `search` is applied **before** `limit` so a caller never has to widen the
/// page to find a row that exists. `since`/`until` are RFC3339 instants.
#[derive(Debug, Clone)]
pub struct QueryHistoryFilter<'a> {
    pub limit: usize,
    pub connection_id: Option<&'a str>,
    pub database: Option<&'a str>,
    pub schema: Option<&'a str>,
    pub search: Option<&'a str>,
    pub since: Option<&'a str>,
    pub until: Option<&'a str>,
    pub order: HistoryOrder,
}

/// Page size used when a caller does not name one.
///
/// Hand-written rather than `#[derive(Default)]` on purpose: a derived default
/// would be `limit: 0`, and `LIMIT 0` returns an empty page that reads as "you
/// have no history" instead of "you asked for nothing". Every `..Default` call
/// site would silently render an empty list.
pub const DEFAULT_HISTORY_PAGE_SIZE: usize = 200;

impl Default for QueryHistoryFilter<'_> {
    fn default() -> Self {
        Self {
            limit: DEFAULT_HISTORY_PAGE_SIZE,
            connection_id: None,
            database: None,
            schema: None,
            search: None,
            since: None,
            until: None,
            order: HistoryOrder::Recent,
        }
    }
}

/// One page of query history plus the number of rows the filter matched *before*
/// the page was cut.
///
/// Without `total` a truncated page is indistinguishable from a complete one, so
/// the UI cannot tell "you searched and found nothing" from "the row you are
/// looking for fell outside the 200 most recent".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryHistoryPage {
    pub entries: Vec<QueryHistoryEntry>,
    /// Rows matching the filter with `limit` ignored.
    pub total: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_filter_asks_for_a_real_page() {
        let filter = QueryHistoryFilter::default();
        assert_eq!(filter.limit, DEFAULT_HISTORY_PAGE_SIZE);
        assert_eq!(filter.order, HistoryOrder::Recent);
    }

    #[test]
    fn scope_parses_only_known_words() {
        assert_eq!(HistoryScope::parse("query"), Some(HistoryScope::Query));
        assert_eq!(
            HistoryScope::parse("workflow"),
            Some(HistoryScope::Workflow)
        );
        assert_eq!(HistoryScope::parse("all"), Some(HistoryScope::All));
        assert_eq!(HistoryScope::parse("Query"), None);
        assert_eq!(HistoryScope::parse(""), None);
    }

    #[test]
    fn order_parses_only_known_words() {
        assert_eq!(HistoryOrder::parse("slowest"), Some(HistoryOrder::Slowest));
        assert_eq!(HistoryOrder::parse("recent"), Some(HistoryOrder::Recent));
        assert_eq!(HistoryOrder::parse("newest"), None);
    }
}
