# W5 测试报告 — 术语规范文档与守护规则（含 BUG-007 复核）

- **被测提交**：`903bf5aa6b4d36d6a50c9a815f643ec6a889a702`（分支 `feature/db-session-id-rename`）
- **测试角色**：独立验证（只测试、不修复）；测试期间的反向注入改动均已逐字节恢复，收尾时 `git status --porcelain` 为空
- **测试日期**：2025-07（本会话）

---

## 执行摘要

W5 的六项交付物（naming.md 规范页、双 README 索引挂链、守护脚本 + npm script + 单测、AGENTS.md 两处修改、BUG-007 文档修复）**全部落实且与代码一致**。五项门禁全部通过；naming.md 六组关键论断逐一对照源码核实无误；守护脚本对注入的两种违例形态均能以 `file:line: 内容 + 原因` 报告并 exit 1，白名单为"文件+行内容"双条件精确豁免（删除白名单条目后历史注释即触发违例）。**未发现缺陷，结论：通过。**

| 门禁 | 结果 |
|------|------|
| `node scripts/check-id-terminology.mjs` | exit 0（891 files scanned，2 条白名单命中跳过） |
| `npm run test:ids` | exit 0 |
| `npx vitest run` | **240 文件 / 1894 用例全绿**（含本新增 `check-id-terminology.test.ts` 4 例） |
| `npx tsc --noEmit` | exit 0 |
| `cargo test -p datazen --lib` | **1126 通过 / 2 失败 / 2 忽略**——仅既有 `tests::resolve_log_settings_defaults_without_settings_file`、`tests::resolve_log_settings_reads_custom_level_and_path`（`lib.rs:1104` `PermissionDenied`，sandbox 环境限制），与已知清单完全一致 |

---

## T1 独立门禁执行

逐条按指定命令执行：

```text
node scripts/check-id-terminology.mjs        → [ok] 2 allow-listed occurrence(s) skipped; ok (891 files scanned); exit=0
npm run test:ids                             → exit=0
npx vitest run                               → Test Files 240 passed (240) / Tests 1894 passed (1894); exit=0
npx tsc --noEmit                             → exit=0（无输出）
CARGO_TARGET_DIR=<共享缓存> cargo test -p datazen --lib
                                             → 1126 passed; 2 failed; 2 ignored（失败即既有 sandbox 2 例）
```

补充确认：240 个 vitest 文件中包含本次新增的 `scripts/__tests__/check-id-terminology.test.ts (4 tests)`。

## T2 naming.md 准确性审计（对照源码逐条核实）

| # | naming.md 论断 | 源码锚点（实测） | 结论 |
|---|----------------|------------------|------|
| ① | connectionId 由前端生成（`shared.tsx` `newId()`，形如 `conn_xxxxxxxx`），经 `save_connection` 落盘 | `src/components/connection/shared.tsx:3` `` return `conn_${Math.random().toString(36).slice(2, 10)}` ``（8 位 base36）；`useConnectionForm.ts:201` `id: editId ?? newId()` → `:259` `saveConnection(draft)` → `connectionStore.ts:168` → `commands/connection.ts:7` `invoke('save_connection')` | ✅ 一致 |
| ② | dbSessionId 生成于 `ConnectionManager::connect` 的 `driver.connect()` → `ConnectionHandle.id` | `connection_manager.rs:115` `pub async fn connect(&self, connection_id: &str)`；`:181` `let handle = driver.connect(&effective_config).await?`；`:118` `let db_session_id = handle.id.clone();` 随后注册进活动池并写归属映射 | ✅ 一致 |
| ③a | 空闲驱逐参数：默认每 5 min 扫描、空闲 30 min 驱逐 | `:105` `idle_timeout: Duration::from_secs(1800)`；`:494` `interval(Duration::from_secs(300))`；`:475` 按 `last_used > idle_timeout` 移除 | ✅ 一致 |
| ③b | 驱逐后 reconnect 复用原 dbSessionId（"驱逐不换 id"不变式） | `reconnect()`（`:361`）从 `session_owner_map` 反查 owner → 从 Store 重读最新配置 → `connections.insert(db_session_id.to_string(), …)` 以同一 id 重新注册；`get_session` 未命中即走 reconnect；日志注释 "Evicting idle db session (session_owner_map entry kept for auto-reconnect)" | ✅ 一致 |
| ④ | session_owner_map 方向：`map[db_session_id] = connection_id` | `connect()` 内 `insert(db_session_id.clone(), connection_id.to_string())`；`owner_connection_id(&self, db_session_id) -> Option<String>` 以会话 id 为键反查配置 id；`disconnect()` 同步移除该条目 | ✅ 方向正确 |
| ⑤a | `resolve_session` 双模顺序：先按 dbSessionId（活动池/归属映射复活），未命中再当 connectionId 走 `get_or_connect_session` | `resolve_session()`（`:345`）：先 `get_session(id)`，失败则 `get_or_connect_session(id)` 后再 `get_session` 返回新会话 id；函数 doc 注释明确 "**db_session_id first**, then falling back to connection_id" | ✅ 一致 |
| ⑤b | "标注场景表" 5 行代码锚点全部存在且语义相符 | 见下方明细 | ✅ 全部命中 |
| ⑥ | 决策表各行语义方向 | CRUD/归属/调度/历史收藏/MCP 契约/UI 归属 → connectionId；SQL 执行/cancel/流式/Schema 浏览/execute_driver_command/导出执行体 → dbSessionId | ✅ 无装反 |

