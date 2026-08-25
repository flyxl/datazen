# DataZen 回归测试报告（合并前全量）

- **分支**：`feature/db-session-id-rename` @ `a74e03aa`（主检出直接处于该分支，worktree 已删除）
- **基线**：`main` @ `5a6fd22f`
- **测试范围**：R1 全量门禁（`scripts/run-regression.sh`）＋ R2 E2E minimal（`scripts/run-e2e-minimal.sh`）
- **角色限定**：只测试与报告，不修复；除本报告外未修改任何 git 跟踪文件
- **日期**：2026-08-25

---

## R1 全量门禁

命令：`bash scripts/run-regression.sh`（后台执行，一次通过，exit code 0）

### 结果汇总表（脚本原文）

```
======================== 回归结果汇总 ========================
#    步骤                                         结果     耗时
1    cargo test -p datazen --lib [注入+HOME包装+复跑] PASS       0m42s
2    npx vitest run                                 PASS       0m41s
3    npx vitest run --config vitest.drivers.config.ts PASS       0m03s
4    node scripts/check-id-terminology.mjs          PASS       0m00s
5    npx tsc --noEmit                               PASS       0m06s
6    npx vite build                                 PASS       0m05s
=============================================================
全量回归门禁通过 ✔
```

### 关键数字

| 步骤 | 本次结果 | 基线 | 判定 |
|------|---------|------|------|
| ① cargo lib | **1128 passed / 0 failed / 2 ignored**，第 1 轮即全绿（未触发失败子集复跑） | ≥1126 过；已知负载型偶发经 HOME 沙箱应转绿 | ✅ 达标 |
| ② vitest Host | **240 文件 / 1894 用例全过** | 240 文件 / 1894 绿 | ✅ 完全一致 |
| ③ vitest drivers | **14 文件 / 84 用例全过** | 84 绿 | ✅ 一致 |
| ④ ID 术语守护 | ok（891 文件扫描，2 处白名单命中跳过），exit 0 | exit 0 | ✅ 一致 |
| ⑤ tsc --noEmit | 零错误 | 零错 | ✅ 一致 |
| ⑥ vite build | 零错误（2905 modules） | 零错 | ✅ 一致 |

cargo lib 相对"≥1126"多出的 2 例为分支新增守护用例的正常增长；`ai_generate_schema_doc_selects_tables_when_many` 本轮第 1 轮即在 HOME 沙箱下通过，无需复跑兜底。

---

## R2 E2E minimal

### 尝试一：默认参数（环境硬阻塞，已按预案处置）

命令：`bash scripts/run-e2e-minimal.sh`

| 步骤 | 结果 | 耗时 |
|------|------|------|
| ① codegen 就绪检查 | PASS | 0m00s |
| ② 前端构建（vite build） | PASS | 0m05s |
| ③ webdriver 构建（注入 basic + tauri build --debug） | PASS（打包器退出码 1，但 `.app` 二进制在本轮产出，走脚本内置容错） | 1m06s |
| ④ DB 准备 | PASS | 0m00s |
| ⑤ WDIO minimal 集 | **FAIL**（exit=1，dur=0m15s） | 0m15s |

**步骤⑤失败根因（环境硬阻塞，与重构无关）**：应用进程启动即 panic，日志尾部证据：

```
thread 'main' panicked at .../tracing-appender-0.2.5/src/rolling.rs:156:14:
initializing rolling file appender failed: InitError { context: "failed to create log file",
source: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" } }
Port 4445 not ready after 15000ms
[e2e-runner] WebDriver port 4445 did not open.
```

默认配置下第④⑤步以真实 `$HOME` 运行应用，滚动日志写入真实用户目录被本会话沙箱拒绝（同轮还有 WebKit 缓存目录 `/Users/wuxiaolong/Library/Caches/com.tbeasy.datazen/...` 写入被拒的告警）。这是受限运行环境的文件权限问题，不是代码缺陷。

### 尝试二：脚本内置的隔离开关（实际生效的一轮）

命令：`E2E_ISOLATE_HOME=1 bash scripts/run-e2e-minimal.sh --skip-build`（复用尝试一刚产出的 webdriver 构建产物；该开关由脚本原生提供：第④步 HOME 沙箱化 + 自动 `DATAZEN_KEYRING=file`）

