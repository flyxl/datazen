# 测试计划：最近一次 remote 更新 + 后续修复

- **版本基准**：`origin/main` @ `a4d8ce3`（2026-08-21 fast-forward pull，自 `6adbb54` 拉入 ~35 提交）
- **覆盖范围**：本次更新引入的 R1 主窗口页面重构、ops §5.4 MVPs、Data Sync/Transfer V1，以及为此做的回归修复（import 路径、类型错误、build 严格化）
- **测试分层**：Host 单元测试（Vitest）→ 驱动 Rust 单测/集成 → Host E2E（WebdriverIO）→ 驱动 E2E → 手工黑盒
- **落点规则**：驱动专属测试一律放 `packages/drivers/<id>/`，不得进 Host；Host 只测宿主能力

---

## 0. 前置与准入门槛

| 项 | 命令 | 通过标准 |
|----|------|---------|
| 类型检查 | `pnpm typecheck` | `tsc --noEmit` 0 错误 |
| Host 单测 | `npx vitest run` | 全绿 |
| Host Rust | `cargo test -p datazen --lib` | 全绿 |
| 驱动单测 | `cargo test -p datazen-driver-{mysql,postgres}` 等 | 全绿 |
| 前端 build | `pnpm build` | 类型检查通过 → 产物生成于 `dist/` |
| E2E 构建 | `pnpm tauri:build --debug --features webdriver` | 可启动 |

> 门禁：上述任一项报错即阻断合入。**Host 单元测试**（`pnpm test:unit`）在提交合并前必须通过。

---

## 2. 回归修复点专项测试

本次修复集中在：`ObjectFilterDialog.tsx` import 路径、`ContentView` backup/restore、`ConnectionPage` selectTableRef、`ProcessListView` column 映射、`SavedTasksBanner` 状态比较、未使用 import 清理、build 类型检查强制化。

### 2.1 ObjectFilter 对话框（单元）
- 位置：`src/components/connection/ObjectFilterDialog.tsx` + `src/lib/objectFilter.ts`
- 已有：`src/lib/__tests__/objectFilter.test.ts`（需确认覆盖度）
- 新增用例：
  - 路径解析：组件从相对路径正确解析 `Button/Dialog/Input`、`useI18n`、`objectFilter`、`types`（已被 `tsc --noEmit` 兜底，仍需 runtime 渲染验证）
  - 渲染：打开/关闭；无 connection 时不崩（`useEffect` guard）
  - 编辑：`hideSystemSchemas`、`tableNameInclude/Exclude` 初值回填、修改后 `onSave` 传回正确 `ConnectionConfig`
  - 状态：保存中 `saving` 禁用交互；保存失败保持打开并提示
- 产物断言：`src/windows/connection/__tests__/ObjectFilterDialog.test.tsx`（new）

### 2.2 backup / restore 右键菜单（ContentView）
- 已有：`src/lib/__tests__/schemaTreeContextMenu.test.ts` + `src/windows/connection/__tests__/ContentView.test.tsx`
- 新增：
  - database 节点右键出现「备份/还原」项，且仅在 `supportsBackup` 时存在
  - 触发时调用 `openBackupWindow('backup'|'restore', {configId, database})`
  - `ContentView.test.tsx` 用 mock 验证 handler 接线（不真开子窗口）
  - `mainWindowContextMenu.test.ts` 回归不受影响

### 2.3 selectTableRef 三参签名（ConnectionPage）
- 新增：mock `selectTableRef` 为三参函数，触发 table 选择，断言收到 `(table, schema, database)`

### 2.4 ProcessListView 列映射
- 已有：`src/lib/processListResult.ts`
- 新增：`processListResult.test.ts` 断言 `commandResultColumns` 从 `ColumnInfo[]` 产出 `{id,name,type}`，空数组回退为默认列

### 2.5 SavedTasksBanner 状态机
- 新增：mock `syncState`，断言 `'executing'` 时隐藏 banner、`'done'/'idle'` 显示

### 2.6 构建门禁
- CI 校验：`package.json` 中 `build`/`build:with-drivers` 使用 `tsc --noEmit`（防回退 `--noCheck`）
- 验证：故意注入一个类型错误 → `pnpm build` 应在产物生成前失败

---

## 3. 新增功能测试（重点）

### 3.1 运维 §5.4：Pin / 对象过滤器 / 进程列表 / 服务器状态 / DDL 警告 / 备份预填

**单元（Host）**
- `objectFilter`：`shouldShowSchema` / `matchesTableNameFilter` / `filterTableItems` while hidden 系统库、include/exclude 组合、大小写
- `mainWindowContextMenu`：Pin/Filter 菜单项存在与状态；已有 `mainWindowContextMenu.test.ts`
- `ddlApplyWarnings`：危险 DDL（drop/truncate/无 where 更新）风险提示命中；已有 `ddlApplyWarnings.test.ts`

