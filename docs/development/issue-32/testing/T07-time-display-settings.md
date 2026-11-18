# T07 测试报告：设置 UI——时间显示

- 被测对象：`packages/frontend/src/windows/settings/ValuePresentationSettingsSection.tsx`（新增，699 行：草稿 + 显式保存、格式/时区选择、经贡献 prepare/present 的实时预览、只读规则列表）+ `packages/frontend/src/windows/settings/TableDataSettingsSection.tsx`（挂载点，+5 行）+ `packages/frontend/src/locales/en/settings.ts` 与 `packages/frontend/src/locales/zh-CN/settings.ts`（各 +37 key，见 O-1）+ 自带测试 `packages/frontend/src/windows/settings/__tests__/ValuePresentationSettingsSection.test.tsx`（27 项）
- 分支：`feature/issue-32-value-presentation`（HEAD `5d26ca2e8`，作者 issue-32-agent；T07 两提交：code `86d2bb178` + test `5d26ca2e8`，基底 `8eb364707`；验证开始时工作区干净）
- 测试角色：独立 tester。未修改任何被测源码或测试；唯一可写文件为本报告。
- 依据：`docs/archive/development/issue-32-value-presentation-design.md` §4（设置 UI 贡献）/ §5（配置模型）/ §6（加载/持久化）/ §8.8（预览与实际展示同一解析实现）、`docs/archive/development/issue-32-value-presentation-prd.md` §3.1（设置 → 表格数据 → 时间显示）/ §3.3（生效反馈）/ §4 / §5 / §6 验收 1/2/3/8/10
- 日期：2026-11-12

## 1. 测试环境与命令

环境：Node v24.22.1，vitest 4.1.10（仓库自带），tsrs 0.18.0 / tsc 7.0.2，系统时区 UTC（全部探针均显式传入时区与语言，不依赖机器默认值；一处本地时区用例只断言格式不硬编码时区）。对抗性探针运行于独立临时项目 `/tmp/probe-t07/`（通过 `node_modules` 符号链接复用仓库依赖，`environment: jsdom`，被测源码以绝对路径导入，不污染仓库工作区；探针按任务要求用后删除，原始日志 `~/workspace/t07-probes.log`；另有 G1/G2/G3/G4/G5 日志 `~/workspace/t07-g{1,2,3a,3b,4,5}.log`）。

环境说明：本机 `PATH` 中无 `pnpm` 二进制（与 T02–T06 报告相同情况）；typecheck 改用 `npx -y pnpm@12.18.0 <script>` 调用，版本号实测 12.18.0，属等价调用，不影响门禁结论。T07 为纯前端改动，未碰桌面 settings 链路，故未跑桌面 settings 套件（任务亦注明 T07 一般不需要）。

| 命令 | 用途 |
|------|------|
| `npx vitest run packages/frontend/src/windows/settings/__tests__/ValuePresentationSettingsSection.test.tsx packages/frontend/src/windows/settings/__tests__/TableDataSettingsSection.test.tsx packages/frontend/src/windows/settings/__tests__/SettingsContent.test.tsx --retry=0` | G1：T07 定向（新测试 27 项 + 挂载点 4 项 + 设置页 34 项） |
| `npx vitest run packages/frontend/src/windows/settings packages/frontend/src/lib/valuePresentation packages/frontend/src/stores/__tests__/settingsStore.test.ts --retry=0` | G2：接线回归（设置页全量 + valuePresentation 全库 + settingsStore） |
| `npx -y pnpm@12.18.0 typecheck` / `npx -y pnpm@12.18.0 typecheck:tsc` | G3：双引擎类型门禁（tsrs / 权威 tsc） |
| `node scripts/check-test-module-naming.mjs` | G4：测试命名门禁 |
| `npx vitest run <G1 路径> --retry=0 --coverage --coverage.include='packages/frontend/src/windows/settings/ValuePresentationSettingsSection.tsx'` | G5：新文件覆盖率（v8 报告 + coverage-final.json） |
| `git show 86d2bb178 / 5d26ca2e8` 全 diff + grep 新增行 `any` / 跳过断言 + i18n key 对照脚本 | G6：改动范围审读 |
| `npx vitest run --config /tmp/probe-t07/vitest.config.ts` | 对抗性探针 P1–P9（22 项） |
| `node /tmp/probe-t07/i18n-check.mjs` | P10：en / zh-CN key 文件级逐一对照 |

