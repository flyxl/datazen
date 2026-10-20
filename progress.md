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
## NativeTool trusted deployment / product probe Job admission (2026-10-19, READY_FOR_TEST)

Isolated coder tree `/tmp/datazen-p6-native-product-jobs`, branch `codex/p6-native-product-jobs`, base cefdd2301e8de2e3f8ab9d5f944482a844991475. Workflow integration/tester trees untouched. Read repository AGENTS and consumer-adapters NativeTool/Job contracts, existing Settings/Artifact/IPC/identity/SQLite/Runtime assembly sources. No env files opened/read/sourced.

Implemented compile-time trusted deployment registry, default empty/deny-all. Manifest only from include_str build input, never writable Settings/IPC/PATH/env. Exact approved tool selection/binary SHA/version line + fixed reviewed versionProbeV1 bounds (1 process, 0 DB sockets); duplicate/unknown/unproven contracts refuse. Registry choices do not expose paths. Deployment digest freezes binary/expected-version/resource contract. No actual pg_dump/mysqldump version/binary is registered by this change, no real DB CLI is enabled or claimed supported; nonzero DB socket declarations explicitly unsupported.

Five product IPC commands choices/submit/get/list/cancel use host identity + ManageJob policy; strict request only selection+UUID idempotency key, no client path/options/env/cwd/credential/identity/cost/target. Durable SQLite projection now accepts tightly bounded typed nativeToolSelection ONLY for nativeToolProbe and requires it; selection+deployment digest are frozen. First committed admission alone dispatches in a detached worker independent of IPC waiter; idempotent retries return old Job without reconstructing handler. Read/cancel require owner/kind, list bounded 100. Startup marks native Jobs under actual desktop identity for verification without replay. Runtime instance is environment-wide and receives SAME physical coordinator as DesktopSessionBackend, published only after matching Runtime assembly succeeds; existing kernel process/socket admission/owner/cancel/cleanup/effects remain unchanged. Dispatch faults record fixed pending-verification reason. No raw CLI output or credential/path enters durable projection. Probe output uses existing byte counters/cleanup/effects, no Artifact export.

Static review only: rustfmt --edition 2021 --config skip_children=true on changed Rust files EXIT=0; git diff --check EXIT=0. Corrected a duplicate adapter accessor during static review before commit. No tests added/run, no cargo check/build/test, typecheck, webdriver/WDIO or live execution. Compile/type/link/IPC and behavior all require independent tester review; current boundary is an implementation handoff, NOT acceptance. No AI or Workflow model/executor/target/chaining/scheduler changes. Parent instructed tester scheduling: original Workflow tester finishes first, then NativeTool tester runs one at a time. No other subagent spawned.

Required independent checks: default empty allowlist no process/no admission; strict DTO unknown/private fields, unregistered/changed digest/bad bounds/unknown versions refused; shared-budget same-instance evidence; actual SQLite admission/projected plan/same-key/no replay/reopen; detached dispatch persists terminal receipt after caller disconnect; owner and kind read/cancel isolation; native cancellation and cleanup failures retain budgets and Unknown; restart only marks pending verification. No live approved binary/actual DB/socket count/Artifact export/NativeTool UI/PG-MySQL combined journey proof exists. Real DB-consuming operations remain Unsupported until per-version full bounds and complete production journey are defensible.

## NativeTool product Host/SQLite regression coverage and required digest (2026-10-19, READY_FOR_TEST)

New isolated coder tree `/tmp/datazen-p6-native-product-host-tests`, branch `codex/p6-native-product-host-tests`, base 9a3f3c27d862f1bbc55e1ff6cef37e5fec6e5d16. Product original tree, integration and tester trees unchanged. No tests or compile/build commands executed, per explicit instruction. No env contents read, no UI/real CLI/DB/Workflow/AI work, no protocol types changed.

Added four product-specific Host regressions in commands/native_tool_tests.rs, cfg(all(test, unix)), calling exact command-body helpers rather than generic job APIs alone. Uses actual AppState policy/identity, actual SQLite host/JobRuntime and the existing desktop native process adapter with cfg(test)-only SHA-pinned synthetic POSIX scripts; no pg_dump/mysqldump or DB connection. Scripts only append a fixture-owned marker and print an exact synthetic version. Production default embedded manifest remains empty and production registration source is unchanged.

