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

## P3/P4/P6 wake continuation (2026-10-12)

- Current isolated candidate `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-p346-candidate` is `6db665478`: it includes dedicated metadata owner/schema fixes (`e98e37bdf`), MySQL close-retry fixes (`5e17c3744`, `53ffe99f6`), TableView recovery UI/journey (`b668f56f6`), and P6 SQL timeout/cancellation/receipt bounds (`493f4f5db`, `6db665478`). Candidate is clean; full-range `git diff --check` passed.
- P3 MySQL close retry now retains live resource, original pools, transaction bookkeeping and physical permit after disconnect failure/timeout. A still-open transaction blocks reuse but not physical teardown; only confirmed disconnect plus budget release marks the handle closed. Driver regression source covers retry after injected close failure, not executed per test restrictions.
- P4 TableView recovery commit marks a session lost after explicit lost/epoch/`resourceLost` evidence, preserves the execution receipt/error, and offers explicit reopen. Its PG/MySQL live termination journey is added but not yet accepted. A follow-up coder is tracing/fixing the confirmed concern that some physical disconnect errors may map to `SqlError → hostRejected/unknown` instead of resource-lost evidence; it must remain precise and never relabel ordinary SQL errors.
- P6 moves SQL deadlines inside `independent_sql`: cancel is followed by terminal polling; confirmed terminal results alone permit Skip/Fallback. Receipt uncertainty aborts the workflow with execution/effect diagnostics. Pre-dispatch and artifact-read waits are bounded; terminal artifact-read timeout preserves known effects. PG/MySQL workflow timeout WDIO journeys are added.
- Fresh independent Luna max verifier `/private/tmp/datazen-p346-verifier-r3` is aligned to candidate `6db665478`; it has node_modules and an ignored `.env` symlink without exposing its content. It is building `pnpm tauri:build:webdriver` and will run only PG/MySQL WDIO for database discovery, schema selection, TableView recovery, retained resources and Workflow timeout. Previous six-case multi-database result was against older source; do not use it to accept this candidate.
- Active unfinished coder: P4 disconnect-to-resourceLost classification; acceptance is in a separate verifier worktree. No computer-use, unit, Cargo, or typecheck runs are authorized.

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

## Latest wake continuation status (2026-10-12)

- Candidate now includes the precise physical-disconnect classification fix `eb6d10547` and successful TableView recovery banner fix `3f32bb6a8`; latest candidate HEAD is `3f32bb6a8b2d0a1392c82b3414a092010e4c232c`. Both changes are isolated from the verifier tree. Full candidate `git diff --check` passes.
- P4 coder `2ddb2b242` maps only confirmed PG/MySQL retained-session loss to typed `SessionLost`/`resourceLost`; normal SQL, permission and cancel errors keep their ordinary classification. Follow-up `d21e2d92f` clears the stale transaction error only after explicit new-session recovery succeeds and adjusts the PG/MySQL WDIO journey.
- Independent verifier `/private/tmp/datazen-p346-verifier-r3` is still building old pinned candidate `6db665478` to finish its already-started build check; it has not started WDIO. This result cannot accept the latest candidate. A fresh Luna max verifier run must build `3f32bb6a8` and run only PG/MySQL WDIO for database discovery, schema selection, TableView recovery, retained resources and Workflow timeout.
- To repair the verifier-only `node_modules` symlink, tester replaced it with a real directory and ran `pnpm install --frozen-lockfile` in that independent verifier tree (0 downloads). This conflicts with the repo environment note; no install has been run in the primary checkout. Reuse that verification tree after the current build exits and record the deviation.
- No P3/P4/P6 WDIO acceptance exists yet for the current candidate; do not merge to main or mark complete. No computer-use, unit, Cargo or typecheck runs are authorized.

## Final acceptance candidate (2026-10-12)

