# 测试-修复循环：查询历史 database/schema 分组（Test Loop）

> 「测试 agent ↔ 开发 agent ↔ 验证 agent」循环的唯一共享记忆。
> 规则：测试 agent 只记录、不改产品代码；开发 agent 只修列出的 bug；验证 agent 复测并回写结论。
> 注：原 docs/progress/ 已被文档重构移除，本文件迁移至 docs/reviews/。

## 被测对象（功能改动摘要）

查询历史按 database/schema 分组：
- 后端：query_history 表迁移 v3 加 schema 列；get_query_history(limit, config_id, database, schema) 过滤；记录点写入会话真实 database；去重作用域 (config_id, database, schema)
- 前端：src/lib/historyGroups.ts 分组纯函数；QueryPanel 历史侧栏默认「当前库」作用域 + 可切「全部」（分组头）；i18n 键 query.historyScopeCurrent/historyScopeAll/historyUnknownDb
- 附带产品变更：backup.rs 新增 execute_sql_file 命令（webdriver-only 路径直调，镜像 restore_database 模式），lib.rs 已注册

## 环境须知（agent 必读）

1. Rust 测试必须用包装脚本：node scripts/with-plugin-inject.mjs --drivers=basic -- cargo test -p datazen --lib
   （裸 cargo test 因 capabilities 缺 redis 权限失败是已知环境现象，不是 bug）
2. 前端类型检查：npx tsc -p tsconfig.json --noEmit；单测：npx vitest run [路径]
3. E2E 需要 webdriver 完整构建 + 本地 PG/MySQL；运行级 E2E 在 bug 清零后统一执行
4. 数据库可用性未知，不要假设 PG/MySQL 已启动
5. 文档已重构：E2E 指南现位于 docs/development/e2e-testing.md

## Bug 登记表

