# Issues 71–73

Status: READY_FOR_TEST

Completed:
- #71: removed stale numeric rule ranges from platform boundary gate header/error messages and CI step name.
- #72: 16 guarded scripts now share realpathSync + pathToFileURL CLI entry recognition. Existing panic gate already had robust recognition and remains unchanged. Regression tests execute each script through a symlink containing spaces/Unicode and verify imported scripts stay silent. Build/injection scripts execute in disposable copied-source fixtures with external commands blocked; real repository files are never injected by those probes. Literal module URL matching is also asserted.
- #73: per-rule verdict includes rule-attributed errors. A real subprocess CLI probe scans a temporary backend-client source directory with valid a.ts and dangling z.ts, reports F-07 failure and 0 violations / 1 error, then succeeds with 0 violations / 0 errors after unlinking the failing entry.

Validation:
- `pnpm --config.verify-deps-before-run=false exec vitest run scripts/__tests__`: EXIT=0, `Test Files  46 passed (46)`, `Tests  814 passed (814)`, Duration 17.22s.
- `pnpm --config.verify-deps-before-run=false typecheck`: EXIT=0 (restore guards + main tsc + scripts tsc + pack-ep tsc).
- `git diff --check`: EXIT=0.
- Both test commands recorded before/after HEAD `d122d2a37e246067a163e2cc5fbd5403c02bfee7`, and workspace SHA `f22061174853c2fdf9d3f1d78918257893e96faa92a9cf2ede547583ab1f86f2`, unchanged throughout. Fingerprint is SHA256 of git diff HEAD plus hashes of untracked script files.
- Earlier expanded fixture regression failed because the copied fixture lacked fflate; resolved by copying that dependency into the temporary fixture. Final complete scripts suite passes.

Logs:
- /var/folders/2y/5w1_2qg97r7b9bfv20b4nybh0000gn/T/datazen-scripts7173.hkmQjnenbW.log
- /var/folders/2y/5w1_2qg97r7b9bfv20b4nybh0000gn/T/datazen-typecheck7173.zTDljtCDaY.log

Remaining: coordinator independent verification/integration. Remove this ledger before final delivery.
