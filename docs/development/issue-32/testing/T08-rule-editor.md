# T08 测试报告：规则编辑 UI（可编辑值呈现规则）

- 被测对象：`packages/frontend/src/lib/valuePresentation/valuePresentationRules.ts`（新增，767 行：规则模型、受限正则手写解析器、AND 匹配、首条命中冻结）+ `packages/frontend/src/windows/settings/ValuePresentationRuleEditor.tsx`（新增，750 行：单条规则编辑表单 + 样本命中/呈示预览）+ `packages/frontend/src/lib/valuePresentation/settings.ts`（rules 存储 + 快照门控）+ `packages/frontend/src/lib/valuePresentation/presentationPlan.ts`（规则进计划 + `matchedRuleName`）+ `packages/frontend/src/lib/valuePresentation/timestampContribution.ts`（导出 `timestampOptionsToConfigValue`）+ `packages/frontend/src/lib/valuePresentation/index.ts`（规则导出）+ `packages/frontend/src/windows/settings/ValuePresentationSettingsSection.tsx`（可编辑规则列表，删 T07 `readStoredRules` workaround）+ `packages/frontend/src/locales/en/settings.ts` 与 `packages/frontend/src/locales/zh-CN/settings.ts`（rules.* key，见 O-1）+ `packages/backend-client/src/settings.ts`（规则类型镜像）+ Rust `crates/datazen/src/settings/domain.rs`（不透明 `rules` 字段）+ `crates/datazen-desktop/src/platform/settings.rs` 与 `crates/datazen-desktop/src/store/tests/settings_shared_tests.rs`（夹具补齐，各 +1 行）+ 自带测试 `valuePresentationRules.test.ts`（70 项）、`settings.test.ts`（23 项）、`presentationPlan.test.ts`（83 项）、`ValuePresentationSettingsSection.test.tsx`（41 项）
- 分支：`feature/issue-32-value-presentation`（HEAD `99abd37f5`，作者 issue-32-agent；T08 两提交：code `a1951af8b` + test `99abd37f5`，基底 `237f82db2`；验证开始时工作区干净）
- 测试角色：独立 tester。未修改任何被测源码或测试；唯一可写文件为本报告。
- 依据：`docs/archive/development/issue-32-value-presentation-design.md` §5（配置模型与规则：顺序唯一优先级、无 priority 数值、pattern 存 source+受限 flags 不持久化 RegExp、缺失来源不满足表级条件、正则安全、规则数与输入长度设限）/ §6（加载/持久化）/ §7（共享展示管线）、`docs/archive/development/issue-32-value-presentation-prd.md` §3.1（规则段）/ §4（覆盖 > 规则 > 默认；命中后冻结不回落）/ §5 / §6 验收 4/5/8
- 日期：2026-11-12

## 1. 测试环境与命令

环境：Node v24.22.1，vitest 4.1.10（仓库自带），tsrs 0.18.0-ts7.1.0-dev / tsc 7.0.2，cargo 1.99.1（`~/.cargo/bin`），系统时区 UTC（全部探针均显式传入时区与语言，不依赖机器默认值）。对抗性探针运行于独立临时项目 `~/workspace/t08-probe/`（通过 `node_modules` 符号链接复用仓库依赖，`environment: jsdom`，被测源码以绝对路径导入，不污染仓库工作区；探针按任务要求用后删除，原始日志 `~/workspace/t08-p-logic.log` 与 `~/workspace/t08-p-panel.log`；另有 G1/G2/G3/G4/G5/G7 日志 `~/workspace/t08-g{1,1b,2,3a,3b,4,5,5b,7,7-kernel,7-kernel-lib,7-kernel-norun,7-test}.log`）。

环境说明：本机 `PATH` 中无 `pnpm` 二进制（与 T02–T07 报告相同情况）；typecheck 改用 `npx -y pnpm@12.18.0 <script>` 调用，版本号实测 12.18.0，属等价调用，不影响门禁结论。

| 命令 | 用途 |
|------|------|
| `npx vitest run packages/frontend/src/lib/valuePresentation/__tests__/valuePresentationRules.test.ts packages/frontend/src/lib/valuePresentation/__tests__/settings.test.ts packages/frontend/src/lib/valuePresentation/__tests__/presentationPlan.test.ts packages/frontend/src/windows/settings/__tests__/ValuePresentationSettingsSection.test.tsx --retry=0` | G1：T08 定向（新测试 70 + 23 + 83 + 41 项） |
| `npx vitest run packages/frontend/src/components/DataTable/__tests__/DataTable.presentation.test.tsx packages/frontend/src/components/DataTable/__tests__/DetailPanel.displayState.test.tsx packages/frontend/src/components/DataTable/__tests__/DetailPanel.presentation.test.tsx packages/frontend/src/hooks/__tests__/useValuePresentation.test.tsx packages/frontend/src/windows/connection/__tests__/ContentViewDrawers.detailDisplay.test.tsx --retry=0` | G1b：code 提交内夹具文件补跑（G1/G2 未覆盖的 5 个文件） |
| `npx vitest run packages/frontend/src/windows/settings packages/frontend/src/lib/valuePresentation packages/frontend/src/stores/__tests__/settingsStore.test.ts --retry=0` | G2：接线回归（设置页全量 + valuePresentation 全库 + settingsStore） |
| `npx -y pnpm@12.18.0 typecheck` / `npx -y pnpm@12.18.0 typecheck:tsc` | G3：双引擎类型门禁（tsrs / 权威 tsc） |
| `node scripts/check-test-module-naming.mjs` | G4：测试命名门禁 |
| `npx vitest run <G1 路径> --retry=0 --coverage --coverage.include='…valuePresentationRules.ts' --coverage.include='…ValuePresentationRuleEditor.tsx'`（另加一遍 5 文件全量 include） | G5：新文件覆盖率（v8 报告 + coverage-final.json） |
| `git show a1951af8b / 99abd37f5` 全 diff + grep 新增行 `any` / 跳过断言 + i18n key 对照脚本 + Prettier | G6：改动范围审读 |
| `CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0' cargo check -p datazen_desktop --lib --tests`；`cargo check -p datazen --lib --features settings`；`cargo test --no-run -p datazen --lib --features settings`；`cargo test -p datazen_desktop --lib settings_shared`（链接复核，`USER=root`） | G7：Rust（见 §2.1 与 §7） |
| `npx vitest run --config ~/workspace/t08-probe/vitest.config.ts probes/logic.test.ts probes/panel.test.tsx` | 对抗性探针 P1–P11（72 项） |
| `node ~/workspace/t08-i18n.mjs` / `node ~/workspace/t08-i18n-values.mjs` | P11：en / zh-CN rules.* key 文件级逐一对照 |

