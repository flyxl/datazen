# P6 integration temporary ledger

This development ledger is temporary and must be deleted before the P6 branch is merged to main.

Integration branch: `feature/p6-restore-apply-evidence`.

## Accepted and integrated

- Backup retained Runtime implementation and repairs: `b395562ee`, `b9b0e3cb3`. Independent PostgreSQL/MySQL live backup journeys, Renderer tests and compile checks passed.
- Restore Apply: PG/MySQL Host boundary 2/2 and driver live journeys 1/1 each passed on `b9b0e3cb3`.
- Wapp stateful instance sessions: coder commit `b5f94785a`, tester accepted; integrated as `a9c76752a`. Host 2/2, bridge 54/54, SDK 69/69, typecheck equivalents passed. Live PG/MySQL Wapp SQL and approval UI remain untested.
- Monitor/Workflow scheduler startup reactor fix: coder commit `d3236b4af`, tester accepted; integrated as `7c2902495`. Startup tests 2/2 and PostgreSQL Workflow WDIO journey 1 passing.
- NativeTool fail-closed kernel: coder commit `2320a3da6`, independent tester accepted. Runtime 6/6, Host 5/5, platform-api/runtime cargo checks and boundary guard passed. Desktop only permits hash-pinned `pg_dump`/`mysqldump --version` probes with zero DB sockets; real database CLI operations, backup/restore and product IPC/UI remain unsupported.

## In progress

- Monitor scheduled desktop PG/MySQL journey: coder commit `d2de58eae` is ready for independent tester. It adds webdriver-only scheduled tick/evidence projection and physical dispatch counts. WDIO/live timer journeys have not yet run.

## Remaining P6 scope

- Monitor desktop scheduled journey tester acceptance.
- MCP owner/TTL full fault/live acceptance.
- MySQL transactionBlock remains fail-closed pending a defensible proof.
- NativeTool database operations and full product IPC/UI remain unsupported; this implementation is a safe kernel/probe slice only.
- Explicit AI editor-session grants/revocation are tracked by the separate AI migration owner; do not modify AI here.

P6 is not closed. Remove this ledger during final acceptance cleanup.
