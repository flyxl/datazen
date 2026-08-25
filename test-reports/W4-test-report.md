# W4 独立测试报告 — 外部契约与文档对齐

- **被测提交**：`d3f67525`（`docs(ids): W4 external contract & documentation alignment`）
- **分支 / 基线**：`feature/db-session-id-rename`，merge-base `c8154e4b` 与 `main`
- **测试 agent**：独立全新会话（只测试、不修复）
- **执行环境**：macOS 沙箱；`CARGO_TARGET_DIR=/Users/wuxiaolong/code/rust-projects/datazen/target`
- **结论**：✅ **通过**（1 个 P3 既有遗留缺陷 + 4 条观察项，均不阻塞 W4）

---

## 执行摘要

W4 提交实际改动 **12 个文件**（+225/−144）：新建 `CHANGELOG.md`、9 个活文档清扫、`src-tauri/src/mcp/server.rs` 测试加固、以及 `ID_RENAME_PROGRESS.md` 进度更新。行为代码零改动属实（server.rs 仅 `#[cfg(test)]` 内变更）。CHANGELOG 六条破坏性变更全部与代码现实相符；六处文档示例抽查全部与现行实现一致；zh/en/site 双语替换完全平行。门禁三项达标（lib 1126 过/2 既有沙箱失败、vitest 1890 全绿、SEO 触碰文件全过）；mcp/server.rs 行覆盖率 **87.60%** ≥ 80%。发现 1 处未被 W4 触碰的活文档残留术语（`docs/TODO-screenshots.md:35`），使"grep 遗漏=0"口径在把 docs/ 根下非归档文件计入活文档时不成立。

---

## T1 独立门禁执行

| 门禁 | 命令 | 结果 | 判定 |
|------|------|------|------|
| Host Rust 单测 | `cargo test -p datazen --lib` | **1126 passed, 2 failed**, 2 ignored | ✅（仅既有失败） |
| 前端单测 | `npx vitest run` | **239 files / 1890 tests 全绿**（42.04s） | ✅ |
| Site SEO | `node scripts/check-site-seo.mjs` | exit 1，**8 check(s) failed** | ✅*（触碰文件全过，见下） |
| 规模核对 | `git diff main --stat` / `git show d3f67525 --shortstat` | 分支整体 274 files；W4 提交 **12 files, +225/−144** | ⚠️ 见 OBS-1 |

- 2 个失败用例均为既有限定：`tests::resolve_log_settings_defaults_without_settings_file`（`PermissionDenied: Operation not permitted`）、`tests::resolve_log_settings_reads_custom_level_and_path`（PoisonError 连锁）——与已知 sandbox 环境限制一致。
- **SEO 8 处失败根因分析（独立核实）**：8 条 FAIL 全部集中在 **W4 未触碰**的 `workflow.html` / `zh/workflow.html` / `databases.html` / `zh/databases.html`（每页 hreflang zh-CN/x-default 各 1 条）。这些页面在本分支上与 main 的分叉点版本逐字节相同；失败原因是其 `<link>` 标签属性被 prettier 折行，而检查脚本按"单行子串"匹配。**main 在分支切出后已由 `ed0b5de7` 加入 `normalizeLinks` 修复该误报**（merge-base 版本无此函数，main tip 有），分支基线落后导致复现。合并后自然消失，无需本分支处理 → 记 OBS-4。
- W4 触碰的 `site/docs.html` / `site/zh/docs.html` 在 SEO 检查中 **零失败**。

---

## T2 准确性审计

### T2.1 CHANGELOG 六条 ↔ 代码现实映射表

