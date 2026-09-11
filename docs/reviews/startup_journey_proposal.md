# DataZen Startup Journey（首次启动旅程）产品方案 v3

> 状态：提案 v3（Q2–Q6 已确认决策修订版），不改宿主业务代码。
> 输入：`docs/reviews/deepseek_startup_journey.html`（静态 demo）、v2 方案。
> 本轮已确认决策：**Q2 Skip 全程可见 / Q3 复用 NewConnectionDialog 真布局 / Q4 sample 全英文 + Dashboard 主路径 / Q5 onboardingVersion 版本化 / Q6 来源预检机制设计**（见 §8，全部闭环）。
> 视觉基线：`feat/ui-refresh-dark` 分支换肤 tokens（`src/styles/themes.css`，accent `#4fc3f7`、背景 `#0b0e14`）。
> 配套产物：流程图 `docs/reviews/startup_journey_flow.html`（archify workflow，主路径=导入优先），
> 高保真可交互原型 `docs/reviews/startup_journey_prototype.html`（S0 三入口 + S1 真布局 + S2/S3 可点）。

---

## 0. 复用点清单（本次修订的核心约束）

| 向导步骤 | 复用组件 | 调用方式 | 复用内容 |
|---|---|---|---|
| S0 导入入口 | `ConnectionShareDialog`（import 模式） | `openConnectionShareDialog('import')`（`src/lib/connectionShare.ts`，zustand store） | 文件导入 + 从外部应用导入（dbx/navicat/datagrip/dbeaver/tableplus，走 `connectionCommands.importConnections*`）；`ConnectionShareDialogHost` 已在 MainPage 全分支挂载，**向导内零新增挂载**；来源预检复用 `detect_connection_import_path`（见 §9） |
| S1 手动创建 | **`NewConnectionDialog` 内容区整体复刻**（左驱动列表 + 右 `ConnectionFormBody` + 底部三按钮），不是只嵌表单体 | 照搬 `NewConnectionDialog.tsx:146-218`：左 `aside.w-[220px]`（驱动搜索框 + `DB_REGISTRY × sortDbTypesByPopularity × getAvailableDrivers()` 纵向列表，`DbTypeBadge` 图标+名称，隐藏不可用项）+ 右 `ConnectionFormBody(variant="window")` + footer（Test / Cancel / Save） | 字段/校验/test/save/`useConnectionClipboardFill` 全套行为与正式新建连接一致，零双轨维护；向导外层只包步骤条 + Back/Continue，**Save 成功作为 Continue 硬门**（不再是 demo 的 test 即解锁——真实组件里保存才是完成态） |
| S2 AI | `ModelProfileDialog` 的 draft 逻辑 | 抽 `useAiProfileDraft` hook（Phase 1 定抽取还是精简版） | provider id（`open_ai / deep_seek / ollama / custom`）、`validateConfig` 软门、`saveProfile(isDefault=true)` |
| 对话框宿主 | `ConnectionShareDialogHost` + `ConnectionEditorDialogHost` | MainPage 已挂载（全分支），向导只管调 store open | 无需在向导内处理 portal/跨窗口 |

> 结论：**向导层只做"壳 + 状态机 + 分流"，所有表单/对话框/校验/加密都是现有实现**。`WelcomePage` 保持不动（保护 `welcome.ts` F5-E2E-001~005）。

---

## 1. 目标用户与成功指标

### 1.1 目标用户（按优先级重排，导入用户置顶）

| 人群 | 描述 | 向导对他的价值 | 对应入口 |
|---|---|---|---|
| A. 迁移用户（有现成连接）**← 主力** | 已有 DBeaver/DataGrip/Navicat/TablePlus/.env/DataZen 文件 | S0 一键导入，2 步进工作区 | S0 主卡：导入连接 |
| B. 新手（首次用数据库客户端） | 从其他客户端跳槽，或学生 | 手动建连 + 测试门，一条路走到能查数据 | S0 次卡：手动创建 |
| C. 尝鲜用户（先看看） | 不想配任何东西 | Sample SQLite 一键进工作区 | S0 第三卡：示例数据 |
| D. 回访用户 | 已完成过向导 | **永远不再打扰**（除非手动重置） | — |