| ID | 严重度 | 文件/位置 | 描述 | 发现轮 | 状态 | 验证 |
|----|--------|-----------|------|--------|------|------|
| BUG-R1-001 | P1 | src-tauri/src/store/key_store.rs:391（波及 :486，锁 :277） | 门禁命令确定性挂起：`keyring_forced_without_legacy_key_fails_closed_when_keyring_unavailable` 强制 `DATAZEN_KEYRING=keyring` 触碰真实 macOS 钥匙串，Security 调用无限阻塞（两次独立运行均冻结于同一位置 >5min，日志输出 "has been running for over 60 seconds"）；其持有 `ENV_LOCK` 令 `load_or_create_from_file_explicit_mode_skips_keyring` 锁饿死假死（两测试同时报慢）。外部设 `DATAZEN_KEYRING=file` 无效（测试内部 set_var 覆盖）。同文件 `keyring_creates_and_reloads_master_key`(504) 有 `#[ignore]` 保护而这两个没有。**存量问题（本功能未改此文件）**，但使规定门禁命令不可用；临时绕过：追加 `-- --skip store::key_store::tests::keyring_forced_without_legacy_key_fails_closed_when_keyring_unavailable`。建议比照 504 加 `#[ignore]` 或给钥匙串调用加超时/可用性预检 | ROUND-1 | verified（修复方式：钥匙串调用改走子线程 + 10s recv_timeout 超时守卫，超时打印提示并跳过、释放 ENV_LOCK） | ROUND-3 复测通过：DATAZEN_KEYRING=file 包装脚本全量（无 --skip）两次独立运行均完整结束、无挂死（测试本体 7.03s / 7.36s，无 >60s 慢测试告警）；失败仅 resolve_log_settings ×2 沙箱 EPERM（按口径记 SKIP）与 inspect_plugin flake 偶发 1 例（隔离重跑通过，另登记 BUG-R3-001） |
| BUG-R1-002 | P2 | src-tauri/src/commands/driver_command.rs:148 | `let _ = state.store.add_query_history(entry).await;` 完全吞掉持久化错误且无日志：SQLite 写失败（磁盘满/库损坏/锁超时）时该条历史静默丢失。对比同函数 119-126 行 config 解析失败至少有 warn 日志。建议至少 `tracing::warn!` 记录 err | ROUND-1 | verified | ROUND-3 读码确认 :148-152 = if let Err(e) + tracing::warn!(error = %e, "add_query_history failed")，无 ?/unwrap/提前返回，控制流不变；driver_command 相关测试（mod tests :548，四处调用点）含于 cargo 全量套件并通过 |
| BUG-R1-003 | P2 | src-tauri/src/commands/driver_command.rs:140（关联 models.rs:17-19、history_db.rs:194/222/301-308） | `schema: None` 硬编码，且这是生产代码唯一 `QueryHistoryEntry` 构造点（grep 证实其余均为测试）→ v3 新增的 `schema` 列、`get_query_history` 的 schema 过滤、去重第三维 `(config_id,database,schema)`、前端 `types.schema` 全部成无写入方的预留死代码，「按 database/schema 分组」实际仅 database 生效。models.rs 注释称 PG search_path 未跟踪属已知留白，需产品确认是否在本轮验收范围。附带缺口：无「同 config+db 不同 schema 不互相去重」用例 | ROUND-1 | wontfix（用户裁决方案 A：本轮按 database 分组交付；schema 列保留为扩展点，会话级 schema 跟踪留待后续迭代，见轮次日志 ROUND-2 裁决记录） | 无需复测 |
| BUG-R1-004 | P2 | src/windows/connection/QueryPanel.tsx:649-652 | `historyScopeMode='current'` 且 `findGroupForDatabase` 返回 null（单库驱动 currentDatabase=null 必然发生；多库当前库暂无历史也发生）时静默回退渲染**全部分组**，「当前库」按钮仍高亮、无任何提示 → 用户会把其他库的历史 SQL 应用到当前上下文，触发该功能本要避免的 "table not exist"。建议回退时显示提示文案或禁用 current 按钮 | ROUND-1 | verified | ROUND-3 读码确认：QueryPanel.tsx:647 historyScopeFallback=current 且 findGroupForDatabase 为空；:1217 渲染条件恰为「current+无匹配组+history.length>0」；:1219 data-testid=history-scope-fallback-hint 存在；i18n 三处一致（en.ts:621 / zh-CN.ts:606 / 组件 :1222）；:652-655 回退仍渲染全部分组（单库行为保留）；QueryPanelHistory.test.tsx 用例④组件级覆盖 |
| BUG-R1-005 | P2 | src/stores/panelStore.ts:433（关联 history_db.rs:12） | `loadHistory` 固定 `getQueryHistory(100, configId)`，后端上限却是 `MAX_QUERY_HISTORY=1000`：「当前库」作用域只在最新 100 条窗口内过滤，当前库较老历史不可见、分组计数失真。100 条上限是存量行为，新作用域语义放大了误导面。建议提高拉取量或按 database 分页 | ROUND-1 | verified（ROUND-4b 复测通过：断言已同步 1000 且无其他漂移，src/stores 全绿；ROUND-4 修正后 ROUND-3 打回项闭环） | ROUND-3：panelStore.ts:433=getQueryHistory(1000, configId) ✅（全 src 唯一生产调用点）；但 panelStore.test.ts:476/:531 仍断言 toHaveBeenCalledWith(100,'cfg-1') → loadHistory/openQueryHistory 两用例必败，单文件复跑 2 failed，npx vitest run 全量 1862 通过/2 失败、真实 exit 1；ROUND-2 门禁只跑 vitest 子集未暴露。ROUND-4：panelStore.test.ts:476/:531 两处断言 100→1000 同步修正，npx vitest run src/stores → 13 文件 / 228 用例全绿（含 panelStore.test.ts 39 例）。ROUND-4b 独立复测：读码确认 :476/:531 均为 toHaveBeenCalledWith(1000,'cfg-1')，git diff 该文件仅此 2 行变更（100→1000）、无其他断言漂移；亲自跑 npx vitest run src/stores = 13 文件/228 用例全绿（exit 0） |
| BUG-R1-006 | P2 | src/lib/historyGroups.ts:49-52 | 组排序用 `executedAt` 字符串 `localeCompare`：RFC3339 同秒内混合精度时（chrono AutoSi 对整秒不带小数位：`…T00:00:00Z` vs `…T00:00:00.5Z`，字典序 `.` < `Z` 与时间序相反），跨秒边界组序可能颠倒。概率低影响小，建议改毫秒数值比较 | ROUND-1 | verified | ROUND-3 读码+心算：historyGroups.ts:51-58 比较器 bn-an 数值降序、任一侧 NaN 回退 bt.localeCompare(at)；构造 X=…T00:00:00Z(Date.parse=1787270400000) vs Y=…T00:00:00.5Z(1787270400500)：bn-an=+500>0→更新者 Y 在前 ✅（旧字典序 '.'(46)<'Z'(90) 会把更旧的 X 排前=错序，确被消除）；跨秒 c=…T00:00:01Z(1787270401000) 数值序亦正确；组内保持后端 newest-first |
| BUG-R1-007 | P2 | src/windows/connection/__tests__/QueryPanel.executeCancel.test.tsx:108-112 | 测试缺口（假绿风险）：QueryPanel 唯一组件测试 mock 掉 `queryHistory/loadHistory`，历史侧栏全部新 UI 分支（作用域切换、搜索×作用域叠加、未知库分组头、clear 后重算、BUG-R1-004 回退分支）组件级零覆盖；`historyGroups.test.ts` 未覆盖 `database: undefined` 与同 executedAt 排序稳定性。lib 层 7 例绿 ≠ UI 分支被验证 | ROUND-1 | verified | ROUND-3 通读 217 行：真实 zustand setState 注入 5 条真实形态历史（跨 app/analytics、毫秒混合精度、newest-first），mock 仅外围 hook/重组件，分组与渲染走真实实现——非 mock 数据源的假绿；5 用例对应声明①-⑤（含回退分支④、空历史⑤）；单独 npx vitest run 该文件 5 passed (5)、exit 0 |