| # | CHANGELOG 声明 | 代码证据 | 判定 |
|---|----------------|----------|------|
| ① | MCP 工具入参 `config_id`→`connection_id`，旧键被拒、无别名 | `mcp/server.rs:24-93` 全部入参结构体字段为 `connection_id`；守护测试 `query_input_accepts_connection_id`(:918) / `query_input_rejects_removed_config_id_field`(:925) 断言旧键 payload 反序列化失败；`tool_help.rs` 示例全部为 `connection_id` | ✅ 属实（机制细节见 OBS-2） |
| ② | 资源模板 URI + query-history 输出字段改名 | `server.rs:863` `ResourceTemplate::new("datazen://schema/{connectionId}/{database}")`；`store/models.rs:11` `QueryHistoryEntry` 带 `#[serde(rename_all="camelCase")]` 字段 `connection_id`→序列化为 `"connectionId"`；W4 加固测试 `mcp_read_resource_inner_paths`(:1272) 预置一条历史并断言输出含 `\"connectionId\"` 且不含 `configId`（diff 确认为本提交新增，且仅测试代码） | ✅ 属实 |
| ③ | history.sqlite v3→v4 列名迁移存在、启动自动一次性执行、数据保留 | `history_db.rs:211-247` v<4 时 `ALTER TABLE … RENAME COLUMN config_id TO connection_id`（query_history + favorite_queries，带列存在探测，不删数据）；调用链：`HistoryDb::open`→`run_migrations()`(:89) ← `Store::init`(store/mod.rs:105) / `Store::init_with_path`(:129) ← GUI 启动(lib.rs:553) 与无头 `--mcp` 启动(lib.rs:667)；`schema_version` 表幂等护栏 → "自动一次性"表述成立 | ✅ 属实 |
| ④ | Schema Diff 配置 JSON v2，导入明确拒收 v1 | `src/commands/schemaDiff.ts:48-56` `SchemaDiffConfigJson { version: 2; sourceConnectionId; targetConnectionId }`；`SchemaDiffWindow.tsx:264-266` `cfg.version !== 2 \|\| !cfg.sourceConnectionId \|\| !cfg.targetConnectionId` → throw `t('schemaDiff.invalidConfig')` | ✅ 属实 |
| ⑤ | SyncTask 字段改名（运行会话 + 归属连接双字段），旧载荷无法反序列化 | `store/models.rs:41-68`：`source_db_session_id/target_db_session_id` + `source_connection_id/target_connection_id`（camelCase serde）；旧键载荷缺必填新键必然反序列化失败；`store/sync_tasks.rs` `load_json_file::<Vec<SyncTask>>…unwrap_or_default()` → 旧任务静默弃置、应用正常启动，与"需重建后重跑"的指引吻合 | ✅ 属实 |
| ⑥ | 插件桥 `command.invoke` 参数 `configId`→`connectionId`，缺/错键报错 `{connectionId, command, args?}`；plugin-sdk 同步 | `src/lib/extensionBridge.ts:209-212` 校验 `!connectionId \|\| !command` → `BRIDGE_ERROR.BAD_REQUEST 'command.invoke requires {connectionId, command, args?}'`（消息文本与 CHANGELOG 一致）；`packages/extension-sdk/src/bridge.ts:185` 文档即 `dz.command.invoke({ connectionId, command: 'query', args })`；守护测试 extensionBridge.test.ts 多处覆盖 | ✅ 属实 |

**六条全部与代码相符，无事实性错误。**

### T2.2 文档示例 vs 现行实现抽查（6 处 ≥ 5）

1. **services.md §1.1 ↔ `services/connection_manager.rs`** — ✅
   `connect(connection_id)->Result<String,ConnectionError>`、`disconnect(db_session_id)`、`get_session` 签名；错误变体 `ConnectionConfigNotFound(String)` / `DbSessionNotFound(String)` / `DriverNotFound(DatabaseType)`；`idle_timeout = Duration::from_secs(1800)` 全部与源码一致（connection_manager.rs:81-115-277-310-105）。示例自带"设计示意、实际实现含 connect_locks 增强"免责声明。§双模解析描述与 `resolve_session`(:345，先 db_session_id 后回退建连) 及 `db_tools::resolve_connection` 一致。
2. **components.md 命令封装 ↔ `src/commands/*.ts`** — ✅
   逐键核对：`connect({connectionId})`、`pingConnection/releaseConnection/disconnect({dbSessionId})`（connection.ts:16-23）、`getDatabases/getTables/getTableSchema({dbSessionId,…})`（database.ts:29-42）、`getExplain/cancelQuery/getQueryHistory(limit, connectionId)`（query.ts:41-48）全部逐字一致；`get_er_data(db_session_id, database)` 与 schema.rs:188 一致。executeQuery 示例形式差异见 OBS-3。