### 1.2 成功指标（North Star：S3 到达率；主指标改为导入成功率）

| 指标 | 定义 | 目标 |
|---|---|---|
| **导入成功率（主指标）** | 点 S0 导入卡 → 导入成功 ≥1 个连接 → 到达 S2 | ≥ 75%（低于则优化来源检测/错误提示） |
| S3 到达率 | 进入向导 → 看到 Done 页 | ≥ 85% |
| S1 一次通过率（手动路径） | 首次 Test connection 即成功 | ≥ 60%（低于则优化默认值/错误提示） |
| AI 配置率 | Done 时已有可用 AI profile | ≥ 30%（AI 可选，不强求） |
| 7 日留存（向导完成 vs 跳过） | 完成向导的用户次周打开率 | 显著高于纯跳过组 |
| 负向指标：向导中途杀进程率 | S1/S2 关闭窗口 | < 5%（过高说明某步卡人） |

---

## 2. 分步流程定义

状态机：`S0 → S1 → S2 → S3 → Workspace`，外加三条捷径（`S0 → S2[import]`、`S0 → S3[sample]`、`S2 → S3[skip]`）。
流程图见 `startup_journey_flow.html`（archify workflow showcase 9/9，含 happy-path / shortcuts / errors 三个视图，主路径=导入优先）。
流程图表达约定：S1 测试失败不画独立节点（失败是表单的行内状态，用户不离页；`conn_form` sublabel 注明 inline errors）；sample 路径不画独立节点（三入口语义由 S0 节点 sublabel 承载，布种细节见 Entry Contract 卡）。

### S0 欢迎选择（Welcome choice，三入口，导入主推）

- **进入条件**：MainPage 挂载后 `onboarding.completed !== true || onboarding.version < 1`（见 §4），且连接加载无错误。
- **内容**：三张 choice 卡（排序即优先级）+ 无 import-line（导入已升为主卡，不再是底部小字链接）：
  1. **主卡（primary 高亮边框）：Import connections** —— "From DBeaver, DataGrip, Navicat, TablePlus or a file"；
  2. 次卡：Create a connection manually —— "PostgreSQL, MySQL, SQLite, Redis and more"；
  3. 第三卡：Explore with sample data —— "A local SQLite playground — nothing to configure"。
- **退出条件**：
  - 点导入卡 → 调 `openConnectionShareDialog('import')` 打开**现有** `ConnectionShareDialog`（import 模式，来源 file/外部应用二选一，自动检测已安装客户端路径）→ 导入成功 ≥1 个连接 → **视为 S1 完成，直接进 S2**；取消/关闭对话框 → 回 S0（无状态丢失）；
  - 点手动卡 → S1；
  - 点 sample 卡 → 布种示例库 → S3（AI 记为未配置）。
- **校验/错误**：无（纯选择页，Back/Continue 隐藏，只有卡片可点——沿用 demo 的 footer 状态机）。导入对话框自身的错误态（密码错/格式不识别/无可用来源）由 `ConnectionShareDialog` 现有逻辑承载（`CopyableError` + `ipcConnectionShareError` 翻译），向导不包一层。
- **跳过策略（Q2 已确认：Skip 全程可见）**：S0/S1/S2 底部都有 `✕ Skip setup`（S3 无需）。点击行为全程一致：清空 AI key，直达 S3 Done（S0 点跳过时 entry 记为 sample 语义的"空完成"——S3 summary 的 Connection 行显示未配置，见 §2-S3）。
- **Q1 闭环**：MVP 即"导入对话框入口"（一行调用，零新代码）；向导内不内嵌导入结果步——导入成功后的连接列表由 S2→S3 的 summary 回填（从 store 读刚导入的连接），足够。

### S1 连接数据库（Connect your first database，仅手动路径）