Coverage authored, NOT executed or accepted:
1. Both PG/MySQL probe IDs: approved choice projection, actual submit to terminal Job, frozen nativeToolSelection + deployment digest, bounded successful domain receipt, separately opened SQLite store preserving exact payload, immediate and terminal same-key retries returning old Job, physical synthetic marker stays one invocation; changed approved deployment contract under same id/version conflicts on same key without another invocation.
2. Default/invalid deployment deterministic rejection before admission; nine invalid manifest shapes/contracts/bounds/hash/private fields/tool IDs/duplicate cases. Strict request+selection reject 16 private-field injections, bad UUID and unregistered version; no jobs or marker effects.
3. Product get/list/cancel enforce real SQLite principal ownership and native kind, not only a policy rejection; foreign/other-kind records stay queued and invisible/uncancellable, owned native queued Job cancels as NotStarted without spawn.
4. SQLite durable projection rejects thirteen missing/private/wrong-kind/invalid-selection/digest cases before admission, checks all kinds of jobs remain absent.

Tester follow-up found a real source-confirmed validator gap: native selection was required but planDigest only validated when present, and the common digest helper permits short tagged hex. Minimal production fix in jobs/codec.rs requires nativeToolProbe planDigest to exist and exactly match writer format sha256: + 64 lowercase hex; no other Job validator changes. Missing/null/numeric/empty/short/nonhex/uppercase digest regressions begin from a real freeze-generated valid payload. This enforces required canonical projection, not an independent registry attestation: product freeze remains the trusted source of digest consistency, and clients cannot submit planDigest.

Testability changes: thin IPC wrappers delegate unchanged signatures/DTOs to private command bodies; deployment from_build delegates to a private parser; test-only fixture manifest reconstruction reuses the SAME physical budget Arc and appears behind cfg(test), with adapter hook after production source (no structural scanner truncation). No production IPC/settings/env manifest override introduced.

Static only: targeted rustfmt --edition 2021 --config skip_children=true EXIT=0; git diff --check EXIT=0. NOT RUN cargo check/test/build, Host tests, Runtime tests, typecheck, WDIO, live CLI/DB or subprocess fixtures. Thus new test source may still have compile/runtime issues and must be independently verified by the original tester. Intended focused command is cargo test -p datazen --lib commands::native_tool::tests, run by tester only with required driver injection/isolation and safe /tmp log capture.

Exact changed files:
- src-tauri/src/commands/native_tool.rs
- src-tauri/src/commands/native_tool_tests.rs (new)
- src-tauri/src/platform/adapter.rs (cfg(test) hook only)
- src-tauri/src/platform/native_tool/deployment.rs
- src-tauri/src/store/app_db/jobs/codec.rs
- progress.md

## NativeTool product admission — independently accepted (2026-10-19)

Integrated after independent review from commit `65daf045ac2dcc2977f146233638797892c4419f` (parent `9a3f3c27d862f1bbc55e1ff6cef37e5fec6e5d16`), now `ae3a75479350186ff8fad491d630db3bc0967d67` on the P6 integration branch. Tester worktree was detached, clean, and held HEAD `65daf045ac2dcc2977f146233638797892c4419f` plus tree SHA `a6bded404d9021eb02090c31b914b31d6041c22b` before/after review.

- `cargo test -p datazen --lib commands::native_tool::tests`: EXIT=0, 4/4.
- `cargo test -p datazen --lib job_tests`: EXIT=0, 12/12.
- `cargo test -p datazen-runtime --lib native_tool::tests`: EXIT=0, 9/9.
- `cargo check -p datazen --lib`: EXIT=0, 77 warnings, no errors.
- `git diff --check` and `git diff --cached --check`: EXIT=0; NativeTool production paths contain zero `ConnectionManager` references.

Evidence covers exact PG/MySQL probe selection and frozen digest, invalid/default deployment refusal, actual Host/SQLite admission/reopen/idempotent retry without redispatch, ownership/kind isolation, projection validation, and NativeTool cleanup-budget retention. Synthetic executable only; no approved real CLI, real database, or DB socket operation ran. Real `pg_dump`/`mysqldump` jobs remain unsupported and are a P6 follow-up, not accepted by the probe scaffold.

