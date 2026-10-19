# P6 integration temporary ledger

This development ledger is temporary and must be deleted before the P6 branch is merged to main.

Integration branch: `feature/p6-restore-apply-evidence`.

## Accepted and integrated

- Backup retained Runtime implementation and repairs: `b395562ee`, `b9b0e3cb3`. Independent PostgreSQL/MySQL live backup journeys, Renderer tests and compile checks passed.
- Restore Apply: PG/MySQL Host boundary 2/2 and driver live journeys 1/1 each passed on `b9b0e3cb3`.
- Wapp stateful instance sessions: coder commit `b5f94785a`, tester accepted; integrated as `a9c76752a`. Host 2/2, bridge 54/54, SDK 69/69, typecheck equivalents passed. Live PG/MySQL Wapp SQL and approval UI remain untested.
- Monitor/Workflow scheduler startup reactor fix: coder commit `d3236b4af`, tester accepted; integrated as `7c2902495`. Startup tests 2/2 and PostgreSQL Workflow WDIO journey 1 passing.
- NativeTool fail-closed kernel: coder commit `2320a3da6`, independent tester accepted. Runtime 6/6, Host 5/5, platform-api/runtime cargo checks and boundary guard passed. Desktop only permits hash-pinned `pg_dump`/`mysqldump --version` probes with zero DB sockets; real database CLI operations, backup/restore and product IPC/UI remain unsupported.
- MCP cancelled-open ownership repair: coder commit `26f93717b`, independently accepted and integrated as `daae66596`. `cargo test -p datazen --lib mcp::session`: EXIT=0, 17 passed, including cancel/late-acquisition cleanup, disconnect race, and delivered open/idempotent close. `mcp::server::tests`: EXIT=101, 48 passed/2 failed on both this commit and base `1d47a8f75`; identical existing `Runtime session unavailable` failures in prompt/resource tests. MCP tester trees/logs were cleaned after stable HEAD/fingerprint checks.
- Monitor scheduled desktop PG/MySQL journey: ancestry `d2de58eae` → `25ab384c5` → `8f8b0ee15` → `d439c375c`, integrated as `6a6e8de28`, `a038e13d5`, `80000e3d6`, `3e501172b`. Earlier runs exposed missing required Workflow `description`, cleanup masking, then production occurrence admission rejection due to unprefixed SHA-256 format. Final coder fix accepts exact 64-hex production revision while retaining tagged compatibility and adds bounded stage diagnostics. Independent `pnpm install --frozen-lockfile --offline`, `pnpm tauri:build:webdriver`, PG WDIO and MySQL WDIO all EXIT=0. Each driver journey reported 1 passing, traversed receipt, marker `73129`, duplicate-slot no-dispatch, revoke and a 62-second physical-dispatch observation. Tester HEAD `d439c375c3ece73391eea1c9146fe324bd96bd68` and fingerprint `53a6f114b805d9efbb07ec818fabe8d5c6f982aa6be05488d756038139417c09` stable.
- MySQL 8.4.7 privilege-cache probe: coder commit `1d70d9220`, independently accepted earlier as `e7480c68e`; three transaction-plan refusal tests and two probe identity/name tests passed, and the explicitly disabled admin gate failed closed before configuration read/connect. This was transactionBlock-specific and has now been removed from current integration by `fdc93ea7c` per the v1-only Workflow scope reset.
- NativeTool cleanup panic budget correction: coder commit `b39cc7510`, independently accepted and integrated as `76d35a31f`. Real JobRuntime + controlled cleanup-fault journey 1/1 and full NativeTool unit suite 7/7 passed; process permit and two-socket budget remain reserved through observer panic, no second Job is spawned or replayed, and budgets release only after cleanup confirmation. Scoped rustfmt and `git diff --check` passed; package-wide fmt still reports an unrelated, unchanged module-order issue in `packages/runtime/src/lib.rs:23`. No CLI or database operation ran.
- NativeTool abandoned-caller/cancel cleanup budget correction: coder commit `95e471bc2`, independently accepted and integrated in this cherry-pick. `cargo test -p datazen-runtime --lib native_tool::tests`: EXIT=0, 9/9. Real JobRuntime + synthetic NativeToolPort verifies caller abort, Job cancel intent, Unknown/Cancelled/Unconfirmed receipt, no replay, process/socket budget retention through uncertain cleanup and release only after late confirmation. Not a real CLI/DB journey.

