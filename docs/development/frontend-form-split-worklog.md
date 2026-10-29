# 前端形态拆分实施工作日志（form-split worklog）

> 本文件随每个 form-split PR 同步提交，记录实际执行顺序、决策与验证结果，
> 避免工作进度丢失。条目按时间倒序（最新在上）。基线：`2a08e55`（F4a 合入后）。

## 2026-10-30（会话七）— R6 收口：driverInvoke 死代码焚烧——生产共享树 @tauri-apps import 归零（F6 输入条件达成）

> F6 计划的输入条件：「`src/` 内 `@tauri-apps` import 已归零，分家变为
> 纯机械动作」。核查发现最后一个真实 import 藏在**生成文件模板**里：
> `src/extensions/generated.ts` 的 `driverInvoke()`（`plugin:driverId|command`
> 直调轨）。全树消费审计（src/ + packages/ + scripts/ + tests）：**零消费者**
> ——纯死代码。驱动侧命令需要宿主轨的，走 driverGateway 域客户端
> （backend transport 轨，F3 已建）。

### 改动

- `scripts/resolve-drivers.mjs` 模板：删 `import { invoke } from
  '@tauri-apps/api/core'` 与 `driverInvoke()` 函数；保留 `DRIVER_COMMANDS`
  元数据表 + `hasDriverCommand`（不触宿主运行时，stash 工作流 fixture 依赖
  其形状）。
- `scripts/driver-deinject.mjs`（stub 形态模板）：同步删两块。
- `src/extensions/generated.ts` 为 .gitignore 构建产物（.gitignore:66）——
  本地 `--codegen-only` 重生成验证产出无 invoke、幂等。

### 终态审计（生产树 @tauri-apps）

- 真实 import 全部收拢桌面隔离区：`src/platform/desktop/*`（8 文件）+
  `src/shell/desktop/TitleBar.tsx`（1）——共 9 处，全部 F6 随 apps/desktop 走。
- 其余匹配全是**注释**（wappStore/useClipboardCopyFeedback/contracts/
  BrandSidebar/SettingsContent/tauriWindow 基建）与 gitignored 产物内的
  本批注释。
- 测试树终态见批次二盘点。

### 验证

- `scripts/__tests__` 793/795 通过；2 失败均为 cargo ENOENT 已知环境限制
  （CI rust 腿验证，worklog 会话一即有记录），与本批无关。
- typecheck（tsrs root/scripts/scripts-checkjs/pack-ep）OK——scripts 变体
  曾现 2 条 with-driver-inject JSDoc 推断瞬态错误，stash 对照 + 三次复跑
  均不可复现（tsrs 缓存态噪声）。
- 闸门 0 blocking / 1 advisory（不变）。

---

## 2026-10-30（会话七）— F6-tests 批次二：9 个残留共享区测试归位，测试树 @tauri-apps 归零达标

> 承接批次一。9 个残留文件按断言性质三路归位，共享区测试树从此只在
> 注释/字符串断言里出现 `@tauri-apps`——`packages/frontend` 的测试面
> 宿主无关（F6 目录迁移的输入条件之一）。

### 三路归位

- **stale mock 纯删除**：ConnectionPage.test —— 生产轨道早已走平台端口
  spy（`hasOpenChildWindowsMock` 绑进 `configurePlatform`），残留的
  `vi.mock('@tauri-apps/api/webviewWindow')` + `webviewGetAllMock`
  零断言消费，删除四处置点。
- **桌面轨道测试搬家（8 件 → `src/platform/__tests__/`，`.desktop` 后缀）**：
  这批测试绑**真实** `createDesktopBackendTransport()` 并断言 IPC 层契约
  （`resolve_foreign_key_targets` 等命令名/参数封窗、宿主错误文本经
  ApiError cause 链回环、Channel 订阅机器）——本质是「组件 × 桌面轨道」
  的集成审计，归桌面区测试，F6 目录迁移时随 apps/desktop 走：
  fkErrorRoundTrip / fkJumpSemantics.audit / gridErrorRoundTrip.audit /
  namedViews.contract / fkArrowRequest.audit / fkNavigation.journey /
  fkViewportRestore.audit / desktopSubscription（纯 transport subscribe
  机器）。零断言损失（保留 IPC 名映射覆盖），仅改相对 import 路径。