## 2. 测试项清单

### 2.1 既有验证（仓库门禁）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| G1 | T08 定向单测 | 见 §1 表格 | ✅ 4 文件 **217/217 通过**（rules 70、settings 23、presentationPlan 83、面板 41；退出码 0；与 coder 申报一致） |
| G1b | 夹具文件补跑 | code 提交内 5 个夹具测试文件（G1/G2 未覆盖者） | ✅ 5 文件 **34/34 通过**（DataTable.presentation 7、DetailPanel.presentation 9、DetailPanel.displayState 10、useValuePresentation 6、ContentViewDrawers.detailDisplay 2；退出码 0） |
| G2 | 接线回归 | 设置页全量 + valuePresentation 全库 + settingsStore | ✅ 38 文件 **637/637 通过**（退出码 0） |
| G3 | 类型门禁（双引擎） | `pnpm typecheck` / `pnpm typecheck:tsc`（经 `npx -y pnpm@12.18.0` 等价调用） | ✅ 两者均 OK（tsrs 0.18.0-ts7.1.0-dev 与 tsc 7.0.2，退出码均为 0） |
| G4 | 测试命名门禁 | `node scripts/check-test-module-naming.mjs` | ✅ ok（1837 Rust 文件、256 cfg(test) 模块；2577 TS 文件、8 wdio spec 树；退出码 0） |
| G5 | 新文件覆盖率 | 见 §1 表格（两遍：任务指定 2 文件 include + 5 文件全量 include） | ✅ 全部 ≥80%（见下表明细；测试 217/217 通过，退出码 0） |
| G6 | 改动范围与源码审读 | 两提交全 diff + grep + i18n 对照 + Prettier | ✅ 通过（见 §2.3；但有两处申报数字出入，见 O-1/O-7） |
| G7 | Rust | cargo check 双端 + 内核 lib 测试编译复核 + 桌面链接复核 | ❌ **不通过**：见 §4 BUG-001——`cargo test -p datazen --lib --features settings` 编译失败（2× E0063）；其余分项见下 |

> G5 明细（v8 报告；`% Stmts / % Branch / % Funcs / % Lines`，5 文件全量 include 一遍）：
>
> | 文件 | % Stmts | % Branch | % Funcs | % Lines | 未覆盖行（语句行） |
> |------|---------|----------|---------|---------|-------------------|
> | `lib/valuePresentation/valuePresentationRules.ts`（新） | 92.3 | 89.65 | 100 | 92.9 | 113, 114, 122, 123, 126, 130, 202, 231, 251, 274, 336, 347, 349, 360, 389, 390, 392, 469, 470, 479, 484, 494, 509, 513, 694 |
> | `windows/settings/ValuePresentationRuleEditor.tsx`（新） | 100 | 86.4 | 100 | 100 | 语句全覆盖；分支未覆盖行：122–126、129、131、134、137、140、142、146、148、151、153、188、658 |
> | `lib/valuePresentation/settings.ts` | 96.96 | 100 | 100 | 96.66 | 160 |
> | `lib/valuePresentation/presentationPlan.ts` | 96.42 | 96.62 | 100 | 96.38 | 168, 231, 374 |
> | `windows/settings/ValuePresentationSettingsSection.tsx` | 90 | 88.82 | 90.76 | 91.66 | 218, 274, 383, 385, 388, 393, 395, 398, 412, 414, 417, 421, 422, 476, 480, 489, 495, 508, 724, 770 |
>
> 与 coder 申报逐项一致（rules 92.3/92.9、编辑器 100/100、settings 96.96/96.66、presentationPlan 96.42/96.38、面板 90/91.66）。任务指定 2 文件 include 的一遍同样退出码 0（All files 94.21 / 88.67 / 100 / 94.6），未出现 T06 报告的全局阈值退出码问题。

> G7 分项：
>
> | 分项 | 命令 | 结果 |
> |------|------|------|
> | 桌面 check | `cargo check -p datazen_desktop --lib --tests`（低内存参数） | ✅ 退出码 0（51 warnings，均为既有警告，如 `store/settings.rs:6` 未用导入，与 T08 无关；T08 的 Rust 改动无新增警告） |
> | 内核 lib check | `cargo check -p datazen --lib --features settings` | ✅ 退出码 0（生产代码编译通过） |
> | 内核 lib 测试编译 | `cargo test --no-run -p datazen --lib --features settings` | ❌ 退出码 101：2× `error[E0063]: missing field 'rules'`（见 BUG-001） |
> | 桌面 test 链接 | `USER=root cargo test -p datazen_desktop --lib settings_shared` | ⚠️ 未能链接：`rust-lld: error: unable to find library -lwebkit2gtk-4.1 / -lgtk-3 / -lgtk-3 / -lgdk-3 / -lgdk_pixbuf-2.0`——coder 申报属实（沙箱缺系统库），桌面测试（含新增的 rules round-trip 测试）留给 CI；不是代码问题 |