**驱动层（写在各驱动 crate）**
- `packages/drivers/postgres/tests/process_commands.rs`：`list_processes` / `kill_process` 参数与返回形状
- `packages/drivers/postgres/tests/server_status_commands.rs`：`server_status` 返回字段
- `packages/drivers/mysql/tests/process_commands.rs` / `server_status_commands.rs`：同上（MySQL 方言差异）
- `use_database`、`schema_object_commands` 回归
- `Cargo.toml` 占位段保持为空，避免提交

**E2E**
- 新增/扩 `e2e/specs/ops-dashboard.ts`（已有）或 `navigator-context-menu.ts`：
  - Pin 后连接置顶、刷新保持
  - 对象过滤保存后树节点隐藏/显示符合 include-exclude
  - 打开进程列表面板 → 显示行 → 选中 kill → 行消失
  - 打开服务器状态面板显示指标
  - DB 节点右键「备份/还原」→ 子窗口以预填库名打开

### 3.2 Data Sync Diff Workspace（V1）
- 已有：`src/windows/data-sync/`、`src/@init/commands/sync/`；E2E `data-sync-window.ts` / `data-sync-real.ts`；驱动 `sync_adapter_smoke.rs`
- 用例：keyset 比对分页/游标正确、schema picker 过滤、行级 review 标记、SQL preview 生成、apply 后源/目标一致
- 契约矩阵：`pnpm e2e:contract:matrix`（PG/MySQL/SQLite）

### 3.3 Data Transfer（异构迁移 V1）
- 已有：`data-transfer-window.ts`（DTW-001~003 smoke）、`data_transfer/*` 后端
- 用例：三种模式（CREATE / Drop+Create / IR）、连接/映射编辑保存、preview 行数/类型、execute 成功/失败回滚

### 3.4 R1 主窗口页面重构（F1–F6）
- 已有 E2E：`main-window.ts`、`settings.ts`、`welcome.ts`、`docs-online.ts`、`homepage-ee`、`unified-*`
- 用例：
  - F1 设置页内嵌主窗：进入 SettingsPage 无新窗口
  - F3 底部侧边栏「设置」入口可达、高亮正确
  - F5 首次启动无连接 → 欢迎页；有连接 → 直接工作区;欢迎页错误态（load 失败）显示报错并可重试
  - F6 帮助/文档 → 跳转官网而非内嵌
  - F2 回归：所有 `*Page` 路由与关闭标签行为

---

## 4. WebdriverIO E2E 全流程测试（重点）

> 目标：用 WebdriverIO 将**本次新功能完整走通整条用户链路**（建数据 → UI 操作 → 后端执行 → 断言结果），而非只做静态 smoke。**每个流程必须自备/清理测试数据**，并优先用真实 PG/MySQL 库断言落库结果。
> 现有基础设施：`wdio.conf.ts` 已强制 zh-CN + 安全模式，并在 `before` upseed 一个 `本地 PostgreSQL` 连接（`conn_e2e_pg`）；helpers 提供 `executeSQL` / `withSafeModeOff` / `selectDzOption` / `data-*testid` 定位。

### 4.0 测试数据构造规范（贯穿所有用例）

| 数据名 | 构造方式 | 生命周期 |
|--------|---------|---------|
| PG 源库表 | 通过 `invoke('connect'+'execute_query')` 建 `e2e_ops_*` 表 + 插入固定行 | `before` 建、每条用例断言、`after` DROP |
| PG/MySQL 目标库 | `e2e/setup-sync-dbs.sh`（`datazen_sync_src/tgt`、`datazen_readonly` 只读用户） | 先在 setup 阶段跑一次 |
| SQLite 本地库 | `e2e/create-sqlite-test-db.mjs`（`fixtures/test.db`，users/posts/tags 带 5 行固定数据） | 创建后复用，跑完清理 |
| 进程/服务器状态 | 无需造表，依赖已有连接（PG `list_processes` 至少 1 个 idle 连接可返回） | 断言非空即可 |
| 连接/分组 | `save_connection` / UI 新建；用唯一名称（附时间戳）避免跨跑残留 | `after` `delete_connection` |
| 幂等守则 | 每个 `it` 独立建/改/断；不清空就跑失败即确定性回归 | 混入确保原子 |

> ⚠️ **Safe Mode**：默认开启会拦截 `DROP/TRUNCATE`。所有建表/清库 DDL 走 `withSafeModeOff(() => executeSQL(...))`（helpers 已封装）。断言时优先基于**数据库真实状态**（`SELECT count(*)` / 目标表行数），而非仅看 UI 文案。

### 4.1 新增功能：运维 §5.4 ops