- **桌面组件测试归位**：TitleBar.test → `src/shell/desktop/__tests__/`
  （被测组件 TitleBar 本就在 shell/desktop；mock event/window 正当）。

### 批次一顺手修复

- WorkspaceIntegration.test 的未用 `clearPlatform` 导入（tsrs TS6133）。

### 终态盘点

- 共享区测试树 `@tauri-apps` 仅剩 3 处合规：WorkspaceIntegration 与
  OnboardingWizard 的**注释**、pathIpcWiring 的**字符串断言**（断言源码
  不含 `@tauri-apps/api/core`，非 import）。
- 桌面区测试（platform/__tests__ 14 件 + shell/desktop 1 件）保留 tauri
  mock——它们随 F6 目录迁移进 apps/desktop。

### 验证

- 定点：9 件迁移/清理 97/97 绿；TitleBar 3/3 绿。
- 块回归：platform+shell+lib+DataTable+connection+workspace 386 文件
  **3758 通过 / 1 skip**（598s）。
- typecheck（tsrs root/scripts/pack-ep）OK；闸门 0 blocking / 1 advisory。

---

## 2026-10-30（会话七）— F6-tests 批次一：37 个共享区测试的 `@tauri-apps/api/core` mock 焚烧

> 承接会话六（生产侧 shim 焚烧完）。本批对齐测试侧：共享区测试不再
> `vi.mock('@tauri-apps/api/core')`——invoke 轨道已骑 bound backend
> transport（R6 形态），mock 面换到 `setBackendClient('test', …)` /
> spy platform port，测试不再 import 宿主运行时。这是 F6 目录迁移的
> 前置：`packages/frontend` 里的测试文件必须宿主无关。

### 改动模式（三类）

- **纯删除**（多数）：测试早已通过 `setBackendClient('test', …)` 绑定
  transport，残留的 `vi.mock('@tauri-apps/api/core')` 只是历史噪声——
  删掉后 invoke 轨道走 transport 绑定，断言面不变（connection/dashboard/
  file/history/importExport/schemaDiff/syncPlan/transfer/transferJobs/
  wapps/BackupWindow/QueryPanel 族/DataSync/DataTransfer 族/
  wappBridge 族/sqlFileExecution/themePackApply/fetchRelationDdl 等 28 文件）。
- **换 spy platform port**：WorkspaceIntegration（EventChannel 端口
  `listenCrossWindow` spy——顺带修复 stale mock 静默跳过 guard 断言的真
  缺陷）；QueryArtifactExport（artifact 命令已上 legacy 轨，绑定
  transport）；BrandSidebar 侧 firstFrame 测试改 `getAppVersion` 端口直答。
- **杂项对齐**：OnboardingWizard / SchemaDiff 族 / DashboardPanel /
  DataTransferWindow 的 firstFrameI18n 与 ContentView 等随 mock 删除重排
  绑定时序。

### 残留盘点（下一批次输入）

- 测试树剩 9 个共享区文件仍 `vi.mock('@tauri-apps/*')`：fk 族四件
  （fkArrowRequest/fkNavigation.journey/fkViewportRestore.audit/
  fkErrorRoundTrip）+ fkJumpSemantics.audit + gridErrorRoundTrip.audit +
  namedViews.contract + lib/session/desktopSubscription +
  ConnectionPage（webviewWindow mock）。
- 合规不动：`src/platform/__tests__/*` 四件桌面机器测试 + TitleBar
  （shell/desktop 组件）+ pathIpcWiring（字符串断言非 import）+ 两处注释。

### 验证

- `vitest run`（37 文件定点）：**407/407 绿**（83s）。
- 闸门 0 blocking / 1 advisory（不变）。

---

## 2026-10-30（会话六）— F6-pre：六个 R7 shim 焚烧（转端口访问器），分家变纯机械

> 计划 §5.6 的输入条件「分家变为纯机械动作」的最后一英里。六个 F2 过渡 shim
> （windowManager / crossWindowBus / pathPicker / updater / usePlatform /
> tauriBackendTransport）全部从「re-export 桌面区」转为「共享端口访问器」或删除；
> 高层窗口助手与 updater 的纯推理逻辑**搬回家**（共享侧），桌面区只剩机器。
> R7 闸门集合收缩到 `src/main.tsx` 一项（随目录迁移消亡）。

