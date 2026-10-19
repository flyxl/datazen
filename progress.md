# P6 Wapp instance sessions — READY_FOR_TEST

Branch: codex/p6-wapp-session-lifecycle; base b9b0e3cb30a6d56fdd6a6bba9333ce32191c8376.
Worktree: /tmp/datazen-p6-wapp-session (macOS canonical /private/tmp/datazen-p6-wapp-session).
Coder implementation and local checks only; independent acceptance remains pending.

## Confirmed gap and implementation

consumer-adapters §5 and workflow-resource-model §9.3 describe instance-owned stateful sessions. Production had only short command.invoke, with no stateful permission, approval, backend instance identity, protocol or cleanup worker.

Implemented session:stateful + command:invoke permission intersection, native host approval per open, backend UUID instance/client identity, opaque session references, explicit database (PG also schema), PG/MySQL-only admission, open/use/close/receipt bridge and SDK API. query/execute use existing Runtime policy/readOnly/Safe Mode and retained ResourceProvider; no AI or driver implementation changed.

One-hour hard instance lease; session TTL defaults five minutes and caps one hour; max eight live/pending-cleanup sessions. Timer revokes expired references independently of lookup, cancels recorded in-flight work, closes original owner/handle with RollbackAndClose. Pending/Quarantined/errors retain cleanup ownership and retry 100ms–30s until Confirmed. End during open keeps worker alive until late acquisition is registered/revoked. End/reload/disable/uninstall revoke only this instance/Wapp, never scan by connection ID. Host frame nonce rotates on load, source+nonce are checked for session calls, old response generation is suppressed; same request ID/input shares the original promise, changed input rejects. Receipt is instance-scoped, no handles/attachment/credentials, max 1000 result rows.

## Final stable-snapshot local gates

Final checks recorded both HEAD and worktree SHA before/after. Worktree digest includes tracked/untracked non-ignored files, excludes protected .env/.env.test names without opening them.
HEAD both: b9b0e3cb30a6d56fdd6a6bba9333ce32191c8376
WORKTREE_SHA256 both: 325b4805358e3e6c464168a3d2de624a1f007d2dbb008a40727ade11e6328de1
Only this progress record was updated after those checks; source/tests/docs were unchanged.

- `cargo test -p datazen --lib commands::wapps::sessions::tests`: EXIT=0; `test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 1404 filtered out; finished in 0.09s`.
- `cargo check -p datazen --lib`: EXIT=0; existing warnings remain (77 host warnings).
- `node node_modules/vitest/vitest.mjs run src/lib/__tests__/wappBridge.sessions.test.ts src/lib/__tests__/wappBridge.test.ts src/lib/__tests__/wappBridge.security.test.ts`: EXIT=0; `Test Files 3 passed (3)`, `Tests 54 passed (54)`.
- `node node_modules/vitest/vitest.mjs run packages/wapp-sdk/__tests__`: EXIT=0; `Test Files 5 passed (5)`, `Tests 69 passed (69)`.
- Direct equivalent of all four pnpm typecheck steps: `node scripts/check-restore-block-guards.mjs`, `node node_modules/typescript/bin/tsc --noEmit`, `node node_modules/typescript/bin/tsc -p tsconfig.scripts.json --noEmit`, `node node_modules/typescript/bin/tsc -p tsconfig.pack-ep.json --noEmit`: each EXIT=0.
- `git diff --check`: EXIT=0.

Earlier attempts: `pnpm typecheck` EXIT=1 because the local pnpm binary tries installing into symlinked node_modules and rejects that directory; it never reached typecheck. The direct four-step equivalent above passed. First `cargo check` EXIT=101 for absent generated resources/builtin-ep; normal community resolve-pro/codegen generated ignored resources; final check passed. No restore of stale manifests was used.

## Evidence / resources / limitations

Logs: /tmp/datazen-p6-wapp-rust-final.log; /tmp/datazen-p6-wapp-final-gates.log; /tmp/datazen-p6-wapp-sdk-gate.log; /tmp/datazen-p6-wapp-typecheck-equivalent.log. Earlier attempts retained as /tmp/datazen-p6-wapp-*.log. Snapshot helper: /tmp/datazen-p6-wapp-snapshot.py.

Independent Host Runtime mock-provider tests cover TTL without lookup, cross-instance same-connection isolation, original physical registration/budget release, and end/open acquisition race. Bridge journeys cover both manifest gates, source/nonce, changed replay, duplicate open, reload response suppression and detach/end. No live PG/MySQL SQL journey, native approval UI/WDIO or quarantine fault-injection acceptance was run here. Full Wapp Job/Workflow and generic short-command instance ownership remain outside this stateful-session change. Close truthfully returns pending; references/result indexes do not survive restart and unknown effects are not replayed.

No live database connections, external temp databases, processes or credentials were created. No .env contents read. node_modules is an ignored symlink to the original checkout; local target/codegen artifacts remain ignored. Worktree and branch intentionally retained for tester; tester must use a different worktree. Delete this progress.md when integration/acceptance is complete, before final merge to main.
