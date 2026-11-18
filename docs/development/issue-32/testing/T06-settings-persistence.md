# T06 测试报告：设置持久化链路（双端共享契约）

- 被测对象：`packages/frontend/src/lib/valuePresentation/settings.ts`（新增，宽容解析 + 快照构建）+ `packages/frontend/src/hooks/useValuePresentationSnapshot.ts`（新增）+ `packages/frontend/src/lib/valuePresentation/registerBundledContributions.ts`（新增，启动注册内置贡献）+ `packages/frontend/src/stores/settingsStore.ts`（`valuePresentation` 字段 + `valuePresentationRevision` 计数器 + `updateValuePresentationSettings`）+ `packages/backend-client/src/settings.ts`（镜面类型 + 透传）+ `packages/desktop/src/platform/desktop/settingsClient.ts` + `packages/frontend/src/bootstrap/startSharedApp.tsx`（启动注册）+ `packages/frontend/src/commands/settings.ts` + `packages/frontend/src/types/index.ts` + `packages/frontend/src/windows/connection/{ContentView,PanelContentRenderer}.tsx` + `packages/frontend/src/windows/connection/result-workspace/ResultTableView.tsx`（快照接线）+ Rust `crates/datazen/src/settings/{domain,application,mod}.rs`（`SharedSettings.value_presentation`，options 为 `serde_json::Value` 透传）+ `crates/datazen-server/src/storage/settings.rs` + `crates/datazen-desktop/src/store/settings.rs`（桌面 `AppSettings` 映射，含 `d431b0cd5` 补齐）+ 自带测试 `settings.test.ts`（valuePresentation）、`settingsStore.test.ts`、`useValuePresentationSnapshot.test.tsx`、`registerBundledContributions.test.ts`、`settingsExport.test.ts`、`backend-client/__tests__/settings.test.ts`、`settingsClient.test.ts`、桌面 `platform/settings.rs` 与 `store/tests/settings_shared_tests.rs`（`39d03a23d` 新增 4 项）
- 分支：`feature/issue-32-value-presentation`（HEAD `39d03a23d`，作者 issue-32-agent；T06 四提交：`4b2891ba6` feat + `c2dfa7920` test + `d431b0cd5` fix + `39d03a23d` test，基底 `b1c6ef3d8`；验证开始时工作区干净）
- 测试角色：协调方直接验证（原派出的独立 tester 因子任务指令变更被协调方关闭，未产出报告；本报告全部数字均为协调方实测，见 §1）。未修改任何被测源码或测试。
- 依据：`docs/archive/development/issue-32-value-presentation-design.md` §5 §6 §7、T04 `presentationPlan.ts` 与 T04 报告观察项 O-1/O-5、T05 报告观察项 O-2
- 日期：2026-11-12

## 1. 测试环境与命令

环境：Node v24.22.1，vitest 4.1.10（仓库自带），tsc 7.0.2 / tsrs 0.18.0，cargo 1.99.1（`~/.cargo/bin`），系统时区 UTC（全部探针均显式传入时区与语言，不依赖机器默认值）。对抗性探针运行于独立临时项目 `/tmp/probe-t06/`（通过 `node_modules` 符号链接复用仓库依赖，`environment: jsdom`，被测源码以绝对路径导入，不污染仓库工作区；探针按任务要求用后删除——`/tmp` 在任务中途被环境重启清理，探针文件随之清除，最终 `git status` 复核仓库除本报告外无任何改动；探针结果以运行输出为准，见 §2.2，原始日志 `~/workspace/t06-probes.log`）。

环境说明：本机 `PATH` 中无 `pnpm` 二进制（与 T02–T05 报告相同情况）；typecheck 改用 `npx -y pnpm@12.18.0 <script>` 调用，版本号实测 12.18.0，属等价调用，不影响门禁结论。本机以 root 身份运行测试，`USER` 与 `LOGNAME` 均为空、无登录会话，见 §7——桌面 settings 套件必须带 `USER` 跑，否则 5 项既有契约测试在身份关上失败（环境问题，非代码问题）。另：任务中途发生一次运行时重启，`/usr` 与 `/tmp` 回滚（桌面编译依赖丢失后按 `libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libayatana-appindicator3-dev librsvg2-dev` + `libzstd1` 降级到 `1.5.5+dfsg2-2build1.1` 重装；其中降级须加 `--allow-downgrades`，见 §7.5 附记），`~/workspace` 完好；G6/G7 均用 `CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0'` 跑（首次默认参数编译被 OOM 杀死，见 §7.5）。