## Workflow v1/block refusal — independent acceptance update (2026-10-19)

The earlier pending-test and corrected-assertion notes above are historical. The original Workflow tester accepted current integration commit `e87a966994c303aa4ea4039a4d1cd7a23f001d35`: `workflow_block_refusal` 1/1 (covers 12 cases with zero host calls), feature-core workflow 69/69, Host workflow 37/37, and v1 backward-compatibility/reuse journeys 1/1 each. The shared executor rejects unsupported blocks before host work. Legacy v1 target override and step-result chaining remain intact. No v2 protocol/model/YAML/IPC changes or new transaction semantics remain in this P6 slice.
## Table export bounded Runtime output slice (coder, pending independent review)

- Worktree `/tmp/datazen-p6-table-export-runtime`, branch `codex/p6-table-export-runtime`, base `e87a966994c303aa4ea4039a4d1cd7a23f001d35`. NativeTool and integration/tester trees untouched.
- Removed export data materialization through `RuntimeCommandSource::read_session_command` / `MultiQueryResult`; profile lookup remains a direct Runtime/store lookup. No RuntimeSchemaSource, ConnectionManager, AI, backup binding, public DTO, ExportSource, plan, format or IPC changes.
- New concrete Runtime host capability binds one dedicated tableExport session and one idempotent query submission to owner/client/auth/delegation, canonical target and configuration revision. A capacity-one output slot awaits producer delivery outside execution/global locks, counts the encoded batch (2 MiB), and rejects >256 rows. Driver PG/MySQL production streams already bound row decoding to 1 MiB/256 rows and await writes; other drivers are unsupported here.
- Host publisher is internally awaitable; ordinary artifact and backup publication remain their prior paths. Only an admitted live export capability bypasses the interactive query row limit. Bound execution rejects legacy synchronous publication and never falls back to a result artifact after revocation.
- Consumer revalidates session/profile/policy before batches, runs each formatter write in the blocking pool, and awaits it before receiving another. Cancellation watcher independently revokes capability and cancels the same execution; binding drop/invalidations wake pending sends. Cleanup uses existing Runtime close owner and requires ResourceRelease::Confirmed before Done. Caller abort starts cancel/close through a guard; no replacement execution or budget release is manufactured. An already executing synchronous file write cannot be forcibly interrupted; staging is never successfully published after cancellation or uncertain cleanup.
- Added source regressions: five Runtime binding tests (capacity/pending send/global lock accessibility, consumer drop/late rejection/no artifact, owner/client/target/revision + invalidation/Unknown, independent revoker, batch bounds); one real Runtime use-case fixture test for dedicated session/owner/target/revision/closed session rejection; three Host delivery tests (ordered metadata/rows, malformed/truncated/extra statement rejection, formatter panic). Existing Host export Runtime target test remains and fixture now emits statementColumns before rows.
- Static-only commands: scoped `rustfmt --edition 2021 ...` EXIT=0; `git diff --check` EXIT=0. No tests, cargo check/build, typecheck, WDIO or live database commands run, per parent instruction. These new test sources are **not yet compiled or accepted**.
- Tester acceptance still required: Runtime focused tests and existing Runtime journeys, Host delivery/export tests and Host compile; PG/MySQL actual large-table streaming beyond interactive cap, slow-consumer cancellation, producer/drain/cleanup faults and staging delivery. Runtime physical connection budget currently follows the existing ClientSession/fixed-session classification; this slice does not introduce a Job-specific resource class or new public owner port.


## Runtime P4 fixture correction after Workflow v1 reset (coder, pending review)