- P6 review found its timeout journey covered only legacy `query`. Original P6 coder added PG/MySQL cases for `Command(query)+Skip` and `Command(execute)+Fallback`, asserting `StepTimeout`, execution/effect evidence, confirmed terminal execution, zero live slow-query count before observer/next, and ordered successful continuation. Commit `01ef98d24` is integrated as candidate commit `8f6d8a2c8`.
- Latest candidate HEAD `8f6d8a2c8a36be405bb1bf43b7874a5bc9205029`, tree `53f618dca9e0f9d2b67cb2b95d107d2fc33d8754`; candidate clean and full-range `git diff --check` passes.
- P3 cancellation receipt fix `72289de8` is present in candidate as equivalent commit `e796bfeac`; dynamic same-session cancellation/cross-session isolation remains for WDIO. P3 driver quality audit found no new static blocker in MySQL close/quarantine/retry; close-timeout dynamic retry remains unproven.
- P6 coder audit confirmed code matches `4293b02e6`; remaining fail-closed receipt/cancel/terminal uncertainty and terminal artifact-read timeout boundaries remain source-reviewed but are not failure-injected by current WDIO.
- Fresh Luna max tester `/root/p346_final_acceptance_r2` is aligned to the exact candidate HEAD/tree above. It must rebuild webdriver and run only the PostgreSQL/MySQL WDIO journeys enumerated in its task. A previous fresh tester hit the workspace spend cap before doing verification; the second attempt is active.
- Keep P3/P4/P6 in progress until the matching build and all runnable PG/MySQL WDIO journeys complete. Do not merge to main or open a PR without a new user request. Only the project integration branch may receive accepted work; remove temporary `hub.md`/`progress.md` at final delivery as required by AGENTS.md.

## First matching-candidate WDIO round (2026-10-12)

- Fresh build on verifier exact HEAD `8f6d8a2c8a36be405bb1bf43b7874a5bc9205029` succeeded (`pnpm tauri:build:webdriver` EXIT=0); verifier start/end HEAD/tree matched and stayed clean. Nine serial WDIO invocations, 13 Mocha cases: 7 passed / 6 failed.
- Passed: Host multi-database discovery PG 3/3 + MySQL 3/3; PostgreSQL retained-resource driver journey 1/1.
- Failed: Schema Diff schema-selection at selecting `sourceSchema` (WebDriverError); TableView recovery PG and MySQL at navigator lookup/click for `recovery_probe` (WebDriverError); PostgreSQL workflow-timeout at `slow` terminal-status assertion; MySQL retained-resource and workflow-timeout both stopped before test body with `MYSQL_PROFILE_MISSING`.
- P4 coder resumed to investigate/fix schema selection and TableView journey. P6 coder resumed to inspect PG terminal-state assertion without weakening its contract. P3 driver coder resumed to seed a MySQL profile through the existing test config and clean it up safely; no `.env` content may be read or surfaced.
- All six failures are open; no current candidate acceptance. Each repair must be followed by a fresh webdriver build and a new independent Luna max WDIO verifier, running only PG/MySQL journeys.

## Singleflight acceptance and next P4/P3 fixes (2026-10-12)