| BUG-R3-001 | P3 | src-tauri/src/plugins/install.rs:849（inspect_plugin_package_returns_manifest_without_installing） | 偶发 flake（非本轮功能改动引入）：并行全量下 before 快照捕到他测残留的共享暂存目录内容（assert left:0 right:1）；ROUND-2 已观察 1 次、ROUND-3 全量复现 1 次；隔离重跑（--exact --test-threads=1）稳定通过（0.05s ok）。建议该测试使用独占临时目录或与相关测试串行化 | ROUND-3 | verified（ROUND-4b 复测通过） | ROUND-4：生产暂存根硬编码 `std::env::temp_dir()`，改独占目录需改 `inspect_plugin_package` 签名并波及调用方（超出单测试文件范围，按打回指令不动生产代码）；本 crate 内创建 `.datazen-inspect-*` 的测试全部位于 install.rs tests 模块，新增文件级 `static INSPECT_TMP_LOCK: Mutex<()>`，三个触发 inspect 的测试全程持锁串行（returns_manifest / accepts_top_level_folder / rejects_invalid），count 断言不再互踩。install 套件默认并行线程 10 连跑全绿（15 passed ×10）。ROUND-4b 独立复测：读码确认 INSPECT_TMP_LOCK 为 tests 模块文件级 static Mutex<()>（install.rs:822，helper 含 PoisonError 恢复），触发 inspect 的全部三个测试（:843/:868/:886，第三个体内 3 次 inspect 调用）均以首行 guard 全程持锁、无绕锁用例；git diff 确认 install.rs 仅 +17 行且全部位于 mod tests（生产代码零改动）；亲自连跑 5 次（默认并行线程）= 5×「ok. 15 passed; 0 failed」，RUN1-5 EXIT=0，无 flake |

状态取值：open → fixing → fixed → verified / wontfix(需注明理由)

## 轮次进度日志

### ROUND-0（编排者）
- 功能实现完成：后端 48 相关单测绿；前端 historyGroups 7 单测绿；tsc 无错

### ROUND-0.5（编排者）
- 新增 E2E spec ×3：execute-sql-file.ts（SF-E01/E02）、query-history.ts（QH-001~005）、ops-server-status-processes.ts（OPS-SS/PL 共 4 例）；均已过 e2e tsconfig 校验
- 产品代码变更：execute_sql_file 命令（见上）

### ROUND-1（测试 agent）