### 2.2 对抗性探针（~/workspace/t08-probe，独立于 coder 自带测试编写，72 项）

逻辑探针 62 项（`probes/logic.test.ts`）+ 面板探针 10 项（`probes/panel.test.tsx`），全部通过。

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| P1 | 端到端 PRD 例独立复核 | 存储值 → 快照 → 计划 → 呈示：`created_at`/`updated_at` BIGINT + `^(created_at|updated_at)$` → Unix 毫秒 → iso-tz → Asia/Shanghai；不命中列走全局默认 | ✅ 通过。`present(1705316400000)` = `2024-01-15T19:00:00+08:00`（逐字，`matchedRuleName: 'unix-ms'`）；不命中 BIGINT 列状态 `default`、`present` 为 `unchanged/no-renderer`；时间语义列不命中时走全局默认（native iso UTC，`2026-01-15T14:30:00Z`） |
| P2 | 首条命中冻结 | 两条都命中的规则（毫秒 vs 秒；毫秒 vs native）各建计划；命中规则下喂不可解析行值 | ✅ 通过。`matchedRuleName` 恒为首条；`1705316400` 按毫秒解释为 `1970-01-21T01:41:56.400+08:00`（未回落到秒规则的 2024 年）；不可解析行值返回 `unchanged`，未回落到 native 第二条规则 |
| P3 | AND 条件 + 缺失来源 | 6 个条件逐一改错；无来源上下文直调匹配；计划层 source 缺省 | ✅ 通过。任一条件不满足即不命中；无表来源的查询列不满足表级条件（含表名、连接等）；仅列名/类型名条件的规则可命中查询列（计划层 `matchedRuleName: 'col-only'`） |
| P4 | 禁用/删除/调序改变命中 | 面板 UI 操作 → 保存 → 把已存 payload 再喂快照 → 计划，断言运行时命中规则名 | ✅ 通过。禁用首条 → 第二条获胜；删除唯一规则 → 无命中；第二条上移 → 获胜者翻转为第二条；规则草稿未保存时 store 未被调用、已存值无 rules |
| P5 | 受限正则拒绝项逐项 + 边界 | `validateRestrictedPattern` / `validateValuePresentationRule` 直调拒绝矩阵 27 项、接受矩阵 18 项；边界直调 + 编辑器 UI 拒绝 | ✅ 通过。反向引用（`\1`、`\2`、`\k<>`）、环视（4 种）、`\p`/`\u`/`\x`、量词套量词（`(a+)+`、`a**`、`a+*`）、被量化的含交替分组、被量化的可空分组、未闭合结构、非法 `{2,1}`、超限 `{101}`、无前置原子 `{2}a`、flags `g` 全部拒绝；合法子集（字面量、`.`、`^$`、字符类、分组/`(?:)`/交替、量词 `{n,m}` m≤100、惰性 `?`、flags `''`/`'i'`）全部接受；128 字符 pattern 可存、129 拒绝；64 字符名可存、65 拒绝（逻辑层与编辑器 UI 均验证）；20 条保留、21 条截断；空条件拒绝；重名检测（含大小写敏感与编辑时忽略自身） |
| P6 | 正则不在渲染线程跑任意用户正则 | 源码审读 + 全局 RegExp 计数包装：计划构建时计数，200 次 `present()` 后计数不变 | ✅ 通过。构建一列计划仅编译 1 次（≤4  bound 内）；200 次 `present()` 新增编译 0 次；源码审读：全 `lib/valuePresentation` 仅 `valuePresentationRules.ts` 含 `new RegExp`（`matchNamePattern` 与 `validateRestrictedPattern` 的防御性编译），渲染路径（GridCell/DetailPanel）不触正则 |
| P7 | 存储 | 旧 payload / 未知版本 v99 直调解析 + 快照；面板层 v99 保存 | ✅ 通过。旧 payload 无 rules → 解析结果无 rules 字段、快照 rules `[]`；v99 原样保留 version 99、不解析 rules、快照 rules `[]` 且 revision 透传；v1 数组逐条验证、无效条目丢弃；保存仅 version 1 写 rules——面板层 v99 保存保持 version 99 但**不写 rules 字段**（见 §5 O-3） |
| P8 | 样本命中预览 | 编辑器 UI：样本 miss / 不可解析样本 / 命中样本；样本预览文本与运行时计划逐字对照 | ✅ 通过。样本 miss 时仍可保存规则；不可解析样本显示 `unchanged`、不崩、仍可保存；命中样本预览 `2024-01-15T19:00:00+08:00` 与运行时计划输出逐字一致；无样本时显示 sampleHint 且仍可保存（仓库自带测试亦覆盖） |
| P9 | 总开关门控 | enabled=false 快照；全局 options 非法 + 规则有效 | ✅ 通过。enabled=false → 快照 rules `[]`；全局 options 非法（format `bogus`）时规则仍进快照并生效（`configs` 缺 timestamp、规则计划 `ready`、输出逐字正确）；默认快照 rules `[]`；全局默认路由在 options 无效时先报 enabled 门禁 `disabled`（符合 presentationPlan 文件头记载的检查顺序） |
| P10 | 保存失败 | 面板 UI：mock `updateValuePresentationSettings` reject 一次 → 报错、旧配置、草稿；重试成功 | ✅ 通过。显示 `saveFailed`、无 `saved` 标记；mock store 未变、旧规则在运行时仍获胜（`old-rule`）；草稿保留（新规则仍在列表）；重试保存成功后两条规则均入库、首条仍获胜 |
| P11 | i18n | 文件级脚本：提取两文件 rules.* key 集、组件实际 `t()` 引用、逐 key 取值对照 | ✅ 通过（附 O-1）。两文件各 39 key（37 新增 + `title`/`empty` 既有），key 集完全一致；`rules.editHint` 两边同删；组件引用的 38 个 rules.* key 在两文件均存在，无缺 key；zh 值均非空、与 en 无一相同 |

