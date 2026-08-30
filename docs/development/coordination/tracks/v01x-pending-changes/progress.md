# v01x-pending-changes 进度

## 1. 功能摘要

- 编号：v01x-pending-changes
- 范围：PendingRowChange、Preview change plan、Commit/Rollback、事务和无主键安全门槛
- 状态：实现完成，本代理自测通过，待独立测试代理验收
- 编码 commit：本次最终 `feat(data): ...` 提交
- 测试验收：待独立测试代理执行

### 初步审计里程碑（2026-08-30）

- 已确认当前 `tableDataStore` 的 `updateCell`、`commitChanges`、`deleteSelectedRows` 和 `deleteRows` 会直接调用旧的 UPDATE/DELETE IPC；本轨将保留旧 command wire 兼容性，但让 Store 的现有入口转为暂存语义。
- 已确认 `driver-api` 已提供 `build_update_sql` / `build_delete_sql`，Preview/Commit 将由 Rust 共用一套计划生成与 fingerprint 校验逻辑，不在前端拼接 SQL。
- 已新增 `src/lib/tableChanges.ts`，定义 `RowIdentity`、`PendingRowChange`、`RowChangePlan` 及稳定主键身份工具；无主键不生成可提交身份。
- 写集边界已冻结：不修改 `DataTable.tsx`、`panelStore.ts`、`QueryPanel.tsx`、共享 locales、Web/Core identity/workspace/audit；bootstrap 遗留规格文档和 `hub.md` 不纳入提交。

## 2. E2E 用例

| 编号 | 场景 | 类型 | 状态 |
|---|---|---|---|
| PC-E2E-001 | 编辑单元格后只产生 pending change，不立即 UPDATE | 留待 R 回归 | 待执行 |
| PC-E2E-002 | Delete/Backspace 后只产生删除标记，不立即 DELETE | 留待 R 回归 | 待执行 |
| PC-E2E-003 | Preview SQL 不执行，Commit 成功后刷新数据 | 留待 R 回归 | 待执行 |
| PC-E2E-004 | Commit 失败后 pending changes 保留 | 留待 R 回归 | 待执行（Store 单测已覆盖） |

## 3. 本代理自测结果

- `npx tsc --noEmit`：通过。
- 定向 Vitest：2 个文件、31 个测试通过（`tableDataStore` 与 `tableChanges`）。
- `cargo test -p datazen --lib commands::data`：14 passed、0 failed。
- `cargo test -p datazen --lib`：1181 passed、2 ignored、0 failed。
- `rustfmt --edition 2021 --check src-tauri/src/commands/data.rs`：通过。
- `cargo fmt --check --package datazen`：通过；仅本地格式化了 gitignored 生成文件 `src-tauri/src/driver_init.rs`，未纳入写集。
- `git diff --check`：通过。
- 以上为本代理自测记录；覆盖率百分比未单独采集，独立测试代理负责后续验收。

## 4. 设计决策 / 遗留

- 无主键表禁止 UPDATE/DELETE，不能用全列匹配作为默认降级方案。
- Preview 和 Commit 必须使用同一 immutable plan/fingerprint。
- Preview 返回 driver SQL builder 生成的可展示模板、参数摘要和 warning；参数摘要不是数据库实际收到的 wire SQL，也不实现 SQL Audit。
- Commit 只接受绑定 session/table/database、PK identity、原值/当前值的 plan，并在服务端重建 SQL；fingerprint 过期或上下文不符时拒绝执行。
- Store 继续保留旧 `updateCell`/`commitChanges`/`discardChanges` 名称作为兼容别名，但其写入路径已改为 staged/preview/commit/discard 语义；旧 IPC command 保留 wire 兼容性。
- 遗留风险：Host E2E 尚未在本轨执行；Delete/Backspace 菜单/键盘接线由后续轨道负责，本轨仅提供 staged Store/domain 接口。