⑤b 明细（每场景抽查一个锚点）：

| 场景表行 | 实测锚点 |
|----------|----------|
| 插件桥透传 | `src/lib/extensionBridge.ts:205` `handleCommandInvoke` 只读 `p.connectionId`；`:223-224` `entry?.dbSessionId ?? connectionId` —— 查到活动会话透传其 dbSessionId，否则原始 id 交后端双模解析（`:220-222` 有对应注释） |
| Workflow runtime | `workflow/command_runtime.rs:33` 与 `workflow/executor.rs:657` 均对 Step/Workflow 的 connection 字段调 `resolve_session` |
| Driver Command IPC 遗留调用方 | `commands/driver_command.rs:177-179` `resolve_command_driver` 注释明确 "resolve_session is dual-mode … (legacy callers …)" 兼容路径 |
| MCP DB tools / AI db tools | `services/db_tools.rs:14-33` `resolve_connection` doc 注明 "db_session_id first, connection_id fallback"，入参即外部契约的 `connection_id` |
| 导出执行体 | `commands/export.rs:83` `ExportTablesRequest.db_session_id`；`:563` `.resolve_session(&request.db_session_id)` |

第 5 节改名映射抽查：持久化 `SyncTask`（`store/models.rs:43`）确为 `sourceDbSessionId/targetDbSessionId` + `sourceConnectionId/targetConnectionId` 两类字段分离；`history_db.rs:228-229` 将 `query_history`/`favorite_queries` 的 `config_id` 列 rename 为 `connection_id` 并 `INSERT INTO schema_version VALUES (4)`（v3→v4、数据保留）；`schemaDiff.ts:47-50` 为 v2 `version: 2` + `sourceConnectionId/targetConnectionId` 且注释声明 v1 `configId` 载荷拒绝导入。均与文档一致。

## T3 守护脚本判别力实验

### 1. 模式与白名单合理性审阅

- 8 条禁止模式与 W5 摘要一致：`configId` / `config_id` / `activeConfigId` / `sourceConfigId` / `targetConfigId` / `catConfigId` 六个旧名 + 两条"把 config id 塞进会话键"的装反形态（`connectionId: config…`、`dbSessionId: config…`）。每条都带 reason，命中输出含原因与 naming.md 指引。
- 2 条白名单均为"文件路径 + 行内容正则"双条件匹配（`rel === entry.file && entry.line.test(text)`）：① `src/commands/schemaDiff.ts` 中描述 v1 拒绝行为的历史格式注释；② `e2e/specs/bugfix-admin-commands.ts` 的 `invokeBackend<string>('connect', { connectionId: config.id …})`（connect IPC 契约上收的就是持久化连接 id，属正确用法）。
- 扫描范围与命名.md §6 相符：`SCAN_DIRS = src/ packages/ e2e/`，跳过 `node_modules/dist/coverage/.git` 与两个 codegen 文件（`src/plugins/generated*.ts`）。基线扫描 891 个文件，恰好 2 条白名单命中跳过，exit 0。

### 2. 反向注入实验（已全部逐字节恢复）

| 实验 | 注入内容 | 期望 | 实测 |
|------|----------|------|------|
| a. 旧属性名 | 向 `src/lib/dataTypeColors.ts` 追加 `const activeConfigId = 1;` | exit 1 + file:line | ✅ exit 1，报 `src/lib/dataTypeColors.ts:58: const activeConfigId = 1; -> old prop name; use activeConnectionId` |
| b. 装反形态 | 同文件追加 `const probe = { connectionId: config.id };` | 命中装反模式 exit 1 | ✅ 同次运行报第 59 行 `-> config id assigned into a connection/session key (reversed form)` |
| c. 白名单精确性 | 从 `ALLOWLIST` 删除 schemaDiff.ts 条目后重跑 | 该历史注释应触发违例 | ✅ exit 1，报 `src/commands/schemaDiff.ts:47: /// connectionId(dbSessionId) terminology (v1 configs with configId are rejected).` —— 证明白名单是行级精确豁免而非整文件忽略 |

恢复方式与证据：实验 a/b 文件经备份 cp 回写，sha256 前后一致（`2786dbf1…`）；实验 c 脚本经 `git checkout --` 恢复，与 `git show HEAD:` 的 sha256 一致（`68a34893…`）。恢复后复跑守护 exit 0、`test:ids` exit 0，最终 `git status --porcelain` 为空（0 个脏文件）。

### 3. 单测覆盖审阅

`scripts/__tests__/check-id-terminology.test.ts` 4 用例在临时目录构造真实文件运行被测函数，断言有效：