- Clean PG/MySQL `multi-database.ts` WDIO on verifier HEAD `368a49e44554fb1336460a705e9325e463d6a2cb` (candidate source `d49edd39e`): EXIT=0, 6 passing / 0 failing / 0 pending. Both drivers passed E2E-001 database nodes, E2E-002 expand/tables, and E2E-003 QueryPanel selector. Fresh webdriver build EXIT=0 (24.54s; 2 bundles). Build code fingerprint `f708f6c3…`; WDIO patch diff SHA `8c28620a…` unchanged. No app/WDIO processes remain. This verifies the observed duplicate discovery scenario is fixed; the old unpaired receipt trace remains insufficient to prove a unique root cause.
- Candidate now includes coder commit `d20bf7c76` as `e98e37bdf`: explicit persistent connectionId flows through dedicated database discovery, Schema Diff catalog/object calls, Data Sync metadata, saved setups, plus schema options use catalog.schemas. WDIO schema journey now covers first-step database/schema selection, an empty selectable schema, and selected-schema objects on step two. Candidate HEAD `e98e37bdff3ad5afb1614b8aefb9b735668f1484`; `git diff --check` clean. Not built or tested yet.
- P4 coder resumed separate TableView SessionLost recovery patch at `d20bf7c76` parent; must commit before next matching build.
- P3 driver coder resumed MySQL repeated-close/quarantine confirmation patch in its driver worktree. Audit confirms host's quarantine verifier repeats close on the original handle; missing MySQL live record must not be upgraded to `Closed` unless the physical close is actually confirmed. Provider cancellation cleanup race is only proven under direct provider concurrency; current GUI actor serializes cancel after backend cleanup, so keep it as an acceptance edge, not a confirmed GUI defect.
- After both production patches land and are reviewed, advance the verifier from `368a49e44` to exact candidate commit(s), preserve only reviewed WDIO patches, run fresh `pnpm tauri:build:webdriver`, then PG/MySQL multi-database and PostgreSQL schema-selection, retained-resource and feasible workflow journeys. Do not run non-WDIO tests.

## Resumed repair and acceptance round (2026-10-12)

- The one-shot wake schedule `05-35-p3-p4-p6` fired. A deletion request returned `not_found`; the app confirmed the schedule was already absent.
- Fresh independent WDIO on candidate `8f6d8a2c8` reported 7 pass / 6 fail across 13 PG/MySQL cases. Open failures: Schema Diff schema selection, PG/MySQL TableView recovery navigator lookup, PG Workflow timeout (`legacy-query + Skip` reported `success`), and two MySQL journeys blocked before execution by missing profile setup.
- P3 fixture commit `8224e098e` is integrated as candidate `3ade53825`. Its workflow journey overlapped the richer P6 test; conflict was resolved by retaining all four SQL entry/strategy cases and wrapping them in the owned MySQL profile fixture. P3 and P6 commits pass `git show --check`; no test was run during integration.
- P6 coder commit `c7de75aaa` is integrated as `481e1186b`. Root cause: execution polling can return a ready nonterminal state after its deadline and continue until SQL succeeds. The loop now checks the step and cancellation grace deadlines before each lookup, then uses the existing cancellation/terminal confirmation path. Contract assertion remains unchanged. Static review done; independent WDIO still pending.
- P4 coder resumed in its isolated worktree. It found `DesktopSessionBackend` manually constructing catalog `ConnectionHandle` with `provider_id` as `pool_id`; this cannot resolve the actual PG/MySQL pool. It is adding an ownership/epoch-checked native handle path, alongside schema selection and TableView recovery journey fixes. No other agent is changing this seam.
- Current candidate `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-p346-candidate` is `481e1186bdc9bf4f0900ac0002899d1d17967df8` (tree `2a29e17bebd30a0a452196ed3bad140763854e00`), clean. It is not yet built or tested after these repair commits.
- Next gate after P4 commit: review and integrate, then a fresh GPT-6 Luna + max verifier in a separate worktree runs `pnpm tauri:build:webdriver` and only the specified PostgreSQL/MySQL WDIO journeys. No unit/Cargo/Vitest/typecheck/computer-use tests; do not read `.env` contents. Keep the candidate unaccepted until every runnable journey passes.

## P4 integration and fresh verification (2026-10-12)