| 命令 | 用途 |
|------|------|
| `npx vitest run packages/frontend/src/lib/valuePresentation packages/frontend/src/hooks/__tests__/useValuePresentation.test.tsx packages/frontend/src/hooks/__tests__/useValuePresentationSnapshot.test.tsx packages/frontend/src/stores/__tests__/settingsStore.test.ts packages/frontend/src/lib/__tests__/settingsExport.test.ts packages/backend-client/__tests__/settings.test.ts packages/desktop/src/platform/__tests__/settingsClient.test.ts packages/frontend/src/windows/settings/__tests__/SettingsContent.test.tsx packages/frontend/src/windows/settings/__tests__/SqlSnippetsCard.test.tsx --retry=0` | G1：T06 定向（T06 触及的全部测试文件 + valuePresentation 全库） |
| `npx vitest run packages/frontend/src/windows/connection packages/frontend/src/components/DataTable --retry=0` | G2：接线回归（windows/connection 全量 + DataTable） |
| `npx -y pnpm@12.18.0 typecheck` / `npx -y pnpm@12.18.0 typecheck:tsc` | G3：双引擎类型门禁（tsrs / 权威 tsc） |
| `node scripts/check-test-module-naming.mjs` | G4：测试命名门禁 |
| `npx vitest run <G1 路径> --retry=0 --coverage` | G5：新文件覆盖率（v8 报告 + coverage-final.json） |
| `cargo test -p datazen --lib --features settings settings`（`CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0'`） | G6：内核 settings 测试 |
| `cargo test -p datazen_desktop --lib settings`（同上低内存参数；无 USER / `USER=root` 各一遍） | G7：桌面 settings 测试，两边对照（§7） |
| `cargo fmt --check` | G8：格式门禁 |
| `npx vitest run --config /tmp/probe-t06/vitest.config.ts` | 对抗性探针 P1–P9（P4 为 Rust 侧，由 G7 两边重跑验证） |
| `git show 4b2891ba6/c2dfa7920/d431b0cd5/39d03a23d` 全 diff + 源码审读 | G9：改动范围、`any` / 跳过断言、设计符合性 |

## 2. 测试项清单

### 2.1 既有验证（仓库门禁）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| G1 | T06 定向单测 | 见 §1 表格 | ✅ 15 文件 **399/399 通过**（退出码 0） |
| G2 | 接线回归 | `windows/connection` + `DataTable` | ✅ 168 文件 **1672/1672 通过**（退出码 0；与 coder 申报一致） |
| G3 | 类型门禁（双引擎） | `pnpm typecheck` / `pnpm typecheck:tsc`（经 `npx -y pnpm@12.18.0` 等价调用） | ✅ 两者均 OK（退出码均为 0） |
| G4 | 测试命名门禁 | `node scripts/check-test-module-naming.mjs` | ✅ ok（1837 Rust 文件、256 cfg(test) 模块；2572 TS 文件、8 wdio spec 树；退出码 0） |
| G5 | 新文件覆盖率 | `npx vitest run <G1 路径> --retry=0 --coverage` | ✅ 全部 ≥80%（见下表明细）。测试本身 399/399 通过；进程退出码 1，原因唯一是全局阈值（All files 5.16 / 5.12 / 3.91 / 5.49，只跑子集所致，与 T04/T05 报告的已知现象相同），不是测试失败、不是新文件未达标 |
| G6 | 内核 settings 测试 | `cargo test -p datazen --lib --features settings settings`（`CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0'`） | ✅ **14/14 通过**（退出码 0；42 项被过滤） |
| G7 | 桌面 settings 测试（两边对照） | `cargo test -p datazen_desktop --lib settings`（同上低内存参数），无 USER 与 `USER=root` 各一遍 | ✅ 无 USER：**39 通过、5 失败**（5 项均为 §7 的既有契约测试，环境问题）；`USER=root`：**44 通过、0 失败**（退出码 0）。见 §7 |
| G8 | 格式门禁 | `cargo fmt --check` | ✅ 干净（退出码 0） |
| G9 | 改动范围与源码审读 | 四提交全 diff + grep | ✅ 通过（见 §2.3） |