- Isolated worktree `/tmp/datazen-p6-runtime-v1-fixtures`, branch `codex/p6-runtime-v1-fixtures`, base export commit `b22bf28a72ab6ad61fa41b5afea5246e455206de`; no edits to export/tester/integration trees.
- Tester reports p4_usecase_journeys compilation fails on both e87 baseline and b22 export snapshot: three obsolete StageOwnerPermit::open_workflow_block references in fixtures/p4_workflow_job.rs. That production API was removed by the authorized v1-only Workflow scope reset.
- Removed only the obsolete Workflow block Job handler/run harness and its four tests: accepted_workflow_stage_retains_one_session_and_isolates_second_block; block_deadline_releases_session_without_gui_attachment; failed_job_stage_drops_and_releases_retained_resource; cancelled_job_between_enqueue_and_dispatch_rejects_sql_and_releases_resource. These asserted the retired block-owner lifecycle and cannot be reinterpreted as v1 execution evidence.
- Retained unchanged caller_owner_fields_cannot_authorize_job_or_block, including both caller-invented Job and WorkflowBlock owner rejection and zero backend opens. All other P4 connection/job/use-case tests, including export binding regressions, remain byte-for-byte unchanged. No protocol/model/production code changes and no replacement Workflow Job API.
- Scoped rustfmt --check and git diff --check EXIT=0. No test/compile commands run, per instruction. Independent tester must compile the whole p4_usecase_journeys target and verify remaining generic owner/connection/job contracts after export acceptance.
- Static gate snapshot before/after: HEAD `b22bf28a72ab6ad61fa41b5afea5246e455206de`, tracked diff SHA-256 `9bcf4ffbb9ac4796fae885f80c45ea01c79800cbac3d7d6c3de5de73423787c8` (before this evidence-only append). Logs `/tmp/datazen-p6-runtime-v1-fixtures-fmt.log`, `/tmp/datazen-p6-runtime-v1-fixtures-diff.log`; `RUSTFMT_EXIT=0`, `DIFF_CHECK_EXIT=0`. Source-text comparison: `RETAINED_NEGATIVE_TEST_IDENTICAL=true`; this is a static comparison, not a test run.

## Table export Runtime streaming — independently accepted (2026-10-19)

Coder commit `b22bf28a72ab6ad61fa41b5afea5246e455206de` is integrated as `b17604eeb` in the P6 branch. The original tester validated a detached, clean tree fixed at the coder commit; HEAD and tree fingerprint were stable.

- `cargo test -p datazen-runtime --lib application::export_output::tests`: EXIT=0, 5/5.
- `cargo test -p datazen --lib commands::export`: EXIT=0, 20/20.
- `cargo check -p datazen --lib`: EXIT=0, 77 warnings, no errors; this also compiles the Wapp host surface.
- Wapp production-source scan: zero `ConnectionManager` references.
- Source review verified bounded one-slot streaming/backpressure, execution-scoped owner/client/target/revision checks, cancellation of the same execution, late-output rejection, and no Done/file delivery after Unknown or unconfirmed cleanup.

No real PG/MySQL large-table export, slow-consumer cancellation, or staging cleanup live journey ran. Those remain explicit P6 acceptance work. The Runtime P4 integration target initially failed on three stale `open_workflow_block` calls; the same failure was reproduced on e87 base. After the separate fixture-only cleanup below, `cargo test -p datazen-runtime --test p4_usecase_journeys export_output` passed 1/1 and the complete target passed 18/18.

## Retired Workflow block-owner Runtime fixtures — independently accepted (2026-10-19)

Coder commit `fb9c1e160974030334dcede7e66abfc59cc743bb` is integrated as `b72a22ca6`. It removes only four P4 fixture cases for the removed Workflow block-owner API; the caller-owner rejection test remains unchanged and passes. Focused export fixture passed 1/1; full P4 journey target passed 18/18. No production, model, IPC or Workflow v1 behavior changed.

## Runtime migration continuation on the P6 integration branch (2026-10-20)