> 注：导入路径成功的用户**跳过 S1**（已视为 S1 完成）。S1 只服务"手动创建"人群。
> **Q3 已闭环：S1 照搬 `NewConnectionDialog` 真实布局**（`src/components/connection/NewConnectionDialog.tsx:146-218`），删除 v2 原型里的顶部驱动卡片横排假布局。原型 S1 必须长成：左 220px 驱动列表（搜索框 + DbTypeBadge 图标 + 名称纵向行，按欢迎度排序，不可用驱动隐藏）+ 右表单区 + 底部 Test/Cancel/Save 三按钮。

- **进入条件**：S0 点手动卡。
- **内容 = `NewConnectionDialog` 的内容区复刻到向导步骤里**（对话框壳 `fixed inset-0 z-50` portal 不要，向导步骤区直接渲染 `flex min-h-0 flex-1` 那一段）：
  - 左：驱动列表（`DB_REGISTRY` 按 `sortDbTypesByPopularity` 排序 + 搜索框过滤 + `getAvailableDrivers()` **过滤隐藏**不可用项；`DbTypeBadge` 图标 + 名称纵向行；`useConnectionClipboardFill` 剪贴板连接串自动填充一并启用）；
  - 右：`ConnectionFormBody(variant="window")`（name/dbType/group/字段组/SSL/高级设置全量，**不是** demo 的 5 字段简化版——与正式新建连接同一套校验，零双轨维护）；
  - 底部 footer：Test connection + Cancel（= Back 回 S0）+ Save 三按钮（沿用 `new-conn-test-connection` / `new-conn-cancel` / `new-conn-save` 的 testid，便于 E2E 复用选择器）。
- **退出条件（硬门）**：`form.onSave()` 成功（凭据 AES-256-GCM 落盘）。**向导 Continue 在 Save 成功前 disabled**（真实组件语义：保存才是完成，test 只是校验——v2 的"test 即解锁"改为"save 才解锁"）。
- **校验规则**：沿用 `useConnectionForm.validate()`（必填、端口数字、SQLite 文件路径等）。
- **错误处理**：全部由 `ConnectionFormBody` 现有行内错误承载（可复制，不弹框）；超时（>30s）→ 提示检查 host/端口/防火墙，保留已填表单。
- **跳过策略**：**S1 可 Skip**（Q2 全程可见）：点 `✕ Skip setup` → 按全跳处理（清空 AI key，直达 S3 Done，见 §2-S3）。Cancel（footer 次按钮）= Back 回 S0，不算跳过。

### S2 配置 AI（Set up AI assistance，可选）

- **进入条件**：S1 保存成功。
- **内容**：
  - Provider 网格：从 `useAiStore().providers`（`ProviderListItem[]`）动态渲染，真实 id 为 `open_ai / deep_seek / ollama / custom`（demo 的 OpenAI/Anthropic/DeepSeek/Custom 是示意；Anthropic 走 `custom` + anthropic protocol，见 `ModelProfileDialog.protocolOptions`）。
  - 表单：apiKey（password）+ endpoint（ollama 默认 `http://localhost:11434`，custom 必填）+ model（`fetchRemoteModels` 拉列表，失败则手填）。
  - 安全说明 note（AES-256-GCM 本地加密、直连 provider），沿用 demo 文案。
  - 安全门：`safetyGate` 默认 `cloud_strict`（云端 key）/ `local_trust`（ollama），沿用 `getDefaultSafetyGate`，向导内不暴露（高级项去 Settings 改）。
- **退出条件（软门）**：点 Finish → 若填了 key 则 `validateConfig` → 成功则 `saveProfile(isDefault=true)`；**失败也允许继续**（记 AI 未配置 + warn 级提示，不 block）；点 Skip → 直接 S3。
- **校验规则**：ollama 免 key（apiKey 自动填 `ollama`）；custom 必须填 endpoint；model 为空时允许保存（运行时再选）。
- **错误处理**：validate 失败 → 行内黄 warn（不是红错），按钮仍可点；保存失败 → 红错 + 重试。
- **跳过策略**：Skip setup 按钮（demo 已有），跳过后 S3 的 AI 行显示 "Not configured (can be set later)"。

