//! SQL Server's capability evidence table — the machine-readable twin of
//! [`super::capabilities`].
//!
//! Split out of `resource_provider.rs` so both stay readable: that file carries the
//! adapter wiring and the capability assertions, this one carries only the reasons.
//!
//! Every entry is `(evidence key, explanation)`. The key is one of the twelve
//! camelCase capability-cell names. The explanation is mandatory: a cell with a
//! value but no record is exactly the `evidence_gaps` hole this migration closes.

/// The evidence recorded next to the twelve declarations in
/// [`super::capabilities`].
///
/// [`super::adapter`] merges this through
/// [`LegacyResourceAdapter::with_evidence`], so the claim in [`super::capabilities`]
/// and the reason for it travel together: a reader of the snapshot gets the
/// value *and* the file and line that justify it, instead of a bare enum.
///
/// Two rules govern every entry here.
///
/// * A cell the driver genuinely supports carries the code that runs and is
///   named with `file.rs:NNN`. A cell the driver leaves blank opens with
///   `declined:` and still names what was measured and why that measurement
///   rules the capability out — the point of `declined:` is that a *negative*
///   result is a result, not a silence.
/// * Nothing is cited that was not read while writing this. Every line number
///   below was re-checked against the current tree.
///
/// The keys are the twelve cells
/// ([`capabilities::evidence_gaps`] holds the only other copy of the list);
/// `every_capability_record_cites_the_source_line_it_claims_to_describe`, in
/// [`super::resource_provider`]'s tests, fails if one is dropped or renamed.
pub(crate) fn capability_evidence() -> Vec<(&'static str, String)> {
    vec![
        (
            "statefulSession",
            "Supported, and measured rather than assumed. Two round-trips read state \
             that belongs to one physical TDS connection and nothing else: \
             sqlserver.rs:461-488 `current_isolation_level` issues `DBCC USEROPTIONS \
             WITH NO_INFOMSGS` (sqlserver.rs:462) and parses the session's own current \
             isolation level out of the reply (sqlserver.rs:469-476), and \
             sqlserver.rs:490-509 `ensure_no_open_transaction` issues `SELECT \
             @@TRANCOUNT AS [transaction_count]` (sqlserver.rs:491) and turns a nonzero \
             count into an error (sqlserver.rs:503-507) — called on the begin path at \
             sqlserver.rs:1625. Neither value is cached, replayed or derived: both are \
             asked of the server on every call, which is why the session is measured. \
             The transaction handle registered under the same connection id \
             (sqlserver.rs:1631-1637) survives between statements and is dropped only \
             when the transaction ends (sqlserver.rs:1717, sqlserver.rs:1752). \
             Independent of resource_adapter.rs:271, which reports \
             `SessionContinuity::Unknown` for the descriptor because that is a separate \
             question about *reconnection*."
                .to_string(),
        ),
        (
            "namespaceSwitch",
            "declined: the shape declares Database and Schema because T-SQL genuinely \
             addresses both (sql_target.rs:24-40 `qualify_sql` builds `[db].[schema].t` \
             from both parts), but attaching a *different* database is not a rewrite — \
             sql_target.rs:12 states that no session `USE` is ever issued, and the \
             sqlserver.rs:2095 test asserts no generated SQL contains `USE [`. The \
             qualified text also stays unresolved when only a database is supplied, \
             because `[db].t` would silently mean schema `db` (sql_target.rs:158-165). \
             resource_adapter.rs:407-414 turns that refusal into \
             `ContextChangeDisposition::Unsupported`, and the schema dimension is \
             reached the same way the database one is — by rewriting the statement \
             text, `build_tables_sql` filtering on `WHERE s.name = ...` \
             (sqlserver.rs:196-209) — never by repointing the session."
                .to_string(),
        ),
        (
            "contextObservation",
            "declined: nothing in this crate exposes the login context back out. \
             resource_adapter.rs:397-402 returns `SessionObservation::unobservable()` \
             for every field and explicitly refuses to back-fill the acquisition target, \
             and the execution path fills `context_before` / `context_after` with \
             `SessionContext::unobserved()` (resource_adapter.rs:377-378). What this \
             driver *can* do is read two session facts (sqlserver.rs:461-488 and \
             sqlserver.rs:490-509), which is a stateful session, not an observed context: \
             the isolation level and open-transaction count say nothing about who the \
             connection is authenticated as, so this cell stays blank."
                .to_string(),
        ),
        (
            "transactionObservation",
            "Partial — exactly the beginnings are observable, and the refusal on the \
             endings is the adapter's, not the driver's. Beginnings: \
             resource_adapter.rs:416-425 forwards to `DatabaseDriver::begin_transaction`, \
             and the override is real — sqlserver.rs:1616-1620 refuses a second \
             transaction on the same connection, sqlserver.rs:1625 checks the server's \
             own count first, sqlserver.rs:1626 issues `BEGIN TRANSACTION` and discards \
             the session if that fails, and sqlserver.rs:1630 mints a genuine \
             `sqlserver_tx_<uuid>` handle; the adapter then reports \
             `TransactionObservation::begun(id, 0)`. Endings: the adapter never asks the \
             driver, refusing outright at resource_adapter.rs:429-438 \
             (`commit_transaction`) and resource_adapter.rs:440-449 \
             (`rollback_transaction`), because the legacy path resolves transactions \
             inside its own commands and no outcome can be read back. The driver *can* \
             end one — sqlserver.rs:1691-1723 and sqlserver.rs:1726-1756 both re-validate \
             the handle against the connection's active id before running `COMMIT` / \
             `ROLLBACK` — but the port does not expose that, so this contract can prove \
             the start and not the finish. `Full` would be a claim about endings it \
             cannot make."
                .to_string(),
        ),
        (
            "sessionScopedHandles",
            "Supported, and the binding is explicit. The handle is inserted into the \
             driver's `transactions` map keyed by the connection id \
             (sqlserver.rs:1631-1637, with the id minted at sqlserver.rs:1630), so two \
             connections can never share one transaction. It is re-validated against \
             that key before either outcome runs — sqlserver.rs:1693-1705 rejects a \
             mismatched id and sqlserver.rs:1728-1740 does the same on the rollback path \
             — and it is removed at sqlserver.rs:1717 and sqlserver.rs:1752, i.e. it \
             cannot outlive the transaction. Honest limit: the handle belongs to the \
             driver's own session-scoped registry, not to the adapter's per-execution \
             list, which stays empty because the execution path registers nothing \
             (resource_adapter.rs:387). The claim is about the driver keeping one \
             handle bound to one session, which is what the code above does."
                .to_string(),
        ),
        (
            "resetForReuse",
            "declined: no verified baseline replay exists. resource_adapter.rs:499-506 \
             always answers `ResetDisposition::Discard`, because handing out a resource \
             as `Clean` would assert a return to baseline that nobody checked — and \
             `Verified` is what opens the reuse feature (capabilities.rs:534-539). \
             Nothing in this crate could support it even in principle: the only session \
             state it knows how to read is the isolation level \
             (sqlserver.rs:461-488) and the open-transaction count \
             (sqlserver.rs:490-509), which is not the session's full configuration."
                .to_string(),
        ),
        (
            "preciseCancel",
            "The value stays `Unknown`, and the cell is blank because two different \
             owners control it. resource_adapter.rs:148-152 overwrites whatever the \
             driver declares, deriving it from `supports_query_execution_cancel()`; the \
             default for that predicate is `false` (traits.rs:817-819) and this crate \
             does not override it, so the adapter alone settles the cell and a driver \
             value could not change it. The driver's own account agrees and is stronger \
             than `false`: sqlserver.rs:1765-1767 refuses `cancel_query` outright, \
             because SQL Server's `ATTENTION` packet is per-session and asynchronous \
             (sqlserver.rs:1759-1761) and this driver never opens the second session a \
             cancel would need. So the cell is genuinely unknowable here rather than \
             merely unoverridden — but the value is the adapter's to set, and it is \
             left exactly as it stands."
                .to_string(),
        ),
        (
            "snapshots",
            "declined: the mechanism exists and is not the guarantee. \
             sqlserver.rs:1644 overrides `begin_read_snapshot`, checks the session first \
             (sqlserver.rs:1658), reads the level to restore afterwards \
             (sqlserver.rs:1659), and issues `SET TRANSACTION ISOLATION LEVEL SNAPSHOT; \
             BEGIN TRANSACTION` (sqlserver.rs:1663-1666). It then discards the whole TDS \
             connection when that batch fails (sqlserver.rs:1669-1675), which is itself \
             the reason for caution: SNAPSHOT is only available where \
             ALLOW_SNAPSHOT_ISOLATION is enabled, so it is not universally available and \
             not this driver's to assume. The cell asks for a *scope* — per-table, \
             per-database or coordinated — and a single database-wide isolation level \
             answers none of them. Blank rather than `Supported`."
                .to_string(),
        ),
        (
            "transactions",
            "isolation_levels `[]`, savepoints `Unsupported`, max_open_transactions \
             `Some(1)`. The empty list is a decision, not an omission: \
             sqlserver.rs:1626 sends a bare `BEGIN TRANSACTION` with no level argument, \
             because the transaction starts at whatever the session already had and \
             sqlserver.rs:1635 records `restore_isolation: None` for exactly that \
             reason. The crate *does* issue `SET TRANSACTION ISOLATION LEVEL`, but never \
             as a caller choice on this path — sqlserver.rs:1665 raises SNAPSHOT for the \
             separate `begin_read_snapshot`, and sqlserver.rs:1708 / sqlserver.rs:1743 \
             restore a level read back at sqlserver.rs:1659. Putting a level back is not \
             accepting one, so no level may be advertised as honoured. Savepoints are \
             `Unsupported` for the same reason the transfer DDL is irrelevant here: \
             sqlserver.rs:2280 only proves the splitter recognises `SAVE TRAN \
             savepoint_one`, and this crate exposes no savepoint operation. \
             `Some(1)` is a real measurement, not a convention: sqlserver.rs:1616-1620 \
             refuses a second transaction on one connection, the handle is removed at \
             sqlserver.rs:1717 / sqlserver.rs:1752 so the slot frees on commit and \
             rollback alike, and sqlserver.rs:490-509 additionally refuses to start if \
             the server itself already has one open (checked at sqlserver.rs:1625)."
                .to_string(),
        ),
        (
            "ddlAtomicity",
            "declined: the declaration is empty and every lookup therefore fails closed. \
             This crate has no `ddl_atomicity` override, so the default \
             `DdlAtomicity::Unknown` stands (traits.rs:156-158), and \
             capabilities.rs:240-245 maps every operation it does not have an entry for \
             to `Unknown` — never to `Transactional`. Nothing here was left unmeasured: \
             SQL Server can wrap multi-statement DDL in one transaction, and this crate \
             emits exactly such wrappers for its own transfer path (sqlserver.rs:1501-1503 \
             `BEGIN TRANSACTION;` / `COMMIT TRANSACTION;`). That is a private detail of \
             one driver command, not a per-operation answer for arbitrary DDL, so \
             filling the map would assert an atomicity this crate has not verified for a \
             caller's own DDL. An empty map plus a failing-closed lookup is the honest \
             reading of \"ask, do not assume\"."
                .to_string(),
        ),
        (
            "data",
            "StreamingReadWrite. Read: sqlserver.rs:604-646 `stream_one` holds the live \
             `tiberius` stream and pulls it with `stream.try_next()` \
             (sqlserver.rs:622-626), pushing each decoded row at sqlserver.rs:639; the \
             rows leave the driver in batches through \
             `QueryRowBatcher::push` (query_stream.rs:200-213), which flushes to the \
             caller's callback as soon as a batch fills (query_stream.rs:209-211) instead \
             of materializing the result. Nothing accumulates a `Vec<Vec<Option<Value>>>` \
             first, so this is genuinely streaming rather than a buffered read labelled \
             as one. Write: sqlserver.rs:1604-1608 runs the statement and maps \
             `r.total()`, the server's own affected-row count, so a write reports what \
             the server did rather than a guess from parsing the statement."
                .to_string(),
        ),
        (
            "backup",
            "declined: this crate has no artifact code and no backup command, which is a \
             measurement rather than an assumption. The whole admin surface is four \
             definitions (sqlserver.rs:1803-1806 delegating to admin_commands.rs:11-63: \
             create_database, create_schema, create_user, drop_database), and none of \
             them is a backup or restore. The only mention of the path anywhere in the \
             driver is a comment about *feeding schema metadata back into* it \
             (sqlserver.rs:191 and sqlserver.rs:1184) — that is `TableInfo::schema` \
             round-tripping, not a dump. With no call and no result to read back, the \
             contract can neither prove nor deny the capability, so it stays `Unknown` \
             instead of guessing from the driver's name."
                .to_string(),
        ),
    ]
}