- All remaining migration work in this pass is on `feature/p6-restore-apply-evidence`; stale P6 implementation/test worktrees and branches were removed. The current target worktree is clean after the commit below.
- Integrated the accepted MCP schema-resource, Monitor registry, AI/Runtime, driver-command, schema-command, query, metadata-fixture, and initial Sync Runtime commits from the former main-side work.
- `c4e21b894` / `a4f4d0001` / `36cf60c83`: driver commands, schema commands, and query cancellation now borrow Runtime live leases. Focused evidence from the source commits: driver 40/40, schema 34/34, query 30/30, metadata 13 passed + 1 ignored; `cargo check -p datazen --lib` passed on the integration tree after integration.
- `f3bc71e5c` and `225260b90`: Data Sync Host/job sessions and migration endpoint identity now use Runtime-owned sessions and `LiveConnectionLease`; Schema Diff identity and Data Transfer endpoint identity call the Runtime owner/session boundary instead of `ConnectionManager`.
- Latest stable compile gate: `cargo check -p datazen --lib` EXIT=0 (83 warnings, no errors). Latest focused migration endpoint test: `cargo test -p datazen --lib services::migration_endpoint::tests` EXIT=0, 1/1.
- Remaining red evidence is explicit: `commands::schema_diff` was 17 passed/1 failed on the existing D1 legacy full-plan fixture (`postgres` schema-level driver requires an explicit schema); Sync contract tests still have Runtime fixture/owned-session failures and are not accepted. `ConnectionManager` remains in AppState, connection lifecycle IPC, Data Transfer/Sync legacy paths, feature ports, workflow migration, and its own tests; it is not safe to delete yet.
## Monitor legacy registry retirement (coder, pending independent review)

- Isolated worktree `/tmp/datazen-p6-monitor-registry-retirement`, branch `codex/p6-monitor-registry-retirement`, base `30b94f1bc9e97ac97a2144bd48a95c9020af69c0`. Export/tester/integration trees untouched.
- Confirmed all references before editing: production constructed/stored/copied MonitorConnectionRegistry only in bootstrap AppState and embedded MCP AppState copying; context.rs had two test-only AppState constructors. get/connect/disconnect and the key helper were used only by the registry's own tests. No production scheduler/manual consumer called the registry.
- Deleted unused monitor/connections.rs (441 lines, including tests of the deleted implementation), module/re-export, AppState field and all assembly/copy references. ConnectionManager itself and non-Monitor consumers remain unchanged.
- Added focused Host regression source `commands::mcp::tests::cloned_app_state_shares_the_active_monitor_engine`, verifying the embedded MCP AppState copy still shares the active MonitorEngine, Store and existing ConnectionManager for unrelated consumers. Existing context AppState literals provide compile coverage for the removed field. No new abstraction or protocol.
- Active monitor engine, scheduled lane, grants, occurrence admission/recovery, dashboard manual execution and Workflow code are byte-for-byte unchanged. Updated architecture facts for the retired registry only.
- Static-only scoped rustfmt and git diff --check: EXIT=0. No tests, compile/build, WDIO or live database commands run, per instruction. New regression source remains uncompiled and unaccepted; tester should compile Host and run the targeted clone regression plus existing Monitor startup/scheduled/manual coverage.
- Static gate snapshot: HEAD before/after `30b94f1bc9e97ac97a2144bd48a95c9020af69c0`; full tracked diff SHA-256 before/after `2cbf4cbbffcef035c6f2ba48ec9f926bffc4f878adb831bec0cecd076902ac1c` (before this evidence-only ledger append). Logs `/tmp/datazen-p6-monitor-registry-rustfmt.log` and `/tmp/datazen-p6-monitor-registry-diffcheck.log`; `RUSTFMT_EXIT=0`, `DIFF_CHECK_EXIT=0`. Product/docs diff fingerprint excluding progress.md is `ded0a023784ac7747b5e873ae615f69610fe06d212419c8ef6060cd923f39bfb`. Source scan found no remaining legacy registry symbols; zero diffs in engine/scheduled/grants/dashboard/workflow/AI/three migration command trees.

### Monitor legacy registry retirement — focused acceptance

