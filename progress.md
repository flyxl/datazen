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
- MySQL 8.4.7 privilege-cache probe: coder commit `1d70d9220`, independently accepted and integrated as `e7480c68e`; three transaction-plan refusal tests and two probe identity/name tests passed, and the explicitly disabled admin gate failed closed before configuration read/connect. This offline probe was transactionBlock-specific; per the later v1-only Workflow scope reset, remove it from final integration.
- NativeTool cleanup panic budget correction: coder commit `b39cc7510`, independently accepted and integrated as `76d35a31f`. Real JobRuntime + controlled cleanup-fault journey 1/1 and full NativeTool unit suite 7/7 passed; process permit and two-socket budget remain reserved through observer panic, no second Job is spawned or replayed, and budgets release only after cleanup confirmation. Scoped rustfmt and `git diff --check` passed; package-wide fmt still reports an unrelated, unchanged module-order issue in `packages/runtime/src/lib.rs:23`. No CLI or database operation ran.

## In progress

- **Scope reset (user instruction, 2026-10-19): Workflow stays on its existing v1 protocol and step behavior; do not add YAML/IPC/model changes.** P6 branch v2 sessionBlock/transactionBlock, whole-definition admission, Job owner, and dialect proof work is out of scope and must be removed from the integration result. The main baseline already routes Workflow SQL steps through `command_runtime → independent_sql → feature-core resource_execution → Runtime`; preserve this underlying Runtime connection path and the existing previous-step result/template handoff.
- The unintegrated MySQL transaction-proof line (`55121d874`, `9b4d5c558`, `c1e744f3b`) is out of scope after the Workflow scope reset and must not be cherry-picked. Its independent tester passed c1's offline tests, but this is not a P6 completion item under the v1-only Workflow contract. The integrated MySQL counterexample probe `e7480c68e` is transactionBlock-specific and must be reverted/removed from final integration.
- NativeTool caller-abort/cancel/late-cleanup budget fault tests are ready for independent verification at coder commit `95e471bc2`. Nine NativeTool unit tests passed in coder self-check; cancellation retains original Unknown/Cancelled receipt and process/socket budgets until late cleanup confirms, without replay. This is a synthetic Runtime/port fault journey, not real CLI/DB acceptance. The next coder slice (trusted deployment registration/product Job admission) was interrupted by spend cap; resume the same coder when available.

## Remaining P6 scope

- MCP owner/TTL cancellation fault path is implemented and accepted as above; external MCP transport/PG/MySQL live integration remains outside this specific acceptance.
- Workflow scope is now v1-only; after removing the v2 additions, verify its existing cross-database steps and previous-result/template handoff still use the Runtime connection path. No transactionBlock proof is required for this P6.
- NativeTool's cleanup-panic budget fix is independently accepted; caller-abort/cancel/late-cleanup fault tests await the original tester after its current scope review. The next trusted deployment-registration/product Job-admission coder slice must resume with the original coder after quota recovery. P6 still lacks full-budget and applicable-layer acceptance, product IPC/UI, registered real PG/MySQL tool deployment, and real database/native backup/restore journeys; the current desktop surface is version probes only.
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