- ①②④ PASS；③ SKIP（--skip-build）；应用正常启动：`WebDriver server listening on http://127.0.0.1:4445`，主窗口创建成功。
- 注：runner 提示 "binary is older than dist/index.html"，但两次 vite build 产物哈希完全一致（如 `MainPage-dW2qz38M.js` 等 chunk 名逐一相同），嵌入资产无实质过期。
- DB 准备警告（两轮相同，属 ENV 覆盖面而非代码问题）：`e2e/.env` 未提供 `E2E_PG_PASSWORD` / `E2E_MYSQL_PASSWORD`，且 `setup-sync-dbs.sh` 因缺少 `E2E_PG_RO_PASSWORD` 提前退出——数据同步类 spec 可能受影响（见下方甄别）。

### 尝试二实际结果（WDIO 全量 minimal 集）

命令：`E2E_ISOLATE_HOME=1 bash scripts/run-e2e-minimal.sh --skip-build`（后台运行 31m37s，exit 1）

**官方总账（wdio 最终汇总行）**：`Spec Files: 7 passed, 68 failed, 1 skipped, 76 total (100% completed)`

应用正常启动、4445 就绪、76 个 spec 文件全部调度。**关键分界**：应用进程于 11:57:47Z 后无任何日志静默死亡（stdout/stderr 均无 panic/crash 输出；宿主内存当时约 10.2 GiB/16 GiB），自此 WebDriver 4445 拒绝连接——

- **真实执行的 spec：前 38 个**（[0-0]～[0-37]，11:28–11:56）：7 过 / 31 败；
- **从未执行的 spec：[0-38]（hotkeys.ts）起的 ~38 个**（i18n-\*、main-window、mysql\*、new-connection、object-browser、ops-\*、sql-query、sqlite、table-data/edit/filter/indexes/structure、unified-\*、welcome、workflow\*、zz-\* 等）：全部表现为"会话创建即拒连"秒失败，**不代表这些用例本身失败**。

#### 通过的 7 个 spec

`ai-code-block` / `ai-context` / `ai-features` / `bugfix-admin-commands` / `docs-online` / `edit-delete-connection` / `er-diagram`

#### 真实执行的 31 个失败 spec 逐条甄别