## 2. 测试项清单

### 2.1 既有验证（仓库门禁）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| G1 | T07 定向单测 | 见 §1 表格 | ✅ 3 文件 **65/65 通过**（新文件 27/27、TableData 4/4、SettingsContent 34/34；退出码 0；与 coder 申报一致） |
| G2 | 接线回归 | 设置页全量 + valuePresentation 全库 + settingsStore | ✅ 37 文件 **538/538 通过**（退出码 0） |
| G3 | 类型门禁（双引擎） | `pnpm typecheck` / `pnpm typecheck:tsc`（经 `npx -y pnpm@12.18.0` 等价调用） | ✅ 两者均 OK（tsrs 0.18.0 与 tsc 7.0.2，退出码均为 0） |
| G4 | 测试命名门禁 | `node scripts/check-test-module-naming.mjs` | ✅ ok（1837 Rust 文件、256 cfg(test) 模块；2574 TS 文件、8 wdio spec 树；退出码 0） |
| G5 | 新文件覆盖率 | `npx vitest run <G1 路径> --retry=0 --coverage --coverage.include='…ValuePresentationSettingsSection.tsx'` | ✅ **91.61 / 91.36 / 93.75 / 92.61**（Stmts / Branch / Funcs / Lines），全部 ≥80%；测试 65/65 通过，退出码 0（见下注） |
| G6 | 改动范围与源码审读 | 两提交全 diff + grep + i18n 对照 | ✅ 通过（见 §2.3；但有两处申报数字出入，见 O-1/O-2） |

> G5 注：coder 申报为 91.01 / 90.64 / 93.75 / 92.66，本次实测为 91.61 / 91.36 / 93.75 / 92.61（Stmts/Branch/Lines 各差 ±0.7 以内，Funcs 一致），均 ≥80%。coder 称「全局阈值因只跑子集而报退出码 1」本次未复现：本次退出码为 0——`--coverage.include` 限定为单文件后，报告的 `All files` 行即该文件本身，四项均过阈值，故无全局阈值失败。未覆盖行（据 coverage-final.json）：112、237、238、294、343、397、398、400、403、407、408、410、413、422、423、424、453、460、466、479、611；其中 407–413（保存时贡献二次 `validateConfig` 失败）与 422–424（`updateValuePresentationSettings` 缺失）正是 coder 申报的两条未覆盖防御分支，实测吻合；其余见 §5 O-3。

