# v01x-pending-changes 进度

## 1. 功能摘要

- 编号：v01x-pending-changes
- 范围：PendingRowChange、Preview change plan、Commit/Rollback、事务和无主键安全门槛
- 状态：独立验收完成，发现 3 个待验证 S1 缺陷，暂不通过
- 编码 commit：`2a6ff456868f1d83ea1da456bf0ca1c2a110edc3`
- 测试验收：本记录由独立测试代理维护；未修改业务源代码

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

## 3. 独立验收结果（2026-08-30）

本轮只审查编码 commit 并运行测试，没有修改业务源代码；最终待提交变更仅限本轨 `progress.md` 与 `bugs.md`。测试命令和结果如下：

- `npx vitest run`：263 个测试文件、2115 个测试通过，0 失败。
- `npx tsc --noEmit`：通过，0 个诊断。
- `CARGO_TARGET_DIR=/private/tmp/datazen-test-v01x-pending cargo test -p datazen --lib`：在受限沙箱首次执行为 1135 passed、46 failed、2 ignored；46 个失败均为 WireMock 绑定端口 `Operation not permitted`，不是断言失败。以相同完整命令在允许本地端口的环境复跑：1181 passed、0 failed、2 ignored。
- `CARGO_TARGET_DIR=/private/tmp/datazen-test-v01x-pending cargo test -p datazen --lib commands::data`：14 passed、0 failed、0 ignored、1169 filtered out（过滤器同时命中了 data transfer 相关测试）。
- `rustfmt --edition 2021 --check src-tauri/src/commands/data.rs`：通过。
- `cargo fmt --check --package datazen`：通过。
- `git diff 2a6ff456^ 2a6ff456 --check`：通过。
- `npx vitest run --coverage --coverage.reportsDirectory=/private/tmp/datazen-v01x-coverage src/stores/__tests__/tableDataStore.test.ts src/lib/__tests__/tableChanges.test.ts`：2 个文件、31 个测试通过；命令因仓库全局阈值检查未达到而返回 1，相关文件实际覆盖率为 `tableChanges.ts`：statements 100%、branches 80%、functions 100%、lines 100%；`tableDataStore.ts`：statements 83.17%、branches 67.66%、functions 84.15%、lines 90.36%。
- `npx vitest run --coverage --coverage.reportsDirectory=/private/tmp/datazen-v01x-coverage-full`：263 个文件、2115 个测试通过；命令因既有的非本轨文件覆盖率阈值失败而返回 1。全局报告为 statements 80.83% (7392/9145)、branches 72.61% (4946/6811)、functions 77.31% (1922/2486)、lines 83.12% (6534/7860)；本轨相关文件仍为上述覆盖率。阈值失败集中在 DataTable、ConnectionPage、ObjectBrowser、PrivilegeView、workflow、MainPage、dashboard 等既有/冻结范围。

### 独立静态审查结论

- `updateCell`、`deleteRows`、`deleteSelectedRows` 已转入 Store staged 入口；Store 中没有旧 `commitRowUpdates`/`commitRowDeletes` 直接执行路径。旧 IPC wrapper 仍保留，属于 wire 兼容性。
- RowIdentity 的无主键门槛、复合主键字段构造、changedColumns/revert 处理和 Rust 端事务执行路径已覆盖；但 NULL/重复 identity 与主键值碰撞存在 S1 缺陷，详见本轨 `bugs.md`。
- Preview 不开启事务、不执行数据库写入；Preview 与 Commit 共用 Rust plan builder，并通过 fingerprint 重建校验。Commit 保留现有事务语义，失败路径不清空 pending；成功路径清理 pending 并刷新。
- Commit 的 session 校验存在，但 Store/backend 对 database/schema/table context 的隔离不完整，可能复用错误上下文的 pending/plan，详见 `v01x-pending-changes-BUG-003`。
- 冻结写集通过：编码 commit 未修改 `DataTable`、`panelStore`、`QueryPanel`、共享 locales、Web/Core identity/workspace 或 audit；也未修改 hub、规格文档、SVG、codegen 文件。

### E2E 设计与执行登记

本环境未暴露 black-box skill 所需的 computer-use MCP，因此未宣称已执行桌面 UI E2E；按 playbook 保留为 R 回归。执行时应登记以下四条：

- `PC-E2E-001`（R）：已有带主键表；编辑单元格后检查数据库原值不变、pending 计数/变更存在、没有旧 UPDATE IPC；Preview 后仍不写库，Commit 后值改变并刷新。
- `PC-E2E-002`（R）：无主键表与复合主键表分别执行 Delete/Backspace；断言无主键无 DELETE 计划，复合主键计划包含全部键列；NULL/重复 identity 应拒绝而非生成多行 SQL。
- `PC-E2E-003`（R）：Preview 计划 fingerprint 与随后 Commit 输入一致；Preview 前后数据库快照相同；Commit 成功后按当前上下文刷新。
- `PC-E2E-004`（R）：制造一个提交失败（断开会话/约束冲突）；断言事务回滚、pending 保留、Store 状态为失败；discard/rollback 后重新加载并清空 pending。

## 4. 设计决策 / 遗留

- 无主键表禁止 UPDATE/DELETE，不能用全列匹配作为默认降级方案。
- Preview 和 Commit 必须使用同一 immutable plan/fingerprint。
- Preview 返回 driver SQL builder 生成的可展示模板、参数摘要和 warning；参数摘要不是数据库实际收到的 wire SQL，也不实现 SQL Audit。
- Commit 只接受绑定 session/table/database、PK identity、原值/当前值的 plan，并在服务端重建 SQL；fingerprint 过期或上下文不符时拒绝执行。
- Store 继续保留旧 `updateCell`/`commitChanges`/`discardChanges` 名称作为兼容别名，但其写入路径已改为 staged/preview/commit/discard 语义；旧 IPC command 保留 wire 兼容性。
- 遗留风险：Host E2E 尚未在本轨执行；Delete/Backspace 菜单/键盘接线由后续轨道负责，本轨仅提供 staged Store/domain 接口。另需在修复后复核 fingerprint 是否仅作为状态一致性校验，而非跨数据库/跨 schema 的上下文授权机制。

### S1 安全边界修复（2026-08-30）

- BUG-001：`buildRowIdentity` 对任一主键值为 NULL、undefined、非有限数或不可稳定序列化时返回拒绝；加载结果会记录重复 identity 并禁止 staged；Rust `canonicalize_changes`/plan builder 对空、NULL、重复 original identity 以及重复 effective identity 返回稳定 Validation 错误，因此不会把 NULL 降级为 driver 的 `IS NULL` row identity 条件。
- BUG-002：Store 的 pending map 始终以 original identity 为 key；PK 编辑通过仅存于内存的 row-index anchor 继续指向该 original identity。PK 新值与其他物理行或其他 pending change 冲突时拒绝 staged；不再按当前 PK 扫描或按裸 rowIndex fallback 重定向 pending。
- BUG-003：`TableChangeContext`、Rust `RowChangeTableContext`、plan fingerprint 和 table-state key 均覆盖 `connectionId`、`dbSessionId`、`driverType`、`database`、`schema`、`table`。数据库字段、连接归属、驱动类型或 schema 不完整/不一致时拒绝 preview/commit；commit 不再自动切换 session database。进行中的旧上下文 commit 若用户已切换面板，只更新旧状态，不把界面切回旧 database/schema。
- 复合主键、NULL、重复 identity、单列/复合主键碰撞和跨 database commit 拒绝均增加了定向单测；本轮只触及 pending-changes 业务代码、既有测试辅助及本轨 progress/bugs 记录。