> 探针过程说明：首轮 5 项失败均为**探针期望错误**，代码未动，修正后 72/72 通过：(1) `present()` 的 `interpretation` 是粗粒度 kind（`timestampContribution.ts:251` 返回 `options.interpretation.kind`），Unix 应为 `'unix'` 而非探针原写的 `'unix-milliseconds'`（仓库自带测试亦断言 `'unix'`）；(2) 探针 `planFor` 的默认参数吞掉了显式传入的 `undefined`，导致「无来源」用例实际带了来源，改为布尔开关后通过；(3) P9 全局 options 无效时默认路由状态应为 `disabled`（enabled 门禁先于配置缺失检查），非探针原写的 `missing-config`。另有一处探针脚手架教训：P4/P10 初版夹具未在已存值里放 `enabled: true` 的 timestamp 条目，面板草稿开关默认关，保存后规则被总开关门控拦截——修正夹具后才真正测到「规则动作 → 运行时命中」。

### 2.3 契约类型严格性审读（G6）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| T1 | 改动范围 | `git diff 237f82db2..99abd37f5 --stat` | ✅ 两提交共 22 文件、+3225/−88，全部在 T08 范围内：规则模型/编辑器/面板/存储/计划/贡献导出/索引导出/两 locale/backend-client 镜像/Rust 3 文件/7 处测试夹具（见 T5）/面板测试调整。无无关改动、无删除（除 T07 workaround 的删除） |
| T2 | any 泄漏 / 断言跳过验证 | grep 两提交新增行 `: any`/`as any`/`<any>`/`any[]`、新测试 `it.skip/test.skip/.todo(/xit(/xdescribe` | ✅ 新增行中 `any` 0 处；新测试 0 处跳过（70+23+83+41 项全数，见 G1） |
| T3 | i18n key 数量与内容一致 | `git show` 计数 + P11 脚本 | ⚠️ 见 §5 O-1：实测各新增 37 key，非任务/coder 申报的 38；但两文件 key 集完全一致且与组件引用一一对应，无缺失 |
| T4 | T07 `readStoredRules` workaround 已删无死代码 | 全仓 grep `readStoredRules` / `StoredRuleSummary` | ✅ 通过。0 处残留；code 提交同时删了其纯函数测试（面板测试文件 −14 行） |
| T5 | 夹具补齐合理性 | code 提交内 7 处 `rules: []` 逐项核对 | ✅ 通过。全部是 `ValuePresentationSnapshot` 新必填字段的补齐（DataTable.presentation / DetailPanel.displayState / DetailPanel.presentation / useValuePresentation / presentationPlan / settings / ContentViewDrawers.detailDisplay 各 1 处）；补跑 34 项 + G1 已含 106 项，合计 140 项全部通过。申报「6 处 / 65 项」与实测不符，见 O-7 |
| T6 | 规则无 priority 数值 | grep `priority` 于新规则代码 | ✅ 通过。0 处；顺序即唯一优先级，`findFirstMatchingRule` 按列表顺序返回首条 |
| T7 | Rust 改动最小性 | `git show a1951af8b -- crates/` 全 diff 审读 | ⚠️ 见 §4 BUG-001：`domain.rs` 只加不透明 `rules: Vec<serde_json::Value>` + `#[serde(default)]` + `Default`，未动版本与语义；但同一文件的既有 `#[cfg(test)]` 字面量未同步补字段，导致内核 lib 测试无法编译 |
| T8 | Prettier | `npx prettier --check`（两提交全部改动 TS/TSX 文件） | ✅ 通过。`All matched files use Prettier code style!` |
| T9 | 申报语义逐条核验 | 逐条对照 diff 与源码（见 §3） | ✅ 全部成立（除 BUG-001 与 O-1/O-7 的申报数字出入） |
| T10 | code 提交 amend | author/committer 日期对照 | ⚠️ 见 §5 O-2：code 提交 author 03:10:18 / committer 03:20:51，确曾 amend；test 提交未 amend |

## 3. 设计符合性逐条结论

对照设计文档、PRD 与 coder 申报：

**Coder 申报核验**

