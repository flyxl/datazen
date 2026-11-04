//! The shared reader that turns one `SELECT` into a result origin.
//!
//! This lives in the driver API on purpose: the statement is parsed here, but
//! the **verdict** is still the driver's — a driver that cannot vouch for a
//! statement never calls this, and a driver that does call it still confirms
//! the relation against its own catalog before answering.
//!
//! The reader is deliberately conservative. It reports what it can prove and
//! marks everything else as "not a straight copy of a base column", because a
//! wrong "yes" costs a wrong `UPDATE` while a wrong "no" costs a grey cell.
//!
//! Three facts come out of it:
//!
//! * the **shape** of the row source (one base table, several relations, a
//!   derived source, or unknown);
//! * the **flags** that make a result unwritable regardless of shape
//!   (`DISTINCT`, aggregation, a set operation, a derived source);
//! * the **per-column provenance** — which result columns are straight copies
//!   of which base columns.

use core::ops::ControlFlow;

use sqlparser::ast::{
    Expr, GroupByExpr, ObjectName, ObjectNamePart, Query, Select, SelectItem,
    SelectItemQualifiedWildcardKind, SetExpr, SetOperator, Statement, TableAlias, TableFactor,
    Visit, Visitor,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::result_origin::{
    ResultColumnOrigin, ResultObjectKind, ResultOrigin, ResultOriginDetail, ResultShape,
    ResultTableRef, SetOperation, UndeterminableReason,
};

/// Function names that make a result an aggregate.
///
/// The list is deliberately generous: an unknown aggregate reported as a
/// scalar function would only make a column look editable when it is not,
/// and the column-level check catches that anyway. What matters is that the
/// common aggregates are never mistaken for a plain column copy.
const AGGREGATE_FUNCTIONS: &[&str] = &[
    "any_value",
    "array_agg",
    "avg",
    "bit_and",
    "bit_or",
    "bit_xor",
    "bool_and",
    "bool_or",
    "corr",
    "count",
    "covar_pop",
    "covar_samp",
    "every",
    "group_concat",
    "json_agg",
    "jsonb_agg",
    "listagg",
    "max",
    "median",
    "min",
    "mode",
    "percentile_cont",
    "percentile_disc",
    "stddev",
    "stddev_pop",
    "stddev_samp",
    "string_agg",
    "sum",
    "var_pop",
    "var_samp",
    "variance",
];

/// Read one statement and report where its rows come from.
///
/// `result_columns` are the column names the **executed** result reports.
/// They are needed because `SELECT *` names no column at all: only the result
/// itself knows how many columns the star expanded to.
///
/// A statement that cannot be read at all yields
/// [`ResultOrigin::Undeterminable`]; the reader never guesses.
pub fn analyze_select_origin(sql: &str, result_columns: &[String]) -> ResultOrigin {
    let statements = match Parser::parse_sql(&GenericDialect {}, sql) {
        Ok(statements) => statements,
        Err(_) => return undeterminable(UndeterminableReason::UnsupportedDialect),
    };
    if statements.len() != 1 {
        return undeterminable(UndeterminableReason::UnsupportedDialect);
    }
    let query = match &statements[0] {
        Statement::Query(query) => query,
        _ => return undeterminable(UndeterminableReason::UnsupportedDialect),
    };
    let facts = analyze_query(query);
    let table = facts.table.clone();
    let columns = build_column_origins(&facts, result_columns);
    ResultOrigin::Determined(ResultOriginDetail {
        shape: facts.shape,
        table,
        referenced_tables: facts.referenced_tables,
        deduplicated: facts.deduplicated,
        aggregated: facts.aggregated,
        set_operation: facts.set_operation,
        has_derived_source: facts.has_derived_source,
        // The reader cannot tell a base table from a view: only the driver's
        // own catalog can. Until it does, the verdict stays "unknown", which
        // the host treats as read-only.
        object_kind: ResultObjectKind::Unknown,
        columns,
    })
}

fn undeterminable(reason: UndeterminableReason) -> ResultOrigin {
    ResultOrigin::Undeterminable(reason)
}

/// What the reader found in one statement.
struct StatementFacts {
    shape: ResultShape,
    table: Option<ResultTableRef>,
    referenced_tables: u32,
    deduplicated: bool,
    aggregated: bool,
    set_operation: Option<SetOperation>,
    has_derived_source: bool,
    projection: Vec<ProjectionItem>,
    /// The relation the projection columns may be qualified with.
    qualifier: Option<String>,
}

/// One projected item, before it is aligned with the executed result.
enum ProjectionItem {
    /// A star: it stands for every column of the result.
    Wildcard,
    /// A named expression.
    Column {
        passthrough: bool,
        source_column: Option<String>,
        implied_name: String,
    },
}

fn analyze_query(query: &Query) -> StatementFacts {
    analyze_body(&query.body)
}

fn analyze_body(body: &SetExpr) -> StatementFacts {
    match body {
        SetExpr::Select(select) => analyze_select(select),
        SetExpr::Query(query) => analyze_query(query),
        SetExpr::SetOperation {
            op, left, right, ..
        } => {
            // A set operation combines two row sources; the host refuses it
            // before it ever looks at the shape.
            let mut facts = analyze_body(left);
            let right_facts = analyze_body(right);
            facts.referenced_tables =
                facts.referenced_tables.max(1) + right_facts.referenced_tables.max(1);
            facts.set_operation = Some(match op {
                SetOperator::Union => SetOperation::Union,
                SetOperator::Intersect => SetOperation::Intersect,
                SetOperator::Except | SetOperator::Minus => SetOperation::Except,
            });
            facts.shape = ResultShape::MultiTable;
            facts.table = None;
            facts.qualifier = None;
            facts
        }
        _ => StatementFacts {
            shape: ResultShape::Unknown,
            table: None,
            referenced_tables: 0,
            deduplicated: false,
            aggregated: false,
            set_operation: None,
            has_derived_source: false,
            projection: Vec::new(),
            qualifier: None,
        },
    }
}

fn analyze_select(select: &Select) -> StatementFacts {
    let mut referenced_tables = 0u32;
    let mut join_count = 0u32;
    let mut has_derived_source = false;
    let mut relations: Vec<Relation> = Vec::new();
    let mut from_count = 0usize;

    for with_joins in &select.from {
        from_count += 1;
        match &with_joins.relation {
            TableFactor::Table { name, alias, .. } => {
                referenced_tables += 1;
                relations.push(Relation::new(name, alias.as_ref()));
            }
            // A derived table, a table function, a pivot — none of them is a
            // base relation the rows can be written back to.
            _ => has_derived_source = true,
        }
        for join in &with_joins.joins {
            join_count += 1;
            match &join.relation {
                TableFactor::Table { name, alias, .. } => {
                    referenced_tables += 1;
                    relations.push(Relation::new(name, alias.as_ref()));
                }
                _ => has_derived_source = true,
            }
        }
    }

    let shape = if from_count == 0 {
        ResultShape::Unknown
    } else if has_derived_source {
        ResultShape::Derived
    } else if referenced_tables == 1 && join_count == 0 {
        ResultShape::SingleBaseTable
    } else {
        ResultShape::MultiTable
    };

    let table = if shape == ResultShape::SingleBaseTable {
        relations.first().map(|relation| relation.reference.clone())
    } else {
        None
    };
    // The qualifier a projected column may use: the alias the statement gave
    // the relation, or its real name when it has none.
    let qualifier = relations.first().and_then(|relation| relation.qualifier());

    let mut aggregated = match &select.group_by {
        GroupByExpr::Expressions(exprs, _) => !exprs.is_empty(),
        GroupByExpr::All(_) => true,
    };
    if select.having.is_some() || select.qualify.is_some() {
        aggregated = true;
    }

    let mut projection = Vec::with_capacity(select.projection.len());
    for item in &select.projection {
        match item {
            SelectItem::Wildcard(_) => projection.push(ProjectionItem::Wildcard),
            SelectItem::QualifiedWildcard(kind, _) => match kind {
                SelectItemQualifiedWildcardKind::ObjectName(name) => {
                    if qualified_wildcard_matches(name, &relations) {
                        projection.push(ProjectionItem::Wildcard);
                    } else {
                        projection.push(ProjectionItem::Column {
                            passthrough: false,
                            source_column: None,
                            implied_name: String::new(),
                        });
                    }
                }
                SelectItemQualifiedWildcardKind::Expr(_) => {
                    projection.push(ProjectionItem::Column {
                        passthrough: false,
                        source_column: None,
                        implied_name: String::new(),
                    })
                }
            },
            SelectItem::UnnamedExpr(expr) => {
                projection.push(projection_item(expr, qualifier.as_deref()));
                if expr_has_aggregate(expr) {
                    aggregated = true;
                }
                if expr_has_subquery(expr) {
                    has_derived_source = true;
                }
            }
            SelectItem::ExprWithAlias { expr, alias } => {
                let mut item = projection_item(expr, qualifier.as_deref());
                if let ProjectionItem::Column { implied_name, .. } = &mut item {
                    *implied_name = alias.value.clone();
                }
                projection.push(item);
                if expr_has_aggregate(expr) {
                    aggregated = true;
                }
                if expr_has_subquery(expr) {
                    has_derived_source = true;
                }
            }
        }
    }

    StatementFacts {
        shape,
        table,
        referenced_tables,
        deduplicated: select.distinct.is_some(),
        aggregated,
        set_operation: None,
        has_derived_source,
        projection,
        qualifier,
    }
}

/// One relation the statement reads from.
///
/// The **reference** keeps the real name — the only name a write-back
/// statement may use — while the **qualifier** is the name the statement
/// itself uses to address the relation, which is the alias when it has one.
struct Relation {
    reference: ResultTableRef,
    alias: Option<String>,
}

impl Relation {
    fn new(name: &ObjectName, alias: Option<&TableAlias>) -> Self {
        Self {
            reference: table_ref(name, alias.is_some()),
            alias: alias.map(|alias| alias.name.value.clone()),
        }
    }

    /// The name a projected column may be qualified with.
    fn qualifier(&self) -> Option<String> {
        Some(
            self.alias
                .clone()
                .unwrap_or_else(|| self.reference.name.clone()),
        )
    }
}

/// Turn an object name into the relation it names.
///
/// The name may be `table`, `schema.table` or `catalog.schema.table`; the
/// relation reference keeps the parts apart so a write-back statement can
/// address the relation the way the catalog does.
fn table_ref(name: &ObjectName, aliased: bool) -> ResultTableRef {
    let parts: Vec<String> = name
        .0
        .iter()
        .filter_map(|part| match part {
            ObjectNamePart::Identifier(ident) => Some(ident.value.clone()),
            ObjectNamePart::Function(_) => None,
        })
        .collect();
    let (catalog, schema, name) = match parts.len() {
        0 => (None, None, String::new()),
        1 => (None, None, parts[0].clone()),
        2 => (None, Some(parts[0].clone()), parts[1].clone()),
        _ => (
            Some(parts[parts.len() - 3].clone()),
            Some(parts[parts.len() - 2].clone()),
            parts[parts.len() - 1].clone(),
        ),
    };
    ResultTableRef {
        catalog,
        schema,
        name,
        aliased,
    }
}

/// Whether `t.*` names the only relation of the statement.
fn qualified_wildcard_matches(name: &ObjectName, relations: &[Relation]) -> bool {
    let Some(relation) = relations.first() else {
        return false;
    };
    let reference = &relation.reference;
    let parts: Vec<String> = name
        .0
        .iter()
        .filter_map(|part| match part {
            ObjectNamePart::Identifier(ident) => Some(ident.value.clone()),
            ObjectNamePart::Function(_) => None,
        })
        .collect();
    let Some(last) = parts.last() else {
        return false;
    };
    // `t.*` — the qualifier is the relation itself (or its alias).
    if parts.len() == 1 {
        return last.eq_ignore_ascii_case(&reference.name)
            || relation
                .alias
                .as_deref()
                .is_some_and(|alias| last.eq_ignore_ascii_case(alias));
    }
    // `schema.t.*` — the qualifier is the schema the relation lives in.
    if parts.len() == 2 {
        return reference
            .schema
            .as_deref()
            .is_some_and(|schema| schema.eq_ignore_ascii_case(&parts[0]))
            && last.eq_ignore_ascii_case(&reference.name);
    }
    false
}

/// Describe one projected expression.
fn projection_item(expr: &Expr, qualifier: Option<&str>) -> ProjectionItem {
    match expr {
        Expr::Identifier(ident) => ProjectionItem::Column {
            passthrough: true,
            source_column: Some(ident.value.clone()),
            implied_name: ident.value.clone(),
        },
        Expr::CompoundIdentifier(idents) => {
            let Some(column) = idents.last() else {
                return not_passthrough();
            };
            // `a.b` — the qualifier must be the relation the statement reads
            // from, otherwise the value comes from somewhere else entirely.
            let qualifier_matches = match idents.len() {
                1 => true,
                2 => qualifier.is_some_and(|q| q.eq_ignore_ascii_case(&idents[0].value)),
                3 => qualifier.is_some_and(|q| q.eq_ignore_ascii_case(&idents[1].value)),
                _ => false,
            };
            if qualifier_matches {
                ProjectionItem::Column {
                    passthrough: true,
                    source_column: Some(column.value.clone()),
                    implied_name: column.value.clone(),
                }
            } else {
                not_passthrough()
            }
        }
        Expr::Nested(inner) => projection_item(inner, qualifier),
        _ => not_passthrough(),
    }
}

fn not_passthrough() -> ProjectionItem {
    ProjectionItem::Column {
        passthrough: false,
        source_column: None,
        implied_name: String::new(),
    }
}

/// Align the projection with the executed result and report the provenance.
fn build_column_origins(
    facts: &StatementFacts,
    result_columns: &[String],
) -> Vec<ResultColumnOrigin> {
    let wildcard_only = facts.projection.len() == 1
        && matches!(facts.projection.first(), Some(ProjectionItem::Wildcard));
    let any_wildcard = facts
        .projection
        .iter()
        .any(|item| matches!(item, ProjectionItem::Wildcard));

    if wildcard_only {
        // `SELECT *` — every result column is a straight copy of the base
        // column of the same name.
        return result_columns
            .iter()
            .map(|column| ResultColumnOrigin {
                column: column.clone(),
                passthrough: true,
                table: facts.table.clone(),
                source_column: Some(column.clone()),
            })
            .collect();
    }
    if any_wildcard {
        // A star mixed with other items cannot be attributed column by
        // column, so nothing is claimed to be a straight copy.
        return result_columns
            .iter()
            .map(|column| ResultColumnOrigin {
                column: column.clone(),
                passthrough: false,
                table: None,
                source_column: None,
            })
            .collect();
    }

    facts
        .projection
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let (passthrough, source_column, implied_name) = match item {
                ProjectionItem::Wildcard => (false, None, String::new()),
                ProjectionItem::Column {
                    passthrough,
                    source_column,
                    implied_name,
                } => (*passthrough, source_column.clone(), implied_name.clone()),
            };
            // The result's own name wins: the database may have renamed a
            // column, and the result is what the user sees.
            let column = result_columns.get(index).cloned().unwrap_or(implied_name);
            ResultColumnOrigin {
                column,
                passthrough,
                table: if passthrough {
                    facts.table.clone()
                } else {
                    None
                },
                source_column,
            }
        })
        .collect()
}