3. **state.md panelStore ↔ `src/stores/panelStore.ts`** — ✅
   文档列出的 14 个 action 签名（addPanel/removePanel/removeAllForConnection/setActivePanel/updatePanel/closeOtherPanels/closeAllPanels/updateSql/executeQuery/cancelQuery/loadHistory/loadFavorites/addFavorite/deleteFavorite）与 panelStore.ts:193-224 **逐一 verbatim 相符**，历史/收藏归属参数均为 `connectionId`。
4. **commands.md 历史归属 ↔ `owner_connection_id` 实名** — ✅
   `ConnectionManager::owner_connection_id(&self, db_session_id) -> Option<String>`（connection_manager.rs:200）真实存在；ai.rs:2009、driver_command.rs:122 按此消费；commands.md:147-152 示例语义正确，且 ai.rs:2619 另有守护断言防止以配置 id 误调。
5. **ops-dashboard 指南（configId→connectionId 替换后语义）** — ✅
   "看板只引用已保存连接的 connectionId"：监控注册键确为 `monitor:{connection_id}`（monitor/connections.rs:16），按 connection_id 建 `connect_locks` 串行（connections.rs:36-60）+ 信号量限并发（engine.rs:100-117），指南中"对同一 connectionId 串行""monitor:{connectionId} 句柄"等表述与新代码一一对应；"已保存连接"语境下新术语 connectionId 正确。
6. **mcp.md ↔ server.rs** — ✅
   Tools 清单 = `MCP_ALL_TOOLS` 10 项（server.rs:136-147）；Resources 四项及模板 URI 一致；§1.3 术语表与 §1.3 历史演进注记准确。

### T2.3 中英 / site 一致性

- `ops-dashboard-guide.zh-CN.md` 与 `.en.md`：各 **7 处** `connectionId` 替换、**0 处** configId 残留，逐行平行（对 W4 diff 逐条比对通过）。
- `site/docs.html` 与 `site/zh/docs.html`：各 **7 处**平行替换（en 页 7 + zh 页 7），无单侧遗漏。

---

## T3 覆盖率

```
cargo llvm-cov -p datazen --lib --summary-only -- --skip resolve_log_settings
mcp/server.rs      lines 87.60%   regions 88.10%   functions 79.10%
TOTAL (lib)        lines 77.48%
```

- **mcp/server.rs 行覆盖 87.60%（区域覆盖 88.10%）≥ 80% 达标**，与 W2 的 88.11% 基本持平（差异主要为统计口径）。
- 注：两个沙箱受阻用例会使 llvm-cov 在产出汇总前中止，故加 `-- --skip resolve_log_settings` 后重跑（环境限制，非代码问题）。

---

## T4 文档验证清单（10 条）

| # | 项目 | 结果 |
|---|------|------|
| ① | CHANGELOG 六条 ↔ 代码映射 | ✅ 见 T2.1 表，6/6 属实 |
| ② | 活文档残留扫描（独立分类，排除 rfc/reviews/test-results/todo/） | ⚠️ docs/(非归档)共 42 个 md：3 处带"历史演进/不再出现旧键"标签的合法说明（mcp.md:42、61；services.md:14）＋ **1 处未标注残留 → DEFECT-1**（docs/TODO-screenshots.md:35）。site HTML、README.md、docs/features 全部 0 残留 |
| ③ | 编辑过的 md 内部相对链接可达性 | ✅ 8 个编辑过的 md 全部相对链接目标存在，0 死链 |
| ④ | zh/en 指南术语一致 | ✅ 7/7 平行替换，双侧零残留 |
| ⑤ | site 双语同步 | ✅ en/zh 各 7 处同步替换 |
| ⑥ | AGENTS.md 未被触碰 | ✅ 整个分支（main..HEAD）无任何提交触碰 AGENTS.md，diff 为空 |
| ⑦ | SEO 脚本触碰文件通过 | ✅ site/docs.html 与 zh/docs.html 无任何 FAIL |
| ⑧ | CHANGELOG 迁移指引可操作性 | ✅ 6 条均可操作（见下） |
| ⑨ | 行为零改动确认 | ✅ W4 对 server.rs 的改动全部位于 `#[cfg(test)] mod tests` 内 |
| ⑩ | 历史档案未触碰 | ✅ rfc/reviews/test-results 无 W4 改动；ID_RENAME_PROGRESS.md 更新属进度记录惯例 |