## In progress

- **Scope reset (user instruction, 2026-10-19): Workflow stays on its existing v1 protocol and step behavior; do not add YAML/IPC/model changes.** P6 branch v2 sessionBlock/transactionBlock, whole-definition admission, Job owner, and dialect proof work is out of scope and must be removed from the integration result. The main baseline already routes Workflow SQL steps through `command_runtime → independent_sql → feature-core resource_execution → Runtime`; preserve this underlying Runtime connection path and the existing previous-step result/template handoff.
- The unintegrated MySQL transaction-proof line (`55121d874`, `9b4d5c558`, `c1e744f3b`) is out of scope after the Workflow scope reset and must not be cherry-picked. Its independent tester passed c1's offline tests, but this is not a P6 completion item under the v1-only Workflow contract.
- The next NativeTool slice (trusted deployment registration/product Job admission) was interrupted by spend cap; resume the same coder when available.

## Remaining P6 scope

- MCP owner/TTL cancellation fault path is implemented and accepted as above; external MCP transport/PG/MySQL live integration remains outside this specific acceptance.
- Workflow scope is now v1-only; after removing the v2 additions, verify its existing cross-database steps and previous-result/template handoff still use the Runtime connection path. No transactionBlock proof is required for this P6.
- NativeTool cleanup-panic and abandoned-caller budget fixes are independently accepted. The next trusted deployment-registration/product Job-admission coder slice must resume with the original coder after quota recovery. P6 still lacks full-budget and applicable-layer acceptance, product IPC/UI, registered real PG/MySQL tool deployment, and real database/native backup/restore journeys; the current desktop surface is version probes only.
- Wapp stateful instance sessions are accepted at Host/bridge/SDK layers, but live PostgreSQL/MySQL Wapp SQL and approval UI journeys remain untested; the original Wapp coder is spend-capped, so reuse that agent when available.
- MCP cancelled-open ownership is accepted at Host Runtime fixture level; external MCP transport and PostgreSQL/MySQL live shared-state journeys remain unverified.
- Explicit AI editor-session grants/revocation are tracked by the separate AI migration owner; do not modify AI here.

P6 is not closed. Remove this ledger during final acceptance cleanup.


## P6 MCP cancelled-open ownership — independently accepted

Implementation branch `codex/p6-mcp-session-cleanup`, isolated worktree `/tmp/datazen-p6-mcp-session-cleanup`, integration base `1d47a8f75a9ac1910575c6f99b6918641c4d42d0`. Integration/main and tester worktrees were not modified; no subagent or tester was spawned.

### Evidence audit before implementation

Reviewed backend MCP architecture §1.4 and development-plan P6 (§10), then CodeGraph (root index lacks the latest cleanup file/new branch symbols, so current isolated source was also inspected). Existing implemented coverage is concrete: `session.rs::lookup` enforces owner and expiry before use, `server.rs::request_context` constructs backend-generated client identity, `release_sessions` revokes only that client, and `session_cleanup.rs::start_session_cleanup` retains original owner/handle for Pending/Quarantined/errors until Confirmed. Existing Runtime-backed tests already cover independent TTL timer, expired lookup, active transaction disconnect rollback, other-client isolation, budget release, unknown release retention and single worker. No replacement implementation for those paths was needed.

One uncovered P6 fault path was confirmed: old `tools.rs::open_session` directly awaited Runtime acquisition and registered `SessionBinding` only after that await. Dropping/cancelling the tool future could lose the adapter continuation while Runtime/actor work completed later; no binding/cleanup owner or TTL worker could then own that result. The existing disconnected check only helped if the original tool future continued after acquisition. No aborted-open regression covered this path.

### Implemented facts

`tools.rs::open_session` now calls `session_open.rs::complete_session_open` (current tools line 257). One independent task owns the complete, original Runtime open future; dropping the request waiter detaches that task rather than cancelling acquisition. The task registers the real returned Runtime handle under the same backend-owned client and starts the existing TTL worker. Its reply carries an internal `UndeliveredSession` RAII guard: if the output is discarded or a disconnected client cannot consume it, Drop revokes the binding and starts existing original-owner RollbackAndClose cleanup. Successful delivery disarms the guard, preserving ordinary TTL/use/close behavior. No application SQL, acquisition or unknown-effect operation is retried. No client-supplied owner/handle was introduced, and no executor/driver behavior was copied.