- Community setup used `pnpm install --frozen-lockfile --offline` (EXIT=0), which regenerated ignored driver/edition files required by the fresh worktree. No `.env` or `.env.test` contents were read.
- `cargo check -p datazen --lib` EXIT=0 (78 existing warnings, no errors). The initial pre-setup attempt failed only because the fresh worktree lacked generated Community resources; no source error was reported.
- `cargo test -p datazen --lib commands::mcp::tests` EXIT=0: 7 passed, 0 failed. This includes `cloned_app_state_shares_the_active_monitor_engine`, proving MCP AppState copies retain the same MonitorEngine, Store, and ConnectionManager Arcs.
- `cargo test -p datazen --lib monitor::` EXIT=0: 5 passed, 0 failed, including startup-without-ambient-runtime, grant validation, scheduled timer admission/dispatch, and occurrence idempotency.
- `cargo test -p datazen --lib commands::context::tests::test_resolve_context_dir` EXIT=0: 2 passed, 0 failed. These AppState literals compile without the retired registry field.
- Final production scan for `MonitorConnectionRegistry`, `monitor_connections`, `monitor/connections`, and `connection_registry` returned no matches; `git diff --check` EXIT=0. No scheduler, grants, workflow, dashboard, or Runtime behavior changes were needed after focused validation.

## Runtime migration continuation — 2026-10-20 follow-up

- `919cfa37c`: Sync inspect/prepare/validate/apply paths now resolve live driver/handle/config through Runtime leases. The Sync jobs contract gate passed 7/7 active tests; the non-database target case remains ignored because the test harness has no stable Redis Runtime resource fixture.
- `9c671ea98`: Sync history metadata now reads stable `EndpointIdentity.connection_id` from the process-local plan instead of asking `ConnectionManager` to recover owners from runtime session ids. `cargo check -p datazen --lib` EXIT=0; opaque-plan focused test 1/1.
- `0d66b1aa9`: Sync task conflict checks now open task-local Runtime sessions and close them through Runtime. `commands::sync::tests::check_sync_conflicts` passed 4/4; `cargo check -p datazen --lib` EXIT=0.
- `68c8bd122` and `ef46262e1`: Schema Diff unified planning, source snapshot revalidation, target-object recovery and fingerprinting now borrow Runtime live sessions. Schema Diff focused suite remains 17 passed/1 failed on the existing D1 fixture requiring an explicit PostgreSQL schema; compile gate EXIT=0.
- `99b7542f4`: Data Transfer SQL-file apply assembly now resolves source config through Runtime. `cargo check -p datazen --lib` EXIT=0; `a_plan_id_serves_at_most_one_apply_job` passed 1/1.
- Main worktree audit: root `main` is clean with no staged, unstaged, or untracked changes. The P6 differences are committed only on the feature worktree; no main-local change is safe/necessary to drop.
- `6cf7cfd3d` and `203a84f36`: Data Transfer inspect/preview/execute/job assembly now use Runtime session contexts; stale tests were updated rather than reverting production code. The focused suite is `100 passed; 0 failed`; the read-only policy mutation test now asserts Runtime's fail-closed `connection configuration changed` behavior, and the MySQL fixture registers a Runtime factory.
- `27dfa7748`: Workflow migration now uses Runtime dedicated/live sessions while preserving Workflow v1 semantics. Focused migration tests: 4/4; compile EXIT=0.
- `873df2942`: feature-port session config lookup uses Runtime session view plus Store profile. Compile EXIT=0; one old connection-command fixture failure was later addressed in the connection lifecycle slice.
- `8a209d974` and `eee33c961`: connection lifecycle session operations and standalone connection/tunnel probes now use Runtime or direct Store/DriverRegistry/tunnel helpers. Connection focused tests: 8/8; tunnel focused suites: 13/13 and 18/18. Probe coverage attempts remain blocked by unrelated workspace test compile errors.
- `a0afe5be4` and `a4134a705`: query transaction begin/commit/rollback and Sync test execution helper now borrow Runtime live sessions. Query focused tests: 28 passed, 2 ignored because the old ConnectionManager-specific lost-begin-race fixture cannot represent Runtime public session ids; Sync exec focused tests: 6/6.
- `5b867b6c9`: remaining Schema Diff legacy production paths now use Runtime sessions; the D1 fixture was updated with explicit `public` schema. Schema Diff focused suite: 18/18.
- Coverage evidence: `cargo llvm-cov` has not produced a report. Attempts are blocked before test execution by unrelated workspace test compilation errors (`platform-api` artifact `target_fingerprint`, and separately MySQL `ResourceExecutionHost::execution_finished`). Do not claim the >=80% threshold until a scoped coverage build is made possible.