**执行命令与结果**
1. `npx vitest run` → ✅ 234 文件 / 1859 用例全部通过（47.18s），含 historyGroups.test.ts（7 例）、panelStore.loadHistory IPC 用例
2. `npx tsc -p tsconfig.json --noEmit` → ✅ exit 0，无类型错误
3. `node scripts/with-plugin-inject.mjs --drivers=basic -- cargo test -p datazen --lib`：
   - 直跑两次均挂死于 key_store 钥匙串用例（BUG-R1-001，设 `DATAZEN_KEYRING=file` 亦无效）
   - 追加 `-- --skip store::key_store::tests::keyring_forced_without_legacy_key_fails_closed_when_keyring_unavailable` 后完整跑完：**1103 通过 / 2 失败(环境沙箱) / 2 ignored / 1 filtered out**（测试本体 2.99s）
4. i18n 比对：临时脚本（/tmp/dsh-i18n-query-key-check.mjs，未入仓库）提取 en.ts 与 zh-CN.ts 全部 `query.*` key：**54 = 54，双向差集为空**；historyScopeCurrent/historyScopeAll/historyUnknownDb 两侧均存在 ✅

**SKIP / 环境受限清单（不计 BUG）**
- `keyring_forced_without_legacy_key_fails_closed…`：--skip 绕过（挂起本身已登记 BUG-R1-001）；`load_or_create_from_file_explicit_mode_skips_keyring` 系被前者持锁饿死，跳过前者后随套件正常通过
- lib.rs 的 `resolve_log_settings_defaults_without_settings_file` / `resolve_log_settings_reads_custom_level_and_path` 共 2 例失败为**执行环境文件沙箱限制**：测试写真实 app-data 目录（~/Library/Application Support/com.datazen.app/settings.json）被 EPERM 拒绝（独立探针复现 Operation not permitted），第二例为 SETTINGS_FILE_LOCK 被第一例 panic 毒化的连锁失败。正常开发机不受影响，不修
- `#[ignore]` 用例 2 例按设计跳过；本轮未运行 E2E/wdio/tauri build；套件无真实 DB 连接类跳过项（test result 无 skipped 计数）

**走查结论（细节见 Bug 登记表）**
- 迁移 v3 幂等 ✅：新库 init_schema（无 schema 列，v2 先做 connection_id→config_id 改名并清空旧数据）→ v3 ALTER 补列；老 v2 库 → v3 补列；version=3 重开全跳过；schema_version_survives_reopen 等用例绿
- get_query_history 注入面 ✅：where 子句仅拼接占位符序号，值全部参数绑定；空串 schema 哨兵→`IS NULL`、去重 `schema IS ?3` 的 NULL 语义正确
- 其他调用点行为不变 ✅：ai.rs:2013、mcp/server.rs:618 均 `(…, None, None)`；store/tests.rs、commands/history.rs、driver_command tests 仅签名适配
- QueryPanel 数据流 ✅：currentDatabase 经 ConnectionPage activePanel→setActiveConnection 同步（多连接不串线）；分组 key 来自 Map 去重保证渲染 key 唯一；clearQueryHistory 后 loadHistory 触发 useMemo 重算；单库 currentDatabase=null 时 UI 不空白（走全部分组分支——但其静默性见 BUG-R1-004）
- 记录点语义 ✅：database 取自 ActiveConnection.config.database，`set_active_database`（use_database 后）会更新该值 → 反映会话当前库；断连时降级空串仍记录（归未知库组）；driverType 无连接执行按设计不记录
- 待修问题集中在 BUG-R1-002~007：记录点吞错、schema 维度无写入方（需产品裁决）、current 作用域静默回退、100 条窗口截断、时间戳字典序排序、组件级测试缺口

### ROUND-2/3（编排者）
- 外部文档重构确认：docs/progress→docs/{development,features,reviews}；源码功能零影响，测试有效性不受损
- 三份工作文档已在 docs/reviews/ 恢复；黑盒清单落地 test/manual-checklist-next-move.md
- 已通知测试 agent 新路径（send_message 3343008f）

### ROUND-2（开发 agent）

修复范围：BUG-R1-002 / 004 / 005 / 006 / 007（BUG-R1-003 按产品裁决保持现状；BUG-R1-001 已由编排者修复，本轮确认编译通过且套件无挂死）。

**改动明细**