> G5 明细（v8 报告；`% Stmts / % Branch / % Funcs / % Lines`）：
>
> | 文件 | % Stmts | % Branch | % Funcs | % Lines | 未覆盖行 |
> |------|---------|----------|---------|---------|----------|
> | `lib/valuePresentation/settings.ts`（新） | 96.55 | 100 | 100 | 96.15 | 133（`validateConfig` 抛异常的防御性 `continue`，见 O-3） |
> | `lib/valuePresentation/registerBundledContributions.ts`（新） | 100 | 100 | 100 | 100 | — |
> | `stores/settingsStore.ts` | 92.77 | 93.33 | **80** | 93.58 | 见 O-4：Funcs 恰为 80.0，踩线通过 |
> | `lib/valuePresentation/presentationPlan.ts` | 96 | 96.39 | 100 | 96 | — |
> | `lib/valuePresentation/timestampContribution.ts` | 94.55 | 93.26 | 100 | 94.55 | — |
>
> 注：`hooks/useValuePresentationSnapshot.ts`、`bootstrap/startSharedApp.tsx`、`platform/desktop/settingsClient.ts` 不在根 vitest 配置的 coverage include 清单内（清单只含 `lib/**`、`stores/**`、DataTable、ai 与少量 windows 文件），故无单文件覆盖率数字；其行为由 G1 的 399 项测试覆盖（hook 自带测试 6 项全过，见 §2.3）。这是配置现状，不是 T06 缺口；建议见 O-5。

### 2.2 对抗性探针（/tmp/probe-t06，独立于 coder 自带测试编写，19 项）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| P1a | 旧 payload 兼容：缺字段/非对象输入 | `parseStoredValuePresentationSettings(undefined / 'junk' / {} / {renderers:{}})` | ✅ 通过。全部返回 `{version:1, renderers:{}}`，不抛 |
| P1b | 非对象 renderer 条目丢弃，对象条目原样保留 | 含 `extraField` 的对象条目 + `'not-an-object'` / `42` 条目 | ✅ 通过。对象条目（含未知字段）原样保留；两处非对象条目丢弃 |
| P1c | 解析结果不别名默认值 | 两次解析 `undefined`，改第一份后重解析 | ✅ 通过。两次返回不同对象；改动不污染默认值 |
| P2a | 未知版本保留原文 | version 99 的存储值 → 解析 + 快照构建 | ✅ 通过。解析保留 99 与 renderers；快照为空（configs {}、enabled []、overrides {}），revision 原样透传 7 |
| P2b | store 收到 v99 后仍存 99，不被默认值覆写 | `applySettingsLocally` 喂 v99 → 读 store；再恢复默认 | ✅ 通过。store 存 version 99，revision +1；恢复后为默认 |
| P3a | 快照矩阵：启用+有效 | native iso-tz/Asia/Shanghai 启用 | ✅ 通过。enabled 含 `datazen.timestamp`，configs 内容逐字等于输入 options，revision 透传 |
| P3b | 快照矩阵：启用+非法 options | format `'bogus'` | ✅ 通过。跳过，configs {}、enabled [] |
| P3c | 快照矩阵：disabled | enabled false | ✅ 通过。跳过 |
| P3d | 快照矩阵：未知 renderer id | `'other.renderer'` 启用 | ✅ 通过。跳过（registry 缺失） |
| P3e | 快照矩阵：options 缺失 | options 为 undefined | ✅ 通过。跳过，不抛 |
| P3f | 快照矩阵：validateConfig 抛异常的贡献 | 抛错的假贡献注册进全新 registry | ✅ 通过。防御性跳过，不抛（命中 `settings.ts:133` 的兜底分支，见 O-3） |
| P4 | 桌面丢失场景核心复核 | Rust 侧：G7 两边重跑 + base 对照（见 §7） | ✅ 通过。`adapter_round_trips_value_presentation_through_shared_and_full_document` 在无 USER 下即通过（不走 IPC 身份路径，直接调 `SettingsService`），在 `USER=root` 下与 5 项契约测试一同通过；base 日志证明它在修复前失败（base 6 失败中的第 6 项）、修复后通过。另 3 项新增测试在两边均通过 |
| P5a | store revision 计数 | `applySettingsLocally`：同值新对象 → 不递增；改值 → +1 | ✅ 通过。JSON 相同不递增（对象 identity 变化不影响），改值 +1（T04 O-5 的 revision 不变量落地） |
| P6a | 启动注册幂等 | 全新 registry 上两次 `registerBundledValuePresentationContributions` | ✅ 通过。两次返回同一贡献对象；第二次不改变 registry.revision |
| P7a | hook：store 变化驱动快照重建 | renderHook + setState 驱动 store，revision 11 → 12 | ✅ 通过。快照随 store 变化更新，revision 透传；恢复默认后 enabled 清空 |
| P8a | 端到端 PRD 锚点（文本） | 存储值 → 快照 → 计划 → 呈示：`2026-01-15T14:30:00Z` + native iso-tz/Asia/Shanghai | ✅ 通过。plan status `ready`；`present` = `2026-01-15T22:30:00+08:00`（逐字） |
| P8b-i | number 列无覆盖 → default | unix-ms 全局配置 + number 列，无覆盖 | ✅ 通过。status `default`，`present` = `{kind:'unchanged', reason:'no-renderer'}`——数字不自动解释成 Unix（设计 §5 / T04 裁决；首轮探针曾误判为失败，已按 `timestampContribution.ts:267` 修正探针期望） |
| P8b-ii | Unix 锚点（custom 覆盖） | 同 P8b-i 配置 + 表列 custom 覆盖（source `c1/shop/orders`，epoch_ms） | ✅ 通过。status `ready`；`present(1705316400000)` = `2024-01-15T19:00:00+08:00`；`present(1705316400123)` = `2024-01-15T19:00:00.123+08:00`——BUG-003 不回归（`.123` 保留） |
| P9a | 交叉污染：未知 renderer 不污染；两份快照独立 | datazen.timestamp + other.renderer 同存；另建 unix 配置快照 | ✅ 通过。快照只含 datazen.timestamp；第二份快照不改变第一份的 configs |
| P9b | 仅未知 renderer 启用 → 空快照 | 只有 other.renderer | ✅ 通过。空快照（revision 透传） |

