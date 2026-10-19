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

## In progress

- Monitor scheduled desktop PG/MySQL journey: `d2de58eae` adds webdriver-only scheduled tick/evidence projection and physical dispatch counts; `25ab384c5` fixes MySQL fixture provisioning; `8f8b0ee15` fixes a missing Workflow `description` fixture and preserves primary failures through cleanup. Those first PG/MySQL runs reached authorization but timed out waiting for a real production timer receipt. Coder fixed a production hash-format mismatch in `d439c375c` and added bounded stage diagnostics. Independent WDIO build, PG and MySQL all EXIT=0; each live journey reported 1 passing and traversed receipt, marker, duplicate-slot no-dispatch, revoke, and the 62-second physical-dispatch observation. Tester HEAD/fingerprint stable. **READY_FOR_INTEGRATION**; cherry-pick the ancestry `d2de58eae` → `25ab384c5` → `8f8b0ee15` → `d439c375c`.

## Remaining P6 scope

- Integrate the independently accepted Monitor desktop scheduled journey (`d439c375c` and its ancestry).
- MCP owner/TTL cancellation fault path is implemented and accepted as above; external MCP transport/PG/MySQL live integration remains outside this specific acceptance.
- MySQL transactionBlock remains fail-closed pending a defensible proof. Driver-local security probe `1d70d9220` is in preparation and does not enable the production path.
- NativeTool database operations and full product IPC/UI remain unsupported; the accepted implementation is a safe kernel/probe slice only.
- Workflow Job handler exposes durable state and bounded result projection, but no dedicated Workflow Job control UI or cross-restart Workflow receipt exists; determine whether these are required P6 exit items or documented follow-up before closure.
- Explicit AI editor-session grants/revocation are tracked by the separate AI migration owner; do not modify AI here.

P6 is not closed. Remove this ledger during final acceptance cleanup.


## P6 MCP cancelled-open ownership — READY_FOR_TEST

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
- `cargo test -p datazen --lib mcp::server::tests`: EXIT=101; `test result: FAILED. 48 passed; 2 failed; 0 ignored; 0 measured; 1366 filtered out; finished in 0.67s`. Failures are `mcp_prompt_handlers_build_messages` (tests_integration.rs:157) and `mcp_read_resource_inner_paths` (:272), both `Runtime session is unavailable` in schema-resource/prompt paths. These tests do not invoke the changed stateful open path. No baseline checkout was run, so this is **not** a claim that the failures are proven pre-existing. Log `/tmp/datazen-p6-mcp-final-server-tests.log`.
- Isolated reruns `cargo test -p datazen --lib mcp_prompt_handlers_build_messages` and `cargo test -p datazen --lib mcp_read_resource_inner_paths`: each EXIT=101, `0 passed; 1 failed`, same Runtime error. This rules out suite-only concurrency as the immediate trigger. Logs `/tmp/datazen-p6-mcp-isolated-{prompt,resource}.log`. Neither assertion nor those test files were altered to obtain green results.
- Targeted `rustfmt --edition 2021 --check` for changed Rust files: EXIT=0; log `/tmp/datazen-p6-mcp-rustfmt.log`.
- `git diff --check`: EXIT=0.

Initial compile EXIT=101 for an incorrect ApiError import; corrected to application::error::ApiError. One intermediate compilation was deliberately interrupted (EXIT=130) while improving fixture lifetime/registry assertions and documentation; it is not counted as stable evidence. Final snapshot above is stable and authoritative. Normal ignored driver/pro codegen was used, never stale restore. Full logs are in system /tmp; no .env/.env.test contents were read.

### Remaining limits

This is coder self-check, **not independent tester acceptance** and not P6 closure. Two broader schema-resource/prompt tests remain red and need separate triage; production schema resource/AI functionality was not changed by this targeted ownership repair. No external MCP Client/AI integration, driver implementation, Wapp, Monitor or integration/main files were modified. No real PostgreSQL/MySQL SQL, external DB fixture, GUI, stdio process, WDIO or webdriver build was executed in this turn. A tester must independently review cancellation/TTL ownership and relevant PG/MySQL/transport evidence. Delete this temporary ledger on final integration to main.