| 条目 | 结论 |
|------|------|
| 1. 新 `valuePresentationRules.ts`：规则模型/受限正则/匹配 | ✅ 符合（§3 下表逐项；P1–P6 独立复核） |
| 2. 新 `ValuePresentationRuleEditor.tsx` | ✅ 符合（表单唯一验证入口 `validateValuePresentationRule`；样本预览经贡献 prepare/present；flags 只提供 `''`/`'i'`；P8 独立复核；见 O-5 一处预览措辞观察项） |
| 3. `settings.ts`：rules 存储 + 快照 | ✅ 符合（version 保持 1 + 可选 rules；旧 payload 缺省空列表；未知版本不解析 rules；快照仅时间渲染器启用时带 rules；全局 options 无效不影响规则，P7/P9 独立复核） |
| 4. `presentationPlan.ts`：规则进计划 | ✅ 符合（顺序：raw 覆盖 → custom 覆盖 → 规则 → 全局默认；`matchedRuleName` 记录命中来源；命中后冻结整列解释；贡献缺失时落到默认路由；P1/P2/P3 独立复核） |
| 5. `timestampContribution.ts`：导出 `timestampOptionsToConfigValue` | ✅ 符合（仅把既有内部函数改导出并改一处调用点，无语义变化；G3 通过） |
| 6. 面板 `ValuePresentationSettingsSection.tsx`：可编辑规则列表，删除 T07 workaround | ✅ 符合（新增/编辑/删除/启停/调序；规则草稿随面板草稿一起保存；保存时防御性再验证全部草稿；P4/P10 独立复核；`readStoredRules` 已删，见 G6 T4） |
| 7. 中英文 locales 各 +38 rules.* key（删失效 `rules.editHint`） | ⚠️ 数字不准，见 O-1：实测各 +37 key，两边一致，`editHint` 两边同删，无缺 key |
| 8. `backend-client/src/settings.ts` 镜像规则类型 | ✅ 符合（`SettingsValuePresentationRule` / `Match` / `NamePattern` 逐字段镜像，不持久化 RegExp；`toSharedSettings` 整体透传 `valuePresentation`，rules 随之走保存链路） |
| 9. Rust `crates/datazen/src/settings/domain.rs` 加不透明 rules 字段 | ⚠️ 见 BUG-001：字段本身最小且正确（`Vec<serde_json::Value>` + `#[serde(default)]`，旧 payload 默认为 `[]`，未动版本与语义）；但同一文件既有测试字面量未同步，导致内核 lib 测试编译失败 |
| 10. 6 处测试夹具补齐在 code 提交内 | ⚠️ 见 O-7：实测为 7 处（另含 ContentViewDrawers.detailDisplay），全部在 code 提交内；回归数字亦与申报不符（实测 140 项） |

**设计 §5（与 T08 相关项）**

| 条目 | 结论 |
|------|------|
| 规则数组顺序是唯一优先级，不再保存 priority 数值 | ✅ 符合（无 priority 字段；`findFirstMatchingRule` 顺序返回首条；面板调序改变命中，P4） |
| 规则内名称条件 AND，首条命中后冻结整列解释 | ✅ 符合（`matchValuePresentationRule` 全条件 AND；计划按列冻结，行解析失败不回落，P2/P3） |
| pattern 保存 source 与受限 flags，不持久化 RegExp | ✅ 符合（类型只含 `source`/`flags`；flags 仅 `''`/`'i'`；编辑器不提供其他 flags） |
| 缺失来源不能满足表级条件 | ✅ 符合（连接/数据库/namespace/表任一缺失即不满足；查询列仅列名/类型名规则可命中，P3） |
| 正则安全：受限子集、保存时拒绝、规则数与输入长度设限 | ✅ 符合（手写解析器拒绝 27 类危险/非法模式；量词 `{n,m}` m≤100；20 条/128 字符/64 字符名上限；P5/P6） |
| 不得在渲染线程执行任意用户正则 | ✅ 符合（RegExp 只在计划构建与验证时编译；200 次 `present()` 零新增编译；P6） |

**PRD §3.1 / §4 / §6（与 T08 相关项）**

| 条目 | 结论 |
|------|------|
| §3.1：规则可新增、编辑、删除、启停、调序；条件含连接/数据库/schema/表/列/类型名 | ✅ 符合（面板五项操作齐全；六类条件齐全，P3/P4） |
| §3.1：规则编辑显示当前样本的命中结果及预览；无样本仍可配置、不编造数据 | ✅ 符合（样本预览与运行时同一贡献路径、逐字一致；无样本显示 sampleHint 且可保存，P8） |
| §3.1：非法表达式或配置禁止保存，并就地说明原因 | ✅ 符合（编辑器就地列出路径化原因；面板保存时再验证；P5） |
| §4：表列覆盖 > 全局规则 > 全局默认 | ✅ 符合（计划顺序 raw/custom → 规则 → 默认；P1 的默认路由对照） |
| §4：规则命中后不因某一行解析失败继续寻找低优先级规则 | ✅ 符合（P2） |
| §6 验收 4：列覆盖优先于规则；「原始值」阻断规则；清除覆盖恢复继承 | ➖ 部分范围外：覆盖优先于规则已由计划顺序保证（raw/custom 在规则之前返回，审读确认）；「原始值」阻断与清除恢复是 T10 列覆盖持久化的范围 |
| §6 验收 5：规则顺序可调整，禁用规则不参与匹配，错误规则不能保存 | ✅ 符合（P4/P5） |
| §6 验收 8：修改时区、源时区与单位能正确跨日 | ➖ 部分范围外：T08 未新增跨日用例；规则 options 复用 T02/T03 的时区与单位路径（P1 锚点跨越 UTC+8 边界：`1705316400000` 在 UTC 为 2024-01-15T11:00，呈示为 +08:00 的 19:00） |

## 4. Bug 列表

| 编号 | 标题 | 级别 | 状态 |
|------|------|------|------|
| T08-BUG-001 | T08 code 提交新增 `rules` 必填字段后，`crates/datazen/src/settings/domain.rs` 同文件既有 `#[cfg(test)]` 两处字面量未同步补字段，内核 lib 测试无法编译 | 高（阻塞） | 已关闭（回归验证通过，见 §8） |

**T08-BUG-001 详情**

- 位置：`crates/datazen/src/settings/domain.rs:295`（`value_presentation_defaults_to_version_1_with_no_renderers`）与 `:315`（`value_presentation_round_trips_unknown_version_and_renderers`）。
- 引入：code 提交 `a1951af8b` 给 `ValuePresentationSettings` 增加必填字段 `pub rules: Vec<serde_json::Value>`（`#[serde(default)]`），但同一文件的两处既有测试字面量未补 `rules: vec![]`。基底 `237f82db2` 时该字段不存在，两处字面量可编译；T06 报告 G6 记载同一命令在 T06 HEAD 为 14/14 通过。
- 复现（工作区根目录）：
  ```bash
  CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0' cargo test -p datazen --lib --features settings settings
  # 或 cargo test --no-run -p datazen --lib --features settings
  ```
  输出（退出码 101）：
  ```text
  error[E0063]: missing field `rules` in initializer of `domain::ValuePresentationSettings`
     --> crates/datazen/src/settings/domain.rs:295:13
  error[E0063]: missing field `rules` in initializer of `domain::ValuePresentationSettings`
     --> crates/datazen/src/settings/domain.rs:315:24
  error: could not compile `datazen` (lib test) due to 2 previous errors
  ```