- **BUG-R1-002**：`src-tauri/src/commands/driver_command.rs:148-152` —— `let _ = add_query_history(...)` 改为 `if let Err(e)` + `tracing::warn!(error = %e, "add_query_history failed")`；控制流不变、不向上抛。
- **BUG-R1-004**：`src/windows/connection/QueryPanel.tsx` —— 新增派生量 `historyScopeFallback`（current 模式且 `findGroupForDatabase` 为空），在作用域切换按钮下方条件渲染 `data-testid="history-scope-fallback-hint"` 提示行（仅回退发生且 `history.length > 0` 时显示）；回退渲染全部分组的行为保留（单库驱动需要）。i18n 仅改 en/zh-CN：`en.ts:621` `'query.historyScopeFallbackHint': 'No history recorded for this database yet'`；`zh-CN.ts:606` = '当前数据库暂无历史记录'。
- **BUG-R1-005**：`src/stores/panelStore.ts:433` —— `loadHistory` 拉取上限 100 → 1000（对齐后端 MAX_QUERY_HISTORY）。
- **BUG-R1-006**：`src/lib/historyGroups.ts:48-57` —— 组排序改为 `Date.parse` 数值降序，任一侧 NaN 时回退 `localeCompare`（消除 RFC3339 混合精度字典序颠倒）。
- **BUG-R1-007**：新增 `src/windows/connection/__tests__/QueryPanelHistory.test.tsx`（5 用例）。数据为真实形态 history（5 条、跨 app/analytics 两库、executedAt 含毫秒混合精度、newest-first），按既有方式 mock usePanelStore/useSchemaStore 而非历史数据源。覆盖：① 默认 current 只显示当前库条目且无分组头；② 切 all 显示分组头+计数并保持 recency 顺序（'analytics (2)' → 'app (3)'）且五条全显；③ 搜索与作用域叠加（current 与 all 下过滤与计数联动）；④ current 库无匹配时回退提示出现、切 all 后消失、全部分组仍渲染；⑤ history 为空时不出现提示。注：任务描述所写 `.test.tsxx` 扩展名不匹配 vitest 默认 include（`*.test.tsx`），故命名为 `.test.tsx`。

**门禁结果**

1. `DATAZEN_KEYRING=file node scripts/with-plugin-inject.mjs --drivers=basic -- cargo test -p datazen --lib`：**1104 通过 / 2 失败 / 2 ignored / 0 filtered**。失败的 2 例即 ROUND-1 已登记的 `resolve_log_settings_defaults_without_settings_file` 与 `resolve_log_settings_reads_custom_level_and_path`（沙箱 EPERM 写 ~/Library/Application Support + 连锁 PoisonError），记 SKIP 不算失败。首轮运行另出现 2 例环境型 flake：`ai_generate_schema_doc_selects_tables_when_many`（127.0.0.1 mock server 连接竞态）、`inspect_plugin_package_returns_manifest_without_installing`（并行测试共享暂存目录竞态，before 快照捕到他测残留），隔离重跑均通过（2 passed），复跑全量未复现——非本次改动引入，建议后续作为 flake 观察项登记。
2. `npx tsc -p tsconfig.json --noEmit`：exit 0，零错误。
3. `npx vitest run src/lib/__tests__/historyGroups.test.ts src/windows/connection`：20 文件 / **135 用例全部通过**（含新 QueryPanelHistory.test.tsx 5 例、historyGroups.test.ts 7 例、QueryPanel.executeCancel.test.tsx 4 例）。

**BUG-R1-001 确认**：key_store.rs 超时守卫生效——两轮全量 lib 测试均无挂死（9~13s 跑完，无需 --skip 绕过），编译通过。

### ROUND-3（验证 agent）

**逐项独立复测（全部亲自执行，不采信开发自述）**