| # | spec | 失败要点 | 甄别 |
|---|------|---------|------|
| 1 | `client-parity` | `begin_session_transaction` 等传连接 id 值 `conn_e2e_pg` 当 dbSessionId → `DB session 'conn_e2e_pg' not found`；同 spec 其余 6 用例（Safe Mode、绑定参数、例程/权限、只读拒绝写等）全过 | **🔴 本次重构引入**（见下"重构引入清单"#1；已在隔离复跑中确定性复现） |
| 2 | `driver-commands` | 用例1 `execute_driver_command` request 仍发 `connectionId` 键 → `dbSessionId or driverType is required`；用例2（机械改名键名）反而通过——因 `resolve_session` 双模回退兜住了连接 id 值 | **🔴 本次重构引入**（清单#2；隔离复跑确定性复现：1 过 1 挂） |
| 3 | `execute-sql-file` | SF-E01 `execute_sql_file`/`restore_database` 传连接 id 值当 dbSessionId → not found；SF-E02 过 | **🔴 本次重构引入**（清单#3；隔离复跑确定性复现：1 过 1 挂） |
| 4 | `data-sync-real` | 3 个 before-all 全挂：`database "datazen_sync_src" does not exist` | 🟡 环境：`E2E_PG_RO_PASSWORD` 未提供 → `setup-sync-dbs.sh` L24 硬失败，同步源/目标库从未创建（详见 ENV 甄别节） |
| 5 | `data-sync-window` | DSW-EXEC 同上 sync 库缺失；DSW-001 另有"新建连接按钮找不到" | 🟡 环境（sync 库）＋ 🔵 UI 族（见下） |
| 6 | `data-transfer-window` | DT-CL 同上 sync 库缺失；DTW-001 "新建连接按钮找不到" | 🟡 环境 ＋ 🔵 UI 族 |
| 7 | `backup-database` | BACKUP-011/012 恢复报 `relation ... already exists`（4/42 条失败）。注：该文件迁移**正确**（L77 用 connect 返回的运行时 id），非 ID 契约问题 | ⚪ 既有/状态依赖：预置 product 表已存在而恢复不带 DROP，疑似与库状态/顺序有关 |
| 8 | `app-data-backup` | ADB-001 断言 `ConnectionPage.tsx` 含 `menu:export-connections` 字符串——**main 上同样为 0 处**（监听已迁至 MainPage.tsx）；ADB-003 重导入自导出 zip 被 zip-bomb 守卫误拒（`datazen.sqlite-shm` 压缩比触发） | ⚪ 既有 stale spec ＋ ⚪ 既有守卫误报（均与 ID 无关） |
| 9 | `export-import` | before-all `等待 schema 树加载超时`（clickTableInSidebar→waitForSchemaTreeLoaded 20s）；后端 get_tables/use_database 全部正常返回。**已按要求单独复跑**（`-- --spec e2e/specs/export-import.ts`，隔离 HOME，全新应用实例）——同一位置确定性复现 | 🔵 归因未定（见下"UI 不可见族"；EI-BE-001/002 未走到，批量导出 E2E 判定=待补跑） |
| 10 | `data-types` | 同一 before-all schema 树等待超时，后续断言连锁失败 | 🔵 UI 族 |
| 11 | `detail-panel` | 同上 | 🔵 UI 族 |
| 12 | `chart-expand` | before-all 经 SQL 编辑器建表，`等待 SQL 执行完成超时`（后端流式执行已发出且后续 DROP 成功） | 🔵 UI 族（结果面板就绪信号未出现） |
| 13 | `chart-views` | 同上 | 🔵 UI 族 |
| 14 | `ai-ask-question` | 8 例全因 `等待新窗口打开超时`（点"工作流"后无新窗口）。`menu:workflow → handleOpenWorkflow()` 处理链两分支逐字一致 | ⚪ 既有/环境（子窗口族） |
| 15 | `bugfix-verification` | `Workflow button not found` ×2 | ⚪ 既有（主页操作入口族） |
| 16 | `backup-window` | BKU-001 before-all 打开备份窗口超时 | ⚪ 既有/环境（子窗口族；BackupWindow.tsx diff 为纯键名改名） |
| 17 | `connection-search-group` | CM-007 空白区 contextmenu 绑定断言 false | ⚪ 既有/UI 族（文件未被分支触碰） |
| 18 | `connection-validation` | 4 例 `button*=新建连接 not found` | 🔵 UI 族 |
| 19 | `connection-window` | DB-008 搜索表输入框 not displayed | 🔵 UI 族 |
| 20 | `context-menu-new-query-db` | before-all 60s 整体超时（右键数据库→新建查询导航） | 🔵 UI 族（文件被分支改过 287 行，但均为术语改名；无法进一步归因） |
| 21–26 | `data-dashboard-boards/entry/refresh/sql-add/widget-ux/workflow-add` | 6 个文件同一模式：`[data-testid="action.dashboard"]` still not displayed（主页看板入口卡片找不到） | 🔵 UI 族 |
| 27 | `drag-drop-groups` | DND-003 `data-group-name` 属性数组为空 | 🔵 UI 族 |
| 28 | `edge-cases` | TC-EDGE-004 大结果集 waitUntil 30s 超时 | 🔵 UI 族/负载 |
| 29 | `file-connection-fields-theme` | `button*=新建连接 not found` | 🔵 UI 族 |
| 30 | `homepage-features` | `button*=恢复数据库 not found`、HOME-006 图标断言失败 | 🔵 UI 族 |

#### UI 不可见族（🔵）综合说明

三类高度相关的症状反复出现：①主页操作入口元素找不到（新建连接 / 恢复数据库 / action.dashboard / Workflow）；②连接工作区 `<aside>` 内 20s 内从不出现"表/Tables/视图/Keys"分区头（而"新建查询"按钮能等到、SQL 能执行、后端 get_tables 正常）；③部分工具栏控件（搜索框、格式化/事务控件）找不到。

归因证据：
- 相关前端组件（SchemaTree/useExpandedDbCacheRefresh/windowManager/panelStore 等）相对 main 的 diff 均为**纯术语重命名**，展开刷新逻辑两分支结构逐行对应；
- R1 vitest 240 文件全绿（含 ConnectionNavigatorTree/SchemaTree 相关单测）；
- 症状在全新隔离应用实例上确定性复现（排除顺序污染），但**无法在本环境证明 main 在同场景为绿**（切换检出做对照超出本轮权限与约束）；
- 结论：**列为"低置信可疑项"**——既可能是受限环境（WebKit 渲染/窗口尺寸/持久化设置残留 `.regression-home` 跨轮共享）造成的既有现象，也不能 100% 排除重构引入的 UI 回归。**处置：待用户在正常 GUI 环境复跑 `pnpm e2e:minimal` 或至少 `export-import` + `table-data` 两个 spec 对照定论。**