### S3 完成（Done）

- **进入条件**：S2 Finish/Skip，或 S0 sample 直达，或 S0/S1/S2 任一步 `✕ Skip setup`。
- **内容**：done-mark + 三行 summary（Connection / AI provider / Storage），值从 store 回填：
  - 手动路径：刚保存的连接（name · driver://host:port/db）；
  - **导入路径：刚导入的连接（`imported` 计数 + 第一个连接名，如 "3 connections imported · DBeaver"——`onImportSuccess` 回调自带 `{imported, overwritten, groupsAdded, sourceFormat}`，直接用）**；
  - sample 直达：Connection 行显示 "Sample dataset · SQLite (bundled)"，AI 行显示未配置，**下方加一行下一步提示**："Next: run the prebuilt query and press Add to Dashboard"（见 Q4 主路径）；
  - **全跳（任一步 Skip）**：Connection 行显示 "Not configured (can be added later)"，AI 行显示未配置。
- **退出条件**：点 Open DataZen → `updateSettings({ onboarding: { completed: true, version: 1 } })`（见 §4）→ MainPage 切 ConnectionPage（已有 `connections.length > 0` 分支，自动生效；零连接时回 WelcomePage，见 §4.2）。
- **错误处理**：无（只读页）。

---

## 3. 与现有页面的关系：新建向导页，不改造欢迎页

| 选项 | 结论 |
|---|---|
| A. 改造 WelcomePage 为向导 | ❌ 破坏现有 E2E（`welcome.ts` F5-E2E-001~005 断言欢迎页结构）与"删光连接回欢迎页"的语义 |
| B. **新建 `OnboardingWizard` 页，MainPage 分流** | ✅ **采用**。WelcomePage 保持原样（零连接用户的日常态）；向导是"首次专属壳"，完成后永不再见 |

MainPage 分流逻辑（现状 `connections.length === 0 → WelcomePage` 之前加一层）：

```
connectionsLoaded ?
  loadError ? 错误页（不变）
  : onboarding?.completed !== true || onboarding.version < ONBOARDING_VERSION ? <OnboardingWizard/>   // 新增：首次（Q5 版本化）
  : connections.length === 0 ? <WelcomePage/>            // 不变：日常零连接态
  : <ConnectionPage/>                                    // 不变
```

- `OnboardingWizard` 落点：`src/windows/onboarding/OnboardingWizard.tsx`（新目录，与 connection/settings 平级）。
- S1 表单复用 `ConnectionFormBody` + `useConnectionForm`（已有 `variant="window"`，向导内用 window 形态）。
- S2 表单复用 `ModelProfileDialog` 的 draft 逻辑（抽 `useAiProfileDraft` hook，避免 dialog 壳；或精简版——Phase 1 定）。
- import-line 复用 `openConnectionShareDialog('import')`（已有）。

---

## 4. 持久化状态设计（Q5 已确认：版本化结构）

### 4.1 标记：`AppSettings.onboarding?: { completed: boolean; version: number }`

- 类型：`src/types/index.ts` 的 `AppSettings` 加可选对象字段（可选 = 老版本 `settings.json` 无痛兼容，`undefined` 视为未完成）。
- 常量：`ONBOARDING_VERSION = 1`（与字段同处 types，或 onboarding domain 内；向导大改版时 bump 到 2，老用户 `version < 2` 会再弹一次——这就是版本化的意义）。
- Rust 侧：`src-tauri/src/store/settings.rs` 的 `AppSettings` 镜像加 `#[serde(default)]` 字段（前后端 serde 对齐，参考 monitor 字段模式）。
- 写入时机：**仅 S3 点 Open DataZen 时** `updateSettings({ onboarding: { completed: true, version: ONBOARDING_VERSION } })`（走正常 settings save 管线，自动落 `settings.json`）。
- 中途杀进程：标记未写，下次启动重进向导；S1 已保存的连接保留（不丢失用户劳动），S2 草稿丢弃（可接受）。
- **迁移说明**：Q5 确认时 onboarding 字段尚未落地（无 boolean 存量），无需迁移分支；若实施前有人先合了 boolean 版，则 load 时 `onboardingCompleted === true` → 视为 `{ completed: true, version: 1 }` 一次性 upsert。

