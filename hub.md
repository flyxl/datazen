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
- One-time heartbeat `05-35-p3-p4-p6` is ACTIVE for 2026-10-12 05:35 Asia/Shanghai, attached to this chat; it resumes unfinished work and agents, then closes itself.
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
- Real PostgreSQL probe: legacy driver `list_databases` returned 25 entries. `executeAtTarget(list_databases)` failed before a receipt for both a null namespace database and a valid explicit database; safe projection showed only generic `Error`, no code/status, and message length 15. Host DOM showed zero database nodes; PG multi-database tree and query selector journeys failed. Stop-button WDIO journey passed. MySQL and PostgreSQL retained-resource driver journeys passed 2/2.
- Independent code review found two concrete issues: database discovery was routed through `metadataTarget`, which treated an empty `currentDatabase` as an explicit empty target; `list_databases` was also omitted from Query authorization classifiers and therefore classified as Write.
- Focused fix commits from coder worktree: `a67c3065a` adds `databaseDiscoveryTarget` with null namespace and uses it for `getDatabases`, and corrects the session-backend command classifier; `d2147f03a` corrects the runtime enqueue Query classifier. Combined candidate is now `2bb263fb4`.
- This fix is not accepted yet. Independent verifier must rebuild with `pnpm tauri:build:webdriver`, then rerun PostgreSQL/MySQL multi-database tree and query selector WDIO paths against `2bb263fb4`; record matching gate source fingerprints. No unit tests/Cargo tests/Vitest/computer-use testing are authorized.
- Before delivery, remove the diagnostic-only probe from tester's WDIO diff; review and retain only permanent safe journey assertions.

## P6 Workflow SQL target guard

- Code review found the SQL `Command` path (`query`/`execute`) entered `independent_sql::execute` without the same multi-database target requirement enforced by legacy Workflow `Query` steps.
- Coder commit `40749f978` moves the existing `database_is_required` policy to the shared independent SQL entry. Multi-database drivers now require a step, workflow, or connection database before opening the SQL resource; single-database drivers remain allowed. Explicit malformed namespace values continue to fail validation before this guard.
- This extends the earlier P6 Workflow SQL execution/cancellation/effect-diagnostic work. The commit is not yet part of the accepted candidate and has no compile or WDIO result; include it in the next isolated build and, where feasible, run Workflow WDIO against PostgreSQL/MySQL. No unit tests are authorized.

## Continued candidate state (2026-10-12)

- The candidate now includes the shared Workflow SQL target guard as commit `e6782a879` (cherry-picked from coder commit `40749f978`). It remains uncompiled and unaccepted pending the next matching WDIO build.
- Independent safe probes against fixed candidate `2bb263fb4` confirmed the database-discovery target failure is shared by both dialects: legacy `list_databases` returns 25 PG / 14 MySQL entries, while the all-null connection-scope `execute_at_target` call fails before a receipt. Both DOM trees remain at zero database nodes. Probe-only changes were removed; tester is running the frozen multi-database and query selector/Stop WDIO journeys.
- Coder is tracing and repairing the shared pre-receipt target submission path in its own worktree. Do not accept P4 until the database tree and target selector pass on both drivers. The candidate can advance independently; the verifier remains frozen on `2bb263fb4` for its current WDIO round.