> 探针过程说明：19 项中首轮 1 项失败为**探针自身期望错误**（P8b 误把 number 列无覆盖当作应呈示；T04 已裁决默认路由仅时间语义列，number 列需 custom 覆盖），已修正探针为 P8b-i/P8b-ii 两项、代码未动；修正后 19/19 通过。另有一起探针脚手架事故：首轮探针配置的 include glob 穿透了 `node_modules` 符号链接，误跑全仓测试，已当场 kill 并修正为精确文件路径 + 排除 `node_modules` 后重跑；误跑期间无仓库文件被改动（探针只读）。

### 2.3 契约类型严格性审读（G9）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| T1 | 改动范围 | `git show 4b2891ba6/c2dfa7920/d431b0cd5/39d03a23d --stat` | ✅ 四提交共 19 + 9 + 1 + 2 文件，全部在 T06 范围内（见下表）。无无关改动 |
| T2 | any 泄漏 / 断言跳过验证 | grep 四提交新增行 `any`、新测试 `it.skip/test.skip/.todo(/xit(/xdescribe` | ✅ 新增行中 `any` 0 处（`settings.ts` 以 `unknown` 窄化）；新测试 0 处跳过 |
| T3 | 提交 1 单独 typecheck 不通过（已知观察项） | coder 申报 + G3 复核 | ⚠️ 见 §5 O-1：`4b2891ba6` 单独不通过 typecheck（4 个既有测试夹具补字段在 `c2dfa7920`），两笔合起来通过。中间提交不可独立构建，不影响最终状态，但按「每提交可验证」标准记为瑕疵 |
| T4 | 桌面修复 diff 最小性 | `git show d431b0cd5` 全 diff 审读 | ✅ 通过：仅 `crates/datazen-desktop/src/store/settings.rs` 一文件，+11/−8——import、`value_presentation` 正式字段（`#[serde(default)]`）、`Default`、 `shared_settings()` 改为 `self.value_presentation.clone()`、`apply_shared_settings()` 补映射；同时删掉了标注「F4 式提升是 follow-up」的旧注释 |
| T5 | 桌面丢失场景测试审读 | `git show 39d03a23d` 全 diff 审读 | ✅ 通过：4 项新测试——`value_presentation_round_trips_through_shared_settings`（共享路径 save→read 同值）、`old_settings_file_without_value_presentation_defaults`（旧 blob 缺字段得默认）、`value_presentation_promoted_out_of_extra_catch_all`（顶层键从 `extra` 提升进字段）、`adapter_round_trips_value_presentation_through_shared_and_full_document`（共享路径 + 全量文档路径，见 §7）。fixer 申报「同一测试修复前 FAILED、修复后 ok」，与 base 日志对照一致（见 §7） |
| T6 | 申报语义逐条核验 | 逐条对照 diff 与源码（见 §3） | ✅ 全部成立 |