/// Whether an expression aggregates.
fn expr_has_aggregate(expr: &Expr) -> bool {
    let mut scan = ExpressionScan::default();
    let _ = expr.visit(&mut scan);
    scan.has_aggregate
}

/// Whether an expression contains a subquery.
fn expr_has_subquery(expr: &Expr) -> bool {
    let mut scan = ExpressionScan::default();
    let _ = expr.visit(&mut scan);
    scan.has_subquery
}

/// A walk over one expression, looking for the two things that change the
/// verdict: an aggregate and a subquery.
#[derive(Default)]
struct ExpressionScan {
    has_aggregate: bool,
    has_subquery: bool,
}

impl Visitor for ExpressionScan {
    type Break = ();

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
        match expr {
            Expr::Subquery(_) | Expr::Exists { .. } | Expr::InSubquery { .. } => {
                self.has_subquery = true;
            }
            Expr::Function(function) => {
                if function.over.is_some() || is_aggregate_name(&function.name) {
                    self.has_aggregate = true;
                }
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }
}

/// Whether a function name is an aggregate.
fn is_aggregate_name(name: &ObjectName) -> bool {
    let Some(last) = name.0.iter().rev().find_map(|part| match part {
        ObjectNamePart::Identifier(ident) => Some(ident.value.to_lowercase()),
        ObjectNamePart::Function(_) => None,
    }) else {
        return false;
    };
    AGGREGATE_FUNCTIONS.contains(&last.as_str())
}