- 影响范围：仅内核 crate 的 lib **测试目标**（`settings` feature 门控，`lib.rs:113`）。生产代码不受影响——`cargo check -p datazen --lib --features settings` 退出码 0；`cargo check -p datazen_desktop --lib --tests` 退出码 0（桌面测试目标可编译，含新增的 rules round-trip 测试）。但 T06 确立的门禁命令本身在 HEAD 失败，CI 跑 datazen settings 腿时会红；且新增的桌面 rules round-trip 测试在本地无法链接验证（见 G7），修复后须由 CI 覆盖两端。
- 修复建议（未实施，tester 不改代码）：两处字面量各加 `rules: vec![],`。同时建议补一条「旧 payload 无 rules 字段反序列化为 `[]`」的内核断言时顺带覆盖（现有 `value_presentation_defaults_to_version_1_with_no_renderers` 已断言空对象加载等于默认，修后即覆盖该行为）。
- 注意：`cargo check -p datazen --lib --tests --features settings` 另报 `cm28_concurrent_tunnel` 集成测试目标 `error[E0433]: cannot find 'execution' in 'datazen'`——与 T08 无关（该目标需 `execution` feature，T08 未碰 execution；仅用 `--features settings` 时该目标在基底同样无法编译），不计入本 bug。

本次验证未发现其他 bug。前端 217 项定向、637 项接线回归、72 项对抗探针全部通过；设计符合性除 BUG-001 外全部成立。

## 5. 观察项（非缺陷，供 coder / 后续任务参考）

- **O-1（i18n 申报 38 key vs 实测 37 key）**：任务与 coder 申报均为「各 +38 key」，实测 en 与 zh-CN 各新增 37 key（`git show` 计数与 P11 脚本一致；两文件各 39 个 rules.* key，含既有 `title`/`empty`）。两文件 key 集完全一致、组件引用的 38 个 rules.* key 无一缺失、zh 值均非空且与 en 无一相同、`rules.editHint` 两边同删，属申报数字不准，非代码问题；后续申报以实测为准。同类出入 T07 已有（申报 40 vs 实测 37）。
- **O-2（code 提交 amend）**：code 提交 `a1951af8b` author 03:10:18 / committer 03:20:51，确曾 amend（与 coder 申报的「amend 两次」方向一致，次数未单独核实）；test 提交未 amend。最终状态以 HEAD 为准，无结论影响。
- **O-3（未知版本保存丢 rules 字段——已知局限）**：面板保存时 `rules` 只在 `stored.version === 1` 时写回（`ValuePresentationSettingsSection.tsx` 的 `...(stored.version === 1 ? { rules: [...ruleDrafts] } : {})`）；而宽容解析对 v99 根本不解析 rules。探针 P7 面板用例实测：v99 已存值含一条规则，打开规则列表为空态，保存后写回值保持 `version: 99` 但**不再含 `rules` 字段**（原规则丢失）。该行为与 T06「未知版本保留原文」的字面要求有张力，但属 T06 已有模式（解析即有损，保存写解析后形状；T07 保存同样不带 rules 字段），且 v99 规则形状本就未知、无法按 v1 解释，故按任务指令记为观察项/局限而非新 bug。建议后续任务在 T11 跨窗口同步或文档中明确：未知版本配置在面板保存后会丢失其 rules 字段（其余 renderer 条目仍原样保留）；若需严格保留，应在保存时透传未解析的原始 rules。
- **O-4（>20 条规则的静默截断会在保存时丢尾）**：`validateValuePresentationRules` 超过 20 条只保留前 20 条（解析层静默截断，无问题报告）。编辑器 Add 按钮在 20 条时禁用，故 UI 无法超限；但一份已存 21+ 条规则的 payload 经解析 → 面板保存后，第 21 条起会被丢掉。属设计内行为（上限 20），建议文档注明。
- **O-5（规则非法时样本预览区显示 sampleHint，易误导）**：`ValuePresentationRuleEditor.tsx` 的 `samplePreview` 在 `validated.ok` 为 false 时直接返回 `{ kind: 'no-sample' }`，即使用户已填样本，预览区仍显示「输入样本值和列名……」的 sampleHint，而非法原因只在点「保存规则」后才以 `invalidRule` 报错出现。行为正确（非法规则确实禁存），但预览区文案与用户已填样本的状态不符。建议后续把无效规则时的预览区改为显示验证问题摘要，或保持现状（低优先级）。
- **O-6（未覆盖行清单）**：除 G5 表列出的未覆盖行外，值得注意的几处：`settings.ts:160`（`buildValuePresentationSnapshot` 中 `validateConfig` 抛异常的防御性 `continue`，与 T06 的 `settings.ts:133` 同类分支）；`presentationPlan.ts:168/231/374`（规则步骤的贡献缺失分支与默认路由分支，仓库测试覆盖了行为但未覆盖全部行）；`valuePresentationRules.ts` 未覆盖行多为解析器拒绝分支的细分路径（26 行左右），接受/拒绝矩阵由仓库 70 项与探针 47 项双重覆盖，行覆盖缺口不代表行为缺口。建议接受探针覆盖，不阻塞 T08。
- **O-7（夹具申报数字不准）**：coder 申报「6 处测试夹具补齐」「6 个受影响文件回归 65 通过」；实测 code 提交内 `rules: []` 补齐为 **7 处**（另含 `ContentViewDrawers.detailDisplay.test.tsx`），7 个文件的测试共 **140 项**（G1 已含 presentationPlan 83 + settings 23；本次补跑其余 5 文件 34 项），全部通过。申报数字不准，非代码问题。
- **O-8（Rust 测试需 CI 跑）**：桌面 `cargo test` 本地无法链接（缺 webkit2gtk-4.1/gtk-3 等系统库，见 G7），新增的 `value_presentation_rules_round_trip_through_shared_settings`（test 提交，`settings_shared_tests.rs` +45 行：共享路径 + JSON 双 round-trip，夹具为前端合法形状的规则）只能由 CI 验证；内核 `datazen` 的 settings 腿在 BUG-001 修复后本地可跑（`cargo test -p datazen --lib --features settings settings`）。
- **O-9（探针资产）**：`~/workspace/t08-probe` 为一次性验证项目，已按任务要求删除（探针文件与 vitest 配置随之清除；仓库 `git status` 复核除本报告外干净）。本报告 §2.2 各项方法已描述到可复现程度，日志见 `~/workspace/t08-p-logic.log`、`~/workspace/t08-p-panel.log` 与 `~/workspace/t08-g*.log`。建议将 P5 的拒绝矩阵（27 项）、P6 的 RegExp 计数（200 次 present 零编译）与 P4 的「UI 动作 → 运行时命中」端到端用例吸收为仓库回归测试。
- **O-10（快照带 rules 不要求贡献在场）**：`buildValuePresentationSnapshot` 只以 `timestampEntry.enabled === true` 门控 rules，不检查 registry 是否含该贡献；贡献缺失时快照仍带 rules，计划层规则步骤因 `registry.get` 为空而跳过、落到默认路由（设计内行为，代码注释已说明）。无缺陷，仅记录以便 T11 扩展停用回退时复核。