### 端口面补口（backend-client / contracts）

- `PlatformServices` +2：`pickPath`（桌面=dialog 插件，web=null 即取消语义，
  `PickPathInput` 与 `@datazen/ui` 的 `PathPickerDialogOptions` 结构恒等）；
  `getOsPlatform`（桌面=plugin-os 探测、自带 UA 兜底；web=UA）。os.ts 整文件
  消亡（钩子逻辑回 `hooks/usePlatform.ts`，插件探测并入桌面绑定）。
- `HostSettingsPort` / `DesktopHostServices` +2：`checkForUpdates` /
  `downloadAndInstallUpdate`（类型 `UpdateProgress` / `UpdateCheckResult` 自
  desktop/updater **字节等价**上移 contracts；inert 端口 reject——Update 区
  本就 gate 在 `capabilities.updater` 上，§7.4 语义）。desktop/updater 瘦身为
  纯插件流（check/download/relaunch/maybeCheckOnStartup），channel/variant/
  manifest 纯函数回 `lib/updater.ts`。
- `requireBackendClient()` 新增（client.ts，照 requirePlatformServices 先例）：
  queryPanelSessionRoot 的 lazy `bindDesktopBackend()` 兜底退役——共享代码永不
  自绑宿主适配器。注册表拿到的 facade 是「首属性访问才解析」的 Proxy：peek/
  remove 纯簿记不触线（resetModules 形态的测试图构造安全），首次开 wire 才
  §7.4 响亮失败。

### shim → 访问器逐条

- **crossWindowBus**：共享模块持有 BroadcastChannel 默认轨（自桌面 eventChannel
  的浏览器分支**搬回**，`createBrowserEventChannel` 单一来源，web stub 复用）；
  绑定平台时走 `events` 端口。桌面 eventChannel 瘦身为纯 Tauri；装配根按
  `IS_DESKTOP_BUILD ? 桌面 : 浏览器轨` 绑定（browser-dev 行为不变）。
- **windowManager**：open*Window / emitMenuAction / PENDING_CONNECTION_KEY 等
  高层助手（URL 参数、localStorage、事件、i18n 推理）回共享侧，骑
  `requirePlatform().windows` + 总线；桌面 windowRouter 只留 openSingletonWindow
  （create_sub_window / WebviewWindow 聚焦 / 浏览器 tab 兜底 / 错误对话框）。
  原文件里向 lib 的 import（connectionEditor / settingsStore / docsUrls）随
  高层逻辑一起搬走——桌面区不再反向 import 共享 UI 模块。
- **pathPicker / usePlatform**：访问器 + 钩子回家（见端口补口）。
  `desktop/pathPicker.ts`、`desktop/os.ts` 删除；`platform/tauriBackendTransport.ts`
  shim 删除（消费方：queryPanelSessionRoot 走 requireBackendClient；5 个测试
  re-point 到桌面区真身）。
- **env.ts 上移**：`platform/desktop/env.ts` → `platform/env.ts`（纯
  `import.meta.env`，零宿主运行时）——`IS_DESKTOP_BUILD` 是**编译期构建常量**
  （裁决 #5），前端包为两种形态分别编译时各自折叠；桌面区与共享侧同源引用。

### 测试迁移（shape 不变、mock 面换轨）

- windowManager.test 拆两半：lib 侧绑 spy 平台钉**端口契约**（label/params/尺寸/
  事件载荷）；桌面机器（invoke/webviewWindow/去重/inflight/错误对话框）移
  `platform/__tests__/windowRouter.desktop.test.ts`。crossWindowBus.test 同形
  拆分（未配置=BroadcastChannel 默认轨；绑定=端口转发；Tauri 半边移
  `eventChannel.desktop.test.ts`）。updater 流断言移
  `platform/__tests__/updaterFlow.test.ts`（含 doMock 定制 SKU 的回归守卫）。