New controlled Host Runtime fixture tests (in `session_open.rs`) cover request abort followed by successful late acquisition without disconnect, disconnect during acquisition with late reply rejection, physical registry and budget cleanup, and normal production open delivery with idempotent close. Tests retain their TempDir/keyring guards through completion. They call actual Runtime/Host session APIs backed by the existing mock ResourceProvider; they are not PG/MySQL live or MCP stdio transport acceptance. Architecture/backend/mcp.md and the existing platform development plan now state the implemented cancellation ownership boundary and this evidence limitation.

### Final stable coder gates

Before/after HEAD: `1d47a8f75a9ac1910575c6f99b6918641c4d42d0`; non-ignored worktree SHA256 before/after all final checks and isolated failure reruns: `05e8a2fe2b1818b1a7036379d9d9d5d0a7278a9267fc3ea6b1da5a1e91b2c4b8`. Fingerprints `/tmp/datazen-p6-mcp-final-{before,after}.txt` and `/tmp/datazen-p6-mcp-isolated-{before,after}.txt`. Only this ledger was appended after these gates.

- `cargo test -p datazen --lib mcp::session`: EXIT=0; `test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 1399 filtered out; finished in 0.26s`. Includes all three new tests plus registry/TTL/cleanup regressions. Log `/tmp/datazen-p6-mcp-final-session-tests.log`.
- `cargo test -p datazen --lib mcp::server::tests`: EXIT=101; `test result: FAILED. 48 passed; 2 failed; 0 ignored; 0 measured; 1366 filtered out; finished in 0.67s`. Failures are `mcp_prompt_handlers_build_messages` (tests_integration.rs:157) and `mcp_read_resource_inner_paths` (:272), both `Runtime session is unavailable` in schema-resource/prompt paths. The independent tester reran the same command at base `1d47a8f75`; it produced the same 48/2 result with exactly the same failures. The two failures are confirmed base failures outside the changed stateful open path, though that server test group remains red. Tester logs and trees were cleaned after stable checks.
- Isolated reruns `cargo test -p datazen --lib mcp_prompt_handlers_build_messages` and `cargo test -p datazen --lib mcp_read_resource_inner_paths`: each EXIT=101, `0 passed; 1 failed`, same Runtime error. This rules out suite-only concurrency as the immediate trigger. Logs `/tmp/datazen-p6-mcp-isolated-{prompt,resource}.log`. Neither assertion nor those test files were altered to obtain green results.
- Targeted `rustfmt --edition 2021 --check` for changed Rust files: EXIT=0; log `/tmp/datazen-p6-mcp-rustfmt.log`.
- `git diff --check`: EXIT=0.

Initial compile EXIT=101 for an incorrect ApiError import; corrected to application::error::ApiError. One intermediate compilation was deliberately interrupted (EXIT=130) while improving fixture lifetime/registry assertions and documentation; it is not counted as stable evidence. Final snapshot above is stable and authoritative. Normal ignored driver/pro codegen was used, never stale restore. Full logs are in system /tmp; no .env/.env.test contents were read.

### Remaining limits

Independent tester accepted this cancellation/TTL ownership repair at `26f93717b` and the integration cherry-pick is `daae66596`. Evidence covers Host Runtime-backed cancellation, disconnect race, original-owner cleanup, budget return and idempotent close; it does not claim external MCP stdio/client or PostgreSQL/MySQL SQL acceptance. The same two schema-resource/prompt tests fail on baseline and this commit. No AI/provider/driver implementation was changed. Delete this temporary ledger on final integration to main.


## Workflow v1 scope reset (2026-10-19, pending independent acceptance)

User direction supersedes prior P6 v2 Workflow work: existing v1 protocol/model, per-step cross-database targets, output/template chaining stay unchanged; replace only underlying connection management. Completed primary agent's pre-existing staged/unstaged rollback in integration worktree `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-p6-restore-apply-evidence`, branch `feature/p6-restore-apply-evidence`, original HEAD `5313c362055d37e68a6892ffdf87942270f48245`. No main/other tester trees modified.