### 4.2 重入规则

| 场景 | 行为 |
|---|---|
| 正常完成 | 永不再见（除非向导大改版 bump version） |
| 中途退出/杀进程 | 下次启动重进 S0（S1 已存连接会出现在 S0 的"已有连接"提示行，可直接 Continue 跳 S1——Phase 2 可选优化） |
| 手动重看 | Settings → 通用（或 Appearance 的 more 区）加 "Replay onboarding tour"（`onboarding={completed:false, version:1}`），面向客服/录 demo |
| 删光所有连接 | 回 WelcomePage（现状不变，**不**回向导——向导是首次语义，不是零连接语义） |
| 多窗口 | 向导只在 main 窗口；`datazen:settings-changed` 广播后子窗口无需感知 |

### 4.3 Sample 数据集（Q4 已确认：全英文 + Dashboard 主路径）

- demo 承诺 "bundled SQLite playground"。实现：`{appData}/sample/playground.db`，首次点击时由 Rust `seed_sample_db` 命令生成。
- **表与数据（全英文，Q4）**：`demo_sales(region TEXT, amount REAL, quarter TEXT)`，8 行：4 个 region（`East / North / South / West`）× 2 个 quarter（`Q1 / Q2`），amount 为不等的整数（如 East-Q1 1200 … West-Q2 950）。**禁止中文 region**（v2 的 `华东/华北/华南/西南` 作废）。
- **预置聚合查询（向导布种后自动存一条 query tab 草稿，Phase 3）**：`SELECT SUM(amount), region FROM demo_sales GROUP BY region;`——用户 Open 进工作区后直接 Execute 出数。
- **Dashboard 主路径（Q4）**：S3 sample 行下方提示 "Next: run the prebuilt query and press Add to Dashboard"；Open 后的首个 query 结果区已有 `query-add-to-dashboard` 按钮（`query.addToDashboard`，`QueryTransactionModals.tsx:526`）→ `AddToDashboardDialog`（`dashboard.addToDashboard`）。向导不新增任何 Dashboard 代码，只负责把用户送到"可一键 Add"的状态。
- 连接名固定 `Sample Playground`，group `Sample`，可删除（删除后就是普通零连接态，不触发向导）。

---

## 5. i18n key 规划（只改 `en/*` + 可选 `zh-CN/*`）

新建 domain 包 `src/locales/en/onboarding.ts`（+ `zh-CN/onboarding.ts`），挂到 `en/index.ts` / `zh-CN/index.ts` 的 domain 注册（参考 settings/ai 包模式）。key 清单：

```
onboarding.s0.title / .subtitle
onboarding.s0.importTitle / .importDesc / .importDetected(app)   // 主卡，Q6 预检文案
onboarding.s0.manualTitle / .manualDesc                          // 次卡
onboarding.s0.sampleTitle / .sampleDesc                          // 第三卡
onboarding.s0.skip
onboarding.s1.tag / .title / .hint / .test / .testing / .testedOk / .continue / .back
onboarding.s2.tag / .title / .hint / .apiKey / .model / .endpoint / .note
onboarding.s2.skip / .finish / .validateWarn
onboarding.s3.title / .hint / .rowConnection / .rowAi / .rowStorage / .aiUnconfigured / .open
onboarding.s3.importedSummary(count, source)                     // 导入来源回填
onboarding.sampleName （连接名 Sample Playground）
```

约 25 个 key。其他语言由发布前的 `i18n-sync-check` + i18n-sync skill 补齐（AGENTS.md 规则）。
Q4/Q5/Q6 新增 key：`onboarding.s0.detectNote`（"基于已知路径探测"标注）、`onboarding.s3.nextAddToDashboard`（sample 下一步提示）、`onboarding.s3.notConfigured`（全跳 Connection 行）、`onboarding.replay`（Settings 重播入口）。