## 6. 结论

**通过，可做收尾提交。**

- 回归验证（§8，2026-11-12）：修复提交 `d8b730989` 经 R1–R4 独立复核全部通过，T08-BUG-001 已关闭；其余结论不变（首轮数字见下）。

- 既有门禁：T08 定向 217/217（70+23+83+41）、夹具补跑 34/34、接线回归 637/637、双引擎类型通过、命名门禁通过、Prettier 全过；新文件覆盖率全部 ≥80%（rules 92.3/89.65/100/92.9、编辑器 100/86.4/100/100、settings 96.96/100/100/96.66、presentationPlan 96.42/96.62/100/96.38、面板 90/88.82/90.76/91.66），与 coder 申报逐项一致。
- 对抗性探针 72/72 通过（逻辑 62 + 面板 10）：P1 PRD 例逐字一致、P2 首条冻结且行解析失败不回落、P3 AND 条件与缺失来源、P4 禁用/删除/调序改变运行时命中、P5 受限正则 27 拒/18 受 + 全部边界、P6 渲染线程零新增编译、P7 旧 payload 与未知版本、P8 样本预览与运行时逐字一致、P9 总开关门控与全局 options 无效不影响规则、P10 保存失败旧配置仍生效且草稿保留、P11 两语言 key 一致（附 O-1 的 37 vs 38 申报出入）。
- 设计符合性：设计 §5/§6/§7 与 PRD §3.1/§4、验收 5 均符合；验收 4 的「原始值」阻断与验收 8 的跨日新增用例属 T10/T12 范围。
- 唯一 bug 是 T08-BUG-001：code 提交新增 `rules` 必填字段后未同步 `domain.rs` 同文件两处既有测试字面量，`cargo test -p datazen --lib --features settings` 编译失败（2× E0063）。生产代码编译通过，前端全部通过；修复为两处各加 `rules: vec![]`，改动面极小。修复提交 `d8b730989` 已由 §8 回归验证通过并关闭本 bug；桌面 settings 套件（含新增的 rules round-trip 测试）本地仍无法链接，须由 CI 覆盖。
- 观察项 10 条均为非缺陷（O-1 i18n 申报 key 数不准、O-2 code 提交 amend、O-3 未知版本保存丢 rules、O-4 超 20 条静默截断、O-5 非法规则时预览区文案、O-6 未覆盖行清单、O-7 夹具申报数字不准、O-8 Rust 测试需 CI、O-9 探针资产、O-10 快照带 rules 不要求贡献在场）。
- tasks.md 状态更新与 docs close commit 不在本次验证范围内，由协调方执行。

## 7. 专项：T08-BUG-001 根因、证据与影响面

### 7.1 现象

`cargo test -p datazen --lib --features settings settings`（T06 确立的内核 settings 门禁命令）在 HEAD 退出码 101，编译失败：

```text
error[E0063]: missing field `rules` in initializer of `domain::ValuePresentationSettings`
   --> crates/datazen/src/settings/domain.rs:295:13
error[E0063]: missing field `rules` in initializer of `domain::ValuePresentationSettings`
   --> crates/datazen/src/settings/domain.rs:315:24
error: could not compile `datazen` (lib test) due to 2 previous errors
```

### 7.2 根因（源码核实）

`crates/datazen/src/settings/domain.rs`：

- `:57–62` `ValuePresentationSettings` 经 T08 增加必填字段 `pub rules: Vec<serde_json::Value>`（`#[serde(default)]`，生产侧正确：旧 payload 反序列化为 `[]`，`Default` 已补 `rules: Vec::new()`）。
- 同文件 `#[cfg(test)] mod tests` 内两处既有字面量未同步：
  - `:295` `value_presentation_defaults_to_version_1_with_no_renderers`：`ValuePresentationSettings { version: 1, renderers: Map::new() }`；
  - `:315` `value_presentation_round_trips_unknown_version_and_renderers`：`ValuePresentationSettings { version: 99, renderers }`。