- P4 commit `a5141b7a2` was reviewed and integrated as candidate `6597af184`. `ResourceProvider::connection_handle` is a required method; providers validate owner, runtime epoch, and live registration before returning their native handle. Both catalog metadata entry points now consume the held handle. PostgreSQL schema enumeration includes accessible empty schemas and adds no default schema.
- WDIO journeys now wait for asynchronous Schema Diff endpoint options, verify an empty schema and selected-schema object scope, and locate the TableView recovery fixture through its unique database/table/schema instead of falling back to a seed database.
- Candidate is clean at HEAD `6597af1844e91c8bcb5df153a1d6f8a1fbbf710c`, tree `e8851dc82edf4c8404b05ae45bfa58c8acfec2d2`; new commit and cumulative repair range `git diff --check` pass. No tests/build have run on this exact candidate yet.
- New independent tester `/root/p346_final_acceptance_r3` (GPT-6 Luna + max) is running from `/private/tmp/datazen-p346-verifier-r3`. It will rebuild with `pnpm tauri:build:webdriver` then run only PostgreSQL/MySQL WDIO discovery, Schema Diff, TableView recovery, retained-resource, and Workflow timeout journeys. No source edits or non-WDIO tests are authorized.

## Second WDIO result and repair round (2026-10-12)

- Fresh `pnpm tauri:build:webdriver` on exact candidate `6597af1844e91c8bcb5df153a1d6f8a1fbbf710c` succeeded and produced a runnable `.app`; verifier HEAD/tree/write-tree and clean status matched before/after build.
- One serial WDIO allowlist run completed 13 cases: 9 passed, 4 failed, 0 pending. Multi-database PG/MySQL 6/6 passed; Workflow timeout PG/MySQL 2/2 passed; retained-resource PG 1/1 passed. Failures: Schema Diff source endpoint option stayed unavailable; TableView recovery PG/MySQL could not find the saved fixture connection card; MySQL retained-resource expected failed/cancelled but received succeeded.
- MySQL coder confirmed the retained-resource failure came from fixture SQL semantics: standalone `SELECT SLEEP(10)` can be interrupted and return `1` successfully. Commit `fba14efaa` changes the journey to `SELECT id FROM driver_verify_temp WHERE SLEEP(10) = 0`, which should yield server error 1317 on interruption, and asserts cancel disposition `requested` while retaining strict terminal-state assertions. Integrated as candidate `4cf4f80ff`; no test/build yet on this exact candidate.
- P4 original coder is repairing the two remaining P4 journey failures, tracing the connection option and saved fixture profile visibility. It must keep the source/object/schema contract and both database recovery flows intact.
- The failure round's source review completed on verifier `6597af184`; its results cannot accept newer candidates `4cf4f80ff` or `379e2d645`. A fresh GPT-6 Luna + max verifier was started on the exact latest candidate with only PostgreSQL/MySQL WDIO. No `.env` reads or non-WDIO tests.

## P3/P4 failure repairs integrated (2026-10-12)

- P4 second repair commit `02a25cea0` is integrated as candidate `379e2d645`. Schema Diff WDIO now chooses the exact rendered endpoint label including `(postgresql)`. Table recovery first verifies the fixture profile is persisted, reloads MainPage so the frontend store sees it, mounts the virtualized row through navigator search, selects exact `data-conn-name`, then continues with the fixture database/schema.
- P3 cancellation SQL semantics fix `fba14efaa` is integrated as `4cf4f80ff`: standalone MySQL `SLEEP` may return success after KILL QUERY, so the journey now interrupts `SLEEP` inside a table predicate and expects the stable server error. It also asserts cancel receipt disposition is `requested`; the strict failed/cancelled terminal assertion remains.
- Current candidate is clean at `379e2d6458f5655c880cb01bffe5fb47bb2c59cb`, tree `8ed10ae986de93edb19c7fb1bc1c9033ba7d1218`; `git show --check` and cumulative repair-range `git diff --check` pass. No build or WDIO run yet on this candidate.
- Prior verifier round on `6597af184` is complete at 9/13; source review is returning its final report. Those results are not acceptance for the current candidate. Next action is a fresh Luna max verifier in a distinct clean tree, then only the allowed PG/MySQL WDIO suite.

## Fresh re-verification on current candidate (2026-10-12)