### 批量导出（BatchExport）覆盖判定

- **是否在 minimal 集内：是。** `e2e/wdio.conf.ts` 的 specs 为 `./specs/**/*.ts`（全部 76 个文件），minimal 仅影响驱动选型（DATAZEN_DRIVERS=basic）与构建链，不做 spec 过滤；`run.mjs` 亦不过滤。
- **覆盖内容**：`e2e/specs/export-import.ts` 含 EI-BE-001（连接窗口工具栏 `[data-testid="conn-toolbar-export"]` 打开批量导出对话框并断言 `batchExport.title/selectTables`）、EI-BE-002（Schema 树右键菜单项 `[data-testid="web-context-item-batch-export"]`）、EI-001 右键菜单含批量导出项、EI-GRID-001 DataTable 工具栏导出，共 11 个活跃用例（另 13 个 it.skip 为原生菜单/列选择例外登记）。
- **本轮执行情况**：全量跑与单独跑（任务要求的 `--spec` 单测形式）均在 before-all 即失败（schema 树加载超时），**11 个用例未获执行** → E2E 层批量导出路径本轮**未被验证**。
- **补偿性证据**：单元层批量导出逻辑在 R1 中全绿——`src/lib/__tests__/batchExport.test.ts`（28 例）、`batchExportJob.test.ts`（8 例）、`loadBatchExportTable.test.ts`（11 例）、`exportStream.test.ts`（8 例）。
- **判定**：🟡 待补跑。请在无限制 GUI 环境执行 `bash scripts/run-e2e-minimal.sh --skip-build -- --spec e2e/specs/export-import.ts` 完成闭环。

### ENV 覆盖面甄别（仅变量名，不含任何值）

`e2e/.env` 实际提供的变量名集合（14 个）：`E2E_PG_DB/E2E_PG_HOST/E2E_PG_PORT/E2E_PG_SUPER/E2E_PG_USER/E2E_PG_PASSWORD`、`E2E_MYSQL_DB/E2E_MYSQL_HOST/E2E_MYSQL_PORT/E2E_MYSQL_USER/E2E_MYSQL_PASSWORD`、`E2E_REDIS_HOST/E2E_REDIS_PORT/E2E_REDIS_PASSSWORD`。

| 现象 | 根因 | 定性 |
|------|------|------|
| runner 警告 `E2E_PG_PASSWORD`/`E2E_MYSQL_PASSWORD` 缺失 | 两变量在 .env 中**存在但值为空串**，脚本以 `-z` 判空即告警；本机 PG/MySQL 为 trust/免密认证，psql/mysql CLI 与应用连接全程正常 | ✅ 正常配置，非变量名不匹配 |
| `setup-sync-dbs.sh: line 24: E2E_PG_RO_PASSWORD: Set ...` 后退出 | `E2E_PG_RO_PASSWORD` **完全不存在**于 .env（脚本 `:?` 硬要求）→ 只读角色与 `datazen_sync_src/tgt` 同步库从未创建 | 🟡 **env 未提供**：SYNC-REAL/DSW-EXEC/DT-CL 类失败全部由此 cascading |
| （观察项）`E2E_REDIS_PASSSWORD` 拼写异常（连续三个 S） | 若消费方期望 `E2E_REDIS_PASSWORD` 则密码回退为空；本地 Redis 免认证时无实际影响，`setup-e2e-env.sh` 不引用任何 E2E_REDIS_\* | ⚪ 卫生问题，建议顺手修正 |
| （观察项）`setup-e2e-env.sh` 引用 `E2E_PG_ADMIN_DB` 而 .env 无此变量 | 脚本内有默认值回退，主 seeding 流程未受影响 | ⚪ 无影响 |

### 重构引入的可疑差异清单（合并前需处置）