- 顺手修复**前会话 batch-1 遗漏的四组测试**（HEAD 上即红）：QueryArtifactExport
  （artifact 命令已上 legacy 轨但测试未绑 transport）、ContentViewKvDbSwitch /
  OnboardingWizard（同因 + BrandSidebar 版本读已走 HostSettingsPort）、
  AiSettingsSection（context_get_dir 未绑定 → 3 个未处理 rejection 导致
  vitest exit 1）。全部按既有 `setBackendClient('test', …)` / 平台绑定模式补。
- `check-ci-docs-consistency.mjs` 的 windowManager 指针随实现回迁更新。

### 验证

- typecheck（tsrs root/scripts/pack-ep）OK；闸门 0 blocking / 1 advisory。
- 全量分块回归：stores 903/903；windows+commands+components/ai 1930/1930；
  components 其余（DataTable 422 / filter+query+chart 等 1084 / admin+export
  +history+migration+schema+sql-editor+杂项 705）；drivers 配置 1007/1007；
  e2e-contract 23/23；backend-client+platform+hooks+lib 早前批全绿。
- 已知环境限制不变：cargo 依赖的 2 个脚本测试本机 ENOENT（远程 CI rust 腿验证）。
- 本机无 `tauri build`/webdriver 冒烟（无 cargo）——同 F4a/F4c 策略，开 PR 触发远程 CI。

---

## 2026-10-29（会话五）— F6-a：workspace 协议落地（packages 字段 + install 实测）

> 计划 §5.6 项1 / §7.1 风险项。此提交只落地协议本身：`pnpm-workspace.yaml`
> 增补 `packages: [apps/*, packages/*, packages/drivers/*]`，五个 TS 包成为
> 真 importer；跨包解析**仍骑 alias 表**（F6-b 才退役）——零行为变化，
> 闸门/typecheck/测试全绿即证。

### §7.1 实测记录（本环境的诚实边界）

- **主 checkout**：`corepack pnpm@11.15.0 install` 成功（23s，与 CI 同大版本；
  本仓无 packageManager 字段，CI 用 pnpm/action-setup@4 version 11）。
  lockfile 增 5 个 importer（backend-client/driver-sdk/extension-points/ui/
  wapp-sdk，各为空依赖表——尚无 importer 声明 `@datazen/*`，node_modules
  不出现链接，解析仍走 alias，预期零行为差异）。
- **第二 checkout**（tar 复制树，无 .git/node_modules）：`install
  --frozen-lockfile` 成功（5s，走共享 content-addressable store 硬链）。
  「新 workspace 字段在第二个 checkout 上干净安装」这一 §7.1 实质已证。
- **软链 node_modules 形态无法复现**：本沙箱禁 symlink（`ln -s` /
  `python os.symlink` 均被拒，连带 `git worktree add` 整体不可用——git
  worktree 自身需要 symlink）。§7.1 注记的「工作树 node_modules 软链 +
  workspace hoist directory is not a real directory」失败形态**留待 CI 与
  真实开发者环境观察**；pnpm-workspace.yaml 既有的 verifyDepsBeforeRun
  关闭逻辑不动，若该形态出现，按其注释回退 packages 字段即可（计划 §5.6
  回滚预案的第一段）。

### 验证

- typecheck（tsrs root/scripts/pack-ep）OK；backend-client + platform
  契约批 237/237 绿；闸门 0 blocking / 1 advisory（不变）。

---

## 2026-10-29（会话四）— R6 台账焚烧 batch-2（收口）：8 → **1 advisory**，台账清空

> 承接会话三的 batch-1（34 → 8）。本批把剩余 6 个 R6 台账文件全部端口化，
> `R6_LEGACY_TAURI_FILES` 集合清空，规则转全阻断——自此生产 `src/` 在
> 桌面区外的任何新 `@tauri-apps/*` import 直接红闸门（F6 `packages/frontend`
> 必须宿主运行时无关的绊网就位）。剩余 1 advisory 为 R3 的
> DocumentConnectionView 既有豁免（永久条目，非本计划范围）。

### 端口化清单（六文件 → 三端口一界面）

- **MenuBar（fullscreen）**→ `PlatformServices.toggleFullscreen`：桌面绑定
  用 zone 内 `getCurrentWindow()`（裁决 #5：窗口控制属桌面区）；web 降级为
  document fullscreen API。裸 try/catch 保留。