#### ops-A：连接 Pin 置顶
- 前提：已有 ≥3 个连接卡片（seed PG + 2 个临时连接）。
- 流程：右键连接 → Pin → 断言该卡片在列表**顶部**；展开分组后排序稳定；重启/刷新列表后仍置顶。
- 数据：复用 seed + 临时连接；`after` 删除临时连接、unpin seed。
- 规格：`e2e/specs/navigator-context-menu.ts`（扩）或新 `ops-pin.ts`。

#### ops-B：对象过滤器（ObjectFilterDialog）完整闭环
- 前提：PG 连接打开，schema 树含系统 schema（`pg_catalog`、`information_schema`）+ 业务表。
- 完整链路：
  1. 打开连接 → 右键 DB/工具栏进 **对象过滤** 对话框。
  2. 勾选「隐藏系统库」，`tableNameInclude` 填 `e2e_%`，保存。
  3. 断言：树的 Tables/Views 只显示含 `e2e_` 的表；`pg_catalog`/默认业务表被过滤掉。
  4. 重新打开对话框 → 断言设置被持久化回填（`hideSystemSchemas` 勾选态、include 字符串仍在）。
  5. 清空/取消 → 表恢复显示。
- 数据：`e2e_set_1` / `e2e_set_2`（`setupPgFixtures` 建）、另有 `public` 下非 e2e 表 `plain_table` 作对照。
- 断言支点：schema 树按钮可见性 + `objectFilter` 返回的 `ConnectionConfig` 里 filter 字段。
- 规格：新 `e2e/specs/object-filter.ts`（含对 `ObjectFilterDialog.tsx` 修复路径的回归——能打开对话框即证明 import 修复生效）。

#### ops-C：进程列表 + 服务器状态面板（PG 优先）
- 完整链路：切「运维」→ 打开 **Process List** 面板 → 出现行（含 pid）→ 选中某行 → 点击 Kill → 该 pid 行消失或状态变化。
- **数据构造**：通过后端 `execute_query` 开一条长事务（如 `BEGIN; SELECT pg_sleep(10)`，避免轨迹真实残留）产生可识别的进程行；断言 Kill 后 `SELECT count(*) FROM pg_stat_activity WHERE pid=$pid` = 0。
- 服务器状态：打开面板断言关键指标存在（连接数、事务/锁计数等非空），数值型字段为 number。
- 规格：新 `e2e/specs/ops-process-server.ts`；驱动方言（MySQL vs PG）见驱动 crate 单测，Host 只断 UI 主流程。

#### ops-D：DDL 风险警告 + 备份/还原预填
- 完整链路：对表执行 `DROP TABLE` / `TRUNCATE`（Safe Mode 关）→ 断言弹出**风险确认警告**文案命中 DDL 危险规则 → 确认后落到 DB。
- 备份预填：DB 节点右键「备份数据库」→ 断言备份子窗口用 `database` 预填打开（参考 `backup-window.ts` 现有 URL 直达做法，改走「右键→预填」入口）。
- 规格：新 `e2e/specs/ops-ddl-backup.ts`。

### 4.2 Data Sync 全流程（`data-sync-real.ts` 已较强；补 UI 复位）
- 现状：真实 IPC 全链路 `SYNC-REAL-001~023` 已覆盖（inspect→compare→generate→apply→revalidate），**数据构造已齐全**。
- 补 UI 复位：在连接好的 PG 源/目标上，用 UI Diff Workspace 走「选端点 → 比较 → 看 mapping/row diff → 生成 SQL preview → 执行」完成后，**回查目标库 `sync_users` 行数=源行数**（闭环断言落库）。
- 数据：`datazen_sync_src/tgt` + 固定 OK；`after` DROP 测试表 + `delete_connection`。
- 规格：扩 `data-sync-window.ts`（DSW-WS-00x）。

### 4.3 Data Transfer 全流程（补真实迁移闭环，非 smoke）
- 现状：`data-transfer-window.ts` 只有 DTW-001~003 smoke（开窗口/看模式/空端点报错）——**缺真实迁移流程**。
- 新增完整链路：
  1. 源端点=PG 表 `transfer_src`（含 3 行固定数据、类型多样 int/text/numeric/date），目标端点=PG 空库或 MySQL（异构走 IR）。
  2. 选模式（CREATE / Drop+Create / Data only）、编辑**列映射**（改 1 列映射保存 → 断言 UI 更新）。
  3. Preview → 断言行数与类型；Execute → 断言目标表 `SELECT count(*)` = 3 且列值/类型正确。
  4. 异构风险：类型不匹配时 UI 应给提示（不强制成功，但 path+失败需可感知）。
- 数据：`transfer_src` 用 PG，目标 `transfer_tgt`；`after` DROP 两端、删连接。
- 规格：扩 `data-transfer-window.ts`（新增 DTW-01x 系列 + 数据准备 helper 放 `e2e/fixtures/transferFixtures.ts`）。