| # | 位置 | 问题 | 重现步骤 | 建议处置 |
|---|------|------|---------|---------|
| 1 | `e2e/specs/client-parity.ts` L119/L123/L127-128（及 session_transaction_status 调用） | begin/commit/rollback_session_transaction 以 `dbSessionId:` 键传**连接 id 值**（`pgId` 来自 get_connections），未使用 connect 返回的运行时会话 id；旧双模回退下可过、新严格校验必挂（`begin_session_transaction_impl` 走 `get_session` 严格解析） | `E2E_ISOLATE_HOME=1 bash scripts/run-e2e-minimal.sh --skip-build -- --spec e2e/specs/client-parity.ts` → "session transaction begin/commit/rollback" 必失败 | 修复闭环：仿照 `backup-database.ts` L77 模式 `dbSessionId = await invokeBackend('connect', { connectionId })` 后透传 |
| 2 | `e2e/specs/driver-commands.ts` L33 | `execute_driver_command` 的 request 体仍发 `connectionId` 键；后端 `ExecuteDriverCommandRequest` 字段已更名 `dbSessionId`（serde 忽略未知字段→报 required）。同文件用例2 仅机械改名键名，靠 resolve_session 双模回退侥幸通过，语义仍是"拿连接 id 当会话 id" | `... -- --spec e2e/specs/driver-commands.ts` → 用例1 `dbSessionId or driverType is required` | 修复闭环：request 键改 `dbSessionId` 并改传 connect 返回值（勿依赖双模回退，AGENTS.md 明令新代码不得依赖） |
| 3 | `e2e/specs/execute-sql-file.ts` L49-51/L72/L89 | 变量由 `connId` 改名 `dbSessionId` 但赋值仍为 getConnectionId()（连接 id）；`execute_sql_file`/`restore_database` 严格会话校验拒绝，SF-E01 必失败 | `... -- --spec e2e/specs/execute-sql-file.ts` → SF-E01 收到 not found 错误 | 修复闭环：同 #1 模式 |
| 4 | `e2e/specs/data-transfer-window.ts` L208 | 变量名残留 `tgtConn`（值在新契约下恰为 dbSessionId，功能正确） | 静态可见 | 低优先：随手重命名 |
| 5 | 🔵 UI 不可见族（31 个失败中的 ~20 个） | 见上文综合说明 | — | 待用户 GUI 环境对照复跑定论；若复现则按常规回归排查 |

> 说明：后端严格会话校验及友好错误消息（`maybe you passed a connectionId where a dbSessionId was expected`）本身符合 naming.md 新契约设计，Host 前端 src 迁移完整（tsc/vitest/guard 全绿）；上述 #1–#3 属**分支内 E2E 测试资产未随契约收尾**，使相应 IPC 路径的 E2E 覆盖暂时失效。

---

## 补充静态核查（非门禁项）

1. **e2e tsconfig 类型检查**（`npx tsc --noEmit -p e2e/tsconfig.json`，不在 R1 门禁内）：71 条错误，全部为 `Promise<number>` 比较（TS2365/2367）、`unknown` 入参等通用类型卫生问题，**0 条涉及 connectionId/dbSessionId 术语**。抽查确认报错行在 `main` 上即为同样代码（如 `data-dashboard-refresh.ts` L56/58 的 `charts.length >= 1` 模式 main 与 HEAD 完全一致），属主干既有，wdio 运行时不做类型检查故不影响执行。
2. **`e2e/specs/data-transfer-window.ts` L208 变量名残留**：`const tgtConn = await invokeBackend<string>('connect', { connectionId: TGT_ID })` ——变量名仍叫 `tgtConn`，但按新契约存的是 connect 返回的 dbSessionId，后续 `dbSessionId: tgtConn` 用法正确。仅命名残留，无功能问题。
3. **`e2e/specs/data-sync-real.ts` 旧键清理确认**：全文无 `sourceConfigId/targetConfigId` 残留（W3 报告中的缺陷 D3 已闭环）。

## 工作树状态说明

两轮脚本运行后唯一跟踪文件改动为根 `Cargo.lock` +1 行（`+ "datazen-driver-redis"`）——这是 `scripts/with-plugin-inject.mjs` 注入周期的文档化瞬态副作用（两个脚本头部注释均注明"由编排方在提交前还原"）。测试代理已在全部运行结束后以 `git restore Cargo.lock` 还原，最终工作树干净（仅新增本报告）。`.regression-home/`、`.plugin-file-stash/` 为 gitignore 的运行产物，未动。

---

## R3 总体判定：**存在阻塞项，暂不建议合并**

### R1/R2 vs 基线对照表

