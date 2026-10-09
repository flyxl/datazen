//! The budget ledger: `scan_budgeted` walks `SCAN` under a `ScanBudget`, and `read_dbsize` feeds that budget without being a hard dependency.

use super::batch::scan_round;
use super::batch::ScannedPage;
use crate::connect::Topology;
#[allow(unused_imports)]
use crate::ops::tree::budget::{is_exact_key_pattern, tree_scan_budget, ScanBudget, ScanLoopGuard};
#[allow(unused_imports)]
use crate::ops::workbench::{
    cluster_scan_anchor_slot, parse_opt_int, parse_type_token, type_reply_says_absent,
    SlotRoutedConnection, TTL_MISSING,
};
use redis::aio::ConnectionLike;
use redis::Value as RValue;

/// Walk `SCAN` until the page is full, the cursor wraps, or the budget is spent.
///
/// * `count_hint` — `SCAN COUNT` the loop aims for per round. For a page it is
///   the caller's `count`, so `count` keeps meaning "keys per page" exactly as it
///   did when one `SCAN` served one page; for a full count the hint is small, so
///   the budget is spent over many rounds instead of one.
/// * `stop_at` — how many keys make the page full. `usize::MAX` means "scan until
///   the cursor wraps or the budget is gone" (the `count_matching` shape).
///
/// Reaching `stop_at` with the cursor still open is *pagination*, not
/// truncation: `truncated` stays `false` and `next_cursor` resumes. Only a cap
/// (budget spent, or the round/stall guard) reports `truncated`.
// Every parameter is an independent loop input (cursor, COUNT hint, stop
// condition, filters, ledger, topology); bundling them would hide which one a
// caller actually varies.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn scan_budgeted<C>(
    conn: &mut C,
    start_cursor: u64,
    count_hint: u32,
    stop_at: usize,
    pattern: Option<&str>,
    type_filter: Option<&str>,
    budget: &mut ScanBudget,
    topology: Topology,
) -> Result<ScannedPage, String>
where
    C: ConnectionLike + SlotRoutedConnection + Send,
{
    let mut keys: Vec<String> = Vec::new();
    let mut cursor = start_cursor;
    let mut guard = ScanLoopGuard::new();
    let mut exhausted = false;

    loop {
        let Some(count) = budget.next_count(count_hint) else {
            break;
        };
        budget.charge(count);
        let (next, batch) = scan_round(conn, cursor, count, pattern, type_filter, topology).await?;
        guard.record_round(!batch.is_empty());
        keys.extend(batch);
        cursor = next;
        if cursor == 0 {
            exhausted = true;
            break;
        }
        if keys.len() >= stop_at {
            break;
        }
        if guard.cap_hit() {
            tracing::warn!(
                rounds = guard.rounds(),
                cursor,
                stop_at,
                consumed = budget.consumed(),
                "redis key tree: SCAN stopped making progress, reporting what we have"
            );
            break;
        }
    }

    dedup_keys(&mut keys);
    let page_filled = keys.len() >= stop_at;
    Ok(ScannedPage {
        keys,
        next_cursor: cursor,
        consumed: budget.consumed(),
        truncated: !(exhausted || page_filled),
        exhausted,
    })
}

/// Drop duplicate keys `SCAN` may hand out twice across rounds, keeping the
/// first occurrence so the page order stays the server's.
fn dedup_keys(keys: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::with_capacity(keys.len());
    keys.retain(|key| seen.insert(key.clone()));
}

/// `DBSIZE` of the selected database. Read **once per command** by every op in
/// this module — it both scales the budget and fills the reply's `dbsize`.
///
/// Deliberately unrouted on every topology: redis' own cluster table fans
/// `DBSIZE` out to all masters and sums the answers, which is the whole-database
/// number the UI's "共 M 个 key" needs (see `ops_workbench`'s module docs).
///
/// **It cannot fail**, and that is a contract, not a convenience (redis-tree-backend-BUG-003):
/// `DBSIZE` is routinely refused — an ACL profile without the flag (`-NOPERM`), a
/// managed/proxy tier that hides the command, or one unreachable master under the
/// cluster fan-out above. This value is an *input to a budget* and a *display
/// number*; neither is worth a failed page, and the pre-budget code agreed
/// (`8981d3078`'s `redis_driver_on.rs:138` read it as `unwrap_or(0)`). An
/// unusable reply therefore yields `0`, which makes the budget fall back to
/// [`DEFAULT_TREE_BUDGET`] via [`tree_scan_budget`] — the "DBSIZE 不可得 ⇒ 默认档"
/// degradation the frozen budget contract promises. The signature has no `Result` so a future
/// caller cannot re-raise it into a hard dependency again.
pub(crate) async fn read_dbsize<C>(conn: &mut C) -> u64
where
    C: ConnectionLike + Send,
{
    let raw: Result<RValue, _> = redis::cmd("DBSIZE").query_async(conn).await;
    match raw {
        // A reply we could not read as a number is the same degraded case as an
        // error, so it warns too rather than quietly reporting an empty database.
        Ok(value) => match parse_opt_int(&value) {
            Some(n) if n >= 0 => n as u64,
            _ => {
                tracing::warn!(
                    "redis key tree: DBSIZE reply was unusable, budget falls back to the default tier"
                );
                0
            }
        },
        Err(err) => {
            tracing::warn!(
                error = %err,
                "redis key tree: DBSIZE refused, budget falls back to the default tier"
            );
            0
        }
    }
}

// ---------------------------------------------------------------------------
// Page batch 1: TYPE / TTL / (MEMORY USAGE)
// ---------------------------------------------------------------------------