> T1 明细：`4b2891ba6` 19 文件（+545/−7：Rust SharedSettings 字段 + backend-client 镜面 + settingsClient + settings.ts/hook/注册/startSharedApp + settingsStore + ContentView/PanelContentRenderer/ResultTableView 接线）；`c2dfa7920` 9 文件（+745，纯测试）；`d431b0cd5` 1 文件（+11/−8）；`39d03a23d` 2 文件（+148，纯测试）。

## 3. 设计符合性逐条结论

对照设计文档（§5 / §6 / §7 相关项）与 coder 申报：

**Coder 申报核验**

| 条目 | 结论 |
|------|------|
| 1. 存储形状 `{version: 1, renderers: {id: {enabled, options}}}`；rules 与覆盖不在 T06 | ✅ 符合（`settings.ts` 头注释与类型；P1/P2 验证形状；overrides 恒 `{}`，T10 再填） |
| 2. Rust `SharedSettings` 加 `#[serde(default)] value_presentation`，options 为 `serde_json::Value` 透传；backend-client 镜面类型 + `toSharedSettings` 透传；settingsClient 方法签名不变 | ✅ 符合（diff 审读；G1 的 backend-client/settingsClient 测试通过） |
| 3. 前端 `settings.ts` 宽容解析 + `buildValuePresentationSnapshot`，未知版本原文保留、快照为空 | ✅ 符合（源码审读 + P1/P2/P3；T04 O-1「快照组装处补 `validateConfig`」已落地——`buildValuePresentationSnapshot` 逐条调贡献的 `validateConfig`，无效跳过） |
| 4. settingsStore 加 `valuePresentation` 字段 + `valuePresentationRevision` 计数器（JSON 变化 +1）+ `updateValuePresentationSettings`；`flattenShared` 映射；AppSettings 字段 | ✅ 符合（diff 审读 + P5a；T04 O-5「revision 由 store 计数器保证」已落地） |
| 5. 启动注册内置时间贡献（幂等函数 + startSharedApp 在 `initProExtensions()` 后调用） | ✅ 符合（`registerBundledContributions.ts` 审读 + P6a；T05 遗留的生产缺口——此前无任何生产代码注册 timestampContribution——已补上） |
| 6. 新 hook `useValuePresentationSnapshot`；PanelContentRenderer/ContentView/ResultTableView 均已接线提供快照 | ✅ 符合（hook 源码审读 + P7a；G2 接线回归 1672/1672；T05 O-2「生产快照尚未接通」已解除——T05 建议的整页级回归仍缺，见 §5 O-2） |
| 7. 双端流转：旧 payload 兼容、未知版本与未知 renderer 原样保留、快照构建矩阵、store revision 计数 | ✅ 符合（P1/P2/P3/P5/P9；G1 的 `settings.test.ts` 264 行矩阵与探针独立复核一致） |
| 8. 桌面映射补齐（`d431b0cd5` + `39d03a23d`） | ✅ 符合（T4/T5 审读 + §7；设计 §6「补齐 backend-client、Rust 共享设置 DTO、默认值与宿主映射，不能只加桌面 extra」——此前桌面缺失的正是宿主映射一环，现已补齐） |

**§6 配置加载、持久化与同步（与 T06 相关项）**

