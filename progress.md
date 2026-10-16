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

Independent verification follow-up:
- Luna reproduced scripts 814 passing tests, typecheck success, and normal gate 0 violations / 0 errors.
- Corrected the stale literal CI step name in shared-boundaries-and-ports.md §8.3 to match ci.yml.
- `rg` checked docs for equivalent old step-name references: exactly this one literal step-name reference was stale. Other F-01..F-07 prose ranges remain in ci-test-matrix.md and shared-boundaries-and-ports.md, including historical implementation explanations; no code changes in this follow-up.
- `git diff --check`: EXIT=0. Documentation-only follow-up; no test rerun required.

Current documentation scope cleanup:
- Replaced current F-01..F-07 gate coverage labels in ci-test-matrix.md and shared-boundaries-and-ports.md with all platform boundary rules / the §2.4 rule table.
- Removed obsolete source-line/test-count totals from §8.2; corrected the driver-type isolation responsibility comparison.
- The sole remaining F-01..F-07 range is explicitly historical, limited to the initial Rust dependency path and excluding TS scanning/later rules.
- `git diff --check`: EXIT=0. Documentation-only; no test rerun.
