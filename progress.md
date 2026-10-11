# P346 driver track — READY_FOR_TEST

## Implemented code facts

- PostgreSQL and MySQL InteractiveQuery/AgentSession resources retain a real physical PoolConnection until close. The descriptor claims Fixed and reserves exactly two connections (one data, one independent control). MetadataInspection remains leased with its configured pool budget.
- Connection maps hold Arc<Mutex<PoolConnection>>; registry locks are released before SQL awaits. Independent resources run independently. Cancel uses the immutable backend PID/MySQL thread id and an independent control connection.
- Acquire consumes target.database/schema. Explicit PostgreSQL schema table listing no longer includes public implicitly. Pool removal uses short write guards before asynchronous close.
- PostgreSQL drains protocol, refreshes control statistics and observes the exact backend's transaction state. Aborted transactions retain prior namespace with Partial confidence and do not query the aborted target for facts. COMMIT of an observed aborted transaction reports RolledBack.
- MySQL/MariaDB state uses server evidence when available. MySQL requires enabled thread/transaction instrumentation and consumers; otherwise arbitrary SQL gives Unknown. Successful builtin BEGIN/COMMIT/ROLLBACK supplies exact state. Active/Aborted and conservative MySQL Unknown observations register cleanup transaction handles; Idle clears them. Handles are resource/epoch scoped and rollback the original checkout.
- Fixed get_table_schema introspects on the original physical session, including temporary tables. relationSql is parsed by the driver's dialect; PostgreSQL folds only unquoted identifiers and MySQL resolves two parts as database.table. Both return data:{schema,resolvedTarget}; resolvedTarget includes actual namespace, table and sessionScoped. Quoted identifier parts containing dots remain exact in this path.
- Query/query_stream uses SQLx row streams with awaitable sink backpressure, immediate first-row delivery, at most 256 rows/1 MiB encoded row payload per batch. Statement completions contain columns and affected counts but no duplicate row buffer. Typed boundParameters uses prepared fetch_many on the same pin and requires one statement; nonparameter SQL uses raw SQL protocol.
- Sink rejection requests exact cancellation, stops further publication, and drains for up to five seconds. Control cancellation itself has a two-second bound; final protocol ping has a five-second bound. Unconfirmed drain removes the pinned checkout and data pool, closes the checkout on drop, and leaves immutable fixed identity as a tombstone so the old resource fails SessionLost rather than reconnecting.
- Catalog command output uses ResultSink.write_output({kind:'commandResult',data:...}). Execute retains existing typed boundParameters behavior.
- Existing capability/provider test expectations and mocks were adjusted; no tests were added or executed.

## Checks actually performed

Command: CARGO_TARGET_DIR=/tmp/datazen-p346-drivers-target cargo check -p datazen-driver-postgres -p datazen-driver-mysql --tests

Final log: /tmp/datazen-p346-drivers-check-commit.log

CHECK_EXIT=0

Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.13s

One existing integration-test compile warning: postgres/tests/data_transfer_identity_sequences.rs:136 unused variable database. No production warning.

Before and after HEAD: e4a9eb008f6a527a5033cedfc867baa363ea5571

Before and after scoped tracked+untracked source SHA: b3f32270cfc57226756e825a9a7b1c85f01ef8e7e6f31566f1fdc499136d1926

`git diff --check` exit 0. Dedicated target uses 672 MiB; /tmp volume has 194 GiB free at delivery.

## Remaining validation / declared limitations

- WDIO and live PostgreSQL/MySQL acceptance are pending independent root验收. Compile success does not prove SQLx protocol behavior, cancellation timing, transaction cleanup, permissions or P4 writeback journeys on real servers.
- Fixed arbitrary admin/catalog commands without a same-checkout implementation are explicitly Unsupported; get_table_schema is supported. Host typed schema producers remain Host-owned and must use their resource path.
- MySQL without instrumentation cannot confirm arbitrary SQL transaction state; Unknown is intentional, builtin rollback resolves it. Such contexts cannot qualify for Confirmed writable result mapping.
- A single decoded row larger than 1 MiB is rejected. SQLx may allocate that one row before the driver rejects it; the implementation bounds published batches and avoids whole-result materialization, but does not claim a wire-level single-field allocation bound.
- Cancellation/drain failure returns SessionLost and tombstones the data handle; control pool and budget are released by resource close. Server or socket shutdown timing still needs WDIO/live verification.
- API prerequisite commits: fec6c7fe2b3b82cfe8f63b56e3de9c37b201c597, 87ee7bdcca7b00115b8a399acfdf4efd2c6e1e90, e4a9eb008f6a527a5033cedfc867baa363ea5571.
- This branch-only progress ledger must be removed during accepted integration; it is not a permanent deliverable.