| 条目 | 结论 |
|------|------|
| EP 激活只注册能力，不读取 Store、文件或接口；宿主负责加载配置，验证后以不可变快照传入 prepare | ✅ 符合（`registerBundledContributions.ts` 只注册；配置加载在 settingsStore/hook；快照不可变——P9a 验证两份快照独立） |
| 历史配置缺字段默认关闭 | ✅ 符合（`DEFAULT_VALUE_PRESENTATION_SETTINGS` 为空 renderers；P1a；桌面 `old_settings_file_without_value_presentation_defaults`） |
| 未知配置版本或暂时缺失贡献时保留原文，不用默认值写回覆盖 | ✅ 符合（P2a/P2b；未知 renderer 原样保留在 store，只是不进快照，P3d/P9b） |
| 配置写操作避免跨窗口覆盖整个旧快照；成功保存后发布修订通知 | ➖ 部分范围外：`updateValuePresentationSettings` 走既有 `updateSettings` 保存链路与 `datazen:settings-changed` 事件（T06 只复用既有机制）；路径化更新/版本条件更新的形式化是 T11 范围 |
| 初次加载中使用同一个默认快照；全部就绪后一次发布新快照；刷新失败继续使用最后一份有效快照；保存失败保持已生效快照 | ➖ 未单独立项验证：默认快照由 T05 模块级常量提供、hook 在 store 就绪后发布新快照；刷新/保存失败的保持语义沿用既有 settingsStore 行为，未在 T06 自带测试外单列用例。建议 T11 跨窗口同步时一并覆盖 |

**T04 观察项闭环**

| 条目 | 结论 |
|------|------|
| T04 O-1：全局配置体无效时宿主只认版本与存在性，建议 T06 快照组装处补 `validateConfig` | ✅ 已闭环（`buildValuePresentationSnapshot` 逐条 `validateConfig`，无效跳过；P3b/P3e/P3f） |
| T04 O-5：缓存键经 snapshot.revision 间接覆盖 overrides/enabled，依赖「内容变则 revision 变」不变量 | ✅ 已闭环（store 的 `nextValuePresentationRevision` 以 JSON 比较保证该不变量；P5a） |

## 4. Bug 列表

| 编号 | 标题 | 级别 | 状态 |
|------|------|------|------|
| — | 本次验证未发现新 bug | — | — |

本次验证未发现阻塞或非阻塞 bug。§7 的 5 项桌面 settings 测试失败是**环境问题**（沙箱无桌面登录用户），不是代码缺陷，见 §7 的根因分析与两边重跑对照。

## 5. 观察项（非缺陷，供 coder / 后续任务参考）

- **O-1（提交 1 单独 typecheck 不通过）**：`4b2891ba6` 单独不通过 typecheck，4 个既有测试夹具的字段补齐落在 `c2dfa7920` 才补上；两笔合起来通过（G3 复核）。最终状态无问题，但中间提交不可独立构建；后续任务保持 code/test 分拆时，把夹具字段补齐放进 code 提交。
- **O-2（整页级回归仍缺）**：T05 O-2 建议「T06 落地时补一条整页级回归」——ContentView 整页挂载验证快照从 store 透传到 grid/aside。T06 接了三处提供方（PanelContentRenderer/ContentView/ResultTableView），G2 的 1672 项覆盖了各组件，但没有一条经 ContentView 整页挂载验证快照透传。建议 T07 或整理时补。
- **O-3（`settings.ts:133` 的防御性兜底分支未被仓库测试覆盖）**：`buildValuePresentationSnapshot` 里 `validateConfig` 抛异常的 `try/catch → continue` 分支，仓库覆盖率显示该行未覆盖（G5 未覆盖行 133）；探针 P3f 以抛错假贡献验证了该分支行为正确。建议吸收 P3f 为仓库回归测试，或接受探针覆盖。
- **O-4（settingsStore 函数覆盖率恰踩线）**：`settingsStore.ts` 四项为 92.77 / 93.33 / **80** / 93.58，Funcs 恰为 80.0。下次改动该文件若减一条函数覆盖即跌破门禁。
- **O-5（hook 与启动/客户端文件不在覆盖率清单内）**：`useValuePresentationSnapshot.ts`、`startSharedApp.tsx`、`settingsClient.ts` 不在根 vitest coverage include 清单内，无单文件覆盖率数字；coder 申报的「hook 100」在本配置下无法复核（G5 注）。三文件行为由 G1 测试覆盖（hook 6 项全过）。建议把 `hooks/**` 纳入清单，或在报告口径中注明。
- **O-6（桌面 round-trip 夹具用的 options 不是有效前端选项）**：`39d03a23d` 的 Rust 夹具用 `{"interpretation": "instant", "format": "iso", "displayTimeZone": "UTC"}`——Rust 侧只做不透明存储，round-trip 契约本身成立；但 `"instant"` 不是前端 `TimestampInterpretation` 的合法 kind（合法为 native/unix/text），`"UTC"` 也不是 `TimeZoneSpec` 的合法形状。若后人把该夹具当作可投产的 payload 示例抄走，前端 `validateConfig` 会拒绝。建议把夹具改成前端合法的结构化 options，或加注释说明此处只测不透明透传。
- **O-7（探针资产）**：`/tmp/probe-t06` 为一次性验证项目，已按任务要求删除（`/tmp` 在任务中途被环境重启清理，删除结果等效；仓库 `git status` 复核干净）。本报告 §2.2 各项方法已描述到可复现程度。建议将 P3f（validateConfig 抛异常兜底）、P8b-i（number 列无覆盖不自动解释）与 P5a（revision 计数）吸收为仓库回归测试。