Removed P6-only Workflow session/transaction execution, dedicated Workflow Job dispatch/IPC/owner/TTL binding and PG/MySQL transaction-proof API/implementations/tests. Kept baseline Workflow model/executable/YAML/version and existing IPC signatures. Core Workflow directory and host command_runtime/independent_sql are byte-identical to current main after scope reset. Kept scheduler runtime-entry repair, Monitor durable scheduling/authorization/recovery, Wapp/MCP and Restore/Backup/Export. Monitor's fixed flat read-target metadata normalization was extracted from removed session_block into monitor/fixed_targets; scheduled host no longer implements removed WorkflowHost::resources. Generic NativeTool StageOwnerPermit/claim fencing and native process/runtime implementations remain. Existing transfer/schema-diff test literals only remove the deleted FrozenPlan field; no tests were added.

Static work only, no implementation test/compile results claimed:
- rustfmt --edition 2021 --config skip_children=true applied to modified Rust integration/glue files; EXIT=0. This parsed/formatted source, not a cargo type/link check. Baseline core Workflow files remain exactly as main.
- git diff --check: EXIT=0. Residual deleted-symbol scan found no production/test references to workflow_transaction, WorkflowTransactionPlan, JobSessionLease, workflowOwnerBlocks, workflow_owner_blocks, validate_workflow_owner, begin_job_owned_call, bind_job_permit, verify_session_job, authorize_workflow_job, workflow_list_jobs/workflow_cancel_job or removed session/transaction helper modules.
- No changes to model.rs/executable.rs/YAML/IPC version contracts, AI code, scheduler.rs, NativeTool kernel/desktop adapter, Wapp/MCP, Restore/Backup/Export relative to original HEAD. Review patch/stat/file manifest stored under `/tmp/datazen-p6-workflow-v1-rollback*` (no secrets/env contents read).
- NOT RUN: cargo check/test, driver tests/live suites, pnpm typecheck/build, Vitest, WDIO or other automated gates. Compile/type/link confidence remains unverified. Monitor helper uses the existing baseline TargetDeclaration/target_plan contract; this dependency extraction particularly needs tester review. Historical completed v2 proof reports above are superseded scope history, not current deliverables or acceptance.

Independent acceptance arrangement (not executed here): first focused Host/feature-core/Runtime compilation and existing v1 tests; PG and MySQL driver-local Workflow timeout suites plus v1 cross-DB journey with explicit PG→MySQL and MySQL→PG per-step target changes, first-step scalar/row output consumed by subsequent SQL/condition/foreach templates, legitimate defaults/overrides, failure/timeout without replay of earlier side effects; then existing Monitor timer/authorize/revoke/duplicate occurrence journeys to confirm helper extraction preserves grant scope and scheduling. Keep tester checkout isolated and use webdriver driver injection build. No v2/block/proof acceptance required or claimed. No NativeTool next development until this scope reset is independently reviewed.