- **fetchRelationDdl（clipboard×2）**→ `writeClipboard` + 新端口
  `writeClipboardHtml({ html, altText })`：桌面绑定即原
  `write_clipboard` / `write_clipboard_html` 宿主命令（字节等价）；web 降级
  为 `ClipboardItem`（缺失或无手势时退 plain text，绝不静默 no-op）。
  测试改为端口绑 invoke mock，线断言（命令名+载荷）原样保留。
- **wappBridge（wapp_session 族 + 通知）**：session/storage 走
  `legacyInvoke`（batch-1 轨道）；通知走新端口
  `showNotification({ title, body })`——桌面绑定保持
  `plugin:notification|notify` 线形，web 降级为 Notification API
  （权限拒绝即 reject，调用方本就 best-effort）。
- **surfaceBgCache（host 文件持久化）**→ `HostSettingsPort.persistSurfaceBackground`
  （fire-and-forget）：桌面绑定即 `set_surface_background` IPC；web 为
  inert no-op。运行时 `__TAURI_INTERNALS__` 探测删除，改为
  `getPlatformOrNull()?.hostSettings?.…` 可选链（平台未配置 = 旧探测的
  同一「无处可持久化」情形）。
- **BrandSidebar / SettingsContent（getVersion）**→ `HostSettingsPort.getAppVersion`：
  桌面绑定 zone 内 `@tauri-apps/api/app`；inert 端口 reject，调用方
  gate 在 `capabilities.versionSource`（F5 表已在位）上、`.catch` 保持
  占位符/隐藏行为。SettingsContent 顺带消掉了文件顶部最后一个
  `@tauri-apps` 运行时 import（F5 留下的探测残留随之自然收口）。

### 测试修复（本批唯一非机械改动）

- `wappBridge.test.ts` 的端口绑定原是模块级一次性 `setPlatformServices` +
  全局 `afterEach(clearPlatformServices)`——首个测试后端口即处于未绑定态，
  后续 `ui.notify` 用例全部未捕获 reject。改为 `beforeEach` 重绑（26/26 绿）。

### 验证

- typecheck（tsrs root/scripts/pack-ep）OK；触点批 8 文件 67/67 绿；
  commands+platform+backend-client+MenuBar+SettingsContent 广域回归
  **417/417** 绿。
- 闸门 **0 blocking / 1 advisory**（8 → 1，台账集合已删除内容转空集）；
  生产 `src/` 中桌面区外 `@tauri-apps` 引用全数核对为注释、生成注册表
  （`src/extensions/generated*.ts`，既有豁免）或测试 mock。
- 零 Rust 侧改动：本批端口化的五个宿主命令/插件调用全部复用既有注册
  （`write_clipboard_html` / `set_surface_background` /
  `plugin:notification|notify` / app/window JS API），无需动 `run.rs`。

---

## 2026-10-29（会话三）— R6 core-invoke 台账焚烧 batch-1：34 → 8 advisory

### 迁移设计（照 DriverGateway 先例的第二个过渡逃生舱）

- `packages/backend-client/src/legacyHostCommands.ts`：`LegacyCommandName`
  联合类型（189 个遗留宿主命令名，从全部调用点机械提取）+
  `LegacyHostMethodMap`（mapped type：request loose / response unknown，
  命令名编译期校验）+ `legacyInvoke<T>(command, payload?)` 薄再导出——
  与 `@tauri-apps/api/core` 的 invoke 同名同泛型同元数，调用点零改动，
  仅换 import 行（F2「收编不新建」模式的复用）。
- 载荷原样透传（省略时 undefined，桌面 transport 对 snake_case 扁平路由）：
  与被替换的 invoke 字节等价。
- `FacadeCoversMethodMap` 断言排除 legacy 轨键（由 legacyInvoke 服务，不上门面）。

### 批量迁移（24 文件）

- 17 个 `src/commands/*.ts`（ai/artifact/backup/connection/context/dashboard/
  database/file/history/query/sample/schemaDiff/sync/transfer/transferJobs/
  tunnel/wapps）+ sqlFileExecution + BackupWindow + 2 个 data-sync hook +
  DataTransferWindow + OnboardingWindow + useSchemaDiffEndpoints。