| 检查项 | 基线（main 约定值） | 本次结果 | 判定 |
|--------|--------------------|----------|------|
| cargo lib | ≥1126 过 | **1128 过 / 0 败 / 2 ignored**，第 1 轮全绿无复跑 | ✅ |
| vitest Host | 240 文件 / 1894 绿 | **240 / 1894 全过** | ✅ 完全一致 |
| vitest drivers | 84 绿 | **84 全过**（14 文件） | ✅ 一致 |
| ID 术语守护 | exit 0 | exit 0（891 文件） | ✅ 一致 |
| tsc --noEmit | 零错 | 零错 | ✅ 一致 |
| vite build | 零错 | 零错 | ✅ 一致 |
| E2E minimal | （基线未提供数字；仓库规则要求 Host UI 路径全覆盖且同 PR 更新） | 76 文件：7 过 / 31 真实失败 / ~38 未执行（应用中途死亡）/ 1 skip | 🔴 见下 |

### 阻塞项清单

**A 类｜本次重构引入 → 须修复闭环后方可合并**

| 编号 | 状态 | 内容 |
|------|------|------|
| A-1 | 🔴 未修复 | `client-parity.ts` 会话事务用例以连接 id 冒充 dbSessionId（详见重构引入清单 #1）——E2E 覆盖失效 |
| A-2 | 🔴 未修复 | `driver-commands.ts` 用例1 request 键名未迁移 `connectionId→dbSessionId`（#2）——Driver Command IPC 主路径 E2E 失效 |
| A-3 | 🔴 未修复 | `execute-sql-file.ts` SF-E01 同类迁移遗漏（#3）——execute_sql_file 路径 E2E 失效 |

三处均为测试代码小改动、仓内已有正确范本（`backup-database.ts` L77）；修复后建议跑一次对应 `--spec` 单测即绿。**注**：Host 产品代码（src/ + src-tauri/）本身未发现任何重构引入的缺陷信号。

**B 类｜环境限制 → 待用户环境补跑清单**

| 编号 | 状态 | 内容与建议处置 |
|------|------|----------------|
| B-1 | ⏸ 待补跑 | 全量 minimal 集 ~38 个 spec 因应用进程长跑中静默死亡而**从未执行**（table-data/table-edit/table-filter/table-indexes/table-structure/sql-query/sqlite/mysql/settings/schema-diff-window/workflow-window/ui-window-ops/unified-\* 等）。建议在正常 GUI 环境完整跑一遍 `pnpm e2e:minimal` |
| B-2 | ⏸ 待补跑 | 批量导出 E2E（export-import.ts EI-BE-001/002 等 11 例）因 before-all 的 UI 族失败未走到。建议单独跑 `-- --spec e2e/specs/export-import.ts` |
| B-3 | ⏸ 待补环境变量 | 数据同步真实链路（SYNC-REAL/DSW-EXEC/DT-CL）：需在 e2e/.env 补 `E2E_PG_RO_PASSWORD`（及核对 `E2E_REDIS_PASSSWORD` 拼写）后重跑相关 spec |
| B-4 | ℹ️ 已绕过 | 受限沙箱拒绝应用写真实用户目录 → 默认参数下第⑤步启动即 panic；本轮全部使用脚本内置 `E2E_ISOLATE_HOME=1` 绕过。普通开发机无需此开关 |
| B-5 | ⚠️ 定论待复跑 | 🔵"UI 不可见族"（~20 个失败：主页操作入口/侧栏分区头/部分工具栏控件找不到）。证据倾向既有/环境因素（组件 diff 纯改名、单测全绿），但不能排除重构引入渲染回归。建议在正常 GUI 环境先复跑 `export-import` 与 `table-data` 对照；若仍失败再按常规回归排查（届时优先检查 aside 渲染与主页操作卡片区） |

### 结论一句话

R1 六步门禁全绿且与基线完全一致，产品代码（Rust + 前端）未发现重构引入缺陷；但 R2 证实 **3 个 E2E spec 存在确定性 ID 契约迁移遗漏（A-1~A-3）**，叠加受限环境导致约半数 spec 未获执行（B-1/B-2）、同步链路缺 ENV（B-3）。建议：**修复 A 类三处（小改动）→ 用户在有 GUI/DB 的环境补跑 B 类清单 → 全部转绿后再合并**。

---

## 附：BUG-008/009/010 复测（`be922ce3`，独立复核）

