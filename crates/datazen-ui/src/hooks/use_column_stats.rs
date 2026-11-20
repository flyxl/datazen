//! Port of `packages/frontend/src/hooks/useColumnStats.ts` (T210) — the
//! column-statistics lifecycle (DB-12).
//!
//! The header menu asks for one column; the hook runs the
//! single-statement aggregate **on the connection's query path** and
//! reports the six values the dialog renders. The numbers must never
//! come from the loaded page — a 100M-row table (the PRD's acceptance
//! case) is aggregated by the server, never pulled.
//!
//! ## What maps where
//!
//! - `lib/columnStats.ts` (the SQL builder, the averagable table, the
//!   row parser) is T180's (segment 3); the three pure functions are
//!   carried here until that task lands and absorbs them — the
//!   `lib_helpers` pattern. `escapeIdent`'s registry lookup (the
//!   `DB_REGISTRY` quote char) is the host seam every dialect port uses:
//!   the table carries the resolved quote char.
//! - `queryCommands.executeQueryStream` is the stream seam: [`Self::open`]
//!   hands the host the exact stream call (the SQL, `recordHistory:
//!   false` — machine SQL must not pollute a user's history —
//!   `applyResultLimit: false` — an aggregate answers one row and result
//!   rewriting must not touch a known shape — plus the routing
//!   database/schema), and the host feeds the answers back through
//!   `on_rows` / `on_stream_done` / `on_stream_error`, each stamped with
//!   the stream's revision.
//! - `revisionRef` is the hook's revision counter: a slow first column
//!   must not land over a second one the user asked for while it was
//!   still flying — every answer whose revision is not the newest open
//!   is dropped, and `dismiss` retires the request (a late answer after
//!   close reopens nothing).
//! - The `.then` arm: a stream that finishes without a single row is an
//!   error the user should see ("empty aggregate result" — the server's
//!   own contract says an aggregate always answers one), not zeros.
//! - The state is the dialog's [`ColumnStatsState`] (the single source;
//!   `components/data_table/column_stats_dialog.rs`).
//!
//! Known deltas: JS `Number()`'s hex/whitespace leniency in `toCount`
//! is Rust's `f64::from_str` (plain decimals only; JS's `Number('0x10')`
//! reading 16 is a quirk, not a contract); a JSON object/array cell in
//! min/max renders as its compact JSON text.

use crate::components::data_table::column_stats_dialog::{
    ColumnStats, ColumnStatsState, ColumnStatsStatus, StatsValue,
};

// ---------------------------------------------------------------------------
// lib/columnStats.ts (carried until T180 absorbs it)
// ---------------------------------------------------------------------------

/// The table half of a stats target: identity + the quoting style.
pub(crate) struct ColumnStatsTable<'a> {
    /// The connection's database type (identity; the registry seam
    /// resolves the quote char below).
    pub database_type: &'a str,
    /// PG-family schema qualifier; `None` routes via the database only.
    pub schema: Option<&'a str>,
    pub table_name: &'a str,
    /// The registry-resolved quote char (`escapeIdent`'s `quoteChar`).
    pub quote_char: char,
}

/// Base type names whose `AVG(col)` is defined in every supported
/// dialect, sorted for the binary search. The numeric families aggregate
/// everywhere; `AVG(text)` does not exist in PostgreSQL and would fail
/// the whole single statement — one non-aggregable column must not cost
/// the other five values.
pub(crate) const AVERAGABLE_TYPES: &[&str] = &[
    "bigint",
    "bigserial",
    "decimal",
    "double",
    "float",
    "float4",
    "float8",
    "int",
    "int2",
    "int4",
    "int8",
    "integer",
    "mediumint",
    "money",
    "number",
    "numeric",
    "real",
    "serial",
    "smallint",
    "smallserial",
    "tinyint",
];

fn is_averagable_base(base: &str) -> bool {
    AVERAGABLE_TYPES.binary_search(&base).is_ok()
}

/// `isAveragableType`: whether `AVG(col)` is worth asking for. Unknown
/// types err on the side of *not* averaging (a text column shows five
/// honest values instead of six). The base name — lower-cased, width
/// suffix cut — decides, but the first word decides too: `double
/// precision` (PG) and `int unsigned` (MySQL) are numeric families
/// written in two words.
pub(crate) fn is_averagable_type(data_type: Option<&str>) -> bool {
    let Some(data_type) = data_type else { return false };
    let base = data_type.trim().to_lowercase();
    let base = base.split('(').next().unwrap_or_default().trim();
    if is_averagable_base(base) {
        return true;
    }
    base.split_whitespace().next().is_some_and(is_averagable_base)
}