- union 补漏：ai 的 nativeInvoke 三命令（grant/revoke/touch）与 schemaDiff 的
  两个 prepare 命令（间接传参，库存脚本初版漏检）。

### 测试迁移（照 file.test.ts 先例）

- 19 个测试文件补 `setBackendClient('test', { transport: { call: … } })`
  绑定（元数归一适配器保持无实参断言原样）；resetModules 场景（firstFrameI18n）
  在 beforeEach 重绑新模块图的 backend-client 实例。

### 验证

- typecheck 绿；src/commands + backend-client 344/344 绿；schema-diff 69/69、
  data-sync/data-transfer/backup/onboarding/sqlFile 全绿；闸门 0 blocking /
  **8 advisory**（34 → 8）。
- 剩余 8 = R3 既有豁免 1 条 + R6 六文件待端口化：MenuBar(fullscreen)、
  fetchRelationDdl(clipboard×2)、surfaceBgCache(host 文件持久化)、
  wappBridge(wapp_session+通知)、BrandSidebar/SettingsContent(getVersion)。

---

## 2026-10-29（会话二）— F4c 簇合并收口：223 → **215**（计划终态命中）
> 本段落 rebased 在并行会话的 F5（59dea6941）与 R6 event 焚烧（d712c47ff）之后：
> 两个会话的 F4c 重核结论一致（零死命令）；本提交把交接说明中「合并簇为
> 持续整合项」落实为实际合并，机械计数达到计划 §8 终态 ~215。


### 簇核定（对照计划 §5.4 项6 与 §8 轨迹）

- **死命令重核**：对 223 命令面跑字面引用 + transport.call 双轨审计，
  **零死命令残留**——main 侧 `6c883f2c0`（6 冗余命令）、`ad1ff0102`
  （query history 合并 + 网关薄壳 ×2）、`ce8c37998`（get_er_data +
  get_system_ui_language）已提前消化计划中的死命令清单。计划 §8 的
  267→255→~232 为旧基线估算，机械计数为准。
- **schema plan 族 6→2**：`prepare_schema_unified_plan` 是合并目标且已在线
  （SchemaDiffWindow 双路径：对象走 unified、纯表走 table）。四个 per-kind
  命令（view / routine_trigger / sequence / type）经核定**零产品调用、零
  e2e 调用**——只有 `schemaDiff.ts` 里的死包装方法与其单测在引用。跨方言
  纯表场景 unified 内部已委托 table impl（`unified_plan.rs:144`），但同方言
  纯表走的是 `build_unified_schema_diff_plan_with_source_scope` 独立代码
  路径，与 table builder 非行为等价——**table 命令保留**，不做 6→1 的
  最后一步（风险登记：无法本地编译验证 Rust 行为等价性，遵循「机械计数
  为准 + 不做无法验证的行为切换」）。
- **dir-open 4→2**：`open_log_dir` / `open_workflows_dir` /
  `open_context_dir` 合并为 `open_known_dir(kind: log|workflows|context)`；
  路径保持服务端解析（安全属性不变），`open_path` 保留（webdriver 门控 +
  app-data 校验路径）。
- **file pickers 3→1**：`pick_connections_import_file` /
  `pick_app_data_import_file` / `pick_connection_import_path_with_dialog`
  合并为 `commands::dialog::pick_import_file(kind, mode?, source?)`——放
  dialog 网关（R-1.3 集中化），filter 表留在 feature 模块
  （`connections_open_filters` / `import_file_filters` 升 pub(crate)）。
  `pick_data_transfer_sql_file` 是 save+token 语义（路径不出宿主），
  不属 open-picker 族，保留。
- **dialogs 8→6 / job 读侧 13→5 / AI 一次性 8→2**：经基线对照，这些簇
  在 main 侧 IPC 收敛 PR 中已按不同形状吸收（file_jobs 命令族删除、
  query_history 合并等）；当前面 215 已达终态，不再为凑旧估算数字做
  无验证依据的深部 Rust 手术。

### 改动清单

