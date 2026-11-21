use std::collections::HashMap;

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrendKind {
    Rate,
    Gauge,
}

impl TrendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rate => "rate",
            Self::Gauge => "gauge",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrendSeriesDef {
    pub kind: TrendKind,
    pub counter: Option<&'static str>,
    pub field: Option<&'static str>,
}

pub const TREND_SERIES: [(&str, TrendSeriesDef); 17] = [
    (
        "qps",
        TrendSeriesDef {
            kind: TrendKind::Rate,
            counter: Some("questionsCounter"),
            field: None,
        },
    ),
    (
        "commits",
        TrendSeriesDef {
            kind: TrendKind::Rate,
            counter: Some("xactCommitCounter"),
            field: None,
        },
    ),
    (
        "rollbacks",
        TrendSeriesDef {
            kind: TrendKind::Rate,
            counter: Some("xactRollbackCounter"),
            field: None,
        },
    ),
    (
        "sessions",
        TrendSeriesDef {
            kind: TrendKind::Rate,
            counter: Some("newSessionsCounter"),
            field: None,
        },
    ),
    (
        "netIn",
        TrendSeriesDef {
            kind: TrendKind::Rate,
            counter: Some("bytesInCounter"),
            field: None,
        },
    ),
    (
        "netOut",
        TrendSeriesDef {
            kind: TrendKind::Rate,
            counter: Some("bytesOutCounter"),
            field: None,
        },
    ),
    (
        "blksRead",
        TrendSeriesDef {
            kind: TrendKind::Rate,
            counter: Some("blksReadCounter"),
            field: None,
        },
    ),
    (
        "blksHit",
        TrendSeriesDef {
            kind: TrendKind::Rate,
            counter: Some("blksHitCounter"),
            field: None,
        },
    ),
    (
        "tupFetched",
        TrendSeriesDef {
            kind: TrendKind::Rate,
            counter: Some("tupFetchedCounter"),
            field: None,
        },
    ),
    (
        "tupReturned",
        TrendSeriesDef {
            kind: TrendKind::Rate,
            counter: Some("tupReturnedCounter"),
            field: None,
        },
    ),
    (
        "tupInserted",
        TrendSeriesDef {
            kind: TrendKind::Rate,
            counter: Some("tupInsertedCounter"),
            field: None,
        },
    ),
    (
        "tupUpdated",
        TrendSeriesDef {
            kind: TrendKind::Rate,
            counter: Some("tupUpdatedCounter"),
            field: None,
        },
    ),
    (
        "tupDeleted",
        TrendSeriesDef {
            kind: TrendKind::Rate,
            counter: Some("tupDeletedCounter"),
            field: None,
        },
    ),
    (
        "tempBytes",
        TrendSeriesDef {
            kind: TrendKind::Rate,
            counter: Some("tempBytesCounter"),
            field: None,
        },
    ),
    (
        "sessionTotal",
        TrendSeriesDef {
            kind: TrendKind::Gauge,
            counter: None,
            field: Some("sessionTotal"),
        },
    ),
    (
        "sessionActive",
        TrendSeriesDef {
            kind: TrendKind::Gauge,
            counter: None,
            field: Some("sessionActive"),
        },
    ),
    (
        "sessionIdle",
        TrendSeriesDef {
            kind: TrendKind::Gauge,
            counter: None,
            field: Some("sessionIdle"),
        },
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandKey {
    Select,
    Insert,
    Update,
    Delete,
}

impl CommandKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Insert => "insert",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

pub const COMMAND_KEYS: [CommandKey; 4] = [
    CommandKey::Select,
    CommandKey::Insert,
    CommandKey::Update,
    CommandKey::Delete,
];

pub type StatusRecord = HashMap<String, Value>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrendPoint {
    pub t: i64,
    pub v: f64,
}

pub type TrendSeries = Vec<TrendPoint>;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrendTimeAxis {
    pub domain: (i64, i64),
    pub ticks: Vec<i64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TrendAxisOptions {
    pub min_window_ms: Option<i64>,
    pub max_window_ms: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrendPrevSample {
    pub ts: i64,
    pub values: HashMap<String, Option<f64>>,
}

#[derive(Clone, Debug, Default)]
pub struct TrendUpdateInput {
    pub record: StatusRecord,
    pub now: i64,
    pub period_ms: Option<i64>,
    pub prev: TrendPrevSample,
    pub history: HashMap<String, TrendSeries>,
    pub statements: Option<Value>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrendUpdateResult {
    pub history: HashMap<String, TrendSeries>,
    pub prev: TrendPrevSample,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_shape_is_pinned() {
        assert_eq!(TREND_SERIES.len(), 17);
        assert_eq!(TREND_SERIES[0].0, "qps");
        assert_eq!(TREND_SERIES[0].1.kind, TrendKind::Rate);
        assert_eq!(TREND_SERIES[0].1.counter, Some("questionsCounter"));
        assert_eq!(TREND_SERIES[14].0, "sessionTotal");
        assert_eq!(TREND_SERIES[14].1.kind, TrendKind::Gauge);
        assert_eq!(TREND_SERIES[14].1.field, Some("sessionTotal"));
        assert_eq!(
            COMMAND_KEYS.map(|key| key.as_str()),
            ["select", "insert", "update", "delete"]
        );
        assert_eq!(TrendKind::Rate.as_str(), "rate");
        assert_eq!(TrendKind::Gauge.as_str(), "gauge");
    }
}