### 4.4 R1 主窗口（回归完整流程）
| ID | 流程 | 断言 |
|----|------|------|
| R1-F1 | 从主窗进设置 → 无新窗口 | `getWindowHandles().length === 1` 且 SettingsPage 可见 |
| R1-F3 | 侧边栏「设置」入口 → SettingsPage | 导航激活态正确 |
| R1-F5 | 清空连接后首启 → 欢迎页 → 建连 → 工作区 | 欢迎页元素消失、工作区出现 |
| R1-F6 | 帮助/文档 → 官网跳转 | 不再出现内嵌 Docs window |
- 规格：复用 `settings.ts` / `welcome.ts` / `docs-online.ts`；补 F2 全 `*Page` 导航遍历。

### 4.5 回归冒烟（防御导入/类型修复回归）

| 规格 | 目的 |
|------|------|
| `connection-window.ts` | 基础建连未被 `ObjectFilterDialog` import 修复破坏 |
| `table-data.ts` / `table-filter.ts` | ContentView 右键/数据区未因 backup/restore 接线回归 |
| `backup-database.ts` / `backup-window.ts` | 备份窗口入口正常 |
| `schema-diff-window.ts` / `data-sync-window.ts` | 新运维入口集成 |
| `welcome.ts` | 欢迎页回归 |

### 4.6 E2E 执行与清理规范

- `pnpm e2e:skip-build` + 定向 `--spec` 仅跑新增；全量用 `pnpm e2e:minimal`（DATAZEN_DRIVERS=basic）。
- 每个 spec 的 `after` 必须清理：DROP 测试表、`delete_connection`、恢复 Safe Mode；失败时 `afterEach` 仍需清理（`withSafeModeOff` 的 finally 已保证恢复）。
- 不依赖 UI 文案做真实断言；文案只用来定位，**落库结果（行数/pid/文件）才是通过标准**。
- 驱动相关全流程放 `packages/drivers/<id>/e2e/`，Host `e2e/specs/` 只写宿主能力。

---

## 5. 手工黑盒（`test/`，自动化无法覆盖）

- 暗色主题下新右键菜单（backup/restore、对象过滤）可读性
- 大库上对象过滤包含/排除性能
- 连接 Pin 在多连接 + 分组切换时的排序稳定性
- 备份窗口在预填库名不存在时的错误提示
- Transfer 映射跨 DB 类型的字段类型不匹配提示

---

## 6. 回归/边界与数据清理

- 所有 E2E 用 `setup-sync-dbs.sh` / `create-sqlite-test-db.mjs` 重建测试库，跑后清理
- 驱动 E2E 不进默认 `pnpm e2e`，单独脚本执行
- 更新 `docs/e2e-coverage.md` 记录所有新增 Host UI 路径；无法自动化的登记例外

---

## 7. 验收清单

### 构建与静态
- [ ] `pnpm typecheck` 0 错误
- [ ] `pnpm build` 严格类型检查 → 产物生成于 `dist/`
- [ ] `npx vitest run`（Host 单元）通过
- [ ] `cargo test -p datazen --lib` 通过
- [ ] 各驱动的 Rust 单测/集成（process/server_status/sync_adapter）通过

### WebdriverIO 全流程（§4 新增规格必须通过）
- [ ] ops-A Pin 置顶流通过
- [ ] ops-B 对象过滤器完整闭环（对话框=import 修复回归、过滤、持久化回填、取消恢复）
- [ ] ops-C 进程列表 Kill + 服务器状态流通过（以 `SELECT count(*)` 断言落库）
- [ ] ops-D DDL 风险警告 + 备份预填流通过
- [ ] Data Sync UI 复位流（compare→preview→execute 后目标行数=源行数）
- [ ] Data Transfer 真实迁移闭环（`transfer_src`→`transfer_tgt`，preview+execute 后目标 count=3）
- [ ] R1 主窗口 F1–F6 全流程回归通过
- [ ] 回归冒烟（table-data / table-filter / backup / schema-diff / welcome）通过

### 数据与清理
- [ ] 所有流程自备测试数据（§4.0 规范），`after`/`afterEach` 完整 DROP + 删连接 + 恢复 Safe Mode
- [ ] `setup-sync-dbs.sh` / `create-sqlite-test-db.mjs` 已按需重建测试库
- [ ] 驱动 E2E（clickhouse/duckdb/mongodb/sqlserver 冒烟）按各自脚本通过

### 收尾
- [ ] `docs/e2e-coverage.md` 已登记新增 Host UI 路径与例外

> 执行顺序建议：准入门槛 → 单元 → Host Rust → 驱动 Rust → `pnpm build` → **WebdriverIO 全流程（§4）** → 手工（§5）。E2E 是全流程验收主体，任何一步回归需先定位是否与 `dist` / 类型检查强制化有关。