/// `escapeIdent` (lib/databaseTypes.ts, the registry seam's quoting
/// half): double the delimiter inside, wrap in it; the `none` style
/// passes through.
fn escape_ident(name: &str, quote_char: char) -> String {
    match quote_char {
        '`' => format!("`{}`", name.replace('`', "``")),
        '"' => format!("\"{}\"", name.replace('"', "\"\"")),
        '[' => format!("[{}]", name.replace(']', "]]")),
        _ => name.to_owned(),
    }
}

/// `qualifiedTable`: quote `schema.table` for the dialect — the same
/// walk the SQL templates use.
fn qualified_table(table: &ColumnStatsTable<'_>) -> String {
    [table.schema, Some(table.table_name)]
        .into_iter()
        .flatten()
        .map(|segment| escape_ident(segment, table.quote_char))
        .collect::<Vec<_>>()
        .join(".")
}

/// The stable projection order the row parser expects.
const AVGLESS_ALIASES: [&str; 5] =
    ["row_count", "non_null_count", "distinct_count", "min_value", "max_value"];

/// `buildColumnStatsSql`: the single-statement aggregate for one column
/// — one round trip, computed server-side. Aliases stay unquoted
/// snake_case (valid bare identifiers everywhere; the row is parsed by
/// position).
pub(crate) fn build_column_stats_sql(
    table: &ColumnStatsTable<'_>,
    column: &str,
    include_avg: bool,
) -> String {
    let quoted_column = escape_ident(column, table.quote_char);
    let mut selectors = vec![
        format!("COUNT(*) AS {}", AVGLESS_ALIASES[0]),
        format!("COUNT({quoted_column}) AS {}", AVGLESS_ALIASES[1]),
        format!("COUNT(DISTINCT {quoted_column}) AS {}", AVGLESS_ALIASES[2]),
        format!("MIN({quoted_column}) AS {}", AVGLESS_ALIASES[3]),
        format!("MAX({quoted_column}) AS {}", AVGLESS_ALIASES[4]),
    ];
    if include_avg {
        selectors.push(format!("AVG({quoted_column}) AS avg_value"));
    }
    format!("SELECT {} FROM {}", selectors.join(", "), qualified_table(table))
}

/// `toCount`: `COUNT` reaches the UI as a number in most drivers, but
/// wider counts may arrive as decimal strings; both parse, anything
/// unusable reads as `None`.
fn to_count(value: Option<&serde_json::Value>) -> Option<i64> {
    match value {
        Some(serde_json::Value::Number(number)) => {
            let parsed = number.as_f64()?;
            if parsed.is_finite() {
                Some(parsed as i64)
            } else {
                None
            }
        }
        Some(serde_json::Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return None;
            }
            trimmed.parse::<f64>().ok().filter(|n| n.is_finite()).map(|n| n as i64)
        }
        _ => None,
    }
}

fn to_stats_value(value: Option<&serde_json::Value>) -> StatsValue {
    match value {
        None | Some(serde_json::Value::Null) => StatsValue::Null,
        Some(serde_json::Value::String(text)) => StatsValue::Text(text.clone()),
        Some(serde_json::Value::Number(number)) => {
            StatsValue::Number(number.as_f64().unwrap_or(f64::NAN))
        }
        Some(serde_json::Value::Bool(flag)) => StatsValue::Bool(*flag),
        // A JSON cell renders as its compact JSON text.
        Some(other @ (serde_json::Value::Object(_) | serde_json::Value::Array(_))) => {
            StatsValue::Text(other.to_string())
        }
    }
}

/// `parseColumnStatsRow`: parse the single aggregate row by the position
/// [`build_column_stats_sql`] projected it in. A missing row part reads
/// as `None` — the dialog renders an honest "—", never a wrong number.
pub(crate) fn parse_column_stats_row(
    row: Option<&[serde_json::Value]>,
    include_avg: bool,
) -> ColumnStats {
    let cells = row.unwrap_or_default();
    let row_count = to_count(cells.first());
    let non_null_count = to_count(cells.get(1));
    ColumnStats {
        row_count,
        non_null_count,
        null_count: match (row_count, non_null_count) {
            (Some(rows), Some(non_null)) => Some((rows - non_null).max(0)),
            _ => None,
        },
        distinct_count: to_count(cells.get(2)),
        min: to_stats_value(cells.get(3)),
        max: to_stats_value(cells.get(4)),
        avg: if include_avg {
            to_stats_value(cells.get(5))
        } else {
            StatsValue::Null
        },
        supports_avg: include_avg,
    }
}