- Old verifier `/root/p346_final_acceptance_r3` completed its full report on old candidate `6597af184` (9/13); its worktree remained clean and did not modify source.
- New tester `/root/p346_final_acceptance_r4` (fresh GPT-6 Luna + max instance) is running from the independent `/private/tmp/datazen-p346-verifier-r3`, reset to exact candidate `379e2d6458f5655c880cb01bffe5fb47bb2c59cb` (tree `8ed10ae986de93edb19c7fb1bc1c9033ba7d1218`). It will perform a fresh webdriver build and the same serial seven-file PG/MySQL WDIO allowlist. No source changes or non-WDIO tests are authorized.

## R4 WDIO failures and P4 repair continuation (2026-10-12)

- Fresh webdriver build on exact candidate `379e2d6458f5655c880cb01bffe5fb47bb2c59cb` passed. WDIO result: 10 passed / 1 failed / 0 pending; five spec files passed and two failed. PG/MySQL multi-database 6/6, retained-resource 2/2, workflow-timeout 2/2 passed; MySQL cancellation semantics fix is accepted by live WDIO.
- Remaining failures on this exact candidate: Schema Diff is blocked in Step 1 because the target database dropdown option is unavailable at `schema-diff-schema-selection.ts:71`; Step 2 never runs. TableView recovery spec fails during ESM named-export linking because it imports non-exported `setNavigatorSearch` at `table-session-loss-recovery.ts:6` (definition is private in `e2e/helpers.ts`). Candidate and verifier fingerprints remained `379e2d645` / `8ed10ae`, and both worktrees were clean.
- Original P4 coder `/root/p4_pg_multidb_fix` has been resumed in `/private/tmp/datazen-p4-pg-fix` to fix these two actual failures. The initial Step 2 discovery diagnosis was incorrect and has been superseded by the exact line 71 report. The helper export/import issue is confirmed. No assertions should be weakened and no env contents may be read.
- The completed R4 tester supplied sanitized file/line/parser-stage diagnostics only and did not rerun tests or edit files. After P4 commits, integrate into the isolated candidate and dispatch a fresh GPT-6 Luna + max verifier in a separate clean worktree for webdriver build and the PG/MySQL WDIO allowlist only.
- No unit, Cargo, Vitest, typecheck, or computer-use tests are authorized. Do not accept P3/P4/P6 until the matching candidate passes the fresh independent WDIO round.

## P4 repair integrated, R5 acceptance pending (2026-10-12)

- P4 coder commit `8a4e3bbad` was reviewed and cherry-picked into the isolated candidate as `ad234ba269066bda55b98c2e175975b4be9e9358` (tree `25dea91cd41aa70a2a6eeef4caf2e85e71eb036e`). Candidate and coder worktrees are clean; commit check is clean.
- The database option failure is fixed by sharing only the in-flight `list_databases` promise by explicit connection owner; the promise is removed on success or failure, no result cache is kept, and session-scoped callers without an owner remain isolated. The Table recovery journey now uses an in-file search helper rather than importing a private helper from `e2e/helpers.ts`.
- Fresh verifier `/root/p346_final_acceptance_r5` (GPT-6 Luna + max) is running in `/private/tmp/datazen-p346-verifier-r3`, separate from the candidate. It resets to exact `ad234ba26`, builds with `pnpm tauri:build:webdriver`, then runs only the seven PostgreSQL/MySQL WDIO files listed in this ledger's preceding verification section. It records HEAD/tree/status before and after, and reports sanitized per-spec counts and first failure stages only.
- No non-WDIO tests, computer-use tests, `.env` content reads, or source edits in the verifier tree.

## R5 WDIO result and second P4 repair loop (2026-10-12)