1. **BUG-R1-001** ✅ verified —— 读码 key_store.rs:391-427：测试内 std::thread::spawn(:402) 承载 load_or_create_master_key，rx.recv_timeout(10s)(:405)，超时 eprintln 提示后 return（ENV_LOCK 随之释放）。实测两次 `DATAZEN_KEYRING=file node scripts/with-plugin-inject.mjs --drivers=basic -- cargo test -p datazen --lib`（无 --skip）：第 1 次 **1104 通过 / 2 失败 / 2 ignored**（测试本体 7.03s）、第 2 次 **1103 通过 / 3 失败 / 2 ignored**（7.36s），均无挂死、无 ">60 seconds" 告警。失败构成：resolve_log_settings ×2 = 沙箱 EPERM（Operation not permitted 写 ~/Library/Application Support）+ 连锁 PoisonError，按任务口径记 SKIP；第 2 次多出的 inspect_plugin… 为并行竞态 flake（隔离 --exact --test-threads=1 重跑 0.05s 通过）→ 另登记 BUG-R3-001。
2. **BUG-R1-002** ✅ verified —— 读码 driver_command.rs:148-152：`if let Err(e) = state.store.add_query_history(entry).await { tracing::warn!(error = %e, "add_query_history failed"); }`，无 ?/unwrap/提前返回，控制流不变；driver_command 相关测试（mod tests :548，record_sql_command_outcome 四处调用点）含于上述 cargo 全量并通过。
3. **BUG-R1-004** ✅ verified —— 读码 QueryPanel.tsx：:647 `historyScopeFallback = historyScopeMode==='current' && !currentDbGroup`（findGroupForDatabase 对 null/未匹配返回 null）；:1217 渲染条件恰为「current + 无匹配组 + history.length>0」；:1219 `data-testid="history-scope-fallback-hint"` 存在；i18n 三处一致（en.ts:621 / zh-CN.ts:606 / 组件 :1222 引用）；:652-655 回退仍渲染全部分组，单库驱动行为保留。
4. **BUG-R1-005** ❌ 打回 open —— panelStore.ts:433 确为 `getQueryHistory(1000, configId)`（全 src 唯一生产调用点）✅；但 panelStore.test.ts:476/:531 两处断言仍期望 `(100, 'cfg-1')`，单文件复跑 2 failed（"loadHistory calls IPC…" / "openQueryHistory shows drawer…"）。产品代码对、测试没跟上 → 按「无回归」标准不通过。
5. **BUG-R1-006** ✅ verified —— 读码 historyGroups.ts:51-58：`bn - an` 数值降序，任一侧 NaN 回退 `bt.localeCompare(at)`。混合精度构造用例心算 + node 实测 Date.parse 一致：X=`…T00:00:00Z`(1787270400000)、Y=`…T00:00:00.5Z`(1787270400500)：bn-an=+500>0 → 更新者 Y 在前，正确；旧字典序 'Z'(0x5A) > '.'(0x2E) 会把更旧的 X 排前，错序场景确被消除；跨秒 c=`…T00:00:01Z`(1787270401000) 同理正确；组内保持后端 newest-first 不变。
6. **BUG-R1-007** ✅ verified —— 通读 QueryPanelHistory.test.tsx 全文（217 行）：数据源用真实 zustand usePanelStore.setState 注入 5 条真实形态历史（跨 app/analytics、executedAt 毫秒混合精度、newest-first），mock 仅外围 hook 与重组件，分组/渲染走真实实现——不是 mock 掉数据源的假绿；5 用例与 ROUND-2 声明①-⑤一一对应（含 BUG-R1-004 回退分支④、空历史⑤）。单独 `npx vitest run src/windows/connection/__tests__/QueryPanelHistory.test.tsx`：**5 passed (5)，exit 0**。

**全量门禁**

1. `npx tsc -p tsconfig.json --noEmit` → **exit 0，零错误** ✅
2. `npx vitest run` 全量（去管道直跑取真实退出码）：**Test Files 1 failed | 234 passed (235)；Tests 2 failed | 1862 passed (1864)；exit 1** ❌——失败全部为 panelStore.test.ts 两处陈旧断言（即 R1-005 打回原因）。对比基线：ROUND-1 为 234 文件/1859 用例全绿；本轮 +1 文件/+5 用例恰为新测试文件本身（自身全绿）。注：ROUND-2 门禁只跑 vitest 子集（20 文件），未暴露此回归。
3. cargo 全量见第 1 条（无挂死；除记 SKIP 的沙箱 EPERM ×2 与偶发 flake 外全绿）。

**最终结论**：6 项中 **5 项 verified（R1-001 / 002 / 004 / 006 / 007）**；**R1-005 打回 open**（代码正确、panelStore.test.ts 断言未同步导致全量前端门禁红）。新增 **BUG-R3-001**（P3，inspect_plugin 并行竞态 flake，open）。待办：panelStore.test.ts:476/:531 断言 100→1000 后补跑 `npx vitest run` 即可全量闭环。

### ROUND-4（开发 agent）

