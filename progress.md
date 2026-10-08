# P5 Transfer Job UI progress

## Done

- Apply acceptance stores the returned Job id immediately and polls `get_job` through progress and terminal state.
- Terminal receipts and running jobs both query durable details before presenting the exact terminal verdict in the current window.
- Apply timeouts/ambiguous replies replay the exact request with the same plan-scoped idempotency key.
- Cancel requests target the active apply Job id; UI acknowledgement remains separate from the terminal verdict.
- Shared migration hydration keeps active and terminal projections current from `watchJob`; unmount only stops local polling.
- Reopened windows show active Transfer Jobs with progress and a reachable cancel action.
- Reopened windows query `get_transfer_job_details(jobId)` for the latest settled apply and restore its plan identity, progress, boundaries, error, and optional recovery verdict. Missing/failed details stay uncompleted and uncertain.
- DTJ fixtures now cover matching relation names across distinct databases, receipt replay, and cancel intent on the accepted Job.

## Core integration dependency

- The frontend adapter follows the frozen `JobDetails` DTO. The Core command/type registration is not present in this checkout yet; runtime WDIO must run after that Core change is merged.
- DTJ-001 now asserts restored job id, completed verdict, plan identity, and verified commit boundaries after reopening.

## Verification

- Affected Vitest: 8 files, 92 tests passed.
- Direct `tsc --noEmit`, `tsc -p tsconfig.scripts.json --noEmit`, and `tsc -p tsconfig.pack-ep.json --noEmit` passed.
- Focused DTJ E2E-spec typecheck passed with a temporary config extending the root project. The standalone `e2e/tsconfig.json` invocation is not usable for this one spec because it omits root JSX and path mappings.
- Real WDIO was not run: `e2e/.env` is absent and local PostgreSQL/MySQL ports 5432/3306 are unreachable, so the live DTJ fixture prerequisites are unavailable. No app build was attempted.
- The frozen details IPC is not registered in this checkout yet, so native-app runtime verification also depends on the Core merge.
- Final gate fingerprint was unchanged before/after: HEAD `1ad1893ce0f18d22e14db7d76ac32766a8b5b9ae`, diff SHA `165a25ac7d8a3987e8da1e6441db13cfb1fd64841234e26707d1e3d00785bdb5`, untracked SHA `fd42398d7554ede18d8d4633d9aa3c4efabcc048c039da4913e24e1f81dc0fdc`.