> 复测对象：`be922ce3`（HEAD=`7999782e` 仅 bookkeeping）。修复只动 3 个 spec 文件（+84/−44），既有 webdriver 二进制仍有效，全部复跑沿用 `--skip-build`。方法与编码方相同：`E2E_ISOLATE_HOME=1 bash scripts/run-e2e-minimal.sh --skip-build -- --spec <file>`，三 spec 串行执行。

### 1) 独立代码复核（会话槽/持久化槽语义逐一核对）——PASS

| 文件 | 核对结果 |
|------|---------|
| `client-parity.ts` | 持久化槽改名 `pgConnectionId`（仅用于 get_connections 匹配/save_connection）；12 处会话槽全部改传 connect 返回的 `pgSessionId`；只读会话改捕获 `readOnlySessionId=liveId` 并在清理时正确 disconnect（替代旧的字面量 'conn_e2e_readonly' 冒充）✅ |
| `driver-commands.ts` | `get_connection_commands` 保持持久化契约槽（connectionId）；两个用例的 `execute_driver_command` request 均先 `connect` 取真运行时会话 id，不再依赖 resolve_session 双模回退 ✅ |
| `execute-sql-file.ts` | `getConnectionId→getConnectionConfigId` 正名；before() 以 backup-database 范式 config id → connect → 校验非空 dbSessionId；after() 清理包 `withSafeModeOff`（安全模式拦裸 DROP）并补 disconnect ✅ |

旧键残留 grep：三文件中所有 `connectionId:` 实参均位于 connect / get_connection_commands 持久化契约槽；所有 `dbSessionId:` 实参均来自运行时会话变量（pgSessionId/readOnlySessionId/sessionId/sid/liveId/dbSessionId），无一处以配置 id 冒充。唯一 `conn_e2e_pg` 字面量用于查找持久化配置（合法）。`withSafeModeOff` 存在于 helpers.ts L543。

### 2) 隔离逐 spec 复跑——BUG 全部转绿

| BUG | spec | 复跑结果 | 判定 |
|-----|------|---------|------|
| BUG-008 | `client-parity.ts` | **7 过 / 3 败**：此前确定性失败的 **"session transaction begin/commit/rollback" ✓ 转绿**（begin/status=true → rollback → status=false → begin → commit → status=false 全链路通过）；Safe Mode×2、绑定参数、例程/权限、定时工作流、只读拒绝写均过 | ✅ 已修复 |
| BUG-009 | `driver-commands.ts` | **2/2 全绿**："discovers commands from a connection and executes query" ✓（此前必挂）；"rejects an unsupported driver command" ✓ | ✅ 已修复 |
| BUG-010 | `execute-sql-file.ts` | **2/2 全绿**：SF-E01 ✓（此前必挂）；SF-E02 ✓——且该用例由 withSafeModeOff 暴露出此前被掩盖的安全模式门控，从 false-pass 变为真通过（覆盖质量提升） | ✅ 已修复 |

client-parity 剩余 3 个失败逐条对照 B-5「UI 可见性族」，错误签名精确匹配、无一属于 ID 契约：

| 用例 | 错误签名 | 归类 |
|------|---------|------|
| query toolbar shows format, bind params, and transaction controls | `waitUntil condition timed out after 20000ms`（L198） | B-5 特征③ 工具栏控件不可见 |
| table filter editor opens AND/OR controls | `等待 schema 树加载超时`（waitForSchemaTreeLoaded→clickFirstTable） | B-5 特征② 侧栏分区头不出现 |
| new connection form shows SSH agent and jump host | `button*=新建连接 not found`（L258） | B-5 特征① 主页操作入口找不到 |

### 3) e2e tsconfig 门禁

`npx tsc --noEmit -p e2e/tsconfig.json`：**71 条 = 本报告实测基线持平（提交信息口径 67 系对照不同，不影响 ≤ 判定）**；`client-parity.ts` / `driver-commands.ts` / `execute-sql-file.ts` 三文件 **0 错误**（修复前基线亦为 0）。

### 复测结论

**BUG-008 ✅ / BUG-009 ✅ / BUG-010 ✅ —— 全部通过复测**。阻塞项清单状态更新：**A-1/A-2/A-3 解除**（建议编排方将三者置「已修复」）；剩余阻塞全部为 B 类环境补跑项（B-1/B-2/B-3/B-5），合并阶段进入 B 类补跑流程。