# P3 runtime implementation ledger

## Implemented production paths
- FileArtifactStore injected at desktop assembly (`Store.data_dir/result-artifacts`); publisher writes immutable disk blocks before bounded events and readers use published disk prefix. Writer capability/ACL/TTL, 4096 artifacts, 32 MiB blocks, 256 MiB aggregate bytes, writer-lost restart truncation and corrupt recovery bounds are active.
- Owner-only directories/files on Unix; no-follow opens, bounded metadata/block recovery reads. Export capabilities bind organization/principal, expire after 300 seconds and reject arbitrary server paths; partial JSONL export requires explicit confirmation and durable truncation marker.
- Logical/editor quotas and descriptor physical cost use the shared coordinator; strict refused/unknown costs reject. ProductionLeaseManager participates in opening/execution/return/close. Only declared reusable metadata resources park after driver Clean and host checks; actual pool checkout, key invalidation and 60-second idle retirement run. Unconfirmed closure quarantines and retains physical budget.
- Transaction observations set context_after and handle lifecycle. Repeated exact live transaction handles are idempotent; only exact explicit closure retires a handle. Missing handles never imply closure.
- executeAtTarget admission is an owned task with per-key coordination, receipt recovery and caller-drop cleanup through original actor drain/close; unknown outcomes retain their outcome and uncertain resources retain budget. Completion restores gateway unknown resolution and session/context events.
- Token/receipt/coordinator/gateway directories reclaim by issued-token lifetime, protect active admissions/executions and cap at 4096. Closed session and budget retirement tombstones are bounded; missing observations never erase live unknown-outcome fences. Existing SQL guards, binding, history and cancel token validation retained.
- Host typed metadata executes on original runtime resource via desktop helper, publishes typed commandResult. Safe executionError artifact is best-effort; independent generic errorCode remains readable if disk publication fails.
- Single wildcard relation Verified source requires execution-before same-pin driver schema/resolvedTarget, confirmed exact context equality and completion columns/PK proof. Other statement source context stays Unknown/ReadOnly with actual statement index.

## Dependencies (separate commits)
- driver-api fec6c7fe -> local 93afb6243.
- desktop Host helpers cd7fecd4e -> local 9064db194.
- driver-api streaming 87ee7bdcca -> local da79fce12.
- driver-api prepared streaming e4a9eb008 -> local 2252555de.

## Validation
Only compiler checks performed; no Cargo/Vitest tests and no WDIO run in this track.
Initial formal gate HEAD da79fce12c0950a6234a73bf4646cb56b08114b4, source SHA e0c79b0172eeb594a94d52c73bf4a721ecbad34bfdb8f0812cba8e2bda8ac085.
`CARGO_TARGET_DIR=/tmp/datazen-p346-runtime-target cargo check -p datazen-runtime`: exit 0, `Finished dev profile ... in 4.08s`.
`CARGO_TARGET_DIR=/tmp/datazen-p346-runtime-target cargo check -p datazen --lib --no-default-features`: exit 0, 49 warnings (desktop helper/baseline/disabled feature warnings), `Finished dev profile ... in 21.68s`.
Before/after HEAD and source SHA (including untracked Rust source) matched. Latest interface dependency requires following gate recorded below.
Logs remain /tmp/datazen-p346-final-runtime.log and /tmp/datazen-p346-final-host.log.