## 6. 结论

**通过，可做收尾提交。**

- 既有门禁：T06 定向 399/399、接线回归 1672/1672、双引擎类型通过、命名门禁通过、内核 settings 14/14、桌面 settings `USER=root` 下 44/0、格式干净；新文件覆盖率全部 ≥80%（settings.ts 96.55/100/100/96.15、注册 100、settingsStore 92.77/93.33/80/93.58；hook 等三文件不在清单内，见 O-5）。
- 对抗性探针 19/19 通过：旧 payload 兼容、未知版本保留原文、快照矩阵 6 项、store revision 计数、启动注册幂等、hook 重建、PRD 双锚点端到端（文本 `2026-01-15T22:30:00+08:00`、Unix `2024-01-15T19:00:00+08:00` 逐字一致）、BUG-003 不回归（`.123` 保留）、交叉污染隔离。
- 设计符合性：§6 要求的 backend-client、Rust DTO、默认值与宿主映射四环齐全；T04 O-1/O-5 闭环；T05 O-2 的生产快照接通解除（整页回归仍缺，见 O-2）。
- 桌面映射补齐（`d431b0cd5` + `39d03a23d`）审读通过，丢失场景测试覆盖共享与全量两条路径；§7 的 5 项失败已证实为环境问题。
- 观察项 7 条均为非缺陷（O-1 中间提交不可独立构建、O-2 整页回归仍缺、O-3 兜底分支仓库未覆盖、O-4 settingsStore 函数覆盖踩线、O-5 三文件不在覆盖清单、O-6 Rust 夹具 options 非前端合法形状、O-7 探针资产）。
- tasks.md 状态更新与 docs close commit 不在本次验证范围内，由协调方执行。

## 7. 桌面 settings 测试 5 项失败：环境问题专项（根因 + 两边重跑）

### 7.1 现象

`cargo test -p datazen_desktop --lib settings` 在本沙箱（root 身份，`USER` 与 `LOGNAME` 均为空，无登录会话）下，5 项既有测试失败：

- `commands::config::settings_use_case_contract_tests::a_storage_failure_surfaces_without_a_contract_code`
- `commands::config::settings_use_case_contract_tests::invalid_enum_values_are_refused_before_the_repository`
- `commands::config::settings_use_case_contract_tests::saving_settings_normalizes_and_round_trips`
- `commands::config::settings_use_case_contract_tests::the_ipc_settings_refusal_is_a_plain_string_without_a_contract_code`
- `commands::config::settings_use_case_contract_tests::the_ipc_storage_failure_carries_the_host_sentence_without_storage_details`

失败报错均为：`ContractFailure { code: None, message: "无法确定当前桌面登录用户，拒绝以占位主体继续" }`。

### 7.2 根因（源码核实）

`crates/datazen-desktop/src/platform/identity.rs`：

- `resolve_local_principal()`：优先 `DATAZEN_DESKTOP_USER`（非空），否则非 Windows 取 `USER`、Windows 取 `USERNAME`，trim 后非空才成主体；否则 `None`。
- `DesktopIdentity::from_desktop_user()`：`local_principal()` 为 `None` 时直接返回 `Unauthenticated` 错误「无法确定当前桌面登录用户，拒绝以占位主体继续」——失败关闭，绝不伪造占位主体。
- `local_principal()` 以 `OnceLock` 在进程内只解析一次。

