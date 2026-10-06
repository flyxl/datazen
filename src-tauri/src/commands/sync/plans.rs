//! Server-owned immutable Data Synchronization comparison plans.
//!
//! The comparison response is useful for review, but it is not an execution
//! authority.  The complete reviewed comparison remains in this registry and
//! later commands receive only an opaque plan id plus a validated selection.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use datazen_driver_api::{iter_driver_factories, DatabaseDriver, TableSchema, PROTOCOL_VERSION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::comparison_store::{ComparisonStore, ComparisonTableMetadata};
use crate::data_sync::{
    ChangeOperation, ComparisonResult, ConflictPolicy, RowChange, SyncOptions, SyncSourceFilter,
    TableMappingStatus, TableResult,
};

pub(crate) const SYNC_PLAN_TTL: Duration = Duration::from_secs(15 * 60);
/// Bounds one page sent over the review IPC. The server still owns the full
/// comparison and execution always reloads it from `ComparisonStore`.
pub(crate) const SYNC_COMPARISON_PAGE_SIZE: u32 = 100;
pub(crate) const SYNC_COMPARISON_PAGE_MAX_LIMIT: u32 = 500;
/// SQL preview and execution consume the persisted comparison in bounded
/// chunks. Keep this aligned with the largest review page so one path cannot
/// accidentally retain more row payloads than the IPC contract allows.
pub(crate) const SYNC_COMPARISON_STREAM_PAGE_SIZE: usize = SYNC_COMPARISON_PAGE_MAX_LIMIT as usize;
pub(crate) const SYNC_COMPARISON_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
struct RelationFingerprintEntry {
    database: String,
    schema: Option<String>,
    relation: String,
    table_schema: Option<TableSchema>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_filter: Option<SyncSourceFilter>,
}

/// A deterministic identity for the qualified objects participating in a
/// reviewed comparison.  Endpoint identity is included in the hash so a
/// client cannot redirect an old plan to another database by changing UI
/// fields after review.
pub(crate) fn fingerprint_relations(
    database: &str,
    schema: Option<&str>,
    entries: impl IntoIterator<Item = (String, Option<TableSchema>)>,
) -> Result<String, String> {
    fingerprint_relations_with_filters(
        database,
        schema,
        entries
            .into_iter()
            .map(|(relation, table_schema)| (relation, table_schema, None)),
    )
}