// ---------------------------------------------------------------------------
// The hook (the lifecycle state machine)
// ---------------------------------------------------------------------------

/// React `ColumnStatsRequest`: the stats target plus the routing.
pub(crate) struct ColumnStatsRequest<'a> {
    pub table: ColumnStatsTable<'a>,
    /// The connection's runtime database session.
    pub db_session_id: &'a str,
    /// The logical database the table lives in (routing, never a session
    /// `USE`).
    pub database: &'a str,
}

/// The stream call [`ColumnStatsHook::open`] asks the host to run (the
/// `executeQueryStream` seam's exact arguments).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ColumnStatsStream {
    /// The machine-generated aggregate statement.
    pub sql: String,
    /// Whether `AVG` was included.
    pub include_avg: bool,
    /// The session the statement runs on.
    pub db_session_id: String,
    /// Routing database / schema.
    pub database: String,
    pub schema: Option<String>,
    /// `recordHistory: false` — machine SQL must not pollute a user's
    /// query history.
    pub record_history: bool,
    /// `applyResultLimit: false` — an aggregate answers one row; result
    /// rewriting must not touch a known shape.
    pub apply_result_limit: bool,
}

/// The useColumnStats lifecycle (the revision-guarded state machine).
#[derive(Default)]
pub(crate) struct ColumnStatsHook {
    state: ColumnStatsState,
    /// The revision of the latest `open` (React's `revisionRef`).
    revision: u64,
    /// The open's `includeAvg` (the row parser's expectation).
    include_avg: bool,
    /// Whether the current stream has already delivered a ready row (the
    /// `.then` arm's `prev.status === 'ready'` check).
    resolved: bool,
}

impl ColumnStatsHook {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The current state (the dialog's render source).
    pub(crate) fn state(&self) -> &ColumnStatsState {
        &self.state
    }

    /// `open`: build the SQL (AVG only when the column type aggregates),
    /// retire any previous request, and enter loading. The returned
    /// stream is the host's to run; every answer it produces must be fed
    /// back with the revision it belongs to.
    pub(crate) fn open(
        &mut self,
        request: &ColumnStatsRequest<'_>,
        column: &str,
        column_type: Option<&str>,
    ) -> ColumnStatsStream {
        let include_avg = is_averagable_type(column_type);
        let sql = build_column_stats_sql(&request.table, column, include_avg);
        self.revision += 1;
        self.include_avg = include_avg;
        self.resolved = false;
        self.state = ColumnStatsState {
            status: ColumnStatsStatus::Loading,
            column: Some(column.to_owned()),
            stats: None,
            error: None,
        };
        ColumnStatsStream {
            sql,
            include_avg,
            db_session_id: request.db_session_id.to_owned(),
            database: request.database.to_owned(),
            schema: request.table.schema.map(str::to_owned),
            record_history: false,
            apply_result_limit: false,
        }
    }

    /// The stream's `rows` event (every other event type is ignored —
    /// `event.type !== 'rows'` returns): the first row makes the state
    /// ready, parsed by position. A stale revision lands nothing.
    pub(crate) fn on_rows(&mut self, revision: u64, rows: &[Vec<serde_json::Value>]) {
        if revision != self.revision {
            return;
        }
        let Some(first_row) = rows.first() else {
            return;
        };
        self.state = ColumnStatsState {
            status: ColumnStatsStatus::Ready,
            stats: Some(parse_column_stats_row(Some(first_row), self.include_avg)),
            error: None,
            column: self.state.column.clone(),
        };
        self.resolved = true;
    }

    /// The stream's completion (the `.then` arm): without a ready row it
    /// is an error the user should see — the server's own contract says
    /// an aggregate always answers one.
    pub(crate) fn on_stream_done(&mut self, revision: u64) {
        if revision != self.revision {
            return;
        }
        if self.resolved {
            return;
        }
        self.state = ColumnStatsState {
            status: ColumnStatsStatus::Error,
            column: self.state.column.clone(),
            stats: None,
            error: Some("empty aggregate result".to_owned()),
        };
    }

    /// The stream's rejection (the `.catch` arm): the error message.
    pub(crate) fn on_stream_error(&mut self, revision: u64, message: &str) {
        if revision != self.revision {
            return;
        }
        self.state = ColumnStatsState {
            status: ColumnStatsStatus::Error,
            column: self.state.column.clone(),
            stats: None,
            error: Some(message.to_owned()),
        };
    }

    /// `dismiss`: retire the request (a late answer reopens nothing) and
    /// return to idle.
    pub(crate) fn dismiss(&mut self) {
        self.revision += 1;
        self.resolved = false;
        self.state = ColumnStatsState::default();
    }
}
