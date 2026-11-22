//! The candidate-values loader — `createCandidateValuesLoader` and
//! `loadFkCandidates` (DB-20).
//!
//! React runs one `SELECT DISTINCT … LIMIT 200` only when the user asks for
//! candidates; opening an editor never queries. A single-column foreign key
//! takes the other branch: browse the target table through the same resolver
//! the jump uses, with no key values and the selector's hard cap. The one
//! refusal the loader raises itself is a target whose referenced columns are
//! not unique — picking one would write an ambiguous reference.
//!
//! No command is called here. The host supplies the prepared statement, the
//! FK resolution and the driver facts; this module decides which branch runs,
//! what wire request it carries and how the result becomes picker options.

use std::collections::BTreeMap;

/// React `CANDIDATE_VALUE_LIMIT` (`lib/cellEditorCandidates.ts`).
#[allow(dead_code)] // pure constant; drop with the candidate loader.
pub(crate) const CANDIDATE_VALUE_LIMIT: usize = 200;

/// React `FK_SELECTOR_CANDIDATE_LIMIT` (`lib/fkSelector.ts`).
#[allow(dead_code)] // pure constant; drop with the FK picker.
pub(crate) const FK_SELECTOR_CANDIDATE_LIMIT: usize = 50;

/// One candidate cell as the wire reports it.
#[allow(dead_code)] // VM mirror; drop with the candidate loader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CandidateValueVm {
    Null,
    Text(String),
    Number(String),
    Bool(bool),
    Other(String),
}

impl CandidateValueVm {
    /// React `formatFkCandidate`: only `null`/`undefined` has no option.
    #[allow(dead_code)] // pure helper; drop with the picker.
    pub(crate) fn display(&self) -> Option<String> {
        match self {
            Self::Null => None,
            Self::Text(text) | Self::Number(text) | Self::Other(text) => Some(text.clone()),
            Self::Bool(value) => Some(value.to_string()),
        }
    }
}

/// React `FkCandidateSource` (the group's single local column).
#[allow(dead_code)] // VM mirror; drop when the host owns FK groups.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FkCandidateSourceVm {
    pub(crate) local_column: String,
    pub(crate) constraint_name: Option<String>,
}

/// React `FkCandidateResolution`'s facts as the loader reads them.
#[allow(dead_code)] // VM mirror; drop with the FK picker.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FkResolutionVm {
    pub(crate) unique: bool,
    pub(crate) candidates: Vec<Vec<CandidateValueVm>>,
}

/// The one refusal the loader raises on its own.
#[allow(dead_code)] // decision; drop with the FK picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FkCandidateErrorVm {
    pub(crate) message_key: &'static str,
}

#[allow(dead_code)] // exact React key; drop with the FK picker.
pub(crate) const NOT_UNIQUE_ERROR: FkCandidateErrorVm = FkCandidateErrorVm {
    message_key: "fkSelector.notUnique",
};

/// The host-owned arguments of `createCandidateValuesLoader` plus the single
/// statement the host prepared for the enum branch (`buildDistinctCandidatesSql`
/// is a separate pure module; a missing/empty statement means "no candidates").
#[allow(dead_code)] // VM mirror; drop with the candidate loader.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CandidateLoaderArgsVm {
    pub(crate) db_session_id: String,
    pub(crate) table: String,
    pub(crate) schema: Option<String>,
    pub(crate) database: Option<String>,
    pub(crate) distinct_sql: Option<String>,
    pub(crate) fk_sources: BTreeMap<String, FkCandidateSourceVm>,
}

/// The browse request: no key values, exactly one `limit`, `browse: true`.
#[allow(dead_code)] // host intent; drop with the FK picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FkBrowseRequestVm {
    pub(crate) db_session_id: String,
    pub(crate) database: Option<String>,
    pub(crate) schema: Option<String>,
    pub(crate) table: String,
    pub(crate) columns: Vec<String>,
    pub(crate) constraint_name: Option<String>,
    pub(crate) values: Vec<CandidateValueVm>,
    pub(crate) limit: usize,
    pub(crate) browse: bool,
}

