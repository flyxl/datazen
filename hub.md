# P3 / P4 completion and P6 start

Integration branch: `codex/p3-p4-p6-runtime`. Work is in progress; these phases are not accepted.

## Active tracks

All tracks start from `246835a9c`; the coordinator handles review, integration and WDIO acceptance.

| Track | Agent | Branch | Scope | State |
| --- | --- | --- | --- | --- |
| Drivers | `/root/p346_driver_cancel_fix` | `codex/p346-drivers` | PostgreSQL/MySQL fixed physical sessions and required neutral driver hooks | Coding |
| Runtime | `/root/p346_runtime` | `codex/p346-runtime` | P3 production ArtifactStore, resource policy and transaction handle lifecycle | Coding |
| Desktop | `/root/p346_desktop` | `codex/p346-desktop` | P4 query/table/metadata/result consumers and existing WDIO assertions | Coding |

Worktrees are `.worktrees/datazen-p346-{drivers,runtime,desktop}`. No track may edit another checkout. Review/verification must use a separate worktree from the coder.

## Existing implementation changes

- Query execution, transactions and cancellation now use pane-owned session APIs.
- Closing query tabs preserves UI ownership when backend closure is refused.
- Runtime event delivery and replay are bounded; expired settled executions and issued tokens are reclaimed.
- Cancel submission tokens are checked at the desktop IPC boundary.
- Table ChangeSets use an independent session, parameterized writes, short transaction and explicit cleanup.
- Table previews bind configuration revision, rechecked before writes.
- Workflow SQL steps use executeAtTarget and request cancellation when their future is dropped.

## Evidence

- Rust Host check: EXIT=0 in `/tmp/datazen-p346-final-check.log`.
- Frontend typecheck: EXIT=0 in `/tmp/datazen-p346-final-tsc.log`.
- Check HEAD: `d5d3fdaa2cf73956d79c4f9bd42e5b310b021722`; start/end source SHA: `50086c13d6925886a096b5208383613486583bbcbaee078cebaa966b57451bf6`.
- WDIO build with basic drivers and webdriver: EXIT=0 in `/tmp/datazen-p346-wdio-build.log`; start/end source SHA: `5139052717e502b0e6b1c8b1ce3cddc73e96e4a6e6f887072e67bcf9129d488c`.
- Selected SQL WDIO: EXIT=0, 1 passing. Its body-text assertion can match SQL editor text, so this is NOT proof that the new execution route worked.

## Remaining acceptance work

Coordinator review is active. The resumed tracks remain unaccepted; their working diffs are not integrated.