### 2.2 对抗性探针（/tmp/probe-t07，独立于 coder 自带测试编写，22 项 + P10 文件级对照）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| P1a | 原生锚点独立重算 | 不经过贡献代码，只用标准库 `Intl.DateTimeFormat` + `formatToParts` 从头拼 `iso-tz` 文本，再与 `computeTimestampPreview` 的结果比对 | ✅ 通过。独立重算与贡献路径均为 `2026-01-15T22:30:00+08:00`，逐字一致 |
| P1b | Unix 毫秒锚点独立重算 | 同 P1a，对 `1705316400000` | ✅ 通过。独立重算与贡献路径均为 `2024-01-15T19:00:00+08:00`，逐字一致 |
| P1c | 渲染面板双锚点逐字 | 挂载面板，iso-tz + Asia/Shanghai 下切原生/Unix 毫秒预览来源 | ✅ 通过。`2026-01-15T22:30:00+08:00` / `2024-01-15T19:00:00+08:00` |
| P1d | 跨日（PRD §6 验收 8） | 贡献 `prepare/present` 直调：`2026-01-15T20:30:00Z` + Asia/Shanghai + iso-tz；另附独立 Intl 重算 | ✅ 通过。`2026-01-16T04:30:00+08:00`（日期进位），两路一致 |
| P2a | 非法 IANA `Mars/Olympus` 禁存且就地报错 | 挂载面板，时区选 IANA，输入 `Mars/Olympus` | ✅ 通过。行内 `ianaUnknown` 提示；点保存不调 store，就地 `ianaUnknown` 报错 |
| P2b | 空串 IANA 禁存 | 时区选 IANA，名字留空后点保存 | ✅ 通过。无行内提示（仅非空才提示），保存被拦截，就地 `ianaInvalid` 报错，store 未调 |
| P2c | 含内空格 `Not A Zone` 禁存 | 输入 `Not A Zone` | ✅ 通过。行内 `ianaInvalid` 提示；保存被拦截 |
| P2d | 带尾空格 `Asia/Shanghai ` | 输入 `Asia/Shanghai ` 后点保存 | ⚠️ 待裁决（见 §7）：实测**未禁存**——组件对输入做 `trim()` 后校验，保存为干净的 `Asia/Shanghai`，行内无报错。代码三处 `trim()` 是显式一致行为，非偶然遗漏；但与任务探针括号的字面要求（「带空格」属非法、应禁存）不一致，需裁决 |
| P2e | 合法 IANA 可存 | `Asia/Shanghai`、`America/New_York` 各保存一次 | ✅ 通过。均写回 `{ kind: 'iana', name }` |
| P3a | Unix 秒 vs 毫秒不混淆 | 贡献直调：同一数值 `1705316400` 在 unix 秒与 unix 毫秒解释下分别 `present` | ✅ 通过。秒 → `2024-01-15T19:00:00+08:00`；毫秒 → `1970-01-21T01:41:56.400+08:00`（或无 `.400` 后缀形式），两者不同 |
| P3b | Unix 预览后保存，全局 interpretation 仍为 native | 面板选 Unix 毫秒预览（锚点正确）→ 开启 → 保存，检查写回值 | ✅ 通过。保存的 `options.interpretation` 为 `{ kind: 'native' }` |
| P4a | 草稿隔离：改开关/格式/时区、只看预览，不点保存 → store 未变 | 挂载后改开关、格式、IANA、预览来源，全程不断言 store | ✅ 通过。`updateValuePresentationSettings` 全程 0 次调用，mock store 仍为 `{ version: 1, renderers: {} }`；点保存后恰好 1 次写回 |
| P4b | 保存失败：报错提示、旧配置生效、草稿保留 | mock `updateValuePresentationSettings` `mockRejectedValueOnce` 后开启 + 改格式 + 保存；再成功保存一次 | ✅ 通过。显示 `saveFailed`、无 `saved` 标记、mock store 仍为空；草稿保留（开关仍开），再次保存成功写回新值 |
| P5 | 贡献缺失：面板提示不可用、配置保留、仍可保存 | 空 registry 挂载，已存 `enabled: true` 的 iso-tz/Asia/Shanghai 配置 | ✅ 通过。`contributionMissing` 提示在场、开关显示已启用、预览为 `preview.unavailable`；关闭后保存成功（走 T02 纯验证，不依赖贡献在场） |
| P6a | 未知版本 / 多 renderer 保留 | 已存 `version: 99` + `other.renderer` 条目，开启后保存 | ✅ 通过。写回 `version: 99`，`other.renderer` 深相等原样保留，只替换 `datazen.timestamp` 一项 |
| P6b | 非法 stored options 草稿回落默认、开关保留 | `draftFromStored` 直调：`enabled: true` + `{ format: 'bogus' }` | ✅ 通过。`enabled: true`，options 回落默认 `{ native, iso, local }` |
| P7a | 规则列表：无 rules 空态 | 无 rules 字段存储值挂载，展开列表 | ✅ 通过。计数 `(0)`，展开显示空态 |
| P7b | 有 rules（直接放原始设置值）时只读列出名称 | 原始设置值含 `rules: [{name}, {name}]`，展开 | ✅ 通过。计数 `(2)`，两名称均列出 |
| P7c | rules 非对象元素退回 `#n` 占位不崩 | `rules: ['a-string', 42, {name: 'named'}]` | ✅ 通过。列表含 `#1`、`#2`、`named`；命中覆盖率未覆盖行 112 的分支 |
| P7d | rules 非数组时退回空态不崩 | `rules` 分别为 `null` / 字符串 / 对象；另直调 `readStoredRules(null / 'junk' / [])` 与 `name` 非字符串 | ✅ 通过。均为空态；直调均返回 `[]`（`name: 7` → `{ name: undefined }`） |
| P8 | 文本预览不污染保存 | 面板选文本预览来源（墙上文本按 UTC 解释，预览 `2026-01-15T22:30:00+08:00`）→ 开启 → 保存 | ✅ 通过。保存的 `interpretation` 为 `{ kind: 'native' }`，无 `sourceTimeZone` 字段，写回 JSON 不含 `"text"` |
| P9 | 未启用不写 store：挂载与预览全程不调 update | 挂载 → 切两个预览来源 → 改格式 → 设 IANA → 开开关，全程不保存 | ✅ 通过。`updateValuePresentationSettings` 全程 0 次调用 |
| P10 | i18n：en 与 zh-CN 各 37 key 逐一对照 | 文件级脚本：提取两文件 `settings.valuePresentation*` key 集、组件实际 `t()` 引用的 key 集、逐 key 取值对照 | ✅ 通过。两文件均为 37 key（非申报的 40，见 O-1），key 集完全一致；组件引用的 37 key 在两文件均存在，无缺 key；无 locale key 未被组件使用；zh 值均非空；唯一与 en 相同的 zh 值为 `timeZone.utc` 的 `UTC`（语言中性，可接受） |