- Rust：删 `object_plan_commands.rs`（整文件）；`schema_diff.rs` /
  `object_scope.rs`（死 helper `fetch_schema_view` / `list_schema_views` 一并
  删除）；`config.rs`（open_known_dir 合并体）；`dialog.rs`（pick_import_file
  合并体）；`connection_import/ipc.rs`、`app_archive.rs`（旧 picker 删除）；
  `app_menu.rs`（菜单 OpenLogDir 走 open_known_dir("log")）；`run.rs` 注册
  223→215。
- TS：`schemaDiff.ts` 删 4 死方法 + 4 死测试；`connection.ts` / `backup.ts`
  切 `pick_import_file`；`hostServices.ts` 切 `open_known_dir`；
  `pathIpcWiring.test.ts` / `importExportCommands.test.ts` 断言更新；
  e2e `path-ipc-hardening.ts` / `workflow-window.ts` 源码断言重定向到
  hostServices 区（原断言指向 F2 已迁移走的 settings.ts 旧位置——顺手
  修复了这两处陈旧断言）。
- 闸门：R6 台账移除 `src/commands/settings.ts`（F4a 已清零，过期项）。

### 验证

- typecheck（tsrs root/scripts/pack-ep）OK；commands+platform+backend-client
  **337/337** 绿；schemaDiff 单测 17/17；边界闸门 0 blocking / 41 advisory；
  ci-docs 一致性 OK；注册命令机械计数 **215**（计划 §8 终态目标命中）。
- Rust 侧本机无 cargo（工具链下载受限）：静态核对全部删除项引用面 +
  契约测试源码断言双向核对；宿主 crate 依赖远程 CI 验证（开 PR 触发，
  同 F4a 策略）。

---

## 2026-10-29（续） — R6 台账 event 类焚烧（d712c47ff）+ 会话终态

- `@tauri-apps/api/event` 类违规全部迁移到 F2 EventChannel 端口（裸载荷，无 Tauri 信封）：
  WorkspaceView（wapps:open-page 深链）、sqlFileExecution（restore-progress）、
  QueryPanel（menu:add-favorite）、ai.ts（三条流监听）、file.ts（onExportProgress）、
  BackupWindow（backup-progress）。
- surfaceBgCache 尝试后**回退**：`set_surface_background` 是宿主文件持久化命令而非涂色，
  需专门端口（行为保持优先于台账数字）。
- 台账 41 → 34 advisory；测试从 event mock 改为平台契约 mock（裸载荷断言）。
- 验证：typecheck 绿；闸门 0 blocking / 34 advisory；受影响 125 测试绿。

### 会话终态与续作交接（F6 前）