## Final resumed gate
- HEAD before/after: `2252555dec25c42dafccf94d655f89dfc529ee17`.
- Source SHA before/after, including untracked source: `c8b8664e2682f8709401e3efbb56cda6ac08e226459523d8165f09d2375cc732` (MATCH).
- `cargo check -p datazen-runtime`: EXIT=0, `Finished dev profile [unoptimized + debuginfo] target(s) in 1.27s`.
- `cargo check -p datazen --lib --no-default-features`: EXIT=0, `Finished dev profile [unoptimized + debuginfo] target(s) in 5.40s`, Host 50 warnings (includes inactive export adapter getter and baseline/helper/disabled-feature warnings).
- Both checks used independent `/tmp/datazen-p346-runtime-target`; logs `/tmp/datazen-p346-runtime-gate.log`, `/tmp/datazen-p346-host-gate.log`.
- One resumed intermediate gate failed on a duplicate artifact getter inserted on PlatformEntry; fixed, and the final matching gate above passed. No tests ran.
- Additional actual-budget fixes: multi-connection descriptor cost atomically reserves all unit permits; queued acquisitions consume only their own scheduled grant; caller drop returns an already granted unobserved permit; scheduling grants notify other waiting futures. Physical snapshots/caps include real primary permits. Unpublished disk remnants remain charged to the byte budget until deletion/recovery.
- Other open/context operations now use per-key coordination and short receipt-map locks; context replacements retire old application rows only after actual replacement success, using physical outcome evidence.

## Explicit pending integration/verification
- Real PostgreSQL/MySQL pinned and incremental streaming implementations belong to driver track, not this commit; integrate them before claiming P3 streaming end-to-end complete.
- Fresh independent WDIO validation is pending, including cancellation/admission races, bounded prefix publication, typed catalog, error recovery and original-resource result edits.
- Quarantined physical budget is deliberately not recovered without confirmed close; automatic repair of quarantined resources is not implemented.
- Verified writable mapping is deliberately limited to a single wildcard relation. Multi-statement context source remains unobserved rather than guessed.
- Desktop Save dialog/IPC export wiring is not added in this scope; PlatformAdapter retains the actual result store and exposes a Host getter. Its ArtifactStore export enforces owner/TTL capabilities, but UI export journey is pending.
- Row prefix blocks presently obtain columns at final statementMetadata; desktop has statementColumns consumption and driver was asked to publish it using write_output. This extra live-columns behavior must be checked on the integrated driver revision.
- Ledger is temporary branch coordination data and must be removed during final integration acceptance.


# P4 desktop track

Worktree: `.worktrees/datazen-p346-desktop`; baseline `246835a9c`.

## Implemented

- Query panes lazily own independent runtime sessions. Duplicate panes never copy handles. Community/Pro editor callbacks route through the same owned wrapper; missing owner cannot fall back to legacy execution. Manual BEGIN, transactions, cancellation and observed context use the pane controller.
- Dropdown changes publish UI context only after runtime success. SQL qualification and path completion affect browsing only. Explicit recovery opens a new session at the last confirmed target. Blocked close keeps the tab and transaction controls reachable.
- Table reads execute explicit targets through runtime; short ChangeSets own a transaction/session. Manual table operations use a table-owned handle, preserving transaction ownership on partial failure. Metadata for that manual handle stays on the same resource. Legacy shared row commit commands reject writes.
- Typed catalog separates schemas from relations, retaining empty schema navigation. Batch columns remain bounded at 256. Schema/DDL cache keys include authenticated identity, persistent profile revision and target; invalidation understands the scoped tuple. Object catalog/DDL use independent target runtime leases.
- Final statement metadata supplies verified source and original runtime binding. Result cell writes read immutable server artifacts/PK and validate same-resource schema, context and live binding. Values are bound, qualification is driver-owned. PK edits require table ChangeSets; zero changed rows get same-resource readback classification. Successful result edits invalidate the local writable binding and require explicit query rerun for another edit.
- Existing WDIO query, cancellation/isolation edge and error recovery journeys now assert real result header/cell DOM. Existing Rust tests moved verbatim into commands/data/tests.rs only.

## Checks

