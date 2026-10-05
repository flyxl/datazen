# P4 Hub

> 集成分支的跨轨汇总。**临时进度文件**——P4 开发完成并通过验收后由协调者删除。

# P4 development (2026-10-05)

Integration branch: `codex/p4-integration`; baseline: `fd2d1ef04`.
User scope: develop P4 desktop query/table/metadata migration. Skip performance benchmarks and documentation/comment accuracy audits.

| Wave | Track | Branch | Status |
| --- | --- | --- | --- |
| 0 | Shared runtime application use cases + result/event bridge | codex/p4-usecases | ✅ MERGED `446207524` |
| 0 | Desktop profile/policy/resource backend + IPC assembly | codex/p4-desktop | ✅ MERGED `194aaea3` |
| 0 | Session controller + execution projection + Channel transport | codex/p4-session-client | ✅ MERGED `db2edfa1` |
| 1 | Query session/context/transaction + execution/results consumer | codex/p4-w1-query | ✅ MERGED `56c755799` |
| 1 | Table fixed-target/short transaction consumer | codex/p4-w1-table | ✅ MERGED `3551d57a6` |
| 1 | Metadata scoped resources/cache identity | codex/p4-w1-metadata | ✅ MERGED `7f43d4aaa` |
| R | Community/Pro, continuous journeys, host and driver E2E | pending | NOT_STARTED |

Contract seams: RuntimeConnectionUseCases constructor accepts profile/policy/backend ports; result sink is shared with desktop driver adapter. Frontend controller is transport neutral.
Approved minimum P3 seam extension: accepted execution ID must reach registry actor, physical driver sink, events and cancellation unchanged; preserve existing public return shapes and old methods.

Each track requires independent detached-tree acceptance. Tests write logs to system temp and record HEAD/workspace SHA before and after. Temporary progress/hub files are removed at delivery.