> 探针过程说明：首轮 1 项失败为**探针脚手架缺陷**（P1 未清理挂载，导致 P2 首项 `getByText` 在两个面板间命中重复元素），与被测代码无关；补 `afterEach(cleanup)` 后 22/22 通过，代码未动。另有一起脚手架事故：首轮探针配置的 `setupFiles` 绝对路径被 Vite 的 `server.fs.allow` 挡下（`Cannot find module '/@fs/…'`），加 allowlist 后解决；误跑期间无仓库文件被改动。

### 2.3 契约类型严格性审读（G6）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| T1 | 改动范围 | `git diff 8eb364707..HEAD --stat` | ✅ 两提交共 5 文件、+1362/−0，全部在 T07 范围内：新组件 699 行、新测试 572 行、TableData 挂载 +5 行、两 locale 文件各 +37 key（行数 +45/+41，含多行字符串）。无无关改动、无删除 |
| T2 | any 泄漏 / 断言跳过验证 | grep 两提交新增行 `any`、新测试 `it.skip/test.skip/.todo(/xit(/xdescribe` | ✅ 新增行中 `any` 0 处；新测试 0 处跳过（27 项 `it(` 全数，见 G1） |
| T3 | i18n key 数量与内容一致 | `git show` 计数 + P10 脚本 | ⚠️ 见 §5 O-1：实测各 37 key，非任务/coder 申报的 40；但两文件 key 集完全一致且与组件引用一一对应，无缺失 |
| T4 | TableDataSettingsSection 挂载位置 | diff 审读 + `SettingsContent.tsx:456` | ✅ 通过。`<ValuePresentationSettingsSection />` 挂在 `TableDataSettingsSection` 片段末尾（`gridAutoHideScrollbar` 行之后），经 `SettingsContent.tsx:456` 的 `<TableDataSettingsSection />` 进入设置页，符合 PRD §3.1「设置 → 表格数据 → 时间显示」 |
| T5 | 申报语义逐条核验 | 逐条对照 diff 与源码（见 §3） | ✅ 全部成立（除 P2d 带空格 IANA 一项待裁决，见 §7） |

## 3. 设计符合性逐条结论

对照设计文档、PRD 与 coder 申报：

**Coder 申报核验**