- Direct Node tsc --noEmit, `/tmp/p346-desktop-tsc7.log`: EXIT=0.
- `CARGO_TARGET_DIR=/tmp/datazen-p346-desktop-target cargo check -p datazen --lib`, `/tmp/p346-desktop-cargo-check2.log`: EXIT=0; Finished dev profile in 31.32s.
- Both gates HEAD `cd7fecd4ee9550bcca45f717aa594bc0fe6bf863`, source SHA `ac04eadbe5d626137b5dc00e8d09fb7fc5edac0a2d710db147e7442afe577705` identical before/after, includes tracked and untracked non-env source. Subsequent frontend BEGIN/source fixes passed final gates below.
- Final Node tsc `/tmp/p346-desktop-tsc-final.log`: EXIT=0; final Host check `/tmp/p346-desktop-cargo-final.log`: EXIT=0, Finished dev profile in 0.64s. Both HEAD cd7fecd4e and SHA 2644f0d9b6ff315057c6ba3b24e2746a7a7d336d3117e9b175affc6604883f6d equal before/after. Result concurrency guard tsc delivery gate EXIT=0, HEAD cd7fecd4e and SHA bf578a73d3a542f785236354eae6af75d36c2365d0ef4182880f163236b61879 equal before/after. Final statementColumns consumption and short-session target confirmation passed Node tsc /tmp/p346-desktop-tsc-ready.log EXIT=0 and Host check /tmp/p346-desktop-cargo-ready.log EXIT=0 (Finished dev profile in 17.82s). HEAD cd7fecd4e and SHA 4a7a607262b8bc9515e9dc7a20fa456b3bb5bfd4153e5874d1758ab58da841ec equal before/after. Only existing WDIO edge DOM assertions and this ledger changed subsequently.
- No unit tests added/run. No env content read. WDIO not run in this track; parent will integrate runtime/driver final commits into independent acceptance candidate and use the required webdriver build.

## Dependencies and honest limits

- Verified source producer depends on runtime pre-statement same-pin proof and driver resolvedTarget/bound-stream implementation; not live validated here.
- Query result editing is conservative: exact wildcard single relation proof, PK included, no PK edits, one successful edit then explicit rerun. No automatic replay of arbitrary SQL.
- Original session loss/replacement keeps artifacts readable but writable authority is rejected. UI may retain a formerly live binding until interaction; Host rechecks it before writing.
- P6 subsequent consumers still use legacy SQL: DatabaseObjectView, ImportDialog, IndexesView, DocumentConnectionView, ObjectBrowser, PrivilegeView, TableStructureEditor, navigator/useNavigatorContextMenus, useConnectionContextMenu; exportStream uses the legacy streaming facade. Coordinator classified administration/import/export helpers as P6, while P4 schema/DDL caches and directory reads are migrated. This is not a claim that all legacy SQL consumers are removed.
- Live PG/MySQL validation of table paging/manual transaction, result edit, cancellation/transport recovery, error recovery, Community and Pro remains outstanding. Compile success is not P4 acceptance.

Delete this ledger during accepted integration/merge.

## Delivery checkpoint

Final direct Node tsc `/tmp/p346-desktop-tsc-commit.log`: `EXIT=0`. Before/after `HEAD=cd7fecd4ee9550bcca45f717aa594bc0fe6bf863` and `SOURCE_SHA=3bdb1178defef3150de7ee26b8bfbfaa23ac16c61435424595694868fbf37e2a` match. This checkpoint text is the only change after that gate. Final Rust source was checked by `/tmp/p346-desktop-cargo-ready.log`: `EXIT=0`, `Finished dev profile [unoptimized + debuginfo] target(s) in 17.82s`; both fingerprints matched.

READY_FOR_TEST means reviewable and compilation-ready, not live acceptance. Parent must build with `pnpm tauri:build:webdriver` after runtime/driver integration and run the strengthened existing WDIO tests.


# Workflow SQL diagnostic follow-up

Status: implemented, compilation checked; not independently accepted.

- Failed SQL execution reports execution identity, terminal state, effect outcome and structural error code.
- Result-read failures retain the known effect outcome and execution identity. Reading a result never submits SQL again.
- Error text uses structural codes; driver payloads and credentials are not copied into these diagnostics.

Check: `cargo check -p datazen --lib`, EXIT=0, log `/tmp/datazen-p346-workflow-check.log`.
Start/end HEAD: `f5e4b9ff78cedd0f0d53c44327d0c4ed211a0ccf`.
Start/end tracked source diff SHA: `f569fbabd2f28717a2fa4f7c71ba728c3518698bd093f787e20e7ca234141368`.
No tests were added or run. Independent WDIO acceptance is pending.

Delete this ledger before delivery.