- Fresh webdriver build on exact candidate `ad234ba269066bda55b98c2e175975b4be9e9358` succeeded. WDIO exited 1: five of seven spec files passed. `multi-database` passed 6/6; retained-resource PG/MySQL and workflow-timeout PG/MySQL passed 4/4 total. Schema selection failed and Table recovery's PG/MySQL cases both failed with WebDriver timeouts. Verifier stayed clean and unchanged at tree `25dea91cd41aa70a2a6eeef4caf2e85e71eb036e`.
- Schema's reported stack frame is `e2e/specs/schema-diff-schema-selection.ts:17`, called from `:71`. Source review confirms `:71` selects the target database before clicking Next, so this remains a Step 1 target-database option timeout; an R5 tester note calling it Step 2 was inaccurate. The owner-scoped in-flight discovery fix did not resolve it.
- Table failures registered and ran, but R5 captured no source stack frames or safe stage details. Existing R5 log: `/var/folders/2y/5w1_2qg97r7b9bfv20b4nybh0000gn/T/datazen-p346-wdio.6zUuMMPyaq.log`; raw output remains unshared.
- Original P4 coder `/root/p4_pg_multidb_fix` is resumed again to trace these exact failures and change only the affected flow/journey. No assertions may be weakened; no `.env` content may be read. A fresh Luna + max tester must rerun the exact seven PostgreSQL/MySQL WDIO specs after the repair.

## P4 repair R6 candidate (2026-10-12)

- P4 commit `64b9083ed` was reviewed and cherry-picked into the isolated candidate as `d27b7d5e03f6b5cba33653060921a09cfea05b32` (tree `06e989e02e3e21d592ee0f882bf05af9e01d7e14`). It fixes the Schema Diff test selector to match the label span within the trigger's own listbox, and records database discovery failures as a fixed stage plus allowlisted `ApiError.code`; stale/cancelled discovery results cannot overwrite the current selection. TableView recovery search now dispatches a controlled input change in one DOM operation and reports only a fixed journey stage/category on failure.
- Candidate is clean and `git show --check` passed. No build or WDIO has run on this exact candidate. Prior R5 failure location was schema target database selection; product error classification was absent from the previous run, so the new diagnostic distinguishes unavailable options from rejected discovery without exposing raw errors.
- P6/runtime and addressable audits were resumed and confirmed no remaining code changes; their existing P6 journeys passed in R5. The only open acceptance items are the two P4 WDIO spec files.
- Next: use a fresh GPT-6 Luna + max verifier in a separate worktree; run `pnpm tauri:build:webdriver` and only the seven PostgreSQL/MySQL WDIO specs in the preceding acceptance section. Keep the candidate in progress until matching build and all allowlisted journeys pass.
- Fresh tester `/root/p346_final_acceptance_r6` is now running from `/private/tmp/datazen-p346-verifier-r3`, reset to exact candidate `d27b7d5e03f6b5cba33653060921a09cfea05b32` with tree `06e989e02e3e21d592ee0f882bf05af9e01d7e14`. It will build with `pnpm tauri:build:webdriver`, then run the exact seven-file allowlist serially; logs stay under system temp and source edits/non-WDIO tests are prohibited.
- R6 build passed. WDIO exited 1: `multi-database` 6/6, Schema Diff 0/1, Table recovery 0/2, and the four PG/MySQL driver specs 4/4; five spec files passed, two failed. R6 verifier remained unchanged and clean at the exact candidate. Its `$TMPDIR` WDIO log disappeared after command exit, so the failure line/category is unavailable; no rerun was made by R6.
- A fresh Luna + max diagnostic verifier `/root/p346_p4_failure_diagnostic_r7` is running once on only the two failing P4 specs from the same independent verifier tree and already-built exact app. It uses a durable system-temp log under `/private/tmp`, reports sanitized source/stage/category only, makes no edits, and will not rerun.
- Static P4 review confirmed a stale Schema Diff discovery diagnostic could persist after a later valid endpoint/discovery and block recovery. Coder commit `0a292e975` was reviewed and integrated into the candidate as `91a82877a` (tree `35c184cf082b29a9319c6a476b3b2e438fa3c16d`): it tracks source/target diagnostics separately, clears only the exact owned message on endpoint change or current-request success, and ignores stale request completions. No tests/build ran on this patch; candidate remains unaccepted pending a fresh exact-HEAD webdriver build and PG/MySQL WDIO.