| 条目 | 结论 |
|------|------|
| 1. 启用开关默认关、只改草稿保存才写 store | ✅ 符合（组件 `useState(draftFromStored)` + `handleSave` 才调 store；P4/P9 独立复核；G1 首项锁定未启用不写 store） |
| 2. 格式 iso / iso-tz / locale | ✅ 符合（`OUTPUT_FORMATS` 三档；G1 格式切换用例 + P1c 逐字锚点） |
| 3. 时区 本地/UTC/IANA（`isValidIanaTimeZoneName` + `isValidTimeZoneName` 双重校验，非法行内提示禁存） | ✅ 符合，附 P2d 待裁决：格式校验（无空白/字符集）与存在性校验（Intl 构造）双层均在；`Mars/Olympus` 走 `ianaUnknown`、`Not A Zone`/空串走 `ianaInvalid`，均禁存；但带首尾空格的输入先被 `trim()`，见 §7 |
| 4. 预览经贡献 prepare/present 与运行时同一条路，双锚点逐字 | ✅ 符合（`computeTimestampPreview` 直接调贡献 `validateConfig → prepare → present`，无另写解析；P1a/P1b 以独立 Intl 重算交叉验证逐字一致；设计 §8.8「预览和实际展示调用同一解析实现」成立） |
| 5. Unix 来源显示「Unix 秒/毫秒」标记 | ✅ 符合（`value-presentation-preview-unix-badge` 按预览来源切换；P3b/G1 锚点用例） |
| 6. 全局默认 interpretation 固定 native（Unix/文本只在预览里看效果、不保存；文本预览墙上文本按 UTC 解释、仅预览） | ✅ 符合（`DEFAULT_TIMESTAMP_OPTIONS` 与 `buildPreviewOptions`：Unix/文本解释只在预览选项里构造；`handleSave` 只保存 `optionsToSave`（其 interpretation 恒为草稿的 native）；P3b/P8 独立复核写回值） |
| 7. 可折叠规则列表只展示、无规则空态（v1 存储无 rules 字段，`readStoredRules` 直接读原始设置值，因 T06 宽容解析会丢未来 rules 字段） | ✅ 符合（`aria-expanded` 折叠按钮 + 计数；无 rules → 空态；`readStoredRules(settings.valuePresentation)` 读原始值而非解析后 `stored`；P7a–P7d；T08 再扩展编辑） |
| 8. 保存经 `updateValuePresentationSettings`、非法禁存就地报错、保存失败明确提示且旧配置生效草稿保留、贡献缺失保留配置并提示不可用但仍可保存 | ✅ 符合（`validateDraftForSave` 三层拦截 + `handleSave` 的 try/catch；P4b/P5 独立复核） |
| 9. 未知顶层 version 与其余 renderer 条目原样保留、只替换 `datazen.timestamp` | ✅ 符合（`handleSave` 构造 `next` 时 `version: stored.version` + 展开 `stored.renderers`；P6a 以 version 99 + `other.renderer` 验证） |
| 10. 非法 stored options 草稿回落默认、开关保留 | ✅ 符合（`draftFromStored`：`validateTimestampOptions` 失败 → 默认 options，`enabled` 原样保留；P6b/G1 用例） |
| 11. 未覆盖的两条防御分支（保存时贡献二次 `validateConfig` 失败、`updateValuePresentationSettings` 缺失） | ✅ 属实（G5 未覆盖行 407–413 与 422–424 正是这两条分支；其余未覆盖行见 O-3） |

**PRD §6 验收（与 T07 相关项）**

