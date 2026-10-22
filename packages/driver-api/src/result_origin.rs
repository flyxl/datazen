//! Where did the rows of a query result come from?
//!
//! The host decides whether a cell of a **query result** may be written back
//! to the database. That decision needs facts the host cannot produce on its
//! own: which relation the rows came from, whether that relation is a base
//! table, and which result column is a straight copy of which base column.
//!
//! The host never parses SQL (the parser lives in the driver crates and is not
//! reachable from the host), so **the driver is the only source of these
//! facts**. This module is that contract:
//!
//! * [`ResultOrigin`] — the verdict: "I cannot tell" or "here is what I found";
//! * [`ResultOriginDetail`] — the findings, one entry per result column;
//! * [`analyze::analyze_select_origin`] — a shared, dialect-neutral reader that
//!   turns one `SELECT` into a [`ResultOriginDetail`], so a driver does not
//!   have to write a parser of its own;
//! * [`describe_result_origin_with_catalog`] — the helper a driver calls to
//!   answer the contract end to end: parse the statement, then confirm
//!   against its own catalog that the relation really is a base table.
//!
//! The default answer is [`ResultOrigin::Undeterminable`]: a driver that
//! implements nothing keeps its result grids read-only, exactly as before this
//! contract existed.

use serde::{Deserialize, Serialize};

use crate::types::{DriverError, TableType};
use crate::{ConnectionHandle, DatabaseDriver, SqlTarget};

mod analyze;
#[cfg(test)]
mod tests;

pub use analyze::analyze_select_origin;

/// The verdict on where a result came from.
///
/// The wire form is a tagged union — `{"kind":"undeterminable","detail":…}` or
/// `{"kind":"determined","detail":{…}}` — because an untagged pair of optional
/// payloads cannot tell "no verdict" from "a verdict with no payload".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "detail")]
pub enum ResultOrigin {
    /// The driver cannot say where the result came from.
    Undeterminable(UndeterminableReason),
    /// The driver says exactly where the result came from.
    Determined(ResultOriginDetail),
}

/// Why a driver cannot describe a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UndeterminableReason {
    /// The driver does not implement the capability at all (the default).
    NotImplemented,
    /// The driver implements it, but this dialect or statement shape is beyond
    /// what it can describe.
    UnsupportedDialect,
    /// The metadata the answer depends on cannot be read right now.
    MetadataUnavailable,
}

/// The shape of the row source of a result.
///
/// Only [`ResultShape::SingleBaseTable`] can ever be written back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResultShape {
    /// Every row comes from exactly one base table.
    SingleBaseTable,
    /// The rows come from more than one relation (a join, or an implicit
    /// comma join).
    MultiTable,
    /// The rows come from a derived source rather than a named relation.
    Derived,
    /// The driver cannot classify the shape.
    Unknown,
}

/// A set operation that combines two result sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SetOperation {
    /// `UNION` (and `UNION ALL`).
    Union,
    /// `INTERSECT`.
    Intersect,
    /// `EXCEPT` (and `MINUS`).
    Except,
}

/// What kind of relation the rows come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResultObjectKind {
    /// A base table — the only kind this stage writes back to.
    BaseTable,
    /// A view.
    View,
    /// A materialized view.
    MaterializedView,
    /// The driver cannot tell.
    Unknown,
}

/// One relation the result reads from.
///
/// `aliased` only records that the statement gave the relation an alias; it
/// never makes the relation writable. The **real** name is always in `name`,
/// because that is the only name a write-back statement may use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultTableRef {
    /// Catalog (database) the relation lives in, when the statement named one.
    pub catalog: Option<String>,
    /// Schema the relation lives in, when the statement named one.
    pub schema: Option<String>,
    /// The real relation name — never the alias.
    pub name: String,
    /// Whether the statement used an alias for this relation.
    pub aliased: bool,
}

/// Where one result column comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultColumnOrigin {
    /// The column name **as the result reports it** (an alias, when the
    /// statement gave one).
    pub column: String,
    /// Whether the value is copied straight out of one base-table column,
    /// without an expression, a function, a constant or an aggregate around it.
    pub passthrough: bool,
    /// The relation the value comes from; `None` when it is not a passthrough
    /// column.
    pub table: Option<ResultTableRef>,
    /// The **real** column name on the base table — the only name a write-back
    /// statement may use. `None` for a column that is not a passthrough.
    pub source_column: Option<String>,
}

