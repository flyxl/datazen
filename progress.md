# P5 Transfer Job UI progress

## Done

- Apply acceptance stores the returned Job id immediately and polls `get_job` through progress and terminal state.
- Terminal receipts and running jobs query durable details before presenting the terminal verdict; reopened windows restore the latest durable apply details.
- Apply timeouts and ambiguous replies replay the exact request with the same plan-scoped idempotency key.
- Cancel requests target the active apply Job id; acknowledgement stays separate from the terminal verdict.
- Shared migration hydration keeps active and terminal projections current from `watchJob`; unmount only stops local polling.
- Reopened windows show active Transfer Jobs with progress and a reachable cancel action. Missing/failed details remain uncompleted and uncertain.
- Transfer UI now includes frozen Core SHA `33c7e047d5b75efeb43895a7a2a282f7708a7812`, including the shared `JobRecoveryVerdict` / `JobDetails` DTO.
- `notExecuted` is treated as a known not-started result only for `effectOutcome=notStarted` plus `reasonCode=notDispatchedAfterRestart`, with no write evidence. Other `notExecuted` reason codes remain fail-closed.
- The verdict panel explains the exact restart-before-dispatch reason and leaves other reason codes unchanged.

## Independent validation pending

The Core merge and the changes above have not been tested by the coding agent. Run these from a separate detached worktree at the final feature SHA:

```bash
pnpm exec vitest run \
  src/lib/__tests__/migrationJobVerdict.test.ts \
  src/components/migration/__tests__/MigrationJobVerdictPanel.test.tsx \
  src/hooks/__tests__/useTransferJobRun.test.tsx \
  src/windows/data-transfer/__tests__/DataTransferWindow.test.tsx \
  src/commands/__tests__/transferJobs.test.ts
pnpm typecheck
```

Before this Core integration, the prior UI checkpoint reported 8 focused Vitest files / 92 tests passing, three TypeScript checks passing, and no live WDIO run because its PostgreSQL/MySQL prerequisites were unavailable. Those results do not cover the merged Core API or the new `notExecuted` handling.
