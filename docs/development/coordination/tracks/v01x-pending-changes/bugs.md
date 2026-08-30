# v01x-pending-changes Bug 清单

| Bug ID | 描述 | 状态 | 重现步骤 | 验证记录 |
|---|---|---|---|---|
| v01x-pending-changes-BUG-001 | [S1] NULL 或重复 RowIdentity 可生成多行 UPDATE/DELETE | 已修复，第二轮独立复验通过 | 见下方 | 2026-08-30 f2：domain/Rust 定向回归通过 |
| v01x-pending-changes-BUG-002 | [S1] 主键值变更碰撞会把后续编辑/删除误关联到另一行 | 已修复，第二轮独立复验通过 | 见下方 | 2026-08-30 f2：单列/复合主键 Store 定向回归通过 |
| v01x-pending-changes-BUG-003 | [S1] 同一 session 下 database/schema context 未隔离，可能提交到错误数据库/表上下文 | 已修复，第二轮独立复验通过 | 见下方 | 2026-08-30 f2：context 隔离 Rust/Store 定向回归通过 |

## 初轮验证（2026-08-30）

- 新增功能性 bug：3 个，均为 S1；本轮编码代理已修复，等待独立复验。
- 遗留验证风险：Host E2E 尚未执行；本轨未修改 `DataTable.tsx`，Delete/Backspace 接线仍由后续轨道负责。

## 第二轮独立复验（2026-08-30）

- 目标：`583cfc13952f5f311381c3f29bce7f04703842f6`（含 `2a6ff456`、`32bbdd7e`）。
- 结论：BUG-001、BUG-002、BUG-003 共 3/3 个 S1 修复通过静态审查、定向 Vitest/Rust 回归；未发现新的业务 bug。
- 回归结果：定向 Vitest 3 文件/42 测试通过；全量 Vitest 263 文件/2122 测试通过；`tsc --noEmit`、`cargo fmt --check --package datazen`、目标 diff `git diff --check` 通过；`commands::data` Rust 定向 17/17 通过；Rust 全量在允许本机端口后 1184 passed、0 failed、2 ignored。
- 环境限制：受限沙箱 Rust 全量首次 46 个 WireMock 测试因绑定端口 `Operation not permitted` 失败，放行本机端口后同命令全部通过；这 46 项没有断言失败。桌面 Host E2E 因无 computer-use MCP 未执行，继续留待 R 回归。
- 覆盖率：全量测试均通过但覆盖率命令因既有分组阈值返回 1，全局 statements 80.54%、branches 72.40%、functions 77.18%、lines 82.82%；`tableChanges.ts` 定向 60% statements / 63.46% lines，主要为异常/非法输入分支缺口，不是本轮发现的行为失败。

## v01x-pending-changes-BUG-001

- 严重度：S1（可能更新/删除多条数据，或误修改无法唯一定位的行）。
- 状态：已修复，第二轮独立复验通过。
- 修复：前端 identity builder 拒绝任一 NULL/不稳定 PK 值，加载结果检测重复 identity；Rust canonicalize/plan builder 拒绝空、NULL、重复 original/effective identity。driver 的 `IS NULL` 只保留为通用非-row-identity SQL builder 语义，不再接收 pending row identity。
- 重现步骤：
  1. 在 SQLite 创建一个普通 rowid 表，使声明的单列/复合主键可以出现 NULL，或准备具有相同 PK identity 的重复行；例如使用复合键中可为 NULL 的列。
  2. 加载该表，编辑或删除一个 PK identity 含 NULL 的行；Store 因为 PK 列集合非空而允许 staged change。
  3. 调用 Preview，观察计划被接受；Rust 端会保留 NULL identity，驱动 builder 将对应条件生成 `IS NULL`，重复 identity 也没有被拒绝。
  4. Commit 后，`IS NULL` 条件或重复 identity 可能匹配多条物理行。
- 期望：只要 identity 任一 PK 值为 NULL、identity 不唯一或无法证明唯一，就拒绝生成 UPDATE/DELETE 计划，不执行数据库写入。
- 结果：NULL、不可稳定序列化或重复 identity 在 staged/preview 两层均被安全拒绝，不进入 UPDATE/DELETE plan。

## v01x-pending-changes-BUG-002

- 严重度：S1（主键碰撞时可能把编辑或删除落到错误行）。
- 状态：已修复，第二轮独立复验通过。
- 修复：pending map 只按 original identity 寻址；row-index anchor 只保存到 original key 的内存关联。当前 PK 新值与物理行或其他 pending effective identity 冲突时，Store 明确拒绝 staged，不再使用当前 PK 扫描或裸 rowIndex fallback。
- 重现步骤：
  1. 加载带单列主键的两行：`id=1, name=Alice` 和 `id=2, name=Bob`。
  2. 调用 `updateCell(0, "id", 2)`，第一行 pending 的原 identity 是 `id=1`、当前 identity 变为 `id=2`。
  3. 调用 `updateCell(1, "name", "Robert")`，或调用 `deleteRows([1])`。
  4. 观察 `findPendingForRow`：行 1 的直接 `id=2` key 未命中后，会扫描到行 0 的 pending 当前 identity `id=2`；后续变更被写入行 0 的 pending，Commit 可能执行 `WHERE id=1` 的错误更新或删除。
- 期望：检测当前 identity 与其他物理行/待提交行冲突并阻止该 staged change，或明确报告冲突；任何后续行操作都不得重定向到另一行。
- 结果：单列及复合主键碰撞均不产生 pending change，后续行不会重定向到另一 pending；当前 UI 以稳定 ambiguous identity 错误提示拒绝。

## v01x-pending-changes-BUG-003

- 严重度：S1（可能在用户当前查看的上下文之外提交数据，存在跨库/跨 schema 误写风险）。
- 状态：已修复，第二轮独立复验通过。
- 修复：table-state key、TableChangeContext、RowChangeTableContext、plan fingerprint 均包含 `connectionId`、`dbSessionId`、`driverType`、`database`、`schema`、`table`。connection/session/driver/database/table 缺失时拒绝 preview/commit；schema 采用显式 null 语义并要求当前上下文完全匹配。Rust Commit 移除自动切库路径，connection owner、driver、database、schema 任一不一致均拒绝。
- 重现步骤：
  1. 在同一个 db session 可访问的 `db_a`、`db_b` 中各创建 `users` 表（同一数据库的不同 schema 也可复现）。
  2. 加载 `db_a.users`，编辑一行并 Preview；此时 pending 与 plan 绑定到 `db_a.users`。
  3. 切换并加载 `db_b.users`。由于 Store 状态按裸 `users` 复用，旧 pending/preview plan 可能仍在同一 TableState；database 变化也不会使已存在 plan 失效。
  4. 调用 `commitPendingChanges()`；Store 只检查 session/table，可能复用 `db_a` 的 plan；Rust Commit 的 `ensure_session_database` 还会按 plan 将 session 切回 `db_a` 后执行。
  5. 刷新又可能回到 `db_b` 上下文，造成用户看到的上下文与实际写入上下文不一致。
- 期望：pending/plan 按完整 context（至少 session、connection、database、schema、table、driver type）隔离；上下文任一变化都应使 Preview plan 失效并拒绝 Commit，不得自动切库执行旧计划。
- 结果：上下文任一字段变化都会切换隔离 state key、使旧 plan 不可复用；进行中的旧 commit 不会把界面自动切回旧 database/schema。若调用链无法提供必需字段，返回 context-incomplete 并拒绝写入。