- 对照：基底 `237f82db2` 的同一文件 `:288`/`:308` 处字面量无 `rules` 字段（字段当时不存在），可编译；T06 报告 G6 记载同一命令在 T06 HEAD 为 14/14 通过。缺失字段是 T08 引入无疑。
- 桌面侧同类字面量已同步：`crates/datazen-desktop/src/store/tests/settings_shared_tests.rs:80` 与 `crates/datazen-desktop/src/platform/settings.rs:141` 的 `rules: vec![]` 均在 code 提交内；新增测试 `:118` 的构造也含 rules。漏的只是内核自有测试模块的两处。

### 7.3 影响面（实测分层）

| 层 | 命令 | 结果 |
|----|------|------|
| 内核生产代码 | `cargo check -p datazen --lib --features settings` | ✅ 退出码 0 |
| 内核 lib 测试目标 | `cargo test --no-run -p datazen --lib --features settings` | ❌ 退出码 101，2× E0063 |
| 桌面 lib+测试目标 | `cargo check -p datazen_desktop --lib --tests` | ✅ 退出码 0（可编译，含新增 rules round-trip 测试） |
| 桌面 lib 测试运行 | `cargo test -p datazen_desktop --lib settings_shared` | ⚠️ 本地无法链接（缺 webkit2gtk-4.1/gtk-3/gdk-3/gdk_pixbuf-2.0），CI 跑 |

结论：BUG-001 只断内核 lib 测试目标的编译，不影响生产代码与桌面目标；但它是 T06 门禁命令的直接失败项，CI 的 datazen settings 腿会红，故定为阻塞级。修复后本地可立即重测内核腿；桌面腿仍须 CI。

### 7.4 与 T06 O-1 的关系

T06 观察项 O-1 记载「提交 1 单独不通过 typecheck，4 个既有测试夹具的字段补齐落在 test 提交」——T08 吸取了该教训，TS 侧 7 处夹具全部放进 code 提交（G6 T5），但 Rust 侧同类夹具漏了内核自有测试模块的两处。建议后续任务在「新增必填字段」类改动后，以 `cargo test --no-run -p <crate> --lib --features <feature>` 复核测试目标编译，而不只跑 `cargo check`（后者不编 `#[cfg(test)]` 代码）。

## 8. 回归验证记录（T08-BUG-001 修复）

2026-11-12，tester 对 T08-BUG-001 的修复提交 `d8b730989`（`fix(datazen): sync ValuePresentationSettings test literals with rules field (issue-32 T08 BUG-001)`）做了聚焦回归验证。验证开始时工作区 HEAD 即 `d8b730989`（`git log -3`：`d8b730989` / `99abd37f5` / `a1951af8b`），除本报告未提交外工作区干净；tester 未改任何被测代码，唯一可写文件为本报告。

### R1：修复 diff 审读 ✅ 通过

`git show d8b730989`：1 文件、2 插入、0 删除，仅 `crates/datazen/src/settings/domain.rs` 两处各 +1 行 `rules: vec![]`（新行号 `:298`、`319`），无其他改动。两处字面量字段顺序与 struct 定义一致（`version` → `renderers` → `rules`，见 `domain.rs:57–64`）；`Default` impl（`:70–78`）本身已含 `rules: Vec::new()`，无需改动。插入位置与首轮报错行逐一对应（首轮 §4/§7：`:295` 与 `:315` 的 2× E0063；第二处行号偏移 +1 来自第一处 +1 行）。

### R2：内核 lib 测试 ✅ 通过

`CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0' cargo test -p datazen --lib --features settings`：退出码 0，**56 passed；0 failed**（与修复人申报 56/56 一致）。两处涉事测试均通过（单独过滤复核）：

```text
test settings::domain::tests::value_presentation_defaults_to_version_1_with_no_renderers ... ok
test settings::domain::tests::value_presentation_round_trips_unknown_version_and_renderers ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 52 filtered out
```

修复前失败未在工作区重现（避免污染工作区）：以首轮 §4/§7 的实测记录为准（同一命令退出码 101，2× `error[E0063]: missing field 'rules'`），修复 diff 仅两处 +1 行且与首轮报错行号逐一对应，足以建立因果。另：该命令是 datazen 内核 crate，不走桌面 IPC 身份路径，无需 `USER`。

### R3：桌面 check ✅ 通过

`CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0' cargo check -p datazen_desktop --lib --tests`：退出码 0，51 warnings（与首轮 G7 分项一致，均为既有警告，如 `store/settings.rs:6` 未用导入，与 T08 无关；修复未新增警告）。桌面 `cargo test` 本地仍无法链接（缺 webkit2gtk-4.1/gtk-3 等系统库，见首轮 G7 与 O-8），未装系统依赖，含新增 rules round-trip 测试的桌面 settings 套件留给 CI。

### R4：前端 valuePresentation 全库轻量回归 ✅ 通过

`npx vitest run packages/frontend/src/lib/valuePresentation --retry=0`：退出码 0，**8 文件 382/382 通过**（valuePresentationRules 70、timestampContribution 46、presentationPlan 83、settings 23、timestampFormat 34、temporalMath 96、timestampConfig 26、registerBundledContributions 4）。未全量重跑前端其余门禁，判断依据：修复只动 Rust `#[cfg(test)]` 字面量，不触任何前端代码；首轮前端门禁（G1 217/217、G1b 34/34、G2 637/637、G3 双引擎、G4、G5、G6、P1–P11）已全过，R4 覆盖 T08 触及的前端全库即足以排除修复引入的回归。

**T08-BUG-001 验证通过并关闭。**结论更新为「通过，可做收尾提交」（见 §6）。未发现修复引入的新问题；桌面测试腿仍须 CI 覆盖（R3）。