---

## 9. 来源预检机制设计（Q6 已确认：要做，想清楚再做）

> 结论：**复用现有 `detect_connection_import_path` 命令，零 Rust 新增**；向导 S0 挂载后对 5 个外部来源逐个预检，命中的来源在导入卡副标题/预检行展示，未命中显示通用文案 + 手动选文件兜底。

### 9.1 现有实现（已核实，无需新命令）

- 命令：`detect_connection_import_path(source)`（`src-tauri/src/commands/config.rs:532`，前端 `connectionCommands.detectConnectionImportPath`），入参 `dbx|navicat|datagrip|dbeaver|tableplus`，返回 `{ path, found }`。
- 探测逻辑（`src-tauri/src/commands/connection_import/app_source.rs`）：`collect_defaults` → 各来源 `default_roots` 下先查**已知相对路径**（`known_relatives`，如 DBeaver 的 `workspace6/General/.dbeaver/data-sources.json`），再做**有界目录扫描**（`scan_dir`：深度 ≤5、文件预算 ≤400，跳过 node_modules/.git/Cache 等）。
- 各来源默认根（`default_roots`，随 OS 变化，`PathContext::from_env`）：
  - **DBeaver**：macOS `~/Library/DBeaverData`；各 OS 通用 `<data>/DBeaverData`、`~/.local/share/DBeaverData`。
  - **DataGrip**：`<data>/JetBrains`、`<config>/JetBrains`、`~/.config/JetBrains`（再按 `DataGrip*` 版本目录展开，取 newest）。
  - **TablePlus**：`<data>/com.tinyapp.TablePlus`、`<data_local>/com.tinyapp.TablePlus`、`<config>/tableplus`、`<data>/tableplus`。
  - **Navicat**：`<data>/PremiumSoft CyberTech`、`<data>/PremiumSoft`、`<config>/navicat`、`<data>/navicat`（macOS 另加两个沙箱 Container 路径）。
  - **DBX**：`<data>/com.dbx.app`（+ `DBX_DATA_DIR` 环境变量优先）。
  - 其中 `<data>/<config>` 按 OS 解析：macOS `~/Library/Application Support` / `~/Library/Preferences`；Windows Roaming/Local AppData；Linux `~/.local/share` / `~/.config`。
- 文件判据（`is_source_file`）：DBeaver `data-sources.json`；DataGrip `datasources.xml`；Navicat `*.ncx`；TablePlus `Connections.plist`/`*.tableplusconnection`；DBX `dbx.db`。

### 9.2 向导侧调用设计（Phase 2）

- S0 挂载后 `Promise.allSettled` 并发调 5 次 `detectConnectionImportPath`（单次是纯本地 stat 级操作，无 IPC 风暴风险；失败单个 `found:false` 处理，不 block 渲染）。
- 展示规则：命中的来源名拼进导入卡副标题（`onboarding.s0.importDetected(app)`，如 "Found DBeaver config"；多个命中取第一个，顺序 dbeaver > datagrip > navicat > tableplus > dbx）；**原型里的预检行标注为"基于已知路径探测"**（tooltip 或小字：`onboarding.s0.detectNote` = "Detected from known config paths only"）。
- 全部未命中：副标题退回通用文案（"From DBeaver, DataGrip, … or a file"），用户仍可点 File 手动选（`pickConnectionImportPathWith_dialog` / `pickConnectionsImportFile` 兜底）。

### 9.3 权限与隐私

- 只读**已知应用数据目录下的连接配置文件名**（存在性 + 后续用户主动点的导入解析），**不扫描全盘、不读行外文件**（`is_too_broad_to_scan` 拒绝 home/Program Files 等过宽路径；扫描预算封顶）。
- 文案承诺 local-first 不变：探测结果不出本机，预检行只显示"找到某来源配置"，不展示路径原文（路径只在导入对话框确认为用户自选后使用）。
- 探测失败降级：命令抛错/`found:false` 一律按"未发现"处理，S0 不报错、不转菊花（静默降级到通用文案）。