| 条目 | 结论 |
|------|------|
| 验收 1：新安装或未启用时，现有显示与编辑行为保持不变 | ✅ 符合（面板挂载/预览/草稿编辑全程不写 store，P9；开关默认关；网格与详情的展示只读 store 快照，T07 未改动展示路径） |
| 验收 2：两个入口修改的是同一配置；cell、aside、预览显示结果一致 | ➖ 部分范围外：T07 只交付设置入口（§3.1），列配置面板与两处入口是 T09；设置入口的预览与运行时走同一贡献路径（P1），但 cell/aside 一致性是 T04/T05 的范围，本次未新增整页验证（见 O-5） |
| 验收 3：BIGINT 无显式配置时保持数字；配置 Unix 秒/毫秒后按指定单位展示 | ✅ 既有行为未改动（全局默认 interpretation 恒为 native，数字不自动解释；P3a 验证秒/毫秒显式单位互不混淆；列级 Unix 配置是 T09/T10 的范围） |
| 验收 8：修改时区、源时区与单位能正确跨日；无时区值不发生隐式漂移 | ✅ 符合（P1d 跨日用例 `2026-01-15T20:30:00Z` → `2026-01-16T04:30:00+08:00`；文本预览按 UTC 解释，P8；DST 切换日的预览未在 T07 覆盖，见 O-7） |
| 验收 10：配置保存失败、扩展缺失、格式化异常均可恢复，不阻断网格交互 | ✅ 符合（P4b 保存失败、P5 贡献缺失；格式化异常经贡献返回 `unchanged`/`invalid` 而非抛错，G1 的 `unchanged` 用例 + P1 覆盖） |

**设计 §4/§5/§6（与 T07 相关项）**

| 条目 | 结论 |
|------|------|
| §4：设置 UI 贡献返回 React 内容，值输出保持数据化，由宿主负责布局；首版直接接入展示贡献设置入口 | ✅ 符合（面板为纯 React 内容，保存值仍为 JSON 配置；挂载点由宿主 `TableDataSettingsSection` 决定，扩展自身不选位置） |
| §5：配置按 rendererId 命名空间保存；`native` 不把数字自动解释成 Unix；Unix 必须显式指定单位；无时区文本可用 sourceTimeZone | ✅ 符合（保存形状 `{ version, renderers: { 'datazen.timestamp': { enabled, options } } }`；Unix 单位只出现在预览选项；文本预览的 `sourceTimeZone` 缺省 UTC 且不保存） |
| §6：保存失败保持已生效快照，编辑面板保留草稿与错误；未知配置版本或暂时缺失贡献时保留原文，不用默认值写回覆盖 | ✅ 符合（P4b/P5/P6a；`handleSave` 失败分支只设 error，不触动 store；跨窗口同步的路径化更新是 T11 范围，T07 复用既有 `updateValuePresentationSettings` 保存链路） |

## 4. Bug 列表

| 编号 | 标题 | 级别 | 状态 |
|------|------|------|------|
| — | 本次验证未发现已确认的 bug | — | — |

本次验证未发现阻塞或非阻塞 bug。唯一待裁决项（非已确认缺陷，见 §7）：

| 编号 | 标题 | 级别 | 状态 |
|------|------|------|------|
| T07-Q1 | 带首尾空格的 IANA 名被 `trim()` 后保存，未按「非法禁存」拦截 | 待裁决 | 需协调方/用户裁决：接受 trim 行为则关闭；要求严格拒绝则转为 bug |

- 位置：`packages/frontend/src/windows/settings/ValuePresentationSettingsSection.tsx:349–356`（`optionsToSave` 的 `ianaName.trim()`）、`:380–392`（`validateDraftForSave` 用 `ianaNameTrimmed` 校验）、`:470`（`handleTimeZoneKindChange` 的 `ianaName.trim()`）。
- 复现输入：时区选 IANA，输入 `Asia/Shanghai `（尾空格）→ 行内无报错 → 点保存 → 成功写回 `{ kind: 'iana', name: 'Asia/Shanghai' }`（探针 P2d，`~/workspace/t07-probes.log`）。
- 说明：`isValidIanaTimeZoneName('Asia/Shanghai ')` 本身为 false（正则禁空白），但组件在校验与保存前统一 `trim()`，三处一致，是显式行为而非偶然遗漏；保存的值是干净名，无脏数据入库。若裁决为严格拒绝，改动很小（校验未 trim 的原输入即可）。

## 5. 观察项（非缺陷，供 coder / 后续任务参考）