- Committed integration baseline `c0f15b65319eb331af8ebe11b7c86c7b05fa5460`: direct TypeScript check EXIT=0 (`/tmp/datazen-p346-root-committed-tsc.log`); HEAD unchanged and tracked diff SHA at both boundaries `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.
- Runtime review requested artifact recovery count/size validation, publication failure handling, and export capability owner/TTL binding.
- Runtime review found admission cancellation can orphan a target execution before its receipt; the runtime track is implementing owned admission and cleanup.
- Cross-track contract uses typed `commandResult` artifact output for catalog commands, not fabricated row columns. Driver and runtime implementations must be checked together.
- Host resource purpose must not be guessed from arbitrary `get_`/`list_` prefixes; runtime review requested validated command policy.
- Desktop acceptance must preserve concrete sanitized database diagnostics and the existing error-to-success recovery journey.
- Result writeback review: the baseline `ResultTableView` is a read-only adapter whose edit callback does not persist. Verified mapping and original-runtime writeback are an unimplemented P4 requirement, not an established baseline writeback regression.
- Coordinator found accidental removal of terminal session/context events; runtime confirmed and is restoring them. Target admissions also need per-key coordination instead of a global lock across connection waits.
- Fixed driver sessions must use per-resource locks; holding the transaction-map lock through SQL would serialize independent editors.
- Metadata migration must preserve existing typed `list_catalog` / `read_relation_columns` / `read_relation_schema` semantics, configuration-aware cache identity and bounded batches; switching to legacy `list_tables` is insufficient.
- Driver API dependency commit: `fec6c7fe2b3b82cfe8f63b56e3de9c37b201c597` (retained-session hooks and typed output); not yet independently accepted/integrated.

- Driver fixed physical-session support for PostgreSQL/MySQL; both currently declare stateful editor sessions unsupported.
- Host resource scope must consume provider connection-cost policy rather than constant 1.
- Runtime transaction handle registration/retirement must match driver completion observations, avoiding duplicates and stale handles.
- ArtifactStore concrete production implementation, durability, TTL and bounded result consumption.
- Table reads/metadata caching, result provenance/writeback and session lifecycle compatibility.
- P6 SQL cancellation and isolation acceptance; Workflow blocks and other P6 consumers remain unimplemented.

Only WDIO tests are authorized. No Vitest/Cargo tests or computer-use testing. Never read env file contents; test programs may load configuration privately.

This ledger must be deleted before delivery/merge to main.

## Resumed integration snapshot (2026-10-12)

- Driver implementation `0c107d0a9`, runtime `45bb4492a`, desktop `cd6496c7f` are committed; compile checks passed but independent live acceptance is incomplete.
- Root Workflow diagnostics follow-up `529606200` compiled successfully, not yet independently accepted.
- Isolated combined candidate `.worktrees/datazen-p346-candidate` at `aa6fe601d` includes exact API/helper dependencies, all three implementations, Workflow diagnostics, and WDIO preparation `aad272f3a` / `2f438abf1`. Only process-ledger add/add conflicts required resolution. Production source had no merge conflicts.
- Existing independent driver tester resumed against this explicit candidate for webdriver build and actual PG/MySQL journeys. WDIO has not yet run on this candidate.
- Driver cancellation paths hold a global execution lock across control network waits; repair coder is handling per-execution coordination and neutral early statementColumns production.
- Runtime coder resumed controlled artifact export IPC follow-up. Desktop confirmed autocomplete browsing, latest target reopen, and source-context write guards in committed code.
- New tester model request GPT-6 Luna ultra was rejected by tool: supported maximum is max. User clarification is pending; do not silently substitute a different model or effort. Existing tester continues its current round.
- Root integration checkout still contains only previously integrated baseline, not unaccepted candidate implementations.

## Agent settings and scheduled continuation

- User confirmed future testers: GPT-6 Luna, max. Fresh independent tester `/root/p346_combined_verify_r2` is active.
- User requests future coders: GPT-6.1 Sol, low; preserve currently running agents' work, use this setting when spawning coder instances.
- One-time heartbeat `05-35-p3-p4-p6` fired at 2026-10-12 05:35 Asia/Shanghai. The app reports it already does not exist; no active schedule remains.
- Updated combined candidate `74c61627509103636b6c7f0c57a6f2c55efeb6b6` includes driver cancellation fix `ec83ad3c9` and runtime export/quarantine source from final `e62798e67`; production source merged without conflicts.
- Older frozen candidate `aa6fe601d` webdriver build succeeded. Initial real WDIO 0 passed / 2 failed: PG reached cancellation IPC after earlier identity/parameter/temp/schema/600-row/transaction journeys; MySQL fixture profile missing. Tester is fixing only diagnostic wrapper and fixtures before completing the round. Neither candidate is accepted.

## WDIO execution evidence and confirmed defect

- The previous tester ran the real combined candidate (`aa6fe601d`): initial PG/MySQL retained journeys 0/2 because of one PG cancel receipt contract defect and one missing MySQL profile fixture.
- With test-only fixture/expectation corrections, MySQL retained journey passed 1/1 across identity, typed bound query, temp table + same-pin metadata, 600 rows, transaction rollback, precise cancel, and same-pin recovery. `SELECT SLEEP` may return succeeded with value 1 after cancellation; a `WHERE SLEEP(10)=0` cancel returns failed/cancelled, consistent with server behavior.
- PostgreSQL journey passed earlier identity, bound parameters, temp table/schema, 600 rows, transaction rollback and failed-transaction recovery. Its sole remaining failure is confirmed production mismatch: PG resource provider returns `CancelReceipt.state=None` after precise cancellation, while Host rejects missing state as ProtocolError (`outcomeUnknown`). New coder `/root/p346_pg_cancel_receipt_fix` is repairing driver receipt semantics; fresh Luna max tester `/root/p346_pg_cancel_retest` will independently verify.
- The original tester hit the workspace spend cap during final Host UI WDIO coverage; its worktree has two uncommitted test-only patches for safe error-code projection/MySQL profile fixture and SLEEP cancellation semantics. Preserve these as test inputs, review before applying; no production changes there.
- The one-time 05:35 Asia/Shanghai heartbeat fired and was deleted.

- PG cancel receipt fix committed `72289de8cf2f708bde7920bec89ed941315871fd`: an accepted precise cancel returns `Some(CancelRequested)` (nonterminal) to match `driver-api`; unregistered execution remains `None`. Combined validation candidate advanced to `e796bfeac`. Fresh tester is aligning to it for independent WDIO rerun.

## Host PostgreSQL multi-database discovery regression (2026-10-12)

- Fresh Luna max verifier used an isolated `/private/tmp` worktree at candidate `e796bfeac`; no credentials or raw errors were surfaced.
- The initial PostgreSQL target probe used a malformed request envelope and did not represent the generated-client call. The full WDIO UI showed zero database nodes; a later stage-split safe probe against `2bb263fb4` established the actual cause: target execution is accepted and succeeds, but artifact readback fails. Stop-button WDIO journey passed. MySQL and PostgreSQL retained-resource driver journeys passed 2/2.
- Independent code review found two concrete issues: database discovery was routed through `metadataTarget`, which treated an empty `currentDatabase` as an explicit empty target; `list_databases` was also omitted from Query authorization classifiers and therefore classified as Write.
- Focused fix commits from coder worktree: `a67c3065a` adds `databaseDiscoveryTarget` with null namespace and uses it for `getDatabases`, and corrects the session-backend command classifier; `d2147f03a` corrects the runtime enqueue Query classifier. Combined candidate is now `2bb263fb4`.
- The database-target and authorization fixes were necessary but insufficient. The explicit artifact IPC mapping is now in candidate `300e6cf25`; independent verifier must rebuild that candidate and rerun PostgreSQL/MySQL multi-database tree and query selector WDIO paths with matching source fingerprints. No unit tests/Cargo tests/Vitest/computer-use testing are authorized.
- Before delivery, remove the diagnostic-only probe from tester's WDIO diff; review and retain only permanent safe journey assertions.

## P6 Workflow SQL target guard

- Code review found the SQL `Command` path (`query`/`execute`) entered `independent_sql::execute` without the same multi-database target requirement enforced by legacy Workflow `Query` steps.
- Coder commit `40749f978` moves the existing `database_is_required` policy to the shared independent SQL entry. Multi-database drivers now require a step, workflow, or connection database before opening the SQL resource; single-database drivers remain allowed. Explicit malformed namespace values continue to fail validation before this guard.
- This extends the earlier P6 Workflow SQL execution/cancellation/effect-diagnostic work. It is integrated in candidate `300e6cf25`, with compile and WDIO validation still pending. Include it in the current isolated build and, where feasible, run Workflow WDIO against PostgreSQL/MySQL. No unit tests are authorized.

## Continued candidate state (2026-10-12)

- The candidate includes the shared Workflow SQL target guard as commit `e6782a879`; it remains uncompiled and unaccepted pending the matching WDIO build.
- Safe stage-split probes on fixed candidate `2bb263fb4` confirmed: legacy counts were PG 25 / MySQL 14; generated-client target submissions returned queued receipts, then succeeded with completed effects; artifact readback was the failing stage. Temporary probes are being removed before the verifier moves to the fixed candidate.
- Do not accept P4 until the database tree and target selector pass on both drivers and result-readback works. Candidate `300e6cf25` now contains the explicit IPC mapping; a fresh webdriver build is pending.

## Artifact readback fix and next gate

- The stage-split safe WDIO probe corrected the failure boundary: PostgreSQL and MySQL target submissions both return `queued`, then terminate `succeeded` with `effectOutcome=completed`. The helper fails during artifact readback; the direct legacy database counts remain 25/14.
- Source confirms `ExecutionProjection.recover()` calls `BackendClient.readArtifact`, whose default naming would invoke `read_artifact`, but desktop only registers `read_artifact_chunk`. Coder commit `fd812d7fd` adds the explicit transport override; it is integrated in candidate `300e6cf25`. No budget policy was changed.
- Independent verifier is removing the temporary probe, retaining only reviewed test journeys, moving to `300e6cf25`, rebuilding with `pnpm tauri:build:webdriver`, then running PG/MySQL database tree/selector/Stop plus feasible PG table journeys and PG/MySQL Workflow WDIO. The fix is not accepted until those matching gates finish with stable source hashes.

## Latest candidate and verifier status (2026-10-12 05:35)

- The matching webdriver build attempted from `300e6cf25` failed at Rust E0433 in `workflow/independent_sql.rs`: the executor module is nested under `workflow::workflows`, so `super::executor` did not resolve.
- Coder `4761a67c1` corrected the path; candidate is now `9e8118afb65ff0438210d04b301f795fd7f589dd`. This source fix has not yet passed a fresh webdriver build.
- Independent Luna max verifier is preserving its reviewed WDIO-only patches, aligning to the new candidate, then rebuilding. Its previous `.app` is stale and cannot be used.
- No other unfinished agent is currently active; historical coders are complete. Do not mark P3/P4 accepted until PG/MySQL database discovery and downstream journeys pass on the matching fresh build. P6 remains in progress.
- Static review confirmed `readArtifact` sends `{request: ...}` to the registered `read_artifact_chunk` and normalizes returned bytes. The existing transport unit assertion still named the retired `read_artifact`; candidate now updates that expectation. No unit test was run. This test-only update is outside the verifier's already-running build.
- Fresh `pnpm tauri:build:webdriver` on verifier HEAD `9e8118afb65ff0438210d04b301f795fd7f589dd` succeeded and regenerated the `.app`; build start/end HEAD and WDIO patch hash were stable. PG/MySQL `multi-database.ts` is now running. Awaiting exact full source SHA/log conclusion and E2E results before acceptance.
- PG/MySQL `multi-database.ts` finished EXIT=1: 4 failed, 2 skipped. PG and MySQL E2E-001/002 fail before tree expansion at database discovery; E2E-003 skips for fewer than two nodes; selector-render marker 0. Logs `/private/tmp/datazen-p346-verifier-luna-9e-multidb-wdio.log`; matching HEAD `9e8118afb…`, test diff SHA `8c28620a…`, no untracked paths; start/end unchanged. No repeat run; tester is locating the first failure stage from that log.
- Static audit found a second schema-flow gap in Schema Diff: endpoints hold database+schema, but `listObjects` passed only database into `list_objects`, then Host-filtered schema. The coder fix below forwards schema through the driver command contract. Fresh rebuild/PG+MySQL WDIO is required afterward.
- Schema forwarding fix `e28384f46` was reviewed and integrated in candidate `4d74ea1c7`. The selected schema now travels from endpoints → object loader → target and command input → shared driver schema-object command, which applies an exact schema filter. Host filtering is removed; no `public` default was added. `git diff --check` clean; candidate needs a new webdriver build and PG/MySQL WDIO after the still-unresolved DB discovery failure is diagnosed.

## 05:35 wake continuation (2026-10-12)

- The one-shot automation `05-35-p3-p4-p6` fired. A delete request returned `not_found`; the app reports it already does not exist, so no active schedule remains.
- Candidate advanced to `d49edd39e`: singleflight commit `91fa341de` coalesces concurrent `getDatabases` calls by runtime `dbSessionId` and removes the entry on success or failure; WDIO `schema-diff-schema-selection.ts` asserts each selected PostgreSQL schema shows exactly its own same-named view on Step 2.
- Independent Luna max verifier aligned to production candidate at `368a49e44`; fresh `pnpm tauri:build:webdriver` EXIT=0 (24.54s; 2 bundles), source SHA begins `f708f6c`, WDIO patch SHA begins `2ba7e`; PG/MySQL `multi-database.ts` is running. This gate only validates the active-session database tree.
- Earlier instrumented evidence localized one failed database discovery at submission receipt (`ServiceUnavailable`) while a second call returned 25 databases; two calls were observed but not correlated. Singleflight targets the duplicate calls; the precise causal race remains an inference until clean PG/MySQL WDIO finishes.
- Code-only P4 review found dedicated migration sessions are not in `activeConnectionStore`, while `metadataTarget` reverse-resolves connectionId only from that store. This blocks dedicated `listDatabasesDedicated` and post-selection catalog/object loads. Coder `/root/p4_pg_multidb_fix` is repairing explicit persistent `connectionId` forwarding across Schema Diff, Data Sync and Data Transfer metadata consumers. Related P2: Schema Diff still derives schema options from relations, so empty schemas are omitted; use `ListCatalog.schemas`.
- Code-only P6 review found step timeout drops the SQL Workflow future, schedules cancellation in `CancelOnDrop`, then immediately permits generic `Skip`/`Fallback`; execution may still commit while later work starts. This is a confirmed P1. Stop-before-next-step behavior and bounded cancellation/terminal receipt need focused repair and PostgreSQL/MySQL WDIO journey.
- P4 review also found SessionLost during table transactions can leave `txLost` false, hiding the explicit “open new session” recovery action. No patch yet.
- P3 driver review found MySQL `close_resource` removes live registration before close is confirmed; repeat close can return `Closed` without rechecking physical state. The audit is confirming actor bypass and resource-accounting effects; no patch yet.
- Independent verifier must rebuild after each production patch. Only PostgreSQL/MySQL WDIO is authorized; no computer-use, Vitest or Cargo/unit tests. Do not claim P3/P4/P6 complete until database tree, dedicated migration discovery/schema selection, driver cancel/session lifecycle, P4 result recovery and P6 timeout/cancel journeys pass on matching builds.

## Singleflight acceptance and next P4/P3 fixes (2026-10-12)

- Clean PG/MySQL `multi-database.ts` WDIO on verifier HEAD `368a49e44554fb1336460a705e9325e463d6a2cb` (candidate source `d49edd39e`): EXIT=0, 6 passing / 0 failing / 0 pending. Both drivers passed E2E-001 database nodes, E2E-002 expand/tables, and E2E-003 QueryPanel selector. Fresh webdriver build EXIT=0 (24.54s; 2 bundles). Build code fingerprint `f708f6c3…`; WDIO patch diff SHA `8c28620a…` unchanged. No app/WDIO processes remain. This verifies the observed duplicate discovery scenario is fixed; the old unpaired receipt trace remains insufficient to prove a unique root cause.
- Candidate now includes coder commit `d20bf7c76` as `e98e37bdf`: explicit persistent connectionId flows through dedicated database discovery, Schema Diff catalog/object calls, Data Sync metadata, saved setups, plus schema options use catalog.schemas. WDIO schema journey now covers first-step database/schema selection, an empty selectable schema, and selected-schema objects on step two. Candidate HEAD `e98e37bdff3ad5afb1614b8aefb9b735668f1484`; `git diff --check` clean. Not built or tested yet.
- P4 coder resumed separate TableView SessionLost recovery patch at `d20bf7c76` parent; must commit before next matching build.
- P3 driver coder resumed MySQL repeated-close/quarantine confirmation patch in its driver worktree. Audit confirms host's quarantine verifier repeats close on the original handle; missing MySQL live record must not be upgraded to `Closed` unless the physical close is actually confirmed. Provider cancellation cleanup race is only proven under direct provider concurrency; current GUI actor serializes cancel after backend cleanup, so keep it as an acceptance edge, not a confirmed GUI defect.
- After both production patches land and are reviewed, advance the verifier from `368a49e44` to exact candidate commit(s), preserve only reviewed WDIO patches, run fresh `pnpm tauri:build:webdriver`, then PG/MySQL multi-database and PostgreSQL schema-selection, retained-resource and feasible workflow journeys. Do not run non-WDIO tests.