### 9.4 Rust 命令复用 vs 新增：复用，不新增

- 预检 = 现有 `detect_connection_import_path`；导入 = 现有 `import_connections_from_app` / `import_connections_at_path`。向导 Phase 2 只写前端调用 + 文案，**Rust diff 为零**（除 §4.1 的 settings 镜像字段，那是 Phase 0 的）。

## 6. 测试策略

### 6.1 Host 单测（Vitest，`src/**/__tests__`）

| 文件 | 覆盖 |
|---|---|
| `windows/onboarding/__tests__/wizardState.test.ts(x)` | 状态机：进入/退出条件、tested 门、skip 分支、sample 直达（纯逻辑 + reducer，Journey Test 风格：模拟 S0→S1→test fail→retry→S2→skip→S3 全程击键） |
| `windows/onboarding/__tests__/OnboardingWizard.test.tsx` | 渲染：footer 按钮状态机（S0 时 Continue hidden 等，照搬 demo `render()` 的 4 态断言）、驱动网格动态渲染、summary 回填 |
| 既有回归 | `welcome.test`、`MainPage.test`、settings store test（新字段可选兼容） |

### 6.2 E2E（Host，`e2e/specs/onboarding.ts`，suite journeys）

前置：清连接 + `onboardingCompleted=false`（经 `save_settings` invoke），reload。用 SQLite sample 路径做主断言（不依赖外部 PG）：

1. 首启显示向导 S0（`[data-testid=onboarding-wizard]`），WelcomePage 不出现；
2. S0 点 sample → 直达 S3，summary 有 Sample 行；
3. Open DataZen → 进入 ConnectionPage（sample 连接在树上）；
4. reload → 不再进向导（标记已写）；
5. 清理：删 sample 连接 + sample db 文件，`onboardingCompleted` 置回（避免污染后继 suite，参考 `welcome.ts` 的 reseed 模式）。

真连接路径（S1 test+save）用 E2E PG（`conn_e2e_pg` 同款参数）走一遍；AI S2 只测 skip（真 key 不进仓库，validate 用 mock/断网态测 warn 不 block）。

### 6.3 不碰的测试

- 驱动 crate 内测试（本次无驱动改动）；
- `welcome.ts` F5-E2E（WelcomePage 不动，向导 gate 在它之前——E2E 前置里把 `onboardingCompleted` 置 true 即可 bypass，Phase 0 顺手给 `welcome.ts` 加这行）。

---

## 7. 分阶段实施计划

### Phase 0 — 基建（不改 UI）

| 涉及文件 | 内容 |
|---|---|
| `src/types/index.ts` | `AppSettings` += `onboardingCompleted?: boolean` |
| `src-tauri/src/store/settings.rs` | 镜像字段 + `#[serde(default)]` + roundtrip 单测 |
| `src/locales/en/onboarding.ts`、`src/locales/zh-CN/onboarding.ts` + index 注册 | §5 key 全量 |
| `e2e/specs/welcome.ts` | before/after 置 `onboardingCompleted=true/false`（防新 gate 干扰既有 suite） |

验收：`tsc --noEmit` + `cargo test -p datazen --lib store` + 既有 welcome E2E 通过。

### Phase 1 — 向导壳 + S0 三入口/S3（无后端依赖）

| 涉及文件 | 内容 |
|---|---|
| `src/windows/onboarding/OnboardingWizard.tsx`（新） | demo 4 步状态机 React 化（footer 三按钮 4 态、stepIn 动效、aside 品牌栏）；**S0 三卡（导入主推高亮）+ 导入卡调 `openConnectionShareDialog('import')`**；S3 summary 三来源回填（手动/导入/sample）；S1/S2 先占位（视排期可与 Phase 2/3 合并） |
| `src/windows/onboarding/wizardState.ts(x)`（新） | 状态机 reducer（含 `S0 → S2[import]` 捷径、`importSuccess` 动作）+ 单测 |
| `src/windows/main/MainPage.tsx` | §3 分流（+3 行）；**确认 `ConnectionShareDialogHost` 在向导分支下仍挂载**（现状全分支都有，保持即可） |
| `src/windows/settings/...`（Appearance more 区或通用区） | "Replay onboarding" 入口（`onboardingCompleted=false`） |