/// Fingerprint the qualified schema plus the structured predicates that were
/// used for the reviewed comparison. Filter values are part of the plan
/// identity, so execution cannot silently reuse a plan for another scope.
pub(crate) fn fingerprint_relations_with_filters(
    database: &str,
    schema: Option<&str>,
    entries: impl IntoIterator<Item = (String, Option<TableSchema>, Option<SyncSourceFilter>)>,
) -> Result<String, String> {
    let mut entries: Vec<RelationFingerprintEntry> = entries
        .into_iter()
        .map(
            |(relation, table_schema, source_filter)| RelationFingerprintEntry {
                database: database.to_string(),
                schema: schema.map(str::to_string),
                relation,
                table_schema,
                source_filter,
            },
        )
        .collect();
    entries.sort_by(|left, right| {
        (&left.database, &left.schema, &left.relation).cmp(&(
            &right.database,
            &right.schema,
            &right.relation,
        ))
    });
    let bytes = serde_json::to_vec(&entries)
        .map_err(|error| format!("cannot fingerprint sync schema: {error}"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(crate) fn driver_protocol_version(driver: &dyn DatabaseDriver) -> u32 {
    let driver_type = driver.driver_type();
    iter_driver_factories()
        .into_iter()
        .find(|factory| factory.driver_id() == driver_type)
        .map(|factory| factory.protocol_version())
        .unwrap_or(PROTOCOL_VERSION)
}

pub(crate) fn fingerprint_conflict_policy(policy: ConflictPolicy) -> String {
    format!("{:x}", Sha256::digest(policy_name(policy).as_bytes()))
}

fn policy_name(policy: ConflictPolicy) -> &'static str {
    match policy {
        ConflictPolicy::Abort => "abort",
        ConflictPolicy::Skip => "skip",
        ConflictPolicy::Force => "force",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SyncSelectedRow {
    pub source_table: String,
    pub target_table: String,
    pub operation: ChangeOperation,
    pub key: Vec<datazen_driver_api::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SyncSelectionExclusion {
    pub operation: ChangeOperation,
    pub key: Vec<datazen_driver_api::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SyncSelectionMode {
    #[default]
    All,
    Defaults,
}

/// A server-owned selection of comparison rows for one table and operation.
/// `All` selects every row while `Defaults` selects the rows that would be
/// checked by the normal operation defaults. The client may only add key
/// exclusions; row payloads and SQL never cross this boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SyncTableSelection {
    pub source_table: String,
    pub target_table: String,
    #[serde(default)]
    pub selection_mode: SyncSelectionMode,
    pub operations: Vec<ChangeOperation>,
    #[serde(default)]
    pub excluded_rows: Vec<SyncSelectionExclusion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SyncRunSelection {
    pub revision: u64,
    #[serde(default)]
    pub rows: Vec<SyncSelectedRow>,
    #[serde(default)]
    pub scopes: Vec<SyncTableSelection>,
}

#[derive(Debug)]
struct SelectionScopeMatcher {
    selection_mode: SyncSelectionMode,
    operations: Vec<ChangeOperation>,
    excluded_rows: HashSet<String>,
}

/// The server-owned selection contract compiled into a bounded row matcher.
/// It contains only client supplied keys and scope metadata; row payloads
/// remain in the ComparisonStore and are read one page at a time.
#[derive(Debug)]
pub(crate) struct SelectionMatcher {
    explicit_rows: HashSet<String>,
    scopes: HashMap<(String, String), Vec<SelectionScopeMatcher>>,
}

impl SelectionMatcher {
    pub(crate) fn new(selection: &SyncRunSelection, options: &SyncOptions) -> Result<Self, String> {
        let mut explicit_rows = HashSet::new();
        for row in &selection.rows {
            if !matches!(
                row.operation,
                ChangeOperation::Insert | ChangeOperation::Update | ChangeOperation::Delete
            ) {
                return Err("selection contains an invalid operation".into());
            }
            if !options.allows(row.operation) {
                return Err(
                    "selection contains an operation disabled by the requested options".into(),
                );
            }
            let token = selection_token(
                &row.source_table,
                &row.target_table,
                row.operation,
                &row.key,
            )?;
            if !explicit_rows.insert(token) {
                return Err("selection contains a duplicate row".into());
            }
        }

        let mut scopes = HashMap::<(String, String), Vec<SelectionScopeMatcher>>::new();
        let mut seen_scope_modes = HashSet::new();
        let mut seen_scope_operations = HashMap::<(String, String), Vec<ChangeOperation>>::new();
        for scope in &selection.scopes {
            let pair = (scope.source_table.clone(), scope.target_table.clone());
            if scope.operations.is_empty() {
                return Err("selection scope must include at least one operation".into());
            }
            if !seen_scope_modes.insert((pair.0.clone(), pair.1.clone(), scope.selection_mode)) {
                return Err("selection contains a duplicate table scope mode".into());
            }
            let mut operations = Vec::with_capacity(scope.operations.len());
            for operation in &scope.operations {
                if !matches!(
                    operation,
                    ChangeOperation::Insert | ChangeOperation::Update | ChangeOperation::Delete
                ) {
                    return Err("selection scope contains an invalid operation".into());
                }
                if operations.contains(operation) {
                    return Err("selection scope contains a duplicate operation".into());
                }
                if !options.allows(*operation) {
                    return Err(
                        "selection scope contains an operation disabled by the requested options"
                            .into(),
                    );
                }
                operations.push(*operation);
            }
            let pair_operations = seen_scope_operations.entry(pair.clone()).or_default();
            if operations
                .iter()
                .any(|operation| pair_operations.contains(operation))
            {
                return Err("selection contains overlapping table scope operations".into());
            }
            pair_operations.extend(operations.iter().copied());

            let mut excluded_rows = HashSet::new();
            for exclusion in &scope.excluded_rows {
                if !operations.contains(&exclusion.operation) {
                    return Err("selection exclusion is outside its table scope".into());
                }
                if !options.allows(exclusion.operation) {
                    return Err(
                        "selection exclusion contains an operation disabled by the requested options"
                            .into(),
                    );
                }
                let token = selection_token(
                    &scope.source_table,
                    &scope.target_table,
                    exclusion.operation,
                    &exclusion.key,
                )?;
                if !excluded_rows.insert(token) {
                    return Err("selection contains a duplicate exclusion".into());
                }
            }
            scopes.entry(pair).or_default().push(SelectionScopeMatcher {
                selection_mode: scope.selection_mode,
                operations,
                excluded_rows,
            });
        }
        Ok(Self {
            explicit_rows,
            scopes,
        })
    }

    fn excluded_rows(&self) -> impl Iterator<Item = &String> {
        self.scopes
            .values()
            .flat_map(|scopes| scopes.iter())
            .flat_map(|scope| scope.excluded_rows.iter())
    }

    fn is_selected(
        &self,
        source_table: &str,
        target_table: &str,
        change: &RowChange,
        options: &SyncOptions,
    ) -> Result<bool, String> {
        let token = selection_token(source_table, target_table, change.operation, &change.key)?;
        if self.explicit_rows.contains(&token) {
            return Ok(true);
        }
        Ok(self
            .scopes
            .get(&(source_table.to_string(), target_table.to_string()))
            .into_iter()
            .flat_map(|scopes| scopes.iter())
            .any(|scope| {
                scope.operations.contains(&change.operation)
                    && (scope.selection_mode == SyncSelectionMode::All
                        || change.operation.default_selected(options))
                    && !scope.excluded_rows.contains(&token)
            }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SyncRunRequest {
    pub plan_id: String,
    pub selection: SyncRunSelection,
    pub options: SyncOptions,
    pub job_id: Option<String>,
}

/// Build the unattended profile selection from a server-owned comparison.
///
/// Persisted profiles do not carry row payloads or SQL.  A profile run applies
/// every enabled operation for every matched table in the immutable plan;
/// the plan registry remains the only source of table and row identities.
pub(crate) fn default_profile_selection(
    plan_id: &str,
    revision: u64,
    options: &SyncOptions,
) -> Result<SyncRunSelection, String> {
    let plan = peek_plan(plan_id)?;
    if plan.selection_revision != revision {
        return Err("selection revision is stale; return to comparison".into());
    }
    let summaries = plan.comparison.summaries()?;
    let operations = [
        ChangeOperation::Insert,
        ChangeOperation::Update,
        ChangeOperation::Delete,
    ]
    .into_iter()
    .filter(|operation| options.allows(*operation))
    .collect::<Vec<_>>();
    if operations.is_empty() {
        return Err("profile has no enabled Data Sync operations".into());
    }
    let scopes = summaries
        .into_iter()
        .filter(|table| table.table.status == TableMappingStatus::Matched)
        .map(|table| SyncTableSelection {
            source_table: table.table.source_table,
            target_table: table.table.target_table,
            selection_mode: SyncSelectionMode::All,
            operations: operations.clone(),
            excluded_rows: Vec::new(),
        })
        .collect();
    Ok(SyncRunSelection {
        revision,
        rows: Vec::new(),
        scopes,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncComparisonTableSummary {
    pub source_table: String,
    pub target_table: String,
    pub status: TableMappingStatus,
    pub incompatible_reason: Option<String>,
    pub columns: Vec<String>,
    pub column_types: Vec<String>,
    pub primary_keys: Vec<String>,
    pub unchanged_count: usize,
    pub insert_count: usize,
    pub update_count: usize,
    pub delete_count: usize,
    pub row_count: usize,
    pub page_size: u32,
    pub first_cursor: Option<String>,
    pub has_more: bool,
    pub warnings: Vec<String>,
    pub source_filter: Option<SyncSourceFilter>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncComparisonPreview {
    pub contract_version: u32,
    pub plan_id: String,
    pub selection_revision: u64,
    pub page_size: u32,
    pub tables: Vec<SyncComparisonTableSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncComparisonPage {
    pub contract_version: u32,
    pub plan_id: String,
    pub source_table: String,
    pub target_table: String,
    pub cursor: Option<String>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub page_size: u32,
    pub rows: Vec<RowChange>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SyncComparisonPageRequest {
    pub plan_id: String,
    pub source_table: String,
    pub target_table: String,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

fn cursor_digest(plan_id: &str, source_table: &str, target_table: &str, offset: usize) -> String {
    format!(
        "{:x}",
        Sha256::digest(format!("{plan_id}\0{source_table}\0{target_table}\0{offset}").as_bytes())
    )
}

fn make_cursor(plan_id: &str, source_table: &str, target_table: &str, offset: usize) -> String {
    format!(
        "v{SYNC_COMPARISON_CONTRACT_VERSION}.{offset}.{}",
        cursor_digest(plan_id, source_table, target_table, offset)
    )
}

fn parse_cursor(
    plan_id: &str,
    source_table: &str,
    target_table: &str,
    cursor: &str,
    row_count: usize,
) -> Result<usize, String> {
    let mut parts = cursor.split('.');
    let version = parts.next();
    let offset = parts.next();
    let digest = parts.next();
    if parts.next().is_some() || version != Some("v1") {
        return Err("comparison cursor is invalid or belongs to another contract".into());
    }
    let offset = offset
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(|| "comparison cursor is invalid".to_string())?;
    let expected = cursor_digest(plan_id, source_table, target_table, offset);
    if digest != Some(expected.as_str()) {
        return Err("comparison cursor is invalid or belongs to another table".into());
    }
    if offset >= row_count {
        return Err("comparison cursor is outside the comparison result".into());
    }
    Ok(offset)
}

fn summary_for_table(
    plan_id: &str,
    table: &super::comparison_store::ComparisonTableMetadata,
) -> SyncComparisonTableSummary {
    let row_count = table.row_count;
    SyncComparisonTableSummary {
        source_table: table.table.source_table.clone(),
        target_table: table.table.target_table.clone(),
        status: table.table.status,
        incompatible_reason: table.table.incompatible_reason.clone(),
        columns: table.table.columns.clone(),
        column_types: table.table.column_types.clone(),
        primary_keys: table.table.primary_keys.clone(),
        unchanged_count: table.unchanged_count,
        insert_count: table.insert_count,
        update_count: table.update_count,
        delete_count: table.delete_count,
        row_count,
        page_size: SYNC_COMPARISON_PAGE_SIZE,
        first_cursor: (row_count > 0).then(|| {
            make_cursor(
                plan_id,
                &table.table.source_table,
                &table.table.target_table,
                0,
            )
        }),
        has_more: row_count > SYNC_COMPARISON_PAGE_SIZE as usize,
        warnings: table.table.warnings.clone(),
        source_filter: table.table.source_filter.clone(),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct StoredSyncPlan {
    pub(crate) source_db_session_id: String,
    pub(crate) target_db_session_id: String,
    pub(crate) source_database: String,
    pub(crate) target_database: String,
    pub(crate) source_schema: Option<String>,
    pub(crate) target_schema: Option<String>,
    pub(crate) source_driver_type: String,
    pub(crate) target_driver_type: String,
    pub(crate) source_driver_protocol: u32,
    pub(crate) target_driver_protocol: u32,
    pub(crate) source_schema_fingerprint: String,
    pub(crate) target_schema_fingerprint: String,
    pub(crate) comparison: ComparisonStore,
    pub(crate) options: SyncOptions,
    pub(crate) conflict_policy_fingerprint: String,
    pub(crate) selection_revision: u64,
    pub(crate) target_read_only_at_preview: bool,
    expires_at: Instant,
}

pub(crate) struct SyncPlanStore {
    plans: Mutex<HashMap<String, StoredSyncPlan>>,
}

impl SyncPlanStore {
    pub(crate) fn new() -> Self {
        Self {
            plans: Mutex::new(HashMap::new()),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn issue(
        &self,
        source_db_session_id: String,
        target_db_session_id: String,
        source_database: String,
        target_database: String,
        source_schema: Option<String>,
        target_schema: Option<String>,
        source_driver: &dyn DatabaseDriver,
        target_driver: &dyn DatabaseDriver,
        source_schema_fingerprint: String,
        target_schema_fingerprint: String,
        comparison: ComparisonResult,
        options: SyncOptions,
        target_read_only_at_preview: bool,
    ) -> Result<SyncComparisonPreview, String> {
        let id = Uuid::new_v4().to_string();
        let selection_revision = 1;
        let comparison = ComparisonStore::from_comparison(comparison)?;
        self.issue_with_store(
            id,
            selection_revision,
            source_db_session_id,
            target_db_session_id,
            source_database,
            target_database,
            source_schema,
            target_schema,
            source_driver,
            target_driver,
            source_schema_fingerprint,
            target_schema_fingerprint,
            comparison,
            options,
            target_read_only_at_preview,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn issue_with_store(
        &self,
        id: String,
        selection_revision: u64,
        source_db_session_id: String,
        target_db_session_id: String,
        source_database: String,
        target_database: String,
        source_schema: Option<String>,
        target_schema: Option<String>,
        source_driver: &dyn DatabaseDriver,
        target_driver: &dyn DatabaseDriver,
        source_schema_fingerprint: String,
        target_schema_fingerprint: String,
        comparison: ComparisonStore,
        options: SyncOptions,
        target_read_only_at_preview: bool,
    ) -> Result<SyncComparisonPreview, String> {
        let preview_tables = comparison
            .summaries()?
            .iter()
            .map(|table| summary_for_table(&id, table))
            .collect();
        let conflict_policy_fingerprint = fingerprint_conflict_policy(options.conflict_policy);
        let plan = StoredSyncPlan {
            source_db_session_id,
            target_db_session_id,
            source_database,
            target_database,
            source_schema,
            target_schema,
            source_driver_type: source_driver.driver_type(),
            target_driver_type: target_driver.driver_type(),
            source_driver_protocol: driver_protocol_version(source_driver),
            target_driver_protocol: driver_protocol_version(target_driver),
            source_schema_fingerprint,
            target_schema_fingerprint,
            comparison,
            options,
            conflict_policy_fingerprint,
            selection_revision,
            target_read_only_at_preview,
            expires_at: Instant::now() + SYNC_PLAN_TTL,
        };
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| "sync plan registry is unavailable".to_string())?;
        let now = Instant::now();
        plans.retain(|_, existing| existing.expires_at > now);
        plans.insert(id.clone(), plan);
        Ok(SyncComparisonPreview {
            contract_version: SYNC_COMPARISON_CONTRACT_VERSION,
            plan_id: id,
            selection_revision,
            tables: preview_tables,
            page_size: SYNC_COMPARISON_PAGE_SIZE,
        })
    }

    pub(crate) fn peek(&self, id: &str) -> Result<StoredSyncPlan, String> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| "sync plan registry is unavailable".to_string())?;
        let Some(plan) = plans.get(id) else {
            return Err("sync plan is unknown or has expired; return to comparison".into());
        };
        if plan.expires_at <= Instant::now() {
            plans.remove(id);
            return Err("sync plan has expired; return to comparison".into());
        }
        Ok(plan.clone())
    }

    /// Claim before a write starts.  A claimed plan remains consumed even if
    /// the transaction result is unknown, so the UI can never retry blindly.
    pub(crate) fn claim(&self, id: &str) -> Result<StoredSyncPlan, String> {
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| "sync plan registry is unavailable".to_string())?;
        let Some(plan) = plans.remove(id) else {
            return Err("sync plan is unknown or has expired; return to comparison".into());
        };
        if plan.expires_at <= Instant::now() {
            return Err("sync plan has expired; return to comparison".into());
        }
        Ok(plan)
    }
}

impl Default for SyncPlanStore {
    fn default() -> Self {
        Self::new()
    }
}

fn global_store() -> &'static SyncPlanStore {
    static STORE: OnceLock<SyncPlanStore> = OnceLock::new();
    STORE.get_or_init(SyncPlanStore::new)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn issue_plan(
    source_db_session_id: String,
    target_db_session_id: String,
    source_database: String,
    target_database: String,
    source_schema: Option<String>,
    target_schema: Option<String>,
    source_driver: &dyn DatabaseDriver,
    target_driver: &dyn DatabaseDriver,
    source_schema_fingerprint: String,
    target_schema_fingerprint: String,
    comparison: ComparisonResult,
    options: SyncOptions,
    target_read_only_at_preview: bool,
) -> Result<SyncComparisonPreview, String> {
    global_store().issue(
        source_db_session_id,
        target_db_session_id,
        source_database,
        target_database,
        source_schema,
        target_schema,
        source_driver,
        target_driver,
        source_schema_fingerprint,
        target_schema_fingerprint,
        comparison,
        options,
        target_read_only_at_preview,
    )
}

/// Issue a plan under a caller-minted plan id.
///
/// §2.1: the compare command mints the id *before* the `dataSyncPrepare` Job
/// exists, so the preview and the frozen ChangeSet Artifact name the same plan.
/// The store still refuses to issue a second plan under one id.
#[allow(clippy::too_many_arguments)]
pub(crate) fn issue_plan_with_store_and_id(
    id: String,
    source_db_session_id: String,
    target_db_session_id: String,
    source_database: String,
    target_database: String,
    source_schema: Option<String>,
    target_schema: Option<String>,
    source_driver: &dyn DatabaseDriver,
    target_driver: &dyn DatabaseDriver,
    source_schema_fingerprint: String,
    target_schema_fingerprint: String,
    comparison: ComparisonStore,
    options: SyncOptions,
    target_read_only_at_preview: bool,
) -> Result<SyncComparisonPreview, String> {
    global_store().issue_with_store(
        id,
        1,
        source_db_session_id,
        target_db_session_id,
        source_database,
        target_database,
        source_schema,
        target_schema,
        source_driver,
        target_driver,
        source_schema_fingerprint,
        target_schema_fingerprint,
        comparison,
        options,
        target_read_only_at_preview,
    )
}

pub(crate) fn peek_plan(id: &str) -> Result<StoredSyncPlan, String> {
    global_store().peek(id)
}

pub(crate) fn claim_plan(id: &str) -> Result<StoredSyncPlan, String> {
    global_store().claim(id)
}

pub(crate) fn load_comparison(plan: &StoredSyncPlan) -> Result<ComparisonResult, String> {
    plan.comparison.load()
}

/// Validate a selection against the server-owned comparison without
/// reconstructing the complete ComparisonResult. The manifest and indexed
/// row frames are still validated by `summaries`/`load_table_page`; only one
/// bounded page of row payloads is resident while checking membership.
pub(crate) fn validate_selection_streaming(
    comparison: &ComparisonStore,
    selection: &SyncRunSelection,
    options: &SyncOptions,
) -> Result<SelectionMatcher, String> {
    if selection.revision == 0 {
        return Err("selection revision is required".into());
    }
    let matcher = SelectionMatcher::new(selection, options)?;
    let summaries = comparison.summaries()?;
    let matched_tables: HashSet<_> = summaries
        .iter()
        .filter(|table| table.table.status == TableMappingStatus::Matched)
        .map(|table| {
            (
                table.table.source_table.clone(),
                table.table.target_table.clone(),
            )
        })
        .collect();
    for pair in matcher.scopes.keys() {
        if !matched_tables.contains(pair) {
            return Err("selection scope does not belong to a matched comparison table".into());
        }
    }

    let mut found_explicit = HashSet::new();
    let mut found_exclusions = HashSet::new();
    let mut scoped_rows: HashSet<String> = matcher.excluded_rows().cloned().collect();
    for table in summaries
        .iter()
        .filter(|table| table.table.status == TableMappingStatus::Matched)
    {
        let pair = (
            table.table.source_table.clone(),
            table.table.target_table.clone(),
        );
        let mut offset = 0usize;
        while offset < table.row_count {
            let rows = comparison.load_table_page(
                &table.table.source_table,
                &table.table.target_table,
                offset,
                SYNC_COMPARISON_STREAM_PAGE_SIZE,
            )?;
            if rows.is_empty() {
                return Err("comparison page did not advance while validating selection".into());
            }
            offset = offset.saturating_add(rows.len());
            for change in rows {
                let token = selection_token(
                    &table.table.source_table,
                    &table.table.target_table,
                    change.operation,
                    &change.key,
                )?;
                if matcher.explicit_rows.contains(&token) {
                    found_explicit.insert(token.clone());
                }
                if let Some(scopes) = matcher.scopes.get(&pair) {
                    for scope in scopes {
                        if scope.excluded_rows.contains(&token) {
                            found_exclusions.insert(token.clone());
                        }
                        if scope.operations.contains(&change.operation)
                            && (scope.selection_mode == SyncSelectionMode::All
                                || change.operation.default_selected(options))
                            && !scope.excluded_rows.contains(&token)
                        {
                            scoped_rows.insert(token.clone());
                        }
                    }
                }
            }
        }
    }
    if found_explicit.len() != matcher.explicit_rows.len() {
        return Err("selection contains a row that was not in the comparison plan".into());
    }
    let expected_exclusions: HashSet<_> = matcher.excluded_rows().cloned().collect();
    if found_exclusions.len() != expected_exclusions.len() {
        return Err("selection exclusion was not in the comparison plan".into());
    }
    if matcher
        .explicit_rows
        .iter()
        .any(|token| scoped_rows.contains(token))
    {
        return Err("selection row duplicates a table scope".into());
    }
    Ok(matcher)
}

/// Copy only selected, option-allowed changes from a bounded page into a
/// TableResult suitable for SQL generation. Unselected rows are discarded as
/// soon as their page has been processed.
pub(crate) fn selected_table_page(
    table: &ComparisonTableMetadata,
    rows: Vec<RowChange>,
    matcher: &SelectionMatcher,
    options: &SyncOptions,
) -> Result<Option<TableResult>, String> {
    if table.table.status != TableMappingStatus::Matched {
        return Ok(None);
    }
    let mut selected = Vec::with_capacity(rows.len());
    for mut change in rows {
        change.selected = matcher.is_selected(
            &table.table.source_table,
            &table.table.target_table,
            &change,
            options,
        )?;
        if change.eligible_for_changeset(options) {
            selected.push(change);
        }
    }
    if selected.is_empty() {
        return Ok(None);
    }
    let mut result = table.table.clone();
    result.rows = selected;
    Ok(Some(result))
}

/// Read one review page from the server-owned comparison. File-backed stores
/// use their manifest index and read only the requested table row range;
/// inline stores slice the in-memory comparison.
pub(crate) fn get_comparison_page(
    request: SyncComparisonPageRequest,
) -> Result<SyncComparisonPage, String> {
    let plan = peek_plan(&request.plan_id)?;
    if request.limit == Some(0) {
        return Err("comparison page limit must be greater than zero".into());
    }
    if request
        .limit
        .is_some_and(|value| value > SYNC_COMPARISON_PAGE_MAX_LIMIT)
    {
        return Err(format!(
            "comparison page limit cannot exceed {SYNC_COMPARISON_PAGE_MAX_LIMIT}"
        ));
    }
    let limit = request
        .limit
        .unwrap_or(SYNC_COMPARISON_PAGE_SIZE)
        .min(SYNC_COMPARISON_PAGE_MAX_LIMIT) as usize;
    let table = plan
        .comparison
        .summaries()?
        .into_iter()
        .find(|table| {
            table.table.source_table == request.source_table
                && table.table.target_table == request.target_table
        })
        .ok_or_else(|| "table does not belong to the comparison plan".to_string())?;
    let offset = match request.cursor.as_deref() {
        Some(cursor) => parse_cursor(
            &request.plan_id,
            &request.source_table,
            &request.target_table,
            cursor,
            table.row_count,
        )?,
        None => 0,
    };
    let end = offset.saturating_add(limit).min(table.row_count);
    let next_cursor = (end < table.row_count).then(|| {
        make_cursor(
            &request.plan_id,
            &request.source_table,
            &request.target_table,
            end,
        )
    });
    Ok(SyncComparisonPage {
        contract_version: SYNC_COMPARISON_CONTRACT_VERSION,
        plan_id: request.plan_id,
        source_table: table.table.source_table.clone(),
        target_table: table.table.target_table.clone(),
        cursor: request.cursor,
        next_cursor,
        has_more: end < table.row_count,
        page_size: limit as u32,
        rows: plan.comparison.load_table_page(
            &request.source_table,
            &request.target_table,
            offset,
            limit,
        )?,
    })
}

pub(crate) fn apply_selection(
    comparison: &ComparisonResult,
    selection: &SyncRunSelection,
    options: &SyncOptions,
) -> Result<ComparisonResult, String> {
    validate_selection(comparison, selection, options)?;
    let mut selected = HashSet::new();
    for scope in &selection.scopes {
        let excluded: HashSet<String> = scope
            .excluded_rows
            .iter()
            .map(|row| {
                selection_token(
                    &scope.source_table,
                    &scope.target_table,
                    row.operation,
                    &row.key,
                )
            })
            .collect::<Result<_, _>>()?;
        for table in &comparison.tables {
            if table.source_table != scope.source_table || table.target_table != scope.target_table
            {
                continue;
            }
            for change in &table.rows {
                if scope.operations.contains(&change.operation)
                    && (scope.selection_mode == SyncSelectionMode::All
                        || change.operation.default_selected(options))
                {
                    let token = selection_token(
                        &table.source_table,
                        &table.target_table,
                        change.operation,
                        &change.key,
                    )?;
                    if !excluded.contains(&token) {
                        selected.insert(token);
                    }
                }
            }
        }
    }
    for row in &selection.rows {
        selected.insert(selection_token(
            &row.source_table,
            &row.target_table,
            row.operation,
            &row.key,
        )?);
    }

    let mut result = comparison.clone();
    for table in &mut result.tables {
        for change in &mut table.rows {
            change.selected = selected.contains(&selection_token(
                table.source_table.clone(),
                table.target_table.clone(),
                change.operation,
                &change.key,
            )?);
        }
    }
    Ok(result)
}

pub(crate) fn validate_selection(
    comparison: &ComparisonResult,
    selection: &SyncRunSelection,
    options: &SyncOptions,
) -> Result<(), String> {
    if selection.revision == 0 {
        return Err("selection revision is required".into());
    }
    let mut allowed = HashSet::new();
    let mut matched_tables = HashSet::new();
    for table in &comparison.tables {
        if table.status != TableMappingStatus::Matched {
            continue;
        }
        matched_tables.insert((table.source_table.clone(), table.target_table.clone()));
        for change in &table.rows {
            allowed.insert(selection_token(
                &table.source_table,
                &table.target_table,
                change.operation,
                &change.key,
            )?);
        }
    }
    let mut seen_scopes = HashSet::new();
    let mut seen_scope_operations: HashMap<(String, String), Vec<ChangeOperation>> = HashMap::new();
    let mut scoped_rows = HashSet::new();
    for scope in &selection.scopes {
        let pair = (scope.source_table.clone(), scope.target_table.clone());
        if !matched_tables.contains(&pair) {
            return Err("selection scope does not belong to a matched comparison table".into());
        }
        if scope.operations.is_empty() {
            return Err("selection scope must include at least one operation".into());
        }
        let scope_identity = (pair.0.clone(), pair.1.clone(), scope.selection_mode);
        if !seen_scopes.insert(scope_identity) {
            return Err("selection contains a duplicate table scope mode".into());
        }
        let mut operations = Vec::new();
        for operation in &scope.operations {
            if !matches!(
                operation,
                ChangeOperation::Insert | ChangeOperation::Update | ChangeOperation::Delete
            ) {
                return Err("selection scope contains an invalid operation".into());
            }
            if operations.contains(operation) {
                return Err("selection scope contains a duplicate operation".into());
            }
            operations.push(*operation);
            if !options.allows(*operation) {
                return Err(
                    "selection scope contains an operation disabled by the requested options"
                        .into(),
                );
            }
        }
        let pair_operations = seen_scope_operations.entry(pair).or_default();
        if scope
            .operations
            .iter()
            .any(|operation| pair_operations.contains(operation))
        {
            return Err("selection contains overlapping table scope operations".into());
        }
        pair_operations.extend(scope.operations.iter().copied());
        let mut exclusions = HashSet::new();
        for exclusion in &scope.excluded_rows {
            if !scope.operations.contains(&exclusion.operation) {
                return Err("selection exclusion is outside its table scope".into());
            }
            if !options.allows(exclusion.operation) {
                return Err(
                    "selection exclusion contains an operation disabled by the requested options"
                        .into(),
                );
            }
            let token = selection_token(
                &scope.source_table,
                &scope.target_table,
                exclusion.operation,
                &exclusion.key,
            )?;
            if !allowed.contains(&token) {
                return Err("selection exclusion was not in the comparison plan".into());
            }
            if !exclusions.insert(token.clone()) {
                return Err("selection contains a duplicate exclusion".into());
            }
            scoped_rows.insert(token);
        }
        // Populate the complete scope set so explicit rows can be rejected
        // when they would select the same server-owned row twice.
        for table in comparison.tables.iter().filter(|table| {
            table.status == TableMappingStatus::Matched
                && table.source_table == scope.source_table
                && table.target_table == scope.target_table
        }) {
            for change in &table.rows {
                if !scope.operations.contains(&change.operation)
                    || (scope.selection_mode == SyncSelectionMode::Defaults
                        && !change.operation.default_selected(options))
                {
                    continue;
                }
                let token = selection_token(
                    &table.source_table,
                    &table.target_table,
                    change.operation,
                    &change.key,
                )?;
                if !exclusions.contains(&token) {
                    scoped_rows.insert(token);
                }
            }
        }
    }
    let mut seen_rows = HashSet::new();
    for row in &selection.rows {
        let token = selection_token(
            &row.source_table,
            &row.target_table,
            row.operation,
            &row.key,
        )?;
        if !allowed.contains(&token) {
            return Err("selection contains a row that was not in the comparison plan".into());
        }
        if !options.allows(row.operation) {
            return Err("selection contains an operation disabled by the requested options".into());
        }
        if !seen_rows.insert(token.clone()) {
            return Err("selection contains a duplicate row".into());
        }
        if scoped_rows.contains(&token) {
            return Err("selection row duplicates a table scope".into());
        }
    }
    Ok(())
}

fn selection_token(
    source_table: impl AsRef<str>,
    target_table: impl AsRef<str>,
    operation: ChangeOperation,
    key: &[datazen_driver_api::Value],
) -> Result<String, String> {
    serde_json::to_string(&(source_table.as_ref(), target_table.as_ref(), operation, key))
        .map_err(|error| format!("cannot validate sync selection: {error}"))
}

#[cfg(test)]
pub(crate) fn selected_rows(
    comparison: &ComparisonResult,
    options: &SyncOptions,
) -> Vec<SyncSelectedRow> {
    comparison
        .tables
        .iter()
        .filter(|table| table.status == TableMappingStatus::Matched)
        .flat_map(|table| {
            table.rows.iter().filter_map(|change: &RowChange| {
                if change.selected && change.eligible_for_changeset(options) {
                    Some(SyncSelectedRow {
                        source_table: table.source_table.clone(),
                        target_table: table.target_table.clone(),
                        operation: change.operation,
                        key: change.key.clone(),
                    })
                } else {
                    None
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_sync::{RowChange, TableResult};
    use crate::testing::mock_driver::{MockDriver, MockDriverOptions};
    use datazen_driver_api::Value;

    fn comparison() -> ComparisonResult {
        let options = SyncOptions::default();
        ComparisonResult::new(vec![TableResult::matched(
            "users",
            "users",
            vec![
                RowChange::insert(
                    vec![Value::Integer(1)],
                    vec![Some(Value::Integer(1))],
                    &options,
                ),
                RowChange::insert(
                    vec![Value::Integer(2)],
                    vec![Some(Value::Integer(2))],
                    &options,
                ),
            ],
        )])
    }

    fn table_scope(excluded_rows: Vec<SyncSelectionExclusion>) -> SyncTableSelection {
        SyncTableSelection {
            source_table: "users".into(),
            target_table: "users".into(),
            selection_mode: SyncSelectionMode::All,
            operations: vec![ChangeOperation::Insert],
            excluded_rows,
        }
    }

    #[test]
    fn table_scope_selects_unloaded_rows_and_excludes_one_key() {
        let selection = SyncRunSelection {
            revision: 1,
            rows: Vec::new(),
            scopes: vec![table_scope(vec![SyncSelectionExclusion {
                operation: ChangeOperation::Insert,
                key: vec![Value::Integer(2)],
            }])],
        };
        validate_selection(&comparison(), &selection, &SyncOptions::default()).unwrap();
        let applied = apply_selection(&comparison(), &selection, &SyncOptions::default()).unwrap();
        let selected: Vec<_> = applied.tables[0]
            .rows
            .iter()
            .filter(|row| row.selected)
            .map(|row| row.key.clone())
            .collect();
        assert_eq!(selected.len(), 1);
        assert!(matches!(selected[0].as_slice(), [Value::Integer(1)]));
    }

    #[test]
    fn defaults_scope_selects_server_rows_across_pages_and_applies_exclusions() {
        let selection = SyncRunSelection {
            revision: 1,
            rows: Vec::new(),
            scopes: vec![SyncTableSelection {
                selection_mode: SyncSelectionMode::Defaults,
                excluded_rows: vec![SyncSelectionExclusion {
                    operation: ChangeOperation::Insert,
                    key: vec![Value::Integer(2)],
                }],
                ..table_scope(Vec::new())
            }],
        };
        validate_selection(&comparison(), &selection, &SyncOptions::default()).unwrap();
        let applied = apply_selection(&comparison(), &selection, &SyncOptions::default()).unwrap();
        let selected: Vec<_> = applied.tables[0]
            .rows
            .iter()
            .filter(|row| row.selected)
            .map(|row| row.key.clone())
            .collect();
        assert_eq!(selected.len(), 1);
        assert!(matches!(selected[0].as_slice(), [Value::Integer(1)]));
    }

    #[test]
    fn table_scope_rejects_unknown_duplicate_and_disabled_inputs() {
        let unknown = SyncRunSelection {
            revision: 1,
            rows: Vec::new(),
            scopes: vec![SyncTableSelection {
                source_table: "missing".into(),
                target_table: "missing".into(),
                selection_mode: SyncSelectionMode::All,
                operations: vec![ChangeOperation::Insert],
                excluded_rows: Vec::new(),
            }],
        };
        assert!(validate_selection(&comparison(), &unknown, &SyncOptions::default()).is_err());

        let duplicate = SyncRunSelection {
            revision: 1,
            rows: Vec::new(),
            scopes: vec![table_scope(Vec::new()), table_scope(Vec::new())],
        };
        assert!(validate_selection(&comparison(), &duplicate, &SyncOptions::default()).is_err());

        let disabled = SyncRunSelection {
            revision: 1,
            rows: Vec::new(),
            scopes: vec![SyncTableSelection {
                operations: vec![ChangeOperation::Delete],
                ..table_scope(Vec::new())
            }],
        };
        assert!(validate_selection(&comparison(), &disabled, &SyncOptions::default()).is_err());
    }

    #[test]
    fn table_scope_rejects_duplicate_exclusions_and_explicit_scope_rows() {
        let duplicate_exclusions = SyncRunSelection {
            revision: 1,
            rows: Vec::new(),
            scopes: vec![table_scope(vec![
                SyncSelectionExclusion {
                    operation: ChangeOperation::Insert,
                    key: vec![Value::Integer(2)],
                },
                SyncSelectionExclusion {
                    operation: ChangeOperation::Insert,
                    key: vec![Value::Integer(2)],
                },
            ])],
        };
        assert!(validate_selection(
            &comparison(),
            &duplicate_exclusions,
            &SyncOptions::default()
        )
        .is_err());

        let duplicate_row = SyncRunSelection {
            revision: 1,
            rows: vec![SyncSelectedRow {
                source_table: "users".into(),
                target_table: "users".into(),
                operation: ChangeOperation::Insert,
                key: vec![Value::Integer(1)],
            }],
            scopes: vec![table_scope(Vec::new())],
        };
        assert!(
            validate_selection(&comparison(), &duplicate_row, &SyncOptions::default()).is_err()
        );
    }

    #[test]
    fn table_scope_rejects_explicit_row_that_was_excluded() {
        let selection = SyncRunSelection {
            revision: 1,
            rows: vec![SyncSelectedRow {
                source_table: "users".into(),
                target_table: "users".into(),
                operation: ChangeOperation::Insert,
                key: vec![Value::Integer(2)],
            }],
            scopes: vec![table_scope(vec![SyncSelectionExclusion {
                operation: ChangeOperation::Insert,
                key: vec![Value::Integer(2)],
            }])],
        };
        assert!(validate_selection(&comparison(), &selection, &SyncOptions::default()).is_err());
    }

    #[test]
    fn table_scope_contract_rejects_client_row_payloads() {
        let parsed = serde_json::from_value::<SyncRunSelection>(serde_json::json!({
            "revision": 1,
            "rows": [],
            "scopes": [{
                "sourceTable": "users",
                "targetTable": "users",
                "operations": ["INSERT"],
                "excludedRows": [{
                    "operation": "INSERT",
                    "key": [1],
                    "sourceRow": [1, "attacker supplied payload"]
                }]
            }]
        }));
        assert!(parsed.is_err());
    }

    #[test]
    fn selection_accepts_only_rows_from_the_server_comparison() {
        let selection = SyncRunSelection {
            revision: 1,
            rows: vec![SyncSelectedRow {
                source_table: "users".into(),
                target_table: "users".into(),
                operation: ChangeOperation::Insert,
                key: vec![Value::Integer(1)],
            }],
            scopes: Vec::new(),
        };
        validate_selection(&comparison(), &selection, &SyncOptions::default()).unwrap();
        let bad = SyncRunSelection {
            rows: vec![SyncSelectedRow {
                key: vec![Value::Integer(99)],
                ..selection.rows[0].clone()
            }],
            ..selection
        };
        assert!(validate_selection(&comparison(), &bad, &SyncOptions::default()).is_err());
    }

    #[test]
    fn selected_rows_have_no_source_values_or_sql() {
        let rows = selected_rows(&comparison(), &SyncOptions::default());
        assert_eq!(rows.len(), 2);
        assert!(matches!(rows[0].key.as_slice(), [Value::Integer(1)]));
    }

    #[test]
    fn filter_values_are_part_of_relation_fingerprint() {
        let schema = datazen_driver_api::TableSchema {
            table_name: "users".into(),
            columns: vec![],
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        };
        let active: SyncSourceFilter = serde_json::from_value(serde_json::json!({
            "filters": [{"column": "status", "operator": "eq", "value": "active"}],
            "logic": "and"
        }))
        .unwrap();
        let archived: SyncSourceFilter = serde_json::from_value(serde_json::json!({
            "filters": [{"column": "status", "operator": "eq", "value": "archived"}],
            "logic": "and",
            "recordset": {"start": {"value": "100"}, "limit": 25}
        }))
        .unwrap();
        let first = fingerprint_relations_with_filters(
            "db",
            Some("public"),
            vec![("users".into(), Some(schema.clone()), Some(active))],
        )
        .unwrap();
        let second = fingerprint_relations_with_filters(
            "db",
            Some("public"),
            vec![("users".into(), Some(schema), Some(archived))],
        )
        .unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn tuple_columns_values_and_inclusive_endpoints_are_part_of_plan_fingerprint() {
        let schema = datazen_driver_api::TableSchema {
            table_name: "events".into(),
            columns: vec![],
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
            check_constraints: vec![],
            table_options: Default::default(),
        };
        let fingerprint = |filter: serde_json::Value| {
            fingerprint_relations_with_filters(
                "db",
                Some("public"),
                vec![(
                    "events".into(),
                    Some(schema.clone()),
                    Some(serde_json::from_value(filter).unwrap()),
                )],
            )
            .unwrap()
        };
        let base = serde_json::json!({
            "filters": [],
            "recordset": {"tupleRange": {
                "columns": ["tenant_id", "id"],
                "start": {"values": ["10", "20"], "inclusive": true},
                "end": {"values": ["10", "99"], "inclusive": false}
            }}
        });
        let changed_column = serde_json::json!({
            "filters": [],
            "recordset": {"tupleRange": {
                "columns": ["tenant_id", "region"],
                "start": {"values": ["10", "20"], "inclusive": true},
                "end": {"values": ["10", "99"], "inclusive": false}
            }}
        });
        let changed_value = serde_json::json!({
            "filters": [],
            "recordset": {"tupleRange": {
                "columns": ["tenant_id", "id"],
                "start": {"values": ["10", "21"], "inclusive": true},
                "end": {"values": ["10", "99"], "inclusive": false}
            }}
        });
        let changed_endpoint = serde_json::json!({
            "filters": [],
            "recordset": {"tupleRange": {
                "columns": ["tenant_id", "id"],
                "start": {"values": ["10", "20"], "inclusive": false},
                "end": {"values": ["10", "99"], "inclusive": false}
            }}
        });
        let base_hash = fingerprint(base);
        assert_ne!(base_hash, fingerprint(changed_column));
        assert_ne!(base_hash, fingerprint(changed_value));
        assert_ne!(base_hash, fingerprint(changed_endpoint));
    }

    #[test]
    fn conflict_policy_has_a_distinct_plan_fingerprint() {
        assert_ne!(
            fingerprint_conflict_policy(ConflictPolicy::Abort),
            fingerprint_conflict_policy(ConflictPolicy::Skip)
        );
        assert_ne!(
            fingerprint_conflict_policy(ConflictPolicy::Skip),
            fingerprint_conflict_policy(ConflictPolicy::Force)
        );
    }

    #[test]
    fn run_request_rejects_client_replacement_sql_rows_or_mapping() {
        for field in [
            "statements",
            "tables",
            "mapping",
            "rows",
            "sourceDbSessionId",
        ] {
            let payload = serde_json::json!({
                "planId": "opaque-plan",
                "selection": { "revision": 1, "rows": [] },
                "options": SyncOptions::default(),
                "jobId": null,
                field: []
            });
            assert!(
                serde_json::from_value::<SyncRunRequest>(payload).is_err(),
                "client field {field} must be rejected"
            );
        }
    }

    #[test]
    fn comparison_preview_is_summary_only_and_pages_are_cursor_bound() {
        let source = MockDriver::new("postgres", MockDriverOptions::default());
        let target = MockDriver::new("postgres", MockDriverOptions::default());
        let options = SyncOptions::default();
        let rows = (0..205)
            .map(|value| {
                RowChange::insert(
                    vec![Value::Integer(value)],
                    vec![Some(Value::Integer(value))],
                    &options,
                )
            })
            .collect();
        let preview = issue_plan(
            "source-session".into(),
            "target-session".into(),
            "source-db".into(),
            "target-db".into(),
            None,
            None,
            source.as_ref(),
            target.as_ref(),
            "source-fingerprint".into(),
            "target-fingerprint".into(),
            ComparisonResult::new(vec![TableResult::matched("users", "users", rows)]),
            options,
            false,
        )
        .unwrap();
        let summary = &preview.tables[0];
        assert_eq!(summary.insert_count, 205);
        assert_eq!(summary.row_count, 205);
        assert!(summary.first_cursor.is_some());

        let first = get_comparison_page(SyncComparisonPageRequest {
            plan_id: preview.plan_id.clone(),
            source_table: "users".into(),
            target_table: "users".into(),
            cursor: summary.first_cursor.clone(),
            limit: Some(2),
        })
        .unwrap();
        assert_eq!(first.rows.len(), 2);
        assert!(first.has_more);
        assert_ne!(first.next_cursor, first.cursor);
        let repeated = get_comparison_page(SyncComparisonPageRequest {
            plan_id: preview.plan_id.clone(),
            source_table: "users".into(),
            target_table: "users".into(),
            cursor: first.cursor.clone(),
            limit: Some(2),
        })
        .unwrap();
        assert_eq!(
            serde_json::to_string(&repeated.rows[0].key).unwrap(),
            serde_json::to_string(&first.rows[0].key).unwrap()
        );

        let forged = get_comparison_page(SyncComparisonPageRequest {
            plan_id: preview.plan_id.clone(),
            source_table: "users".into(),
            target_table: "users".into(),
            cursor: Some("v1.999.not-a-valid-signature".into()),
            limit: Some(2),
        });
        assert!(forged.is_err());
        assert!(get_comparison_page(SyncComparisonPageRequest {
            plan_id: preview.plan_id.clone(),
            source_table: "unknown".into(),
            target_table: "users".into(),
            cursor: None,
            limit: Some(2),
        })
        .is_err());
        assert!(get_comparison_page(SyncComparisonPageRequest {
            plan_id: preview.plan_id,
            source_table: "users".into(),
            target_table: "users".into(),
            cursor: None,
            limit: Some(0),
        })
        .is_err());
    }

    #[test]
    fn comparison_page_rejects_claimed_plan_and_overlarge_limit() {
        let source = MockDriver::new("postgres", MockDriverOptions::default());
        let target = MockDriver::new("postgres", MockDriverOptions::default());
        let preview = issue_plan(
            "source-session".into(),
            "target-session".into(),
            "source-db".into(),
            "target-db".into(),
            None,
            None,
            source.as_ref(),
            target.as_ref(),
            "source-fingerprint".into(),
            "target-fingerprint".into(),
            ComparisonResult::new(vec![comparison().tables[0].clone()]),
            SyncOptions::default(),
            false,
        )
        .unwrap();
        assert!(get_comparison_page(SyncComparisonPageRequest {
            plan_id: preview.plan_id.clone(),
            source_table: "users".into(),
            target_table: "users".into(),
            cursor: None,
            limit: Some(SYNC_COMPARISON_PAGE_MAX_LIMIT + 1),
        })
        .is_err());
        let claimed = claim_plan(&preview.plan_id).unwrap();
        drop(claimed);
        assert!(get_comparison_page(SyncComparisonPageRequest {
            plan_id: preview.plan_id,
            source_table: "users".into(),
            target_table: "users".into(),
            cursor: None,
            limit: Some(1),
        })
        .is_err());
    }

    #[test]
    fn indexed_plan_summary_and_page_do_not_call_full_load() {
        let source = MockDriver::new("postgres", MockDriverOptions::default());
        let target = MockDriver::new("postgres", MockDriverOptions::default());
        let large_row = RowChange::insert(
            vec![Value::Integer(7)],
            vec![Some(Value::String("x".repeat(
                super::super::comparison_store::COMPARISON_MEMORY_LIMIT + 1,
            )))],
            &SyncOptions::default(),
        );
        let preview = issue_plan(
            "source-session".into(),
            "target-session".into(),
            "source-db".into(),
            "target-db".into(),
            None,
            None,
            source.as_ref(),
            target.as_ref(),
            "source-fingerprint".into(),
            "target-fingerprint".into(),
            ComparisonResult::new(vec![
                TableResult::matched("users", "users", vec![large_row]),
                TableResult::matched(
                    "orders",
                    "orders",
                    vec![RowChange::insert(
                        vec![Value::Integer(8)],
                        vec![Some(Value::Integer(8))],
                        &SyncOptions::default(),
                    )],
                ),
            ]),
            SyncOptions::default(),
            false,
        )
        .unwrap();
        let plan = peek_plan(&preview.plan_id).unwrap();
        assert!(plan.comparison.is_spilled());
        assert_eq!(preview.tables[0].row_count, 1);
        let before = plan.comparison.full_load_calls();
        let page = get_comparison_page(SyncComparisonPageRequest {
            plan_id: preview.plan_id,
            source_table: "users".into(),
            target_table: "users".into(),
            cursor: None,
            limit: Some(1),
        })
        .unwrap();
        assert_eq!(page.rows.len(), 1);
        assert_eq!(plan.comparison.full_load_calls(), before);
    }

    #[test]
    fn streaming_selection_reads_spilled_rows_in_pages_without_full_load() {
        let source = MockDriver::new("postgres", MockDriverOptions::default());
        let target = MockDriver::new("postgres", MockDriverOptions::default());
        let options = SyncOptions::default();
        let payload = "x".repeat(super::super::comparison_store::COMPARISON_MEMORY_LIMIT + 1);
        let mut rows = Vec::with_capacity(501);
        rows.push(RowChange::insert(
            vec![Value::Integer(0)],
            vec![Some(Value::String(payload))],
            &options,
        ));
        rows.extend((1..=500).map(|key| {
            RowChange::insert(
                vec![Value::Integer(key)],
                vec![Some(Value::Integer(key))],
                &options,
            )
        }));
        let comparison = ComparisonResult::new(vec![TableResult::matched("users", "users", rows)]);
        let comparison = ComparisonStore::from_comparison(comparison).unwrap();
        assert!(comparison.is_spilled());
        let store = SyncPlanStore::new();
        let preview = store
            .issue_with_store(
                "plan-stream".into(),
                1,
                "source-session".into(),
                "target-session".into(),
                "source-db".into(),
                "target-db".into(),
                None,
                None,
                source.as_ref(),
                target.as_ref(),
                "source-fingerprint".into(),
                "target-fingerprint".into(),
                comparison,
                options.clone(),
                false,
            )
            .unwrap();
        let plan = store.peek(&preview.plan_id).unwrap();
        let selection = SyncRunSelection {
            revision: 1,
            rows: Vec::new(),
            scopes: vec![table_scope(Vec::new())],
        };
        let matcher = validate_selection_streaming(&plan.comparison, &selection, &options).unwrap();
        assert_eq!(plan.comparison.full_load_calls(), 0);
        let table = plan
            .comparison
            .summaries()
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let rows = plan
            .comparison
            .load_table_page("users", "users", 500, 1)
            .unwrap();
        let page = selected_table_page(&table, rows, &matcher, &options)
            .unwrap()
            .unwrap();
        assert_eq!(page.rows.len(), 1);
        assert!(matches!(page.rows[0].key.as_slice(), [Value::Integer(500)]));
        assert_eq!(plan.comparison.full_load_calls(), 0);
    }

    #[test]
    fn expired_plan_drops_spilled_comparison_store() {
        let source = MockDriver::new("postgres", MockDriverOptions::default());
        let target = MockDriver::new("postgres", MockDriverOptions::default());
        let large = ComparisonResult::new(vec![TableResult::matched(
            "users",
            "users",
            vec![RowChange::insert(
                vec![Value::Integer(1)],
                vec![Some(Value::String("x".repeat(
                    super::super::comparison_store::COMPARISON_MEMORY_LIMIT + 1,
                )))],
                &SyncOptions::default(),
            )],
        )]);
        let store = SyncPlanStore::new();
        let preview = store
            .issue(
                "source-session".into(),
                "target-session".into(),
                "source-db".into(),
                "target-db".into(),
                None,
                None,
                source.as_ref(),
                target.as_ref(),
                "source-fingerprint".into(),
                "target-fingerprint".into(),
                large,
                SyncOptions::default(),
                false,
            )
            .unwrap();
        let path = {
            let plan = store.peek(&preview.plan_id).unwrap();
            plan.comparison.path().unwrap()
        };
        assert!(path.exists());
        store
            .plans
            .lock()
            .unwrap()
            .get_mut(&preview.plan_id)
            .unwrap()
            .expires_at = Instant::now() - Duration::from_secs(1);
        assert!(store.peek(&preview.plan_id).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn claiming_plan_removes_owner_and_cleans_file_after_claimed_handle_drops() {
        let source = MockDriver::new("postgres", MockDriverOptions::default());
        let target = MockDriver::new("postgres", MockDriverOptions::default());
        let large = ComparisonResult::new(vec![TableResult::matched(
            "users",
            "users",
            vec![RowChange::insert(
                vec![Value::Integer(1)],
                vec![Some(Value::String("x".repeat(
                    super::super::comparison_store::COMPARISON_MEMORY_LIMIT + 1,
                )))],
                &SyncOptions::default(),
            )],
        )]);
        let store = SyncPlanStore::new();
        let preview = store
            .issue(
                "source-session".into(),
                "target-session".into(),
                "source-db".into(),
                "target-db".into(),
                None,
                None,
                source.as_ref(),
                target.as_ref(),
                "source-fingerprint".into(),
                "target-fingerprint".into(),
                large,
                SyncOptions::default(),
                false,
            )
            .unwrap();
        let claimed = store.claim(&preview.plan_id).unwrap();
        let path = claimed.comparison.path().unwrap();
        assert!(store.peek(&preview.plan_id).is_err());
        assert!(path.exists());
        drop(claimed);
        assert!(!path.exists());
    }
}