Exact changed files at review (D=deleted, M=modified, A=added):
```
A	src-tauri/src/monitor/fixed_targets.rs
D	packages/driver-api/src/workflow_transaction.rs
D	packages/drivers/mysql/src/workflow_transaction_tests.rs
D	packages/drivers/postgres/e2e/workflow-transaction-journey.ts
D	packages/drivers/postgres/src/workflow_transaction.rs
D	packages/drivers/postgres/tests/workflow_transaction_evidence.rs
D	packages/feature-core/src/workflow/session_block.rs
D	packages/feature-core/src/workflow/transaction_block.rs
D	packages/feature-core/tests/workflow_block_acquisition.rs
D	packages/feature-core/tests/workflow_v2_dispatch.rs
D	packages/runtime/src/job/owner_tests.rs
D	src-tauri/src/bootstrap/workflow_job_tests.rs
D	src-tauri/src/workflow/job.rs
D	src-tauri/src/workflow/job_owner_tests.rs
D	src-tauri/src/workflow/transaction_evidence.rs
D	src-tauri/src/workflow/transaction_proof.rs
M	docs/architecture/backend/workflow.md
M	docs/architecture/platform/consumer-adapters.md
M	docs/architecture/platform/feature-core.md
M	docs/architecture/platform/workflow-resource-model.md
M	docs/development/platform-development-plan.md
M	packages/data-transfer/src/job/tests.rs
M	packages/data-transfer/src/job/tests/stage_shape.rs
M	packages/driver-api/src/lib.rs
M	packages/driver-api/src/traits.rs
M	packages/drivers/mysql/src/lib.rs
M	packages/drivers/mysql/src/mysql.rs
M	packages/drivers/postgres/src/lib.rs
M	packages/drivers/postgres/src/postgres.rs
M	packages/feature-core/src/workflow/executor.rs
M	packages/feature-core/src/workflow/host.rs
M	packages/feature-core/src/workflow/mod.rs
M	packages/feature-core/src/workflow/resource_execution.rs
M	packages/feature-core/src/workflow/target_plan.rs
M	packages/runtime/src/application/mod.rs
M	packages/runtime/src/application/operations.rs
M	packages/runtime/src/job/memory_runtime_repository.rs
M	packages/runtime/src/job/mod.rs
M	packages/runtime/src/job/owner.rs
M	packages/runtime/src/job/plan.rs
M	packages/runtime/src/job/repository/owner_claims.rs
M	packages/runtime/src/job/runtime_repository.rs
M	packages/schema-diff/tests/job_handler.rs
M	progress.md
M	src-tauri/src/bootstrap/app_state.rs
M	src-tauri/src/bootstrap/run.rs
M	src-tauri/src/commands/workflow.rs
M	src-tauri/src/monitor/grants.rs
M	src-tauri/src/monitor/mod.rs
M	src-tauri/src/monitor/scheduled.rs
M	src-tauri/src/platform/adapter.rs
M	src-tauri/src/store/app_db/jobs/codec.rs
M	src-tauri/src/store/app_db/jobs/owner.rs
M	src-tauri/src/store/app_db/jobs/runtime_ops.rs
M	src-tauri/src/workflow/host.rs
M	src-tauri/src/workflow/mod.rs
```

Staging note: explicitly re-adding paths already deleted from the index produced a pathspec rejection. Reconciled by staging remaining tracked changes and the one new Monitor helper, then amending the just-created local commit so the final reviewed commit contains both original rollback and dependency fixes; no files were restored or lost.

## Workflow block zero-step success refusal (2026-10-19, pending tester)

Confirmed tester P2 after v1 scope reset: baseline model/YAML still accepts nonempty blocks while v1 executor would otherwise ignore them and report success (or dispatch an independent prefix). Smallest production change is an 8-line guard at the start of WorkflowExecutor::execute_with_options, before variables/steps/host work: any nonempty blocks returns existing CapabilityUnsupported IPC string, regardless of absent/1/2 version. No model/YAML/version/IPC type changes and no v1 independent execution or output/template chaining body changes.

Added one focused driver-neutral feature-core regression, `packages/feature-core/tests/workflow_block_refusal.rs`, calling the actual executor across 12 combinations: absent/1/2 versions, session/transaction block, empty/present independent prefix. It expects CapabilityUnsupported and zero host calls, covering both false empty success and unwanted prefix effects. No existing test removed or assertion weakened. This test was NOT run/compiled; independent tester must verify it and legitimate v1 PG/MySQL output/target chaining. The core directory now differs from main solely by this execution guard (and new regression), intentionally superseding prior exact-main static comparison.

Only rustfmt --edition 2021 --config skip_children=true for the two changed/new Rust files (EXIT=0) and git diff --check (EXIT=0). No cargo check/test/build, typecheck, WDIO or live DB operation. No env contents read. Integration tree only, base cefdd2301e8de2e3f8ab9d5f944482a844991475. NativeTool branch/tree remains clean at 9a3f3c27d862f1bbc55e1ff6cef37e5fec6e5d16, untouched. No AI implementation changes. No compile or runtime acceptance claimed.


## Workflow block refusal regression Display assertion correction (2026-10-19)

Original tester reported b697 regression stopped at its first combination because the assertion checked Rust variant spelling CapabilityUnsupported, while existing WorkflowError Display/IPC is `Capability unsupported: {detail}`. Corrected only the assertion to require that existing stable prefix. Production guard/model/protocol/output chaining remain unchanged. Earlier regression coverage describes authored cases, not completed passes: tester had not reached the remaining 11 combinations. No tests/compile rerun by coder; original tester must rerun all 12 combinations. rustfmt and git diff --check EXIT=0. NativeTool 65daf045ac2dcc2977f146233638797892c4419f branch/tree remains untouched and clean.