本沙箱跑测试是 root 身份，`USER`/`LOGNAME` 为空、也无 `DATAZEN_DESKTOP_USER`，故 `local_principal()` 为 `None`；5 项契约测试经 settings IPC 命令组装身份时在该关上失败关闭，**连自己要测的场景都没跑到**（存储失败、枚举校验、保存 round-trip 等均未执行）。与 T06 代码无关——5 项测试均在 T06 改动范围之外（`settings_use_case_contract_tests`，既有测试）。

### 7.3 两边重跑对照（协调方实测，2026-11-12）

命令（均在工作区根目录，`CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0'`，原因见 7.5）：

- 无 USER：`env -u USER -u LOGNAME cargo test -p datazen_desktop --lib settings` → **39 通过、5 失败**（`test result: FAILED. 39 passed; 5 failed; 0 ignored; 0 measured; 1388 filtered out; finished in 0.51s`），退出码 101。5 项失败与 §7.1 所列完全一致，报错均为「无法确定当前桌面登录用户」；4 项新增 T06 测试全部通过（含丢失场景测试 `adapter_round_trips_value_presentation_through_shared_and_full_document ... ok`）。日志 `~/workspace/desktop-test-T06-no-user.log`。
- `USER=root`：`env USER=root LOGNAME=root cargo test -p datazen_desktop --lib settings` → **44 通过、0 失败**（`test result: ok. 44 passed; 0 failed; 0 ignored; 0 measured; 1388 filtered out; finished in 0.32s`），退出码 0；5 项契约测试全部 `ok`。日志 `~/workspace/desktop-test-T06-user-root.log`。

与用户核实的数字一致（44/0；之前 39/5）。

fixer 留下的对照证据（`~/workspace/desktop-test-base.log` 与 `~/workspace/desktop-test-final.log`，供复核参考，非本次实测）：

- base（修复前，无 USER）：35 通过、6 失败——5 项契约测试 + `platform::settings::tests::adapter_round_trips_value_presentation_through_shared_and_full_document`（T06 丢失场景测试，修复前 FAILED）。
- 修复后（无 USER）：39 通过、5 失败——仅 5 项契约测试失败；4 项新增 T06 测试全部通过（含丢失场景测试，修复前 FAILED、修复后 ok）。
- 用户核实：同一条命令只加 `USER=root` 重跑，44 通过、0 失败。

### 7.4 结论与后续要求

- 5 项失败是**环境问题，不是代码问题**：CI 以正常用户身份运行，`USER` 非空，不受影响；本地复现须带 `USER`。
- **以后跑桌面 settings 套件要带 `USER`**（如 `USER=root cargo test -p datazen_desktop --lib settings`；或 `DATAZEN_DESKTOP_USER=<任一非空用户名>`，二者等价——前者走 OS 用户名分支，后者走显式注入分支）。
- P4（桌面丢失场景核心复核）以 §7.3 的两边重跑为准：丢失场景测试在无 USER 下即通过（它不走 IPC 身份路径，直接调 `SettingsService`），在 `USER=root` 下与 5 项契约测试一同通过；且 base 日志证明它在修复前失败、修复后通过。

### 7.5 附记：首次无 USER 重跑被 OOM 杀死（环境问题，与 T06 无关）

- 首次无 USER 重跑（默认并行任务 + `debuginfo=2`）在编译 `datazen` 内核 lib 时被 SIGKILL（signal 9，OOM killer；会话 failureKind `oom_killed`），未能跑到测试；失败日志 `~/workspace/desktop-test-T06-no-user-oom.log`。另有一次运行时重启中断了同一次编译（依赖已随重启回滚重装，见 §1）。
- 按 AGENTS.md 的低内存方案重跑（`CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0'`）后一次成功：增量缓存下数秒即完成编译并跑完测试，得到 §7.3 的数字。G6/G7 均用同一组参数。
- 结论：OOM 是本机内存（6 GiB 上限）与默认并行编译的组合所致，不是 T06 代码或依赖问题；`cargo check -p datazen_desktop` 的通过性由本次测试编译成功覆盖（测试编译严格多于 check）。