/// The enum branch's single statement, with the session/database/schema it runs in.
#[allow(dead_code)] // host intent; drop with the candidate loader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EnumQueryRequestVm {
    pub(crate) db_session_id: String,
    pub(crate) sql: String,
    pub(crate) database: Option<String>,
    pub(crate) schema: Option<String>,
}

/// Which branch the loader takes for one column.
#[allow(dead_code)] // decision; drop with the candidate loader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CandidateRequestPlanVm {
    FkBrowse(FkBrowseRequestVm),
    EnumQuery(EnumQueryRequestVm),
    NoSql,
}

/// React `createCandidateValuesLoader`: the FK source wins; otherwise the
/// prepared DISTINCT statement is used, and `null`/empty means no statement.
#[allow(dead_code)] // pure decision; drop with the candidate loader.
pub(crate) fn candidate_request_plan(
    column_name: &str,
    args: &CandidateLoaderArgsVm,
) -> CandidateRequestPlanVm {
    if let Some(source) = args.fk_sources.get(column_name) {
        return CandidateRequestPlanVm::FkBrowse(FkBrowseRequestVm {
            db_session_id: args.db_session_id.clone(),
            database: args.database.clone(),
            schema: args.schema.clone(),
            table: args.table.clone(),
            columns: vec![source.local_column.clone()],
            constraint_name: source.constraint_name.clone(),
            values: Vec::new(),
            limit: FK_SELECTOR_CANDIDATE_LIMIT,
            browse: true,
        });
    }
    match args.distinct_sql.as_ref() {
        Some(sql) if !sql.is_empty() => CandidateRequestPlanVm::EnumQuery(EnumQueryRequestVm {
            db_session_id: args.db_session_id.clone(),
            sql: sql.clone(),
            database: args.database.clone(),
            schema: args.schema.clone(),
        }),
        _ => CandidateRequestPlanVm::NoSql,
    }
}

/// The first statement of a query result (React `result.results[0]`).
#[allow(dead_code)] // VM mirror; drop with the candidate loader.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct QueryResultVm {
    pub(crate) statements: Vec<QueryStatementVm>,
}

/// One statement's rows.
#[allow(dead_code)] // VM mirror; drop with the candidate loader.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct QueryStatementVm {
    pub(crate) rows: Vec<Vec<CandidateValueVm>>,
}

/// React's enum branch: first statement, first cell of each row, no nulls.
#[allow(dead_code)] // pure helper; drop with the candidate loader.
pub(crate) fn enum_candidate_values(result: Option<&QueryResultVm>) -> Vec<String> {
    let Some(result) = result else {
        return Vec::new();
    };
    let Some(statement) = result.statements.first() else {
        return Vec::new();
    };
    statement
        .rows
        .iter()
        .filter_map(|row| row.first().and_then(CandidateValueVm::display))
        .collect()
}

/// React `candidatesToPickerOptions`: statement order is kept and duplicates
/// are kept; only a null first cell has no option.
#[allow(dead_code)] // pure helper; drop with the FK picker.
pub(crate) fn fk_candidate_values(
    resolution: &FkResolutionVm,
) -> Result<Vec<String>, FkCandidateErrorVm> {
    if !resolution.unique {
        return Err(NOT_UNIQUE_ERROR);
    }
    Ok(resolution
        .candidates
        .iter()
        .filter_map(|row| row.first().and_then(CandidateValueVm::display))
        .collect())
}

/// The loader's result for one request. A FK resolution that the host has not
/// supplied yet offers nothing rather than inventing rows; an unknown request
/// (no statement) is the React `if (!sql) return []` short-circuit.
#[allow(dead_code)] // pure decision; drop with the candidate loader.
pub(crate) fn load_candidate_values(
    plan: &CandidateRequestPlanVm,
    query_result: Option<&QueryResultVm>,
    fk_resolution: Option<&FkResolutionVm>,
) -> Result<Vec<String>, FkCandidateErrorVm> {
    match plan {
        CandidateRequestPlanVm::NoSql => Ok(Vec::new()),
        CandidateRequestPlanVm::EnumQuery(_) => Ok(enum_candidate_values(query_result)),
        CandidateRequestPlanVm::FkBrowse(_) => match fk_resolution {
            Some(resolution) => fk_candidate_values(resolution),
            None => Ok(Vec::new()),
        },
    }
}