- **O-1（i18n 申报 40 key vs 实测 37 key）**：任务与 coder 申报均为「各 +40 key」，实测 en 与 zh-CN 各 37 key（`git show` 计数与 P10 脚本一致）。两文件 key 集完全一致、组件引用的 37 key 无一缺失、无多余 key，属申报数字不准，非代码问题；后续申报以实测为准。
- **O-2（覆盖率实测与申报略有出入）**：coder 申报 91.01 / 90.64 / 93.75 / 92.66，本次实测 91.61 / 91.36 / 93.75 / 92.61，均 ≥80%；coder 称「全局阈值致退出码 1」本次未复现（本次退出码 0，见 §2.1 G5 注）。数字差异在测量噪声范围内，无结论影响。
- **O-3（未覆盖行清单）**：除 coder 申报的两条防御分支（407–413、422–424）外，未覆盖行还有：112（rules 数组的非对象元素 → `{ name: undefined }`，探针 P7c 已覆盖行为）、237–238（`formatPreviewSample` 的非 string/number 回退 `''`；预览样本恒为 string/number，该分支实际不可达）、294（`useSyncExternalStore` 的服务端快照，jsdom 下不可达）、343（`optionsToSave` 含 `sourceTimeZone` 的分支；草稿 interpretation 恒为 native 且不带 sourceTimeZone，该分支实际不可达）、397–403（`validateTimestampOptions` 在 `validateDraftForSave` 内失败；经 UI 能构造的非法配置已被前面的 IANA 检查拦截，该分支实际不可达）、453/460/466/479（`handleReset` 的 IANA 三元与三处 `onChange` 的非法值守卫；Select 只发出合法值，守卫实际不可达）、611（`unchanged` 预览带 reason 的 `: ${reason}` 后缀渲染；纯函数用例覆盖了该结果，渲染分支未覆盖）。建议接受探针覆盖或在后续任务中视情况补渲染用例，不阻塞 T07。
- **O-4（挂载点测试未断言新面板）**：`TableDataSettingsSection.test.tsx` 的 4 项均未断言 `ValuePresentationSettingsSection` 已挂载；挂载由新文件 27 项（自身渲染）与 `SettingsContent` 34 项（整页）间接覆盖。建议后续补一条「数据浏览设置页含时间显示面板」的挂载断言。
- **O-5（T06 O-2 整页级回归仍缺）**：T05/T06 建议的 ContentView 整页挂载验证（快照从 store 透传到 grid/aside）T07 仍未补；T07 只改设置页，不涉及展示管线，建议 T12 全量回归时补。
- **O-6（code 提交单独立即可构建性未单独验证）**：结构上 code 提交 `86d2bb178` 不依赖 test 提交内容（测试文件不被生产代码引用），但未在独立 worktree 中单独跑 typecheck（需另装依赖，成本高）；G3 在 HEAD 通过。记为未验证项而非缺陷；后续任务保持 code/test 分拆时留意夹具类依赖（T06 O-1 的教训）。
- **O-7（预览未覆盖 DST 切换日）**：锚点只用固定偏移时区（Asia/Shanghai、UTC）；`America/New_York` 只验证了可保存，未验证夏令时切换日的预览正确性。设计 §8 的 DST 歧义策略不在 T07 范围，建议 T12 全量回归时补一条 DST 用例。
- **O-8（探针资产）**：`/tmp/probe-t07` 为一次性验证项目，已按任务要求删除（探针文件与 vitest 配置随之清除；仓库 `git status` 复核除本报告外干净）。本报告 §2.2 各项方法已描述到可复现程度，日志见 `~/workspace/t07-probes.log` 与 `~/workspace/t07-g{1,2,3a,3b,4,5}.log`。建议将 P7c（rules 非对象元素 `#n` 占位）与 P1d（跨日）吸收为仓库回归测试。
- **O-9（无 settings locale key 完整性测试）**：仓库无针对 settings locale 包的 key 完整性专项测试（G2 的设置页全量未含此类用例）；P10 以文件级脚本对照补足（37/37 一致）。建议后续补一条 en/zh key 集一致性测试，防止日后增 key 时漏译。

## 6. 结论

**通过，附一项待裁决（T07-Q1，见 §4/§7）。**