**⑧ 迁移指引逐条评估**：
1. MCP 工具：改键名即可、取值语义不变（list_connections 返回 id）——信息充分 ✅
2. MCP 资源：客户端按新键读取、URI 用 `{connectionId}` 占位——充分 ✅
3. SQLite：直接升级免手工；外部直读脚本改列名——充分 ✅
4. Schema Diff：明示 v1 无法导入、需界面重选重导——充分（未提供自动转换但已如实声明）✅
5. 同步任务：升级前完成进行中任务、中断任务重建——充分（与实现"旧载荷静默弃置"行为吻合）✅
6. 插件桥：改参数键 + 升级配套 plugin-sdk——充分 ✅

---

## T5 缺陷清单与建议

### 缺陷（真实问题）

**DEFECT-1（P3，既有遗留、非 W4 触碰）：活文档残留旧术语且与现行代码签名不符**
- 位置：`docs/TODO-screenshots.md:35`
- 问题："toggleDb 需要 `configId` + `connectionId` + `dbName` 三个参数"。现行签名为 `toggleDb(connectionId, dbSessionId, dbName)`（src/windows/connection/ConnectionNavigatorTree.tsx:810-811、调用点 :2181），`configId` 已不存在。该文件位于 docs/ 根目录、不在 rfc/reviews/test-results/todo 归档范围，也未被 W4 触碰，因此 W4 提交信息与进度表中"grep 遗漏=0"的口径仅在"只扫 9 个指定活文档"意义上成立。
- 影响：低（截图排障记录，非外部契约），但属于事实性过期描述。
- 修正建议：将该句改为 "`toggleDb` 需要 `connectionId` + `dbSessionId` + `dbName`"；或将 `docs/TODO-screenshots.md` 明确移入归档目录并在残留扫描口径中排除。

### 观察 / 建议（非缺陷）

- **OBS-1**：W4 声称"11 文件"，提交实际 **12 文件**（另含 ID_RENAME_PROGRESS.md +7 行进度更新）。无实质影响，建议后续声明与 shortstat 对齐。
- **OBS-2**：①的"旧键名会被直接拒绝"机制上是"缺少必填 `connection_id` 导致反序列化失败"，入参结构体并未启用 `deny_unknown_fields`：同时携带 `config_id` + `connection_id` 的载荷会被接受并静默忽略旧键。典型迁移场景（只发旧键）确实被拒，表述基本成立；如需严格闭环可给工具入参结构体加 `#[serde(deny_unknown_fields)]` 或在 CHANGELOG 补一句"混合键时旧键被忽略"。
- **OBS-3**：components.md 的 executeQuery 示例写作扁平化 `invoke('execute_driver_command', { dbSessionId, command:'query', input:{sql} })`；实际经 `driverCommands.execute` 封装、envelope 为 `{ request }`（driver.ts:34-36）。IPC 名与内层键一致、语义无误，仅形式简化，可在示例旁标注真实封装路径。
- **OBS-4**：SEO 8 处失败的根因是分支基线落后 main（main 的 `ed0b5de7` 已加 normalizeLinks 修复 prettier 折行误报），合并后消失，无需在本分支处理；报告期使用该脚本作门禁时建议注明此背景。

---

## 结论

**通过。**

- 门禁：lib 1126 过 / 2 既有沙箱失败；vitest 1890 绿（239 文件）；SEO 触碰文件全过（8 处失败均为未触碰页面的基线问题，main 侧已有修复）；W4 提交规模 12 文件 +225/−144。
- CHANGELOG 六条破坏性变更 6/6 与代码相符；文档示例抽查 6/6 一致；双语/site 完全同步；行为代码零改动属实；mcp/server.rs 覆盖 87.60% ≥ 80%。
- 遗留 1 个 P3 缺陷（DEFECT-1，既有、不阻塞）与 4 条观察项（OBS-1~4），供后续工作项收口。