验收：清标记 reload → 向导 S0 显示三卡（导入主推）；点导入卡 → 现有导入对话框打开；取消回 S0；点 sample → S3 → Open → 不再出现；单测全过。

### Phase 2 — S1 手动创建（复用 NewConnectionDialog 组合）

| 涉及文件 | 内容 |
|---|---|
| 向导 S1 | 左驱动列表（`DB_REGISTRY × getAvailableDrivers()`，照搬 NewConnectionDialog 侧栏：搜索+置灰）+ 右 `ConnectionFormBody(variant=window)` + `useConnectionForm` + `useConnectionClipboardFill`；Continue 门 = `onSave` 成功 |
| `src-tauri/...` | 无（复用既有命令） |
| E2E `e2e/specs/onboarding.ts` | 手动 PG 路径 + **导入路径（DBeaver fixture → S2，见 Q1 闭环）** |

验收：向导内手动建 PG 连接成功并落盘；Continue 门按保存成功生效；**导入 fixture 走通并直达 S2**；E2E 通过。

### Phase 3 — S2 AI + Sample 布种

| 涉及文件 | 内容 |
|---|---|
| 向导 S2 | provider 网格动态 + draft（apiKey/endpoint/model）+ `validateConfig`（软）+ `saveProfile(isDefault)`；Skip 全通 |
| `src-tauri/src/commands/sample.rs`（新，按命令落点规范） | `seed_sample_db`：生成 playground.db（含 demo_sales） |
| E2E | sample 直达 + skip 路径 |

验收：S2 配 ollama（本地免 key）走通；S0 sample 一键进 S3；AI skip 后 summary 显示未配置。

### Phase 4 — 打磨与发布

- 中文文案终校（zh-CN 包）+ i18n-sync 全语言补齐；
- 空态/错误态走查（无可用驱动、磁盘只读导致 save 失败、validate 超时）；
- journeys suite 全量 + contract matrix 回归；
- 文档：本方案归档 + Sprint 记录。

---

## 8. 待确认问题（Q）

- **Q1**：~~S0 的 import-line 范围~~ ✅ **已闭环**：MVP 即"导入对话框入口"（S0 主卡调 `openConnectionShareDialog('import')`，零新代码）；向导内不内嵌导入结果步，summary 从 `onImportSuccess` 回调回填。
- **Q2**：`✕ Skip setup` 全程可见，还是仅 S2（如 demo）？（推荐全程可见，S1 的跳过=回 S0。）
- **Q3**：S1 驱动列表是否包含 git 驱动（kiwi/superset）？构建未注入时置灰即可，还是隐藏？（推荐与 NewConnectionDialog 行为一致——其现状是**过滤隐藏**（`availableDrivers.includes` filter），不是置灰；建议向导照搬隐藏逻辑，Q3 收敛为"与 NewConnectionDialog 一致"。）
- **Q4**：Sample 库的表结构/数据量要不要产品化定义（demo_sales 几行？中文 region 是否保留）？（推荐 Phase 3 定 1 表 8 行，含中文 region，呼应原型饼图。）
- **Q5**：`onboardingCompleted` 是否需要版本号语义（如 `onboardingVersion: 1`，大改版时对老用户再弹一次）？（推荐 MVP 纯 boolean，v2 再说。）
- **Q6（新增）**：S0 导入卡是否展示"已检测到 DBeaver/DataGrip"这类来源提示（调用 `detectConnectionImportPath` 预检）？（推荐 Phase 2 做：有则卡片副标题显示"Found DBeaver config"，无则通用文案；零后端新增，纯前端预检。）