## Cancellation and streaming metadata follow-up — READY_FOR_TEST

- PostgreSQL and MySQL cancellation now gate the addressed execution rather than holding the global execution registry across control connection acquisition/network SQL. Finish waits for the same execution gate before cleanup; entry identity is checked by Arc identity to avoid stale cancellation acting on a replacement registration. Existing immutable physical target identity remains intact.
- Both drivers publish the existing neutral statementColumns output before the first row batch. Statement completion publishes statementMetadata including columns/truncated, covering empty query results too. Rejected early metadata publication follows the existing bounded cancel/drain path.
- Existing registry test fixtures gained the required execution gate field; no tests were added or executed.
- Compile-only command: CARGO_TARGET_DIR=/tmp/datazen-p346-cancel-fix-target cargo check -p datazen-driver-postgres -p datazen-driver-mysql --tests
- CHECK_EXIT=0; log /tmp/datazen-p346-cancel-fix-check.log. Only existing PostgreSQL integration fixture unused variable warning.
- Before/after HEAD 0c107d0a9f258159b495856cc089f48d982ad777; before/after tracked + untracked source SHA c165b0f21c12b3e3298dbb5d902afb96804259adfb3cf6b22ccf39d04049bd5f; both unchanged throughout compilation. git diff --check exit 0.
- Independent WDIO acceptance is still required; this compile does not establish live cancel/finish races or UI streaming correctness.


## Host artifact readback repair (2026-10-12)

- Stage-split PG/MySQL WDIO probes confirmed that target submissions return `queued`, then terminate `succeeded` with `effectOutcome=completed`; each produces one artifact. Reading it fails before a database list reaches the UI.
- `ExecutionProjection.recover()` calls `BackendClient.readArtifact`. Desktop's generic camelCase mapping generated `read_artifact`, but the only registered Host IPC is `read_artifact_chunk`. The explicit override is commit `fd812d7fd`, integrated as candidate commit `300e6cf25`.
- Validation status: source trace and safe stage-split WDIO probe confirm the failing stage; the mapping still needs a matching `pnpm tauri:build:webdriver` and PostgreSQL/MySQL UI WDIO rerun. No tests other than authorized WDIO are in scope.


# P3 runtime implementation ledger

## Implemented production paths
- FileArtifactStore injected at desktop assembly (`Store.data_dir/result-artifacts`); publisher writes immutable disk blocks before bounded events and readers use published disk prefix. Writer capability/ACL/TTL, 4096 artifacts, 32 MiB blocks, 256 MiB aggregate bytes, writer-lost restart truncation and corrupt recovery bounds are active.
- Owner-only directories/files on Unix; no-follow opens, bounded metadata/block recovery reads. Export capabilities bind organization/principal, expire after 300 seconds and reject arbitrary server paths; partial JSONL export requires explicit confirmation and durable truncation marker.
- Logical/editor quotas and descriptor physical cost use the shared coordinator; strict refused/unknown costs reject. ProductionLeaseManager participates in opening/execution/return/close. Only declared reusable metadata resources park after driver Clean and host checks; actual pool checkout, key invalidation and 60-second idle retirement run. Unconfirmed closure quarantines and retains physical budget.
- Transaction observations set context_after and handle lifecycle. Repeated exact live transaction handles are idempotent; only exact explicit closure retires a handle. Missing handles never imply closure.
- executeAtTarget admission is an owned task with per-key coordination, receipt recovery and caller-drop cleanup through original actor drain/close; unknown outcomes retain their outcome and uncertain resources retain budget. Completion restores gateway unknown resolution and session/context events.
- Token/receipt/coordinator/gateway directories reclaim by issued-token lifetime, protect active admissions/executions and cap at 4096. Closed session and budget retirement tombstones are bounded; missing observations never erase live unknown-outcome fences. Existing SQL guards, binding, history and cancel token validation retained.
- Host typed metadata executes on original runtime resource via desktop helper, publishes typed commandResult. Safe executionError artifact is best-effort; independent generic errorCode remains readable if disk publication fails.
- Single wildcard relation Verified source requires execution-before same-pin driver schema/resolvedTarget, confirmed exact context equality and completion columns/PK proof. Other statement source context stays Unknown/ReadOnly with actual statement index.