- **已完成**：F1–F5 全部合入并推送（至 3d66bf002）+ 本批 event 焚烧（d712c47ff）。
- **F6 前置未满足**：`src/` 内 `@tauri-apps` 未归零——剩余约 33 文件全部为
  core-invoke 深迁移类（commands/*.ts 的 legacy 命令族 + BackupWindow/fetchRelationDdl/
  wappBridge/onboarding 等）。每文件需端口/门面设计（kernel 面方法入 MethodMap、
  宿主命令入 HostSettingsPort/新端口、驱动命令走 driverCommands 桥），非机械替换。
- **F6 本体**（workspace 协议 + 目录迁移 + R5 闸门）依计划为单个大 PR，
  须在上述归零后执行；worktree checkout `pnpm install` 实测为 §7.1 强制风险项。
- 续作路径：①烧 core-invoke 台账（按域分批）→ ②F6 restructure
  （`pnpm-workspace.yaml` packages 字段 + apps/desktop + packages/frontend + alias 退役）。

---

---

## 2026-10-29 — F4b + F4c 重核 + F5 合入（74d2eef → 59dea69）

### F4b（74d2eefe8）
- 命名归零完成：`get_table_data_at_target`→`get_table_data`、`read_artifact_chunk`→`read_artifact`、
  `platform_list/create/update/disable_profile`→`list/create/update/disable_connection`；
  **`commandOverrides` 表删除**（toCommandName 纯约定，kernel 轨与命令名 1:1，裁决 #2 第 4 步达成）。
- Connection 命令 wire 对齐 kernel 面（flattened ProfileDraft + kernel SubmissionToken、
  expectedRevision）——预留命令真正可从 connectionClient 调用（激活）；消费方切换依计划 §7
  「不设硬同步门槛」归 D 阶段。
- 导出双流程示范（§3.3）：`TablesExporterPort` 共享签名 `exportTables(request)`；桌面实现走
  save-dialog token 流式写（进度上 EventChannel），`createBlobTablesExporter` 为 web 半边
  （分页读入 → CSV → Blob 下载，仅浏览器 API，R6 天然合规）。
- R8 退役：driver-sdk 事件桥 `bindHostEventListener/listenHostEvent`；装配绑到 EventChannel；
  redis PubSubPanel 改走桥（不再 import @tauri-apps/api/event）。
- capabilities 预备：`settingsSectionsForBackend(ReadonlySet<SettingsSection>)`；
  SettingsContent 读 `capabilities?.settingsSections`——**src/ 内 `backendId ===` 代码级归零**。

### F4c 重核（并入 F5 提交说明）
- 经验式死命令扫描（注册名 × 全树字符串引用）：**0 条真死命令**——main #142/#143 已执行删除计划；
  合并簇为持续整合项（计划 §8 台账随 PR 更新）。F4 退出标准全项达成。

### F5（59dea6941）
- `src/platform/capabilities.ts`：DESKTOP/WEB_CAPABILITIES 静态表（sections/updater/versionSource）；
  桌面装配绑定。UpdateSection 消费 `capabilities.updater`、版本读消费 `versionSource`。
- 每域契约表 `domainContractTables.test.ts`（32 例：方法名 × 请求封套 × 响应直通 + 缺口两分类）。
- 端口契约 `platformContracts.test.ts`（11 例：§7.4 未绑定显式错误 + 绑定后行为；HostSettings
  惰性默认例外）。
- web stub `src/platform/web/`：全 throw transport（CapabilityUnsupported，绝不静默空答）+
  浏览器原生 PlatformServices 降级 + 惰性宿主端口 + Blob 导出半边 + WEB_CAPABILITIES——
  **编译期同形持续验证**。
- playbook：`docs/development/platform-extension-playbook.md`（决策树 + F3 复刻配方 + 测试标准）。
- 验证：typecheck 绿；闸门 0 blocking / 41 advisory（R8 清零）；418 测试绿。

---

---

## 2026-10-29（会话一）— 环境基线与 F4b 并行会话发现

### 会话环境基线（续作前核对）

- 检出 `docs/frontend-form-split-plan` @ `2a08e5507`，工作区干净。
- 工具链：node v24.22.1；pnpm 12.16.1（全局安装）；**cargo 本机不可用**
  （rustup 官方 CDN 与清华镜像均超时，直连组件下载 ~16KB/s 不可行）。
- `pnpm install --frozen-lockfile` 完成；`scripts/typecheck.mjs --engine=tsrs` OK；
  `vitest run packages/backend-client` 194/194 绿；边界闸门 0 blocking / 42 advisory。
- IPC 注册命令实测：**223**（计划轨迹数字 267→255→~232 为旧基线估算，
  main 侧 IPC 合并已提前消化部分——F4c 需按当前面重新核定死命令清单）。

### F4b 并行会话发现（rebase 时合入）

远程在克隆后新增 `74d2eefe8`（F4b）：命名归零完成（commandOverrides 表
删除、platform_*_profile→*_connection 等 7 项改名）、TablesExporterPort
导出双流程示范（桌面流式 + web Blob）、R8 退役（driver-sdk host event
bridge）、`settingsSectionsForBackend(ReadonlySet<SettingsSection>)` capabilities
预备形态（最后一个 backendId === 'desktop' 探测消除）。Connection 消费方
切换按计划 §7.4 无硬同步门槛裁决随 D 阶段走。

### 待办（按计划 §5.4–§5.6）

- [x] F4b：Connection 域示范迁移 + `platform_*_profile` 改名 + override 表消减（并行会话）
- [x] F4c：死命令删除 + 合并簇（本会话，223→215）
- [x] F4 项5：SettingsContent capabilities 预备、PubSubPanel 违规、导出双流程（F4b 并行会话）
- [ ] F5：DESKTOP_CAPABILITIES / 每域契约测试 / web stub / playbook
- [ ] F6：workspace 协议 + 包完整化 + 目录迁移 + R5 闸门