- 既有门禁：T07 定向 65/65（27+4+34）、接线回归 538/538、双引擎类型通过、命名门禁通过；新文件覆盖率 91.61 / 91.36 / 93.75 / 92.61，全部 ≥80%。
- 对抗性探针 22 项 + P10 文件级对照全部通过（P1 双锚点经独立 Intl 重算逐字一致、P1d 跨日进位、P2 非法 IANA 禁存、P3 秒/毫秒不混淆且保存仍为 native、P4 草稿隔离与保存失败恢复、P5 贡献缺失、P6 未知版本/多 renderer 保留、P7 规则列表、P8 文本预览不污染保存、P9 未启用不写 store、P10 两语言 37/37 key 一致）。
- 设计符合性：设计 §4/§5/§6 与 PRD §3.1/§3.3、验收 1/3/8/10 均符合；验收 2 的列入口部分属 T09 范围。
- 未发现已确认的 bug。唯一待裁决项 T07-Q1（带首尾空格的 IANA 名被 trim 后保存）：若裁决接受 trim 行为，T07 可直接关闭；若裁决要求严格拒绝，则转为 bug 修复后再关闭。请协调方裁决。
- 观察项 9 条均为非缺陷（O-1 申报 key 数不准、O-2 覆盖率数字微差、O-3 未覆盖行清单、O-4 挂载点测试未断言、O-5 整页回归仍缺、O-6 code 提交未单独验证、O-7 DST 预览未覆盖、O-8 探针资产、O-9 无 locale key 完整性测试）。
- tasks.md 状态更新与 docs close commit 不在本次验证范围内，由协调方执行。

## 7. 专项：P2d 带空格 IANA 名——trim 后保存 vs「非法禁存」

### 7.1 现象

探针 P2d：时区选 IANA，输入 `Asia/Shanghai `（尾部一个空格），行内无报错，点保存后 `updateValuePresentationSettings` 被调用，写回 `{ kind: 'iana', name: 'Asia/Shanghai' }`（空格已被去掉）。任务探针要求括号的字面表述为「`'Asia/Shanghai '` 带空格」属非法 IANA、应禁存且就地报错；实测行为与该字面要求不一致。

### 7.2 代码核实（非偶然遗漏，是显式一致行为）

`ValuePresentationSettingsSection.tsx` 中三处统一 `trim()`：

- `:349–356` `optionsToSave`：IANA 名取 `ianaName.trim()`；
- `:380–392` `validateDraftForSave`：以 `ianaNameTrimmed` 做 `isValidIanaTimeZoneName` / `isValidTimeZoneName` 校验与报错；
- `:470` `handleTimeZoneKindChange`：切到 IANA 时以 `ianaName.trim()` 初始化草稿时区名。

行内提示同样用 trim 后的值判定（`:359–366` 的 `ianaFormatInvalid` / `ianaUnknown` 均基于 `ianaNameTrimmed`）。因此带空格输入在组件看来就是合法名 `Asia/Shanghai`，全程无报错、无脏数据入库（保存的值不含空格）。

### 7.3 两种裁决及后果

- **接受 trim（建议）**：前后空格是常见输入失误，trim 后接受是标准 UX；`isValidIanaTimeZoneName` 的「禁空白」针对的是名内空白（如 `Not A Zone`，P2c 已验证禁存），与首尾 trim 不冲突。此时 T07-Q1 关闭，T07 可直接做收尾提交。
- **要求严格拒绝**：则 T07-Q1 转为 bug（级别建议低）：校验与行内提示改用未 trim 的原输入，`'Asia/Shanghai '` 应报 `ianaInvalid` 并禁存；修复后补一条仓库回归测试。改动面仅本文件校验处。

### 7.4 与相邻行为的一致性

- 空串（P2b）：trim 后为空 → `ianaInvalid` 禁存，与严格/宽容两种裁决均一致。
- 名内空格 `Not A Zone`（P2c）：trim 不影响 → `ianaInvalid` 禁存，与两种裁决均一致。
- 仅首尾空格有分歧；除该输入外，P2 全部通过。