处理 ROUND-3 打回的 R1-005 与新增的 BUG-R3-001。

- **R1-005（打回闭环，含测试断言修正）**：`src/stores/__tests__/panelStore.test.ts:476/:531` 两处 `expect(mockGetQueryHistory).toHaveBeenCalledWith(100, 'cfg-1')` → `1000`（同一字面量出现两次，replace_all 同步），产品代码 `panelStore.ts:433` 此前已正确。门禁：`npx vitest run src/stores` → **13 文件 / 228 用例全部通过**（panelStore.test.ts 39 例，含 loadHistory / openQueryHistory 两用例）。
- **BUG-R3-001（真实修复，单测试文件内）**：根因是 `inspect_plugin_package` 的暂存目录建在全局 `std::env::temp_dir()`（生产行为），测试 `count_inspect_dirs()` 扫描同一全局目录，并行时兄弟测试的瞬态 `.datazen-inspect-*` 目录串计数。独占临时目录方案需给生产函数注入 staging 根（改签名波及 `commands/plugins.rs` 调用方，超出单测试文件范围）——按打回指令不动生产代码，改为**测试文件内串行化**：本 crate 中创建该前缀目录的测试全部在 install.rs tests 模块，新增 `static INSPECT_TMP_LOCK: Mutex<()>` + `inspect_tmp_lock()`（poison-safe），`returns_manifest_without_installing` / `accepts_top_level_folder_and_plain_dirs` / `rejects_invalid_packages` 三测试全程持锁，count 快照与断言期间不再有并发写入方。

**门禁数字**

1. `npx vitest run src/stores` → **13 文件 / 228 用例全绿，exit 0**。
2. `DATAZEN_KEYRING=file node scripts/with-plugin-inject.mjs --drivers=basic -- cargo test -p datazen --lib`（含 R3-001 修复后的完整套件）→ **1104 通过 / 2 失败 / 2 ignored**；失败仍为已登记沙箱 EPERM 的 `resolve_log_settings` ×2（记 SKIP 不算失败），`inspect_plugin_package_returns_manifest_without_installing` 本轮全量中通过，无挂死。
3. install 套件单独 **10 连跑**（默认并行线程）：`plugins::install` → **15 passed ×10，exit 0 全绿**，flake 未再复现。

**状态**：R1-005 → fixed（含测试断言修正）；BUG-R3-001 → fixed（测试内串行化，生产代码零改动）。

### ROUND-4b（验证 agent，聚焦复测 R1-005 / BUG-R3-001）

1. **BUG-R1-005** ✅ verified —— 读码确认 panelStore.test.ts:476/:531 均已为 `toHaveBeenCalledWith(1000, 'cfg-1')`；git diff 确认该文件相对 HEAD 仅此 2 行变更（100→1000），无其他断言漂移。实测 `npx vitest run src/stores`：**13 文件 / 228 用例全部通过，exit 0**。
2. **BUG-R3-001** ✅ verified —— 读码确认 `INSPECT_TMP_LOCK` 为 tests 模块内文件级 `static Mutex<()>`（install.rs:822，helper 含 PoisonError 恢复）；触发 inspect 的全部三个测试（:843/:868/:886，第三个体内 3 次 inspect 调用）均以首行 guard 全程持锁，无绕锁用例（inspect_plugin_package/count_inspect_dirs 全部引用均在三测试体内）；git diff 确认 install.rs 仅 +17 行且全部位于 mod tests，生产代码零改动。实测默认并行线程连跑 5 次：**5× `ok. 15 passed; 0 failed`（RUN1-5 EXIT=0），无 flake**。

两项均过 → BUG-R1-005 与 BUG-R3-001 登记表状态改为 verified。

### ROUND-5（编排者，循环关闭）
- 终局门禁（编排者亲自执行）：npx vitest run 全量 235 文件 / 1864 用例全绿 exit 0；cargo lib 全量 1104 通过 / 2 SKIP(沙箱 EPERM) / 2 ignored，无挂死
- 登记表终态：R1-001/002/004/005/006/007 verified；R1-003 wontfix（用户裁决 A）；R3-001 verified——无 open 项
- 结论：查询历史按 database 分组功能的测试-修复循环闭环。E2E 运行级验证（webdriver 构建 + 3 个新 spec + 契约矩阵）为后续阶段