- 「returns 1 and reports file:line」断言错误串含 `src/stale.ts:2` 与违规 token 本体 → **真实覆盖 file:line 输出**；
- 「flags the reversed form」覆盖装反模式（仅断言 exit 1，未断言原因文案，见建议区 S3）;
- 「suppresses allow-listed lines only when file and line both match」最有说服力：同文件第 1 行放白名单注释、第 2 行放相同 token → 断言 exit 1 且报 `schemaDiff.ts:2`；只留白名单行 → 断言 exit 0。**真实覆盖了白名单的"文件+行"双条件精确性**。

## T4 AGENTS.md 与索引审计

- 新增「ID 术语」小节（AGENTS.md:123-126，标题 + 1 段正文）：≤5 行达标；内容（connectionId=持久化配置 id 原 configId 落盘 / dbSessionId=运行时会话 id 永不落盘 / 语义分工 / 新代码不得依赖 resolve_session 双模回退）与源码及 naming.md 一致；链接 `docs/architecture/naming.md` 目标存在可达。
- MCP 段（AGENTS.md:98）已改为「DB tools 使用持久化 `connection_id`」，diff 确认由 `config_id` 改来。
- 全文检索无其他旧语义残留（仅 ID 术语小节中"原 configId"为有意的历史说明）。
- 双索引行均存在且链接有效：`docs/architecture/README.md:190` `[ID 术语规范](naming.md)`（同目录相对链接，文件存在）；`docs/README.md:64` `[naming](architecture/naming.md)`（自 docs/ 目录相对链接有效）。naming.md 自身回链 `[返回架构总览](README.md)` 及 `§5 → ../../CHANGELOG.md`（仓库根 CHANGELOG.md 存在）均可解析。

## T5 BUG-007 复核

修复前：`toggleDb` 需要 `configId` + `connectionId` + `dbName`（错误描述）。修复后叙述与代码逐一比对：

| 修复后陈述 | 代码事实 | 结论 |
|------------|----------|------|
| `toggleDb` 实际签名 `(connectionId, dbSessionId, dbName)` | `ConnectionNavigatorTree.tsx:811-812` `async (connectionId: string, dbSessionId: string, dbName: string)` | ✅ |
| click handler 从 row data 取三值 | `:2181` `onClick={() => void toggleDb(row.connectionId, row.dbSessionId, row.dbName)}` | ✅ |
| 展开态 key 用 connectionId（`${connectionId}::${dbName}`） | `:813` `const dbKey = \`${connectionId}::${dbName}\`;` 用于 expandedDbs 增删 | ✅ |
| 拉表走 dbSessionId | `:828` `const tableKey = \`${dbSessionId}::${dbName}\`;` → `reloadDbTables(dbSessionId, dbName)`（`:498-504`）→ `databaseCommands.getTables(dbSessionId, dbName)` | ✅ |

修复后的叙述与组件真实行为完全一致，BUG-007 修复成立。

## T6 缺陷清单

**缺陷：无。**

建议区（不影响通过结论）：

- **S1（口径说明）**：任务摘要称 W5 改动 8 文件，实际提交含 `ID_RENAME_PROGRESS.md` 共 9 文件；差异仅为进度记账文件，非交付物缺失。
- **S2（健壮性）**：`check-id-terminology.mjs` 会递归扫描 `packages/` 下 git 驱动 clone 目录（如 `packages/drivers/{kiwi,…}`，gitignored 非 workspace member），第三方插件仓代码不在本项目术语约束内，本地存在 clone 时可能引入误报噪音。可考虑将非 path 驱动 clone 目录加入 `SKIP_DIR_NAMES` 或维持现状并在 CI 固定环境。当前基线 891 文件扫描零误报。
- **S3（测试增强，风格级）**：「flags the reversed form」用例仅断言 exit 1，可补一条错误文案断言（如同用例 2 对 file:line 的断言），防止未来 reason 文案退化不被察觉。
- **S4（文档微调，可选）**：禁止模式 `/\bconnectionId\s*:\s*["'`]?config\b/` 也会命中"新语义下合法"的字面量 `{ connectionId: config.id }`（e2e 白名单第 2 条正因此存在）。脚本注释已解释动机，但 naming.md §6 仅泛称"装反形态"；可在 §6 补一句提示，避免后续贡献者遇到同类误报时困惑。

---

## 结论

**通过。** 五项门禁全绿（vitest 240 文件 / 1894 用例；tsc 干净；cargo 仅既有 2 例 sandbox 失败）；naming.md 六组关键论断、五个双模标注锚点、决策表方向全部与源码一致；守护脚本判别力经正向（exit 0）+ 三组反向注入（均 exit 1 且 file:line 报告准确）验证，白名单为行级精确豁免；AGENTS.md 与双索引合规；BUG-007 修复叙述与 `ConnectionNavigatorTree.tsx` 真实签名及行为逐点吻合。测试过程零残留（工作区干净，未触碰 ID_RENAME_PROGRESS.md）。
