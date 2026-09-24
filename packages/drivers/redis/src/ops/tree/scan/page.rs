//! The page assembly: `ScanKeysPage` and `scan_keys_page`, which is what the key-tree command actually returns.

use super::batch::PageKey;
use super::budget::read_dbsize;
use super::budget::scan_budgeted;
use super::meta::fetch_key_meta;
use super::meta::KeyMeta;
use super::value::fetch_key_values;
use super::value::ValueFields;
use crate::connect::Topology;
use crate::driver::session::normalize_type_filter;
#[allow(unused_imports)]
use crate::ops::tree::budget::{is_exact_key_pattern, tree_scan_budget, ScanBudget, ScanLoopGuard};
#[allow(unused_imports)]
use crate::ops::workbench::{
    cluster_scan_anchor_slot, parse_opt_int, parse_type_token, type_reply_says_absent,
    SlotRoutedConnection, TTL_MISSING,
};
use datazen_driver_api::{DriverError, KeyEntry};
use redis::aio::ConnectionLike;
use std::time::Instant;

/// One flat `scan_keys` page.
#[derive(Debug, Clone)]
pub struct ScanKeysPage {
    /// Keys with type / TTL / size / preview resolved.
    pub entries: Vec<KeyEntry>,
    /// Resume cursor (`0` = end of this pass).
    pub next_cursor: u64,
    /// Cumulative `COUNT` spent.
    pub consumed: u64,
    /// Budget or round cap hit before the cursor wrapped.
    pub truncated: bool,
    /// `DBSIZE`, read exactly once for this call.
    pub dbsize: u64,
    /// The exact-key short circuit (PRD §4 I-3) served this page.
    pub exact: bool,
}

/// Assemble the reply rows once both batches are in.
///
/// `size` keeps its established meaning: `MEMORY USAGE` bytes when
/// `with_memory` was asked for and answered, otherwise the logical length.
fn build_entries(
    rows: Vec<(String, KeyMeta)>,
    values: &[ValueFields],
    with_memory: bool,
) -> Vec<KeyEntry> {
    rows.into_iter()
        .enumerate()
        .map(|(index, (key, meta))| {
            let logical_len = values
                .get(index)
                .map(|value| value.logical_len)
                .unwrap_or(0);
            KeyEntry {
                key,
                key_type: meta.key_type,
                ttl: meta.ttl,
                size: if with_memory {
                    meta.mem_bytes.unwrap_or(logical_len)
                } else {
                    logical_len
                },
                preview: values
                    .get(index)
                    .map(|value| value.preview.clone())
                    .unwrap_or_default(),
            }
        })
        .collect()
}

/// One page of the flat key browser.
///
/// `budget` is the optional cumulative COUNT cap for this action; `None` lets
/// [`tree_scan_budget`] derive it from the `DBSIZE` read here. An exact key name
/// (no glob character) never touches `SCAN` (PRD §4 I-3), and no value is read
/// for it beyond the same preview commands every other row uses.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn scan_keys_page<C>(
    conn: &mut C,
    pattern: &str,
    cursor: u64,
    count: u32,
    key_type: Option<&str>,
    with_memory: bool,
    no_ttl_only: bool,
    budget: Option<u64>,
    topology: Topology,
    t0: Instant,
) -> Result<ScanKeysPage, DriverError>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    let dbsize = read_dbsize(conn).await;
    let mut ledger = ScanBudget::new(tree_scan_budget(budget, dbsize));
    let exact = is_exact_key_pattern(pattern);

    let (keys, next_cursor, consumed, truncated) = if exact {
        (vec![pattern.to_string()], 0u64, 0u64, false)
    } else {
        let match_pat = (!pattern.is_empty() && pattern != "*").then_some(pattern);
        let page = scan_budgeted(
            conn,
            cursor,
            count.max(1),
            usize::try_from(count.max(1)).unwrap_or(usize::MAX),
            match_pat,
            normalize_type_filter(key_type),
            &mut ledger,
            topology,
        )
        .await
        .map_err(DriverError::QueryFailed)?;
        (page.keys, page.next_cursor, page.consumed, page.truncated)
    };

    let metas = fetch_key_meta(conn, &keys, with_memory, topology)
        .await
        .map_err(DriverError::QueryFailed)?;
    let mut rows: Vec<(String, KeyMeta)> = keys.into_iter().zip(metas).collect();
    // A key the server now says is gone is not a row; a *degraded* reply is
    // (see [`KeyMeta::absent`]), so a short batch cannot silently shrink a page.
    rows.retain(|(_, meta)| !meta.absent);
    // The exact-name short circuit never sent `SCAN … TYPE`, so a caller's type
    // filter the server would have applied has to be re-applied here. A `none`
    // still in the row set (positive `none` was just retained out) means the
    // TYPE reply was unreadable: keep the row rather than hide a real key
    // behind a broken answer.
    if exact {
        if let Some(wanted) = normalize_type_filter(key_type) {
            rows.retain(|(_, meta)| meta.key_type == wanted || meta.key_type == "none");
        }
    }
    if no_ttl_only {
        rows.retain(|(_, meta)| meta.ttl == -1);
    }

    let items: Vec<PageKey> = rows
        .iter()
        .map(|(key, meta)| PageKey {
            key: key.clone(),
            key_type: meta.key_type.clone(),
        })
        .collect();
    let values = fetch_key_values(conn, &items, topology)
        .await
        .map_err(DriverError::QueryFailed)?;
    let entries = build_entries(rows, &values, with_memory);

    tracing::info!(
        elapsed_ms = t0.elapsed().as_millis() as u64,
        keys = entries.len(),
        with_memory,
        exact,
        consumed,
        truncated,
        dbsize,
        // The ledger's own view of the action: which cap applied (default tier
        // vs requested), and whether it is fully spent after this page.
        budget_limit = ledger.limit(),
        budget_spent = ledger.truncated(),
        "redis scan_keys page done"
    );
    Ok(ScanKeysPage {
        entries,
        next_cursor,
        consumed,
        truncated,
        dbsize,
        exact,
    })
}