## Dependencies (separate commits)
- driver-api fec6c7fe -> local 93afb6243.
- desktop Host helpers cd7fecd4e -> local 9064db194.
- driver-api streaming 87ee7bdcca -> local da79fce12.
- driver-api prepared streaming e4a9eb008 -> local 2252555de.

## Validation
Only compiler checks performed; no Cargo/Vitest tests and no WDIO run in this track.
Initial formal gate HEAD da79fce12c0950a6234a73bf4646cb56b08114b4, source SHA e0c79b0172eeb594a94d52c73bf4a721ecbad34bfdb8f0812cba8e2bda8ac085.
`CARGO_TARGET_DIR=/tmp/datazen-p346-runtime-target cargo check -p datazen-runtime`: exit 0, `Finished dev profile ... in 4.08s`.
`CARGO_TARGET_DIR=/tmp/datazen-p346-runtime-target cargo check -p datazen --lib --no-default-features`: exit 0, 49 warnings (desktop helper/baseline/disabled feature warnings), `Finished dev profile ... in 21.68s`.
Before/after HEAD and source SHA (including untracked Rust source) matched. Latest interface dependency requires following gate recorded below.
Logs remain /tmp/datazen-p346-final-runtime.log and /tmp/datazen-p346-final-host.log.

## Final resumed gate
- HEAD before/after: `2252555dec25c42dafccf94d655f89dfc529ee17`.
- Source SHA before/after, including untracked source: `c8b8664e2682f8709401e3efbb56cda6ac08e226459523d8165f09d2375cc732` (MATCH).
- `cargo check -p datazen-runtime`: EXIT=0, `Finished dev profile [unoptimized + debuginfo] target(s) in 1.27s`.
- `cargo check -p datazen --lib --no-default-features`: EXIT=0, `Finished dev profile [unoptimized + debuginfo] target(s) in 5.40s`, Host 50 warnings (includes inactive export adapter getter and baseline/helper/disabled-feature warnings).
- Both checks used independent `/tmp/datazen-p346-runtime-target`; logs `/tmp/datazen-p346-runtime-gate.log`, `/tmp/datazen-p346-host-gate.log`.
- One resumed intermediate gate failed on a duplicate artifact getter inserted on PlatformEntry; fixed, and the final matching gate above passed. No tests ran.
- Additional actual-budget fixes: multi-connection descriptor cost atomically reserves all unit permits; queued acquisitions consume only their own scheduled grant; caller drop returns an already granted unobserved permit; scheduling grants notify other waiting futures. Physical snapshots/caps include real primary permits. Unpublished disk remnants remain charged to the byte budget until deletion/recovery.
- Other open/context operations now use per-key coordination and short receipt-map locks; context replacements retire old application rows only after actual replacement success, using physical outcome evidence.

## Explicit pending integration/verification
- Real PostgreSQL/MySQL pinned and incremental streaming implementations belong to driver track, not this commit; integrate them before claiming P3 streaming end-to-end complete.
- Fresh independent WDIO validation is pending, including cancellation/admission races, bounded prefix publication, typed catalog, error recovery and original-resource result edits.
- Quarantined physical budget is deliberately not recovered without confirmed close; follow-up close-only verification is implemented below, pending WDIO/fault validation.
- Verified writable mapping is deliberately limited to a single wildcard relation. Multi-statement context source remains unobserved rather than guessed.
- Desktop Save dialog/IPC export wiring is implemented in the follow-up below; UI export journey remains pending.
- Row prefix blocks presently obtain columns at final statementMetadata; desktop has statementColumns consumption and driver was asked to publish it using write_output. This extra live-columns behavior must be checked on the integrated driver revision.
- Ledger is temporary branch coordination data and must be removed during final integration acceptance.