/// Everything the driver found about one result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultOriginDetail {
    /// The overall shape of the row source.
    pub shape: ResultShape,
    /// The relation the rows come from; set exactly when the shape is
    /// [`ResultShape::SingleBaseTable`].
    pub table: Option<ResultTableRef>,
    /// How many relations the statement reads from. The host cross-checks it
    /// against the shape instead of counting `FROM` clauses itself.
    pub referenced_tables: u32,
    /// Whether the statement de-duplicates rows (`DISTINCT` and friends).
    pub deduplicated: bool,
    /// Whether the statement aggregates (`GROUP BY`, `HAVING`, aggregate
    /// functions, window functions).
    pub aggregated: bool,
    /// The set operation that combines two result sets, when there is one.
    pub set_operation: Option<SetOperation>,
    /// Whether the rows come from a derived source (a subquery in `FROM`, or a
    /// subquery in the projection). A subquery in `WHERE` does **not** set
    /// this: the column sources are unaffected by it.
    pub has_derived_source: bool,
    /// What kind of relation the rows come from.
    pub object_kind: ResultObjectKind,
    /// One entry per result column, in the order the result reports them.
    /// A length that does not match the executed result makes the whole
    /// verdict unusable.
    pub columns: Vec<ResultColumnOrigin>,
}

/// Answer the result-origin contract for a driver.
///
/// The two halves are:
///
/// 1. [`analyze_select_origin`] reads the statement and reports the shape and
///    the per-column provenance;
/// 2. the relation kind is confirmed against the driver's **own catalog** —
///    a statement naming `orders` does not prove that `orders` is a base table
///    rather than a view over three tables.
///
/// A catalog that cannot be read leaves the kind
/// [`ResultObjectKind::Unknown`], which the host treats as "not provably a
/// base table" — read-only. The helper never guesses.
pub async fn describe_result_origin_with_catalog<D>(
    driver: &D,
    handle: &ConnectionHandle,
    target: SqlTarget<'_>,
    result_columns: &[String],
    sql: &str,
) -> Result<ResultOrigin, DriverError>
where
    D: DatabaseDriver + ?Sized,
{
    let mut origin = analyze_select_origin(sql, result_columns);
    let relation = match &origin {
        ResultOrigin::Determined(detail) => detail.table.clone(),
        ResultOrigin::Undeterminable(_) => None,
    };
    let Some(relation) = relation else {
        return Ok(origin);
    };
    let kind = resolve_relation_kind(driver, handle, target, &relation).await;
    if let ResultOrigin::Determined(detail) = &mut origin {
        detail.object_kind = kind;
    }
    Ok(origin)
}

/// Confirm what kind of relation `relation` is, using the driver's catalog.
///
/// A catalog read that fails is **not** an error: it leaves the kind
/// [`ResultObjectKind::Unknown`], and an unknown kind is read-only. Failing
/// the whole verdict instead would turn a catalog hiccup into a broken
/// result grid.
pub async fn resolve_relation_kind<D>(
    driver: &D,
    handle: &ConnectionHandle,
    target: SqlTarget<'_>,
    relation: &ResultTableRef,
) -> ResultObjectKind
where
    D: DatabaseDriver + ?Sized,
{
    let database = target.database().unwrap_or("");
    let Ok(tables) = driver.get_tables(handle, database, target.schema()).await else {
        return ResultObjectKind::Unknown;
    };
    let wanted_schema = relation.schema.as_deref().map(normalize_name);
    let wanted_name = normalize_name(&relation.name);
    let mut found: Option<&TableType> = None;
    for table in &tables {
        let name_matches = normalize_name(&table.name) == wanted_name;
        let schema_matches = match (wanted_schema.as_deref(), table.schema.as_deref()) {
            (Some(wanted), Some(actual)) => normalize_name(actual) == wanted,
            // The statement named no schema and the catalog reports one: the
            // name is still the deciding part, because the driver already
            // scoped the listing to the target schema.
            (None, _) => true,
            (Some(_), None) => false,
        };
        if name_matches && schema_matches {
            found = Some(&table.table_type);
            break;
        }
    }
    match found {
        Some(TableType::Table) => ResultObjectKind::BaseTable,
        Some(TableType::View) => ResultObjectKind::View,
        Some(TableType::MaterializedView) => ResultObjectKind::MaterializedView,
        Some(TableType::SystemTable) | None => ResultObjectKind::Unknown,
    }
}

/// Compare relation names the way a catalog does: case-insensitively.
///
/// A quoted identifier is case-sensitive in most dialects, but the catalog
/// listing and the statement are both produced by the same driver, so the
/// comparison only has to be forgiving enough to match the same relation
/// written in two spellings.
fn normalize_name(name: &str) -> String {
    name.trim().to_lowercase()
}
