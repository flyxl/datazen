# v01x-pending-changes Bug 清单

| Bug ID | 描述 | 状态 | 重现步骤 | 验证记录 |
|---|---|---|---|---|
| v01x-pending-changes-BUG-001 | [S1] NULL 或重复 RowIdentity 可生成多行 UPDATE/DELETE | 待验证 | 见下方 | 2026-08-30 独立静态审查发现，未修复 |
| v01x-pending-changes-BUG-002 | [S1] 主键值变更碰撞会把后续编辑/删除误关联到另一行 | 待验证 | 见下方 | 2026-08-30 独立静态审查发现，未修复 |
| v01x-pending-changes-BUG-003 | [S1] 同一 session 下 database/schema context 未隔离，可能提交到错误数据库/表上下文 | 待验证 | 见下方 | 2026-08-30 独立静态审查发现，未修复 |

## 本轮验证（2026-08-30）

- 新增功能性 bug：3 个，均为 S1，当前验收结论为不通过，等待编码代理修复后复验。
- 遗留验证风险：Host E2E 尚未执行；本轨未修改 `DataTable.tsx`，Delete/Backspace 接线仍由后续轨道负责。

## v01x-pending-changes-BUG-001

- 严重度：S1（可能更新/删除多条数据，或误修改无法唯一定位的行）。
- 状态：待验证；本轮未修复。
- 证据：`src/lib/tableChanges.ts` 的 `buildRowIdentity` 只拒绝空 PK 列集合，不拒绝 NULL 值；`src-tauri/src/commands/data.rs` 的 `canonicalize_changes` 只拒绝空 identity map；`packages/driver-api` 的默认 builder 会把 NULL 条件渲染为 `IS NULL`。
- 重现步骤：
  1. 在 SQLite 创建一个普通 rowid 表，使声明的单列/复合主键可以出现 NULL，或准备具有相同 PK identity 的重复行；例如使用复合键中可为 NULL 的列。
  2. 加载该表，编辑或删除一个 PK identity 含 NULL 的行；Store 因为 PK 列集合非空而允许 staged change。
  3. 调用 Preview，观察计划被接受；Rust 端会保留 NULL identity，驱动 builder 将对应条件生成 `IS NULL`，重复 identity 也没有被拒绝。
  4. Commit 后，`IS NULL` 条件或重复 identity 可能匹配多条物理行。
- 期望：只要 identity 任一 PK 值为 NULL、identity 不唯一或无法证明唯一，就拒绝生成 UPDATE/DELETE 计划，不执行数据库写入。
- 实际：NULL identity 被视为非空 map；重复 identity 没有唯一性门槛，计划可继续进入 Commit。

## v01x-pending-changes-BUG-002

- 严重度：S1（主键碰撞时可能把编辑或删除落到错误行）。
- 状态：待验证；本轮未修复。
- 证据：`src/stores/tableDataStore.ts` 的 `findPendingForRow` 在直接 key 未命中后，会用 pending change 的当前 PK 值扫描匹配；它没有拒绝“某一行把 PK 改成另一行现有 PK”的碰撞。
- 重现步骤：
  1. 加载带单列主键的两行：`id=1, name=Alice` 和 `id=2, name=Bob`。
  2. 调用 `updateCell(0, "id", 2)`，第一行 pending 的原 identity 是 `id=1`、当前 identity 变为 `id=2`。
  3. 调用 `updateCell(1, "name", "Robert")`，或调用 `deleteRows([1])`。
  4. 观察 `findPendingForRow`：行 1 的直接 `id=2` key 未命中后，会扫描到行 0 的 pending 当前 identity `id=2`；后续变更被写入行 0 的 pending，Commit 可能执行 `WHERE id=1` 的错误更新或删除。
- 期望：检测当前 identity 与其他物理行/待提交行冲突并阻止该 staged change，或明确报告冲突；任何后续行操作都不得重定向到另一行。
- 实际：当前 identity 碰撞时 pending map 可能被错误复用；Store 没有冲突状态或安全拒绝。

## v01x-pending-changes-BUG-003

- 严重度：S1（可能在用户当前查看的上下文之外提交数据，存在跨库/跨 schema 误写风险）。
- 状态：待验证；本轮未修复。
- 证据：`ConnectionTableState.tableStates` 以裸 table name 作为 key；`TableChangeContext` 仅有 `dbSessionId`、`table`、`database`，缺少 schema/connection/database type；Store 的 plan 复用判断只比较 session/table；Rust Commit 先按 plan database 切换 session，再只校验 session id 和 fingerprint。
- 重现步骤：
  1. 在同一个 db session 可访问的 `db_a`、`db_b` 中各创建 `users` 表（同一数据库的不同 schema 也可复现）。
  2. 加载 `db_a.users`，编辑一行并 Preview；此时 pending 与 plan 绑定到 `db_a.users`。
  3. 切换并加载 `db_b.users`。由于 Store 状态按裸 `users` 复用，旧 pending/preview plan 可能仍在同一 TableState；database 变化也不会使已存在 plan 失效。
  4. 调用 `commitPendingChanges()`；Store 只检查 session/table，可能复用 `db_a` 的 plan；Rust Commit 的 `ensure_session_database` 还会按 plan 将 session 切回 `db_a` 后执行。
  5. 刷新又可能回到 `db_b` 上下文，造成用户看到的上下文与实际写入上下文不一致。
- 期望：pending/plan 按完整 context（至少 session、connection、database、schema、table、driver type）隔离；上下文任一变化都应使 Preview plan 失效并拒绝 Commit，不得自动切库执行旧计划。
- 实际：状态 key、Store plan 复用和后端校验均未覆盖完整 context，存在跨上下文提交路径。