## Follow-up: controlled export and quarantine verification (2026-10-12)
- New Host IPC `export_artifact(request: { artifactId })` uses the actual execution-publication FileArtifactStore. It accepts no path, owner, org, capability, or truncation-confirmation input. Trusted identity is assembled by PlatformAdapter. ACL/TTL and finalized lifecycle are checked before dialogs. Truncated artifacts require a native confirmation, followed by the native Save dialog; dismissal returns `{status:"cancelled"}`. JSONL is the only accepted extension. Existing destination files are protected by exclusive creation (no overwrite support).
- Save capability binds artifact ID + organization + principal, expires after 300 seconds, and is single-use. Context delegation is rejected. Export runs on a blocking worker; dropping the IPC future signals cancellation. Each bounded block checks cancellation and capability expiry; completion rechecks artifact ACL/TTL/revocation before sync. Errors/cancellation remove the newly created incomplete file (filesystem removal can itself fail; no overwrite or deletion of preexisting files). Cancellation after successful sync cannot undo a committed export. Native dialog cancellation produces no capability/file.
- Export IO no longer holds the global artifact state mutex through every block. Revocation/expiration can proceed while IO runs and is rechecked before successful completion. Output and store files use owner-only creation/no-follow flags on Unix.
- Isolation decision: architecture connection-management §7.4/close flow says CloseUnconfirmed enters quarantine and is verified later. Therefore a later verification path is required for P3; automatic reconnect/reuse is forbidden and unnecessary. The production ledger now claims quarantined resources for close-only verification every 300 seconds from the actual idle reclaimer. Each original resource close has a 10-second timeout. Only the driver's real Closed confirmation removes the physical resource and settles its budget. Timeout/CloseUnconfirmed/error keeps quarantine, budget, and original unknown execution outcome. No Clean observation is invented.
- Compiler gate: `CARGO_TARGET_DIR=/tmp/datazen-p346-runtime-target cargo check -p datazen --lib --no-default-features`: EXIT=0; 49 Host warnings (baseline/helper/disabled feature); runtime compiles as dependency. Exact terminal: `    Finished `dev` profile [unoptimized + debuginfo] target(s) in 14.88s`. Before/after HEAD both `45bb4492aaf5c4e1c3174bd4b03f807619fe2d69`; source SHA including untracked source both `dab70a1aa7ed7cdb1c7f744a753300b18337c3c704de1f16df3e695c7ab3a699`. Log `/tmp/datazen-p346-export-host.log`. `git diff --check` clean.
- One intermediate build exposed private dialog helper visibility (EXIT=101); corrected using crate-private dialog module, no public path IPC. Final matching gate passed. No WDIO, Cargo tests or Vitest tests ran. This follow-up and 45bb are READY_FOR_TEST, not independently accepted.

## Candidate compile repair (2026-10-12)

- The first webdriver build of `300e6cf25` failed with Rust E0433 in `workflow/independent_sql.rs`: `executor` is nested under `workflow::workflows`, so `super::executor` was an invalid module path.
- Coder fix `4761a67c1` changes the reference to `super::workflows::executor::database_is_required`; integrated candidate is `9e8118afb65ff0438210d04b301f795fd7f589dd`.
- This fixes the statically identified compiler error. A fresh `pnpm tauri:build:webdriver` completed successfully on exact production-code HEAD `9e8118afb65ff0438210d04b301f795fd7f589dd`; it regenerated the `.app`. The old `.app` remains stale. Independent PG/MySQL WDIO acceptance is running/pending.
- Static transport review confirmed the explicit Tauri argument envelope and byte-array conversion match `read_artifact_chunk`. Updated the existing transport unit-test expectation from the obsolete `read_artifact` name to the registered command; no unit tests were run, per scope.
- Fresh PG/MySQL `e2e/specs/multi-database.ts` on the matching build exited 1: 4 failures (MySQL E2E-001/002 and PostgreSQL E2E-001/002), 2 skips (E2E-003). All four stop at database-tree discovery before expansion; selector-render failure marker count is 0. Log: `/private/tmp/datazen-p346-verifier-luna-9e-multidb-wdio.log`; HEAD `9e8118afb65ff0438210d04b301f795fd7f589dd`, test diff SHA `8c28620aad979d66727468382758a2c4419968ea988c3c0055554a9ecacddac7`, untracked SHA empty, start/end stable. Root cause stage still under diagnosis; later journeys are paused.
- Review found Schema Diff already has explicit endpoint schemas but `useSchemaDiffUnifiedObjects.listObjects` passes only database to `list_objects` and filters schema in Host. This is being corrected so the selected schema reaches the driver and is interpreted there; no `public` fallback. The prior WDIO failure remains unaccepted until fresh rebuild and rerun.
