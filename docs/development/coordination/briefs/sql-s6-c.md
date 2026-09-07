# Brief: sql-s6-c — 方言 metadata 审计与驱动任务

> 本轨先只读审计并生成逐驱动任务清单；通用 quote helper 已由 S2-B 唯一实现，本轨不再拥有。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S6-C（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s6-c/progress.md`。

## 独占写锁

审计阶段只写本轨 progress/bugs。获批后每个 path driver 独立子轨，
精确拥有该 `packages/drivers/<id>/ui/meta.ts` 与该驱动 tests。
**禁止**：`generated.ts`、`driver_init.rs`、生成 capability 文件、Host driver ID 判断。

## 步骤

1. 检查 MySQL/ClickHouse/Doris、PG/openGauss、SQL Server registry metadata 与 PRD 一致性；
   评估 quoteChar 变更对通用 SQL generator、DDL/index 路径的影响。
2. SQL Server 子轨修 bracket metadata 时同步验证 CodeMirror MSSQL profile；
   `resolveCmDialect` 通用装配改动留给 S6-D。
3. ClickHouse metadata 与 PRD 有差异：先形成产品决策门；未确认前不改可能影响既有生成器的 quoteChar。
4. 只在获批子轨修正错误 metadata。
5. 驱动专属单测放各 driver UI tests；Git driver 不在仓内则登记插件仓任务，不改生成目录。

## 测试

各获批 path-driver 子轨各自通过测试；Host 通用测试不含驱动 ID 常量。

## 完成定义

- 审计清单完整；获批子轨测试通过；Git driver 外部任务有 owner。
