# T11 测试报告：跨窗口同步与扩展停用回退

- 被测对象：`packages/frontend/src/lib/valuePresentation/valuePresentationSave.ts`（新增，89 行：`ValuePresentationSettingsUpdate` 对象|updater、`applyValuePresentationUpdate`、`withTimestampRenderer`、`withValuePresentationRules`）+ `packages/frontend/src/stores/settingsStore.ts`（代次防护的 `loadSettings`、权威重读的 `updateValuePresentationSettings`、抽取的 `persistSettings` 共用保存核心）+ `packages/frontend/src/hooks/useSettingsSync.ts`（通知只触发重读，不信任广播 payload）+ `packages/frontend/src/lib/valuePresentation/columnOverrides.ts`（`persist` 改 updater 形）+ `packages/frontend/src/components/DataTable/ColumnPresentationDialog.tsx`（注册表订阅 + updater 启用路径）+ `packages/frontend/src/windows/settings/ValuePresentationSettingsSection.tsx`（updater 保存、逐路径脏检查、外部更新提示 + “Reload saved settings”）+ `packages/frontend/src/locales/en/settings.ts` 与 `packages/frontend/src/locales/zh-CN/settings.ts`（各 +2 key：`settings.valuePresentation.externalUpdate` / `reloadSaved`）+ 自带测试 8 文件（新 2：`valuePresentationSave.test.ts` 9 项、`valuePresentationSync.test.tsx` 4 项；改 6）
- 分支：`feature/issue-32-value-presentation`（HEAD `4ae7648a9`，作者 issue-32-agent；T11 三提交：code `327824753` + fix `0a9eee192` + test `4ae7648a9`，基底 `75760d5c0`；验证开始时工作区干净，`git status` 无未提交差异）
- 测试角色：独立 tester。未修改任何被测源码或测试；唯一可写文件为本报告。
- 依据：`docs/archive/development/issue-32-value-presentation-design.md` §6（配置加载、持久化与同步：路径化更新/版本条件更新、保存后发布修订通知、接收端重读权威配置、加载状态 1–5、历史配置缺字段默认关闭、未知版本/缺失贡献保留原文）、§3（注册/卸载触发订阅、贡献版本令相关计划失效）、§7（计划缓存键）；`docs/archive/development/issue-32-value-presentation-prd.md` §5（停用扩展时 cell 与 aside 同时回退，重启用后恢复已保存配置）、§6 验收 10/12；T06 报告 §3（路径化更新/版本条件更新的形式化是 T11 范围；刷新/保存失败的保持语义当时未单独立项验证）
- 日期：2026-11-12

## 1. 测试环境与命令

环境：Node v24.22.1，vitest 4.1.10（仓库自带），tsrs 0.18.0-ts7.1.0-dev.20260929 / tsc 7.0.2，系统时区 UTC（全部探针均显式传入时区与语言，不依赖机器默认值）。对抗性探针运行于独立临时项目 `~/workspace/t11-probe/`（通过 `node_modules` 符号链接复用仓库依赖，`environment: jsdom`，被测源码以绝对路径导入，不污染仓库工作区；探针按任务要求用后删除，原始日志见 `~/workspace/t11-p-logic.log`、`~/workspace/t11-p-panel.log`、`~/workspace/t11-p-panel-bus.log`；另有 G1–G6 日志 `~/workspace/t11-g1.log`、`~/workspace/t11-g1b.log`（修正版）、`~/workspace/t11-g2.log`、`~/workspace/t11-g3a.log`、`~/workspace/t11-g3b.log`、`~/workspace/t11-g4.log`、`~/workspace/t11-g5.log` 与 `~/workspace/t11-g6-{code,fix,test}.diff`、`~/workspace/t11-g6-prettier.log`）。

环境说明：本机 `PATH` 中无 `pnpm` 二进制（与 T02–T10 报告相同情况）；typecheck 改用 `npx -y pnpm@12.18.0 <script>` 调用，版本号实测 12.18.0，属等价调用，不影响门禁结论。

| 命令 | 用途 |
|------|------|
| `npx vitest run <8 个 G1 文件> --retry=0` | G1：T11 定向（valuePresentationSave 9 + valuePresentationSync 4 + settingsStore 37 + useSettingsSync 4 + columnOverrides 32 + 对话框 25 + 入口 2 + Section 44） |
| `npx vitest run packages/frontend/src/windows/settings packages/frontend/src/lib/valuePresentation packages/frontend/src/stores/__tests__/settingsStore.test.ts packages/frontend/src/components/DataTable packages/frontend/src/hooks/__tests__/useSettingsSync.test.tsx --retry=0` | G2：接线回归 |
| `npx -y pnpm@12.18.0 typecheck` / `npx -y pnpm@12.18.0 typecheck:tsc` | G3：双引擎类型门禁（tsrs / 权威 tsc） |
| `node scripts/check-test-module-naming.mjs` | G4：测试命名门禁 |
| `npx vitest run <G1 路径> --retry=0 --coverage --coverage.include='…valuePresentationSave.ts' --coverage.include='…columnOverrides.ts' --coverage.include='…useSettingsSync.ts' --coverage.include='…settingsStore.ts' --coverage.include='…ColumnPresentationDialog.tsx' --coverage.include='…ValuePresentationSettingsSection.tsx'` | G5：新文件覆盖率（v8 报告 + coverage-final.json） |
| `git show 327824753 / 0a9eee192 / 4ae7648a9` 全 diff + grep 新增行 `any` / 跳过断言 + Prettier + `git diff --check` | G6：改动范围审读 |
| `git show` 三提交文件清单 grep `.rs` | G7：Rust（见 §2.1） |
| 探针项目内 `./node_modules/.bin/vitest run --retry=0`（`probes/logic.test.ts` 11 项 + `probes/panel.test.tsx` 6 项 + `probes/bus.test.ts` 1 项） | 对抗性探针 P1–P10（含 P2b、P5-panel、P6a/b/c、P7-panel、P10-panel） |

## 2. 测试项清单

### 2.1 既有验证（仓库门禁）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| G1 | T11 定向单测 | 见 §1 表格 | ✅ 8 文件 **157/157 通过**（9+4+37+4+32+25+2+44；退出码 0；与 coder 申报逐项一致。首轮实测 7 文件 153 项，系 tester 命令笔误——`valuePresentationSync.test.tsx` 的正确路径是 `packages/frontend/src/hooks/__tests__/` 而非 `lib/valuePresentation/__tests__/`，修正后重跑 8 文件 157/157，见 O-11） |
| G2 | 接线回归 | 设置页全量 + valuePresentation 全库 + settingsStore + DataTable 全目录 + useSettingsSync | ✅ 99 文件 **1222/1222 通过**（退出码 0；回归全绿，无未解释失败；coder 的 102/1237 之说与任务指定命令的实测范围不符，见 O-1） |
| G3 | 类型门禁（双引擎） | `pnpm typecheck` / `pnpm typecheck:tsc`（经 `npx -y pnpm@12.18.0` 等价调用） | ✅ 两者均 OK（tsrs 0.18.0-ts7.1.0-dev.20260929 与 tsc 7.0.2，退出码均为 0） |
| G4 | 测试命名门禁 | `node scripts/check-test-module-naming.mjs` | ✅ ok（1837 Rust 文件、256 cfg(test) 模块；2591 TS 文件、8 wdio spec 树；退出码 0。TS 文件数较 T10 的 2588 净 +3，恰为 T11 新增的 3 个 TS/TSX 文件：valuePresentationSave.ts + 2 个新测试文件） |
| G5 | 新文件覆盖率 | 见 §1 表格 | ✅ 全部 ≥80%（见下表明细；测试 157/157 通过，退出码 0；未出现 T06 报告的全局阈值退出码问题） |
| G6 | 改动范围与源码审读 | 三提交全 diff + grep + Prettier + diff-check | ⚠️ 大体通过，附 §4 T11-BUG-001：改动范围、分拆、any/跳过、i18n、fix 最小性均通过；但 coder 申报“修了 T10 P10(d)”在 store 层不成立，见 §4 与 §2.3 |
| G7 | Rust | 三提交文件清单核实 | ✅ T11 未改任何 Rust 文件（三提交 17 个不同文件，`.rs` 0 处；`settingsStore.ts` 在 code 与 fix 两提交中各出现一次）；桌面 cargo 链接问题未触及，无需跑 cargo test |

> G5 明细（v8 报告；`% Stmts / % Branch / % Funcs / % Lines`）：
>
> | 文件 | % Stmts | % Branch | % Funcs | % Lines | 未覆盖语句行 |
> |------|---------|----------|---------|---------|-------------|
> | `lib/valuePresentation/valuePresentationSave.ts`（新） | 100 | 100 | 100 | 100 | — |
> | `lib/valuePresentation/columnOverrides.ts` | 100 | 100 | 100 | 100 | — |
> | `hooks/useSettingsSync.ts` | 100 | 100 | 83.33 | 100 | —（语句全覆盖；唯一未覆盖函数是 `:30–32` 的 `.catch` 回调，即 listen 本身 reject 的路径） |
> | `stores/settingsStore.ts` | 91.52 | 88 | 84.21 | 95.23 | 125, 126, 133, 134, 137, 317, 322, 325, 328, 340 |
> | `components/DataTable/ColumnPresentationDialog.tsx` | 90.24 | 85.46 | 88.63 | 92.61 | 182, 184, 215, 250, 334, 346, 413, 424, 426, 429, 457, 458, 496, 617, 618, 721 |
> | `windows/settings/ValuePresentationSettingsSection.tsx` | 91.03 | 91.05 | 91.17 | 92.57 | 226, 282, 434, 436, 439, 444, 446, 449, 463, 465, 468, 472, 473, 532, 536, 545, 551, 564, 799, 845 |
>
> 与 coder 申报的对照：valuePresentationSave.ts 100/100/100/100、columnOverrides.ts 100/100/100、useSettingsSync.ts 100/100/83.33 逐字一致；settingsStore.ts coder 申报 91.53/88.00/84.21，实测行 91.52（−0.01）；对话框 coder 申报 90.24/85.47/88.64，实测分支 85.46 / 函数 88.63（各 −0.01）；Section coder 申报 91.03/91.05/91.18，实测函数 91.17（−0.01）。三处均为小数点后两位舍入级出入，方向为中性，均 ≥80%，无结论影响，见 O-2。未覆盖行均为防御分支，见 §5 O-12。

### 2.2 对抗性探针（~/workspace/t11-probe，独立于 coder 自带测试编写，逻辑 11 项 + 面板 6 项 + 传输 1 项）

探针方法总述：两个窗口 = 两次 `vi.resetModules()` 后动态 import 的 settingsStore 模块实例（各自的 `settingsLoadGeneration` / `hasLoadedSettings`），共享同一个模拟权威后端（`mocks.backend.settings`）与同一个进程内通知总线（`mocks.bus`：emit 即分发给全部已注册监听，与 Tauri 事件/BroadcastChannel 的“广播到全部窗口”语义一致；payload 由调用方传入，可伪造旧/脏值）。grid/aside 两处消费者 = 同一快照驱动的两个 `useValuePresentation` hook。面板探针用真实 store + 真实 Section + 真实 useSettingsSync，另一窗口的保存模拟为改写权威后端 + 伪造通知，本窗口经重读更新。传输探针用真实 `crossWindowBus` 模块与原生 `BroadcastChannel`。

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| P1 | 双窗口同步 + 脏 payload 不信任 + 反向规则同步 | 逻辑探针：A 窗（store 实例 A）保存全局格式 iso-tz / 上海 → 后端已更新 → B 窗监听收到通知 → 重读权威配置 → B 快照与 grid/aside 计划同步变（`2026-01-15T14:30:00Z` → `2026-01-15T22:30:00+08:00`，两处一致）；伪造携带旧格式 iso / UTC 的通知、后端不动 → B 重读一次后仍以权威值（上海）为准；反向 B 窗保存 unix-ms 规则 → A 窗同样更新，B 的 epoch_ms 列命中新规则（`1705316400000` → `2024-01-15T19:00:00+08:00`，`matchedRuleName: 'unix-ms'`） | ✅ 通过 |
| P2 | 并发保存不丢字段 + 规则 last-writer-wins | 逻辑探针：A、B 均从旧基底加载；A 改全局格式、B 改规则，顺序保存（各自重读权威值）→ 后端同时含新格式与新规则，B 再重读即与最终值一致；A 改格式、B 改列覆盖同样无丢失；两窗同时改规则列表 → 后写胜出（`[RULE_SECONDS]`） | ✅ 通过（后者为 coder 申报的已知局限，见 O-14） |
| P2b | 真正重叠的保存（两次重读都在任一写操作前完成） | 逻辑探针：冻结 `saveSettings`，等两次保存都进入写操作（重读已完成、写被冻结）再按 A 先、B 后放行 | ✅ 通过（按设计内已知残余竞态表现：后写 B 整份胜出，B 的 next 基于重读时的旧基底算出，不含 A 的格式；见 §5 O-3，非 bug） |
| P3 | 过期响应防护 | 逻辑探针：两次 `loadSettings` 用可控 deferred promise；新代次先成功、旧代次后到 → 旧响应丢弃（语言/主题/valuePresentation 均保持新值）；旧代次随后失败 → 同样丢弃，不覆盖新表快照 | ✅ 通过 |
| P4 | 刷新失败保持最后有效快照；初次加载前两处用默认快照 | 逻辑探针：初次加载前两处消费者快照 `enabledRendererIds: []`、计划 `disabled`；首次加载成功（revision 1）→ `getSettings` reject 一次 → 快照与 revision 不变；全新 store 实例首次加载即失败 → 回落默认快照（revision 0） | ✅ 通过 |
| P5 | 停用回退 + 重启用恢复 + 停用期间保存新规则胜出 + per-JS-context | 逻辑探针：注册后 grid/aside 就绪 → `unregister('datazen.timestamp')` → 两处同时回退（时间列 `disabled`、`present` 为 `unchanged`/`disabled`；数字列 `default`），store 配置保留原文、`saveSettings` 未调；停用期间保存新规则（全局改回 iso / UTC）→ 两处仍回退；重注册 → 已保存配置恢复生效，epoch_ms 命中停用期间保存的新规则（非旧计划缓存）；另一注册中心实例不受本实例停用的影响 | ✅ 通过 |
| P5-panel | 面板缺失说明随注册/停用实时出现/消失 | 面板探针（真实 Section）：未注册时 `value-presentation-contribution-missing` 可见 → 注册后消失 → 再停用后重现 | ✅ 通过 |
| P6a | 面板草稿保护：脏草稿 + 外部更新 → 提示并保留草稿，Reload 替换 | 面板探针：已应用值 iso / UTC；脏编辑改格式为 locale（预览变本地化文本）→ 另一窗口保存 iso-tz / 上海（经真实通知 + 重读到达）→ `value-presentation-external-update` 出现、预览仍是草稿 locale 文本、保存仍可用 → 点 Reload saved settings → 提示消失、保存变灰、预览变为 `2026-01-15T22:30:00+08:00` | ✅ 通过 |
| P6b | 面板草稿保护：干净草稿自动应用外部更新 | 面板探针：无编辑时另一窗口保存 iso-tz / 上海 → 无提示、预览自动变为 `2026-01-15T22:30:00+08:00`、保存保持禁用 | ✅ 通过 |
| P6c | 面板脏保存只写脏路径（他窗规则保留） | 面板探针：脏编辑改格式为 locale → 另一窗口加 unix-ms 规则（格式仍 iso / UTC）→ 提示出现 → 点保存 → 后端同时含 locale 格式与他窗规则，提示消失、保存变灰 | ✅ 通过（真实 store 全链路，coder 自带测试用 mock store 覆盖同场景） |
| P7 | 未知版本同步不写回 | 逻辑探针：后端 v99（含 rules / columnOverrides / futureField）→ `loadSettings` 后后端原文不动、`saveSettings` 未调；store 内是解析后的值（`{version: 99, renderers}`，与 coder 自带测试的断言一致） | ✅ 通过 |
| P7b | v99 保存全局格式保留未知字段（T10 P10(d) 修复申报核验） | 逻辑探针：v99 权威值上经 store 保存全局格式 → 断言 `futureField` / `rules` / `columnOverrides` 原样保留 | ❌ **失败，见 §4 T11-BUG-001**：三者全部丢失，只剩 `version: 99` 与改过的渲染器 |
| P7c | v99 保存规则不触动其 rules | 逻辑探针：v99 权威值上经 store 保存规则列表 → 断言 rules 与 futureField 不变 | ❌ **失败，见 §4 T11-BUG-001**：`rules` 与 `futureField` 均为 `undefined` |
| P7-panel | v99 保存的面板层复现 | 面板探针：v99 后端上真实 Section 改格式为 locale 并保存 → 后端 version 仍为 99、格式已改，但 rules / columnOverrides / futureField 全部丢失 | ❌ **失败，见 §4 T11-BUG-001**：与 P7b 同一根因的面板层证据 |
| P8 | fix 提交复核：getSettings 返回非 Promise 时权威重读不抛 TypeError | 逻辑探针：同一同步返回值上旧写法 `(value).catch(...)` 实测抛 `TypeError`；新写法（`await` + try/catch）下 `updateValuePresentationSettings` 保存成功，写回值正确 | ✅ 通过（但仓库内无专属回归测试，且生产 `getSettings` 恒为 async，见 O-8/O-9） |
| P9 | Web 覆盖范围：BroadcastChannel 既有端口 + 桌面 Tauri 事件 | 传输探针（真实 `crossWindowBus`，先 `clearPlatform()` 让模块回落 BroadcastChannel）：另一标签页（原生 `BroadcastChannel('datazen-bus')`）发通知 → `listenCrossWindow` 收到 payload；本窗口 `emitCrossWindow` → 另一标签页收到；事件名不对的收不到；另一名字的 channel 不互通；退订后不再收到。桌面侧源码审读：`packages/desktop/src/platform/desktop/eventChannel.ts` 用 `@tauri-apps/api/event` 的 `emit`/`listen`（广播到全部窗口），T11 未新增适配 | ✅ 通过（Web 传输层实测；桌面传输层源码审读，未在真 Tauri 运行时验证） |
| P10 | 保存失败：跨窗口重读失败保持快照；面板保存失败保留草稿与错误、可重试 | 逻辑探针：`saveSettings` reject 一次 → `updateValuePresentationSettings` reject、store 保持已生效快照、revision 不动；随后伪造通知且重读 reject → 快照不变、`saveSettings` 调用次数不再增加。面板探针：`saveSettings` reject 一次 → 就地报 `saveFailed`、草稿预览不变、store 仍为旧快照、无 saved 标记、保存仍可用 → 重试成功，后端已更新、错误消失、保存变灰 | ✅ 通过 |

> 探针总计：**15/18 通过**，3 项失败均为同一 bug 的三处复现（逻辑 P7b/P7c 与面板 P7-panel，见 §4）。
>
> 探针过程说明：首轮逻辑探针 4 项失败中有 3 项为**探针脚手架错误**，代码未动，修正后通过：(1) `vi.hoisted` 返回对象缺 `mocks` 包装层（`Cannot read properties of undefined (reading 'bus')`）；(2) `timestampContribution` 不在 `@datazen/extension-points` 的导出里（应从前端 `lib/valuePresentation/index` 取，P1/P4/P5）；(3) P10 末尾一处错误断言（`saveSettings` 在本测试前半段已被调用过，`not.toHaveBeenCalledWith` 恒失败，改为调用次数不再增加）。面板探针首轮 1 项失败亦为探针期望错误：`saved` 指示在真实 store 下是瞬时的（保存成功后 store 回写触发 effect 的 `setSaved(false)`，指示随即消失；仓库自带测试用 mock store 才看得到它常驻），已改为断言错误消失 + 后端已更新 + 保存变灰，见 O-10。另：G1 首轮的 7 文件 153 项同样是 tester 的路径笔误，见 O-11。

### 2.3 契约类型严格性审读（G6）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| T1 | 改动范围 | `git show` 三提交 `--stat` + 全 diff 通读 | ✅ 基底到 HEAD 的 diff 共 17 个不同文件、+1527/−242（code 9 + fix 1 + test 8 = 18 个文件项，其中 `settingsStore.ts` 在 code 与 fix 两提交中各出现一次），全部在 T11 范围内：code 9 文件（新 1 + 改 8）、fix 1 文件、test 8 文件（新 2 + 改 6）。无无关改动；tasks.md 未动 |
| T2 | any 泄漏 / 断言跳过验证 | grep 三提交新增行 `: any`/`as any`/`<any>`/`any[]`、新测试 `it.skip/test.skip/.todo(/xit(/xdescribe`、另 grep `ts-ignore`/`ts-expect-error`/`eslint-disable`/`TODO`/`FIXME` | ✅ 新增行中 `any` 0 处；新测试 0 处跳过（157 项全数，见 G1）；无 ts 逃逸、无 TODO |
| T3 | code/test/fix 提交分拆 | 三提交 `--name-only` 对照 | ✅ code 提交不含测试文件（`__tests__` 0 处），test 提交仅 8 个测试文件，fix 提交仅 `settingsStore.ts` 1 文件 |
| T4 | fix 提交的必要性与最小性 | fix diff + P8 探针 + `commands/settings.ts:33` 核实 | ✅ 修复真实且最小（+8/−1）：旧 `.catch()` 写法在同步返回值上抛 `TypeError`（P8 实测）；新写法 `await` + try/catch 容忍同步返回值与同步抛错。但仓库内无专属回归测试，且生产 `getSettings` 恒为 async，见 O-8/O-9 |
| T5 | i18n 中英文 key 一致 | grep 两地 `settings.ts` 的 `externalUpdate` / `reloadSaved` + 消费点对照 | ✅ 一致：en 与 zh-CN 各有两 key，文案对应；消费点仅 `ValuePresentationSettingsSection.tsx:583/590` 两处 |
| T6 | Prettier + 空白检查 | `npx prettier --check`（三提交全部 17 个 TS/TSX 文件）+ `git diff --check 75760d5c0..HEAD` | ✅ 通过。Prettier `All matched files use Prettier code style!`；diff-check 无输出 |
| T7 | `updateValuePresentationSettings` 签名迁移的调用方一致性 | 全仓 grep 调用点 | ✅ 通过。T07 面板、T09/T10 对话框、`saveColumnOverride` 均已改 updater 形式；整份对象形式只保留为兼容旧调用（`applyValuePresentationUpdate` 原样返回），无残留的整份对象调用方拼旧基底 |
| T8 | `useSettingsSync` 不再信任广播 payload | diff + P1 脏 payload 探针 | ✅ 通过。回调签名改为无参，`applySettingsLocally(payload)` 调用已删除；`applySettingsLocally` 本身保留供兼容调用方与单测使用，语义不变（注释已注明） |
| T9 | `persistSettings` 抽取的等价性 | diff 审读 + G2 回归 | ✅ 通过。保存 → 权威响应更新本地 store → revision 条件 +1 → 发布 `datazen:settings-changed` 通知的顺序与旧链路一致；主题应用与语言菜单同步仍只留在 `updateSettings`（T11 的 `updateValuePresentationSettings` 不碰主题/语言菜单，符合其职责） |
| T10 | Section 逐路径脏检查的基线选择 | diff 审读 + P6a/P6c 探针 | ✅ 通过。`formatDirty`/`rulesDirty` 以 `appliedStoredJson`（最后一次同步的已保存值）为准，而非当前 store 值；否则会把未动过的路径误判为脏、用陈旧草稿覆盖对方刚保存的字段。外部更新 effect 的“某路径只有与上次同步值不同、且与新已保存值也不同时才算未保存”判断与保存脏检查同一基线，P6a/P6c 均验证 |
| T11 | Dialog 注册表订阅的写法 | diff 审读 | ✅ 通过。`useSyncExternalStore(registry.subscribe, () => registry.revision)` + `useMemo(registry.get, [registry, contributionRevision])`；面板打开期间停用/重启用时缺失提示即时出现/消失（coder 自带测试覆盖；Section 同款订阅由 P5-panel 独立验证） |
| T12 | `withColumnOverride` 在 updater 内的版本/上限复核 | diff + P2 探针 | ✅ 通过。`saveColumnOverride` 把 `(fresh) => withColumnOverride(fresh, …)` 交给 store，版本与 200 上限复核在 store 的保存前重读值上再做一次；P2 的列覆盖并发保存验证无字段丢失 |

## 3. 设计符合性逐条结论

对照设计文档、PRD 与 coder 申报：

**Coder 申报核验**

| 条目 | 结论 |
|------|------|
| 1. 新 `valuePresentationSave.ts`（路径化保存：`ValuePresentationSettingsUpdate` 对象\|updater、`applyValuePresentationUpdate`、`withTimestampRenderer`、`withValuePresentationRules`、`withColumnOverride`） | ⚠️ 大体符合，附 §4 T11-BUG-001：纯函数层对已传入 `fresh` 的未知字段确实原样保留（coder 自带测试逐字验证）；但 store 层在 updater 运行前已用 `parse` 丢弃了 v99 的未知字段，纯函数修补够不到它们（P7b/P7c/P7-panel） |
| 2. `settingsStore.ts`（代次防护的 `loadSettings`、权威重读的 `updateValuePresentationSettings`） | ✅ 符合（P3 代次丢弃、P4 刷新失败保持、P10 保存失败保持、P2 路径合并均独立复核成立；`hasLoadedSettings` 只在从未成功加载过时回落默认，符合状态 1/3） |
| 3. `useSettingsSync.ts`（通知只触发重读，不信任广播 payload） | ✅ 符合（P1 脏 payload 探针：伪造旧格式通知后 B 窗仍以权威值为准；coder 自带测试同场景） |
| 4. `columnOverrides.ts`（`persist` 改 updater 形） | ✅ 符合（P2 列覆盖并发保存无丢失；`withColumnOverride` 的版本/上限复核在 fresh 上重做，见 §2.3 T12） |
| 5. `ColumnPresentationDialog.tsx`（注册表订阅 + updater 启用路径） | ✅ 符合（订阅写法见 §2.3 T11；启用路径改 updater 后规则与列覆盖取权威原文，P2 的启用路径等价性由 store 层测试覆盖；对话框打开期间停用/重启用的缺失提示即时切换由 coder 自带测试覆盖） |
| 6. `ValuePresentationSettingsSection.tsx`（updater 保存、逐路径脏检查、外部更新提示 + “Reload saved settings”） | ✅ 符合（P6a/P6b/P6c 全链路独立复核：脏草稿保留 + 提示 + Reload 替换；干净草稿自动应用；脏保存只写脏路径） |
| 7. locales 中英文 `settings.ts`（`settings.valuePresentation.externalUpdate` / `reloadSaved`） | ✅ 符合（§2.3 T5：两地 key 一致，文案对应） |
| 8. fix 提交容忍 `getSettings` 返回非 Promise | ✅ 行为符合（P8：旧写法在同步返回值上实测抛 `TypeError`，新写法保存成功）；但仓库内无专属回归测试，且生产路径恒为 async，见 O-8/O-9 |

**设计 §6（配置加载、持久化与同步，与 T11 相关项）**

| 条目 | 结论 |
|------|------|
| 配置写操作用路径化更新/版本条件更新避免跨窗口覆盖整个旧快照 | ⚠️ 部分符合：路径化更新已落地（v1 下 A 改格式、B 改规则/覆盖互不覆盖，P2/P6c）；版本条件更新未做——`saveSettings` 是整份写、无修订号/ETag，需要内核 + backend-client + Rust 三层新契约，超出本任务范围（coder 在 `valuePresentationSave.ts` 文件头裁决 1/3 已写明）。残余重读→保存竞态窗口实测存在，见 O-3 |
| 成功保存后发布作用域与配置修订通知，接收端重新读取权威配置；同窗口从权威响应更新 | ✅ 符合（P1 双向：A 保存 → B 通知 → B 重读 → B 快照/计划更新；反向亦然；保存方 store 由 `persistSettings` 以权威响应更新） |
| 加载状态 1：初次加载中 cell 与 aside 使用同一个默认快照 | ✅ 符合（P4：初次加载前两处 `enabledRendererIds: []`、计划 `disabled`） |
| 加载状态 2：全局与当前表配置全部就绪，一次发布新快照，生成计划 | ✅ 符合（P1：B 重读后快照与两处计划一次同步变） |
| 加载状态 3：后续刷新继续使用最后一份有效快照，成功后整体替换 | ✅ 符合（P4：刷新失败保持快照与 revision；T06 报告 §3 提到当时未单独立项验证的保持语义，本次由 P4/P10 补验证） |
| 加载状态 4：切换上下文，以面板来源和请求代次校验异步响应，旧表响应不得覆盖新表 | ✅ 符合（P3：代次校验丢弃旧响应与旧失败；此处是设置加载的同构问题，见 `settingsStore.ts` 注释） |
| 加载状态 5：保存失败保持已生效快照，编辑面板保留草稿与错误 | ✅ 符合（P10 逻辑层与面板层；T06 报告 §3 提到当时未单独立项验证的保持语义，本次补验证） |
| 历史配置缺字段默认关闭 | ➖ 不在 T11 改动范围：T06 既有行为，未动 |
| 未知配置版本或暂时缺失贡献时保留原文，不用默认值写回覆盖 | ⚠️ 部分符合：同步路径符合（P7：v99 同步不写回，后端原文不动；P5：停用时配置保留原文、不写回）；**保存路径不符合**，见 §4 T11-BUG-001（v99 保存丢 rules / columnOverrides / 未知顶层字段） |
| 纯迁移由贡献声明，持久化由宿主执行，迁移失败可重试 | ➖ 不在 T11 范围 |

**设计 §3 / §7（与 T11 相关项）**

| 条目 | 结论 |
|------|------|
| §3：注册、卸载触发订阅，增加宿主维护的贡献版本，令所有相关计划失效 | ✅ 符合（P5：unregister 后两处计划即时回退；重注册后新规则生效、无旧缓存残留；Dialog 与 Section 均订阅注册表 revision） |
| §3：贡献缺失时保留配置，使用默认展示，配置面板说明缺失原因 | ✅ 符合（P5：配置保留原文、不写回；P5-panel：Section 缺失说明随注册/停用实时出现/消失；Dialog 同款由 coder 自带测试覆盖） |
| §7：计划缓存键至少包含来源身份、列元数据版本、配置修订、贡献修订、locale、localTimeZone | ✅ 符合（P5：贡献修订变化后计划重建、停用期间保存的新规则胜出，无旧缓存残留；配置修订由 `valuePresentationRevision` 携带，见 `useValuePresentationSnapshot`） |

**PRD §5/§6（与 T11 相关项）**

| 条目 | 结论 |
|------|------|
| §5：停用扩展时 cell 与 aside 同时回退，重启用后恢复已保存配置 | ✅ 符合（P5 两处同时回退、配置保留、重启用恢复；停用期间保存的新规则胜出） |
| §5：未加载完整配置时两处统一使用默认展示；刷新时保留上一份有效配置，不能半更新 | ✅ 符合（P4） |
| §5：保存失败应明确提示，不能显示成已保存；旧配置继续生效 | ✅ 符合（P10 逻辑层与面板层：就地报错、无 saved 标记、旧配置继续生效、草稿可重试成功） |
| §6 验收 10：配置保存失败、扩展缺失、格式化异常均可恢复，不阻断网格交互 | ✅ 符合（P10 保存失败恢复；P5 扩展缺失回退且不写回；格式化异常由贡献层 `unchanged` 兜底，属 T02–T04 范围） |
| §6 验收 12：注册、停用、重新启用时间贡献后，两处同步更新且无旧缓存残留 | ✅ 符合（P5：注册/停用/重启用三态下两处计划均同步，停用期间保存的新规则胜出，无旧缓存残留） |

## 4. Bug 列表

| 编号 | 标题 | 级别 | 状态 |
|------|------|------|------|
| T11-BUG-001 | v99 配置经 store 保存后，未知版本字段（rules / columnOverrides / 未知顶层字段）被丢弃，只剩 version 与改过的渲染器 | 低 | 已关闭（回归验证见 §8） |

**T11-BUG-001 详情**

- 位置：`packages/frontend/src/stores/settingsStore.ts:380–394`（`updateValuePresentationSettings` 的保存前重读链路）+ `packages/frontend/src/lib/valuePresentation/settings.ts:124–125`（`parseStoredValuePresentationSettings` 对 `version !== 1` 只返回 `{ version, renderers }`）。
- 引入：code 提交 `327824753`（T11 新保存链路；T08/T10 的整份重写路径有同类丢弃行为，见 T10 报告 O-3，但 T11 申报称已修、任务 P7 要求核验该修复——本次核验表明 store 层未修）。
- 现象：后端权威值为 v99（含 `rules: [{name: 'future-rule'}]`、`columnOverrides`、`futureField: {kept: true}`）时，经 store 保存全局格式（或规则）后，写回值保持 `version: 99` 且改过的渲染器正确，但 `rules`、`columnOverrides`、`futureField` 全部丢失（`undefined`）。设置页面板层同样复现（P7-panel）。
- 根因（源码核实）：`updateValuePresentationSettings` 先调 `readAuthoritativeSettings()`，后者经 `normalizeLoadedSettings` 调 `parseStoredValuePresentationSettings`——该函数对 `version !== 1` 直接返回 `{ version, renderers }`（`settings.ts:124–125`，T06 既有行为：未知版本“不解析 rules / columnOverrides”，只保留 version 值）；随后 `parseStoredValuePresentationSettings(base.valuePresentation)` 再解析一次，`updater` 拿到的 `fresh` 里已无未知字段。`withTimestampRenderer` 的 `{ ...fresh }` 展开只能保留 `fresh` 里**已有**的字段，够不到已被 parse 丢弃的字段——coder 自带测试的 v99 用例（`valuePresentationSave.test.ts:126–135`）传入的是手工构造的、已含 `futureField` 的 `fresh`，只证明了纯函数层的展开语义，未覆盖 store 层的 parse 前置丢弃。`withValuePresentationRules` 对 v99 返回 `fresh` 本身（不写 rules）同样救不了已丢的字段：保存后写回的仍是 parse 后的残值。
- 复现（逻辑层，探针 P7b）：
  ```ts
  // 后端权威值：{ version: 99, renderers: { 'datazen.timestamp': {...iso/UTC } },
  //   rules: [{ name: 'future-rule' }], columnOverrides: { '["conn-1",...]': { mode: 'raw' } },
  //   futureField: { kept: true } }
  await store.updateValuePresentationSettings((fresh) =>
    withTimestampRenderer(fresh, true, FORMAT_SHANGHAI));
  // 实际：写回值 version 99、格式 iso-tz 正确，但 futureField / rules /
  //      columnOverrides 均为 undefined。
  // 预期：三者原样保留（设计 §6「保留原文」）。
  ```
  复现（逻辑层，探针 P7c）：v99 权威值上保存规则列表 → 写回值 `rules` 与 `futureField` 均为 `undefined`（`withValuePresentationRules` 对 v99 应原样返回 fresh，但 fresh 已被 parse 剥离）。
  复现（面板层，探针 P7-panel）：v99 后端上真实 Section 改格式为 locale 并保存 → 后端 version 99、格式 locale 正确，`rules` / `columnOverrides` / `futureField` 全部丢失。
- 影响范围：**只影响未来版本配置（version ≠ 1）**。当前版本恒为 1，野外不存在 v99 配置；v1 下路径化保存保留其他字段已验证（P2/P6c）。触发条件是“未来版本写过的高版本配置被本版本打开并保存”——属降级/跨版本场景，静默删除未来版本的规则与列覆盖。定为**低**：违背设计 §6「未知版本保留原文」与 T11 修复申报，但无当前版本用户可见影响；修复面小（见下）。
- 修复建议（未实施，tester 不改代码）：让 updater 作用于**未解析的权威原文**而非 parse 后的残值。最小改法：在 `updateValuePresentationSettings` 中，`version !== 1` 时以权威响应里的原始 `valuePresentation` 对象作 `fresh`（三个 helper 对未知版本均安全：`withTimestampRenderer` 只做展开替换、`withValuePresentationRules` 对非 v1 原样返回 fresh、`withColumnOverride` 对非 v1 返回同一对象）；`version === 1` 时继续用 parse 后的值（需要其规范化）。或在 `readAuthoritativeSettings` 旁保留一份 raw 副本供保存链路使用。修复后以探针 P7b/P7c/P7-panel 三处用例回归，并把逻辑层用例吸收为仓库回归测试（P7b/P7c 各一）。
- 注意：同步路径不受影响（P7 通过：`loadSettings` 只读不写，后端 v99 原文不动）；v1 保存路径不受影响（P2/P6c 通过）；`version: 99` 本身与改过的渲染器写回正确。

本次验证未发现其他 bug。既有门禁 G1–G5、G7 全部通过，G6 除 BUG-001 外全部通过；对抗探针除 BUG-001 的三处复现外全部通过。

## 5. 观察项（非缺陷，供 coder / 后续任务参考）

- **O-1（G2 申报 102/1237 vs 实测 99/1222，范围口径不同）**：任务指定 G2 命令实测 99 文件 1222/1222（磁盘清点：windows/settings 29 + valuePresentation 12 + DataTable 56 + settingsStore 1 + useSettingsSync 1 = 99，与 vitest 文件数一致，无静默跳过）。coder 的 102/1237 恰为实测口径 + 三个值呈现 hook 测试文件（`useValuePresentation` 6 + `useValuePresentationSnapshot` 6 + `useColumnPresentationSession` 3 = 15 项），三者已独立验证全过（4 文件 19/19，含 `valuePresentationSync` 4 项）。两数可互相算出，回归本身全绿，无未解释失败；后续申报写明口径即可。属 T10 O-1 同类出入。
- **O-2（G5 覆盖率小数点后两位出入）**：settingsStore 行 91.52 vs 申报 91.53、对话框分支 85.46 vs 85.47 / 函数 88.63 vs 88.64、Section 函数 91.17 vs 91.18，各 −0.01，属覆盖运行舍入级出入，均 ≥80%，无结论影响。
- **O-3（重读→保存间仍有竞态窗口——设计内已知残余）**：探针 P2b 实测：两次保存都完成重读（后端仍是基底）后再按 A 先、B 后放行写操作 → 后写 B 整份胜出，A 的格式丢失。根因是 `saveSettings` 无修订号/ETag，`valuePresentationSave.ts` 文件头裁决 3 已明确写明此残余窗口（“只把窗口从整个本地基底陈旧缩到重读→保存的毫秒级间隔”）。按设计内已知局限记为观察项，非新 bug；彻底关闭需后端版本条件更新（三层新契约，超出 T11 范围）。
- **O-4（通用 `updateSettings` 整包竞态未覆盖）**：T11 的重读 + updater 只覆盖了 `updateValuePresentationSettings` 路径；通用 `updateSettings(partial)` 仍从本窗口本地 store 合并整份后保存，两窗口同时改不同通用字段时仍可能互相覆盖。属既有行为，非 T11 回归；若后续要收口，需对通用路径做同类改造或文档注明。
- **O-5（对话框无外部更新提示）**：外部更新提示 + Reload 只在设置页 Section 有；`ColumnPresentationDialog` 打开期间他窗保存后，对话框不提示、仍以打开时的快照为准（保存时走 updater，只改本列路径，不会覆盖他窗字段，故不丢数据，但用户看不到他窗的新值直至重开面板）。属已知不对称，非缺陷；T12 文档或后续任务可决定是否补齐。
- **O-6（停用是 per-JS-context 的）**：注册中心是每个窗口/JS 上下文一份实例；A 窗 `unregister` 不影响 B 窗的注册表（P5 探针实测：另一实例的 `get('datazen.timestamp')` 仍在）。PRD 验收 12 的“两处同步更新”指同一窗口内的 grid 与 aside 两处，不含跨窗口停用同步；跨窗口停用同步不在设计 §3/§6 的通知范围（通知只同步配置，不同步注册表状态）。记为范围澄清。
- **O-7（T07-Q1 已裁决关闭，coder 的“仍待决”说法过时）**：T07-Q1（带首尾空格的 IANA 名被 `trim()` 后保存）已由协调方在 T07 收尾时裁决为**接受 trim 行为并关闭**，不是新的待决项。T07 测试报告正文 §4 表格里仍写“待裁决”是报告成文时的状态，未随裁决更新（不在本次验证范围内）。本次报告特此注明，勿将其当作 T11 的遗留项。
- **O-8（fix 提交无专属仓库回归测试）**：仓库内没有任何测试让 `getSettings` 返回非 Promise（全仓 grep：全部 `mockResolvedValueOnce`/`mockReturnValueOnce(Promise)`，仅 stale-response 测试用真实 Promise）。fix 的行为由本次 P8 独立探针验证，建议把 P8 用例吸收为 `settingsStore.test.ts` 的回归测试（`mockReturnValueOnce(plainObject)` + 保存成功断言）。
- **O-9（fix 的场景只在 mock/契约偏离层）**：生产 `getSettings` 是 `async` 函数（`commands/settings.ts:33`），恒返回 Promise；非 Promise 返回值只可能来自手写 mock 或未来契约偏离。fix 是防御性加固（+8/−1，最小），不改变任何生产路径行为。
- **O-10（`saved` 指示在真实 store 下是瞬时的）**：面板探针 P10 首轮失败为探针期望错误：`{saved && !isDirty}` 的指示在真实 store 下保存成功后随 store 回写触发的 effect（`setSaved(false)`）而消失；仓库自带测试用 mock store（store 永不回写）才看得到它常驻。行为本身符合设计（保存成功即回写、无脏即干净），探针已改为断言错误消失 + 后端已更新 + 保存变灰。
- **O-11（G1 首轮路径笔误，tester 侧）**：首轮 G1 命令把 `valuePresentationSync.test.tsx` 写在 `lib/valuePresentation/__tests__/` 下（该路径不存在，vitest 静默只跑了其余 7 文件、153 项）；修正为 `hooks/__tests__/` 后重跑 8 文件 157/157，与 coder 申报一致。属 tester 命令错误，非代码问题；G1 数字以修正版 `~/workspace/t11-g1b.log` 为准。
- **O-12（未覆盖行清单）**：`settingsStore.ts` 未覆盖行（125, 126, 133, 134, 137, 317, 322, 325, 328, 340）为主题恢复分支、旧后端缺字段回落与 `applySettingsLocally` 兼容路径等防御分支；对话框未覆盖行（182, 184, 215, 250, 334, 346, 413, 424, 426, 429, 457, 458, 496, 617, 618, 721）中 `:457–458`（列保存 `handleSave` 的外层 catch）与 `:496`（默认开启路径的 catch）均为 T10 O-6 已列的既有防御分支（当时行号 `:446–447`/`:493`，因 T11 插入平移）；T11 在对话框改的订阅与 updater 启用路径均有测试/探针覆盖，无新增行为缺口；Section 未覆盖行（226, 282, 434, 436, 439, 444, 446, 449, 463, 465, 468, 472, 473, 532, 536, 545, 551, 564, 799, 845）以规则编辑器分支与防御分支为主，T11 新增的外部更新 effect 与逐路径脏检查均有探针覆盖（P6a/P6b/P6c）。`useSettingsSync.ts` 唯一未覆盖函数是 `:30–32` 的 listen-reject `.catch` 回调。建议接受探针覆盖，不阻塞 T11。
- **O-13（G1/G5 stderr 的 `common.close` 缺译警告）**：与 T09 O-7 / T10 O-7 同一既有模式（`@datazen/ui` 自身注册表，单元测试环境未注册该 key），非 T11 引入；不影响测试结果（退出码 0）。
- **O-14（规则列表 last-writer-wins——coder 申报的已知局限）**：规则列表整体是一条路径，无单条身份可做逐条合并；两窗同时改规则仍是后写胜出（P2 实测）。面板在收到他窗更新时若有未保存的规则编辑会提示、由用户决定是否重新加载（P6a），属设计内行为。
- **O-15（探针资产与回归建议）**：`~/workspace/t11-probe` 为一次性验证项目，已按任务要求删除（探针文件与 vitest 配置随之清除；仓库 `git status` 复核干净，除本报告外无新增文件）。本报告 §2.2 各项方法已描述到可复现程度，日志见 `~/workspace/t11-p-*.log` 与 `~/workspace/t11-g*.log`。建议将 P1（双窗口 + 脏 payload + 反向规则同步）、P2b（重叠保存的残余竞态）、P3（代次丢弃 + 旧失败丢弃）、P6a/P6c（真实 store 全链路的草稿保护与路径化保存）、P8（非 Promise `getSettings`）与 P9（BroadcastChannel 双向）吸收为仓库回归测试；P7b/P7c/P7-panel 三处用例在 T11-BUG-001 修复后应转为通过。

## 6. 结论

**通过，可做收尾提交（回归验证见 §8：T11-BUG-001 已关闭）。**

以下为首轮验证（2026-11-12，HEAD `4ae7648a9`）的结论记录，数字保持原样；修复后的最终结论见 §8。

- 既有门禁：T11 定向 157/157（9+4+37+4+32+25+2+44）、接线回归 1222/1222（99 文件；coder 申报的 102/1237 为含三个 hook 测试文件的宽口径，见 O-1）、双引擎类型通过、命名门禁通过、Prettier 与 diff-check 全过；新文件覆盖率全部 ≥80%（valuePresentationSave.ts 100/100/100/100、columnOverrides.ts 100/100/100/100、useSettingsSync.ts 100/100/83.33/100、settingsStore.ts 91.52/88/84.21/95.23、对话框 90.24/85.46/88.63/92.61、Section 91.03/91.05/91.17/92.57），与 coder 申报逐项一致（附 O-2 的舍入级出入）；三提交无任何 Rust 改动，G7 通过。
- 对抗性探针 15/18 通过：P1 双窗口同步（脏 payload 不信任、反向规则同步）、P2 并发保存不丢字段（规则 last-writer-wins）、P2b 重叠保存的残余竞态（设计内已知）、P3 过期响应防护、P4 刷新失败保持快照与初次加载默认快照、P5 停用回退与重启用恢复（per-JS-context）、P5-panel 面板缺失说明实时切换、P6a/P6b/P6c 面板草稿保护与路径化保存、P7 同步不写回、P8 fix 复核、P9 Web BroadcastChannel 双向传输、P10 保存失败保持与重试，全部通过。3 项失败均为同一 bug 的三处复现（逻辑 P7b/P7c 与面板 P7-panel）。
- 设计符合性：设计 §6 的路径化更新、通知 + 权威重读、加载状态 1–5、§3 的注册/卸载订阅与计划失效、§7 的计划缓存键，PRD §5 的停用回退与 §6 验收 10/12，均符合；唯“未知版本保留原文”在**保存路径**不符合（§4）。
- 唯一 bug 是 T11-BUG-001（低）：v99 配置经 store 保存后，未知版本字段（rules / columnOverrides / 未知顶层字段）被丢弃，只剩 version 与改过的渲染器。根因是保存前重读的 `parseStoredValuePresentationSettings` 对 `version !== 1` 只返回 `{version, renderers}`，updater 拿到的 `fresh` 里已无未知字段；coder 自带测试的 v99 用例只覆盖纯函数层，未覆盖 store 层的 parse 前置丢弃。只影响未来版本配置（当前版本恒为 1），同步路径与 v1 保存路径均不受影响。
- 观察项 15 条均为非缺陷（O-1 G2 口径、O-2 覆盖率舍入、O-3 残余竞态窗口、O-4 通用 `updateSettings` 整包竞态、O-5 对话框无外部更新提示、O-6 停用 per-JS-context、O-7 T07-Q1 已裁决关闭、O-8 fix 无专属回归测试、O-9 fix 场景只在 mock 层、O-10 `saved` 指示瞬时性、O-11 G1 路径笔误、O-12 未覆盖行清单、O-13 `common.close` 警告、O-14 规则 last-writer-wins、O-15 探针资产与回归建议）。
- 回归验证标准（修复后）：P7b/P7c/P7-panel 三处用例通过（v99 保存保留 rules / columnOverrides / 未知顶层字段，version 仍为 99）；G1（157/157）、G2（1222/1222）、G3 双引擎、G5（各文件 ≥80%）重跑全绿；其余探针结论不变。tasks.md 状态更新与 docs close commit 不在本次验证范围内，由协调方执行。

## 7. 专项：T11-BUG-001 根因、证据与影响面

### 7.1 现象

后端权威值为 v99（含 rules、columnOverrides 与未知顶层字段）时，经 store 保存全局格式或规则后，写回值保持 `version: 99` 且改过的渲染器正确，但 rules、columnOverrides 与未知顶层字段全部丢失。设置页面板层同样复现。

### 7.2 根因（源码核实）

`settingsStore.ts` 的 `updateValuePresentationSettings`（`:380–394`）：

- `:385` 调 `readAuthoritativeSettings()`，后者经 `normalizeLoadedSettings`（`:221–233`）调 `parseStoredValuePresentationSettings(raw.valuePresentation)`；
- `settings.ts:124–125`：`if (version !== 1) return { version, renderers };`——对未知版本只保留 version 值与渲染器条目，rules / columnOverrides / 未知顶层字段在 parse 时即被丢弃（T06 既有行为，文件头写明“未知顶层 version 原样保留 version 值、不解析 rules / columnOverrides”）；
- `:387` 再对 `base.valuePresentation` 解析一次，`:388` 的 `applyValuePresentationUpdate(fresh, update)` 拿到的 `fresh` 已是残值；
- `withTimestampRenderer` 的 `{ ...fresh }` 展开（`valuePresentationSave.ts:65–77`）与 `withValuePresentationRules` 的 v99 原样返回（`:83–89`）都只能处理 `fresh` 里**已有**的字段，够不到已被 parse 丢弃的字段。

对照：v1 下同一链路正确保留其他字段（P2/P6c：parse 对 v1 保留 rules / columnOverrides，updater 只改拥有路径）；同步路径（`loadSettings`）只读不写，后端 v99 原文不动（P7 通过）。漏的只是**保存路径上未知版本的原文保留**。

### 7.3 影响面（实测分层）

| 层 | 行为 | 结果 |
|----|------|------|
| 同步（`loadSettings`） | v99 后端原文不动，`saveSettings` 未调；store 内为解析值 `{version: 99, renderers}` | ✅ 正确（P7；与 coder 自带测试断言一致） |
| 保存全局格式（v99） | version 仍为 99、格式已改；`futureField` / `rules` / `columnOverrides` 均为 `undefined` | ❌ 错误（P7b；面板层 P7-panel 同样） |
| 保存规则列表（v99） | version 仍为 99；`rules` 与 `futureField` 均为 `undefined` | ❌ 错误（P7c） |
| 保存列覆盖（v99） | 同一链路，`withColumnOverride` 对非 v1 返回同一对象，但 fresh 已被 parse 剥离 | ❌ 同根因（未单独设探针，链路与 P7b 相同） |
| v1 保存 | 路径化保存保留其他字段 | ✅ 正确（P2/P6c） |
| 触发条件 | 未来版本写过的高版本配置被本版本打开并保存 | 降级/跨版本场景；当前版本恒为 1，野外无 v99 配置 |

结论：BUG-001 只断保存路径上未知版本的**原文保留不变量**（设计 §6），不影响同步、不影响 v1、不写错当前版本数据。定为低级：它违背设计 §6 与 T11 的修复申报，且是静默删除未来版本用户配置的行为，但触发面限于尚不存在的高版本配置，修复面小（§4 修复建议：`version !== 1` 时以权威响应里的原始 `valuePresentation` 对象作 updater 的 `fresh`）。修复后以 P7b/P7c/P7-panel 三处探针用例回归，并建议把逻辑层用例吸收为仓库回归测试。

## 8. 回归验证记录（T11-BUG-001 修复）

2026-11-12，tester 对 T11-BUG-001 的修复做了聚焦回归验证。修复两笔提交：fix `4308d5986`（`fix(issue-32): keep unknown-version valuePresentation fields on save (T11-BUG-001)`）与 test `02432fba8`（`test(issue-32): unknown-version save regression tests (T11-BUG-001)`）。验证开始时工作区 HEAD 即 `02432fba8`（`git log -4`：`02432fba8` / `4308d5986` / `4ae7648a9` / `0a9eee192`，分支 `feature/issue-32-value-presentation`），除本报告未提交外工作区干净；tester 未改任何被测代码，唯一可写文件为本报告。

### R1：修复 diff 审读 ✅ 通过

- `git show 4308d5986`：1 文件、+44/−5，仅 `packages/frontend/src/stores/settingsStore.ts`。`readAuthoritativeSettings` 返回值改为 `{ settings, rawValuePresentation }`（`:247–266`），同时带回归一化后的完整设置与 valuePresentation 原文；新增类型谓词 `isUnknownVersionValuePresentation`（`:268–279`）；`updateValuePresentationSettings`（`:414–425`）在未知版本时以权威响应里的原始 valuePresentation 对象作 updater 的 `fresh`，其余走 parse。无无关文件、无 Rust 改动（两提交文件清单 grep `.rs` 0 处）。
- `git show 02432fba8`：2 文件、+300/−0，仅新增回归测试，无生产改动：`settingsStore.test.ts` +117 行（3 项：保存渲染器 P7b、保存规则 P7c、保存列覆盖各一）与新文件 `ValuePresentationSettingsSectionStore.test.tsx` 183 行（1 项：面板层 P7-panel）。
- 两提交全文审读无 `any` 新增（grep `: any` / `as any` / `<any>` / `any[]` 0 处）；`settings.ts` 的 parse 本身未动（不在两提交文件清单内）；`git diff --check 4ae7648a9..02432fba8` 干净；Prettier 检查三文件全过。
- 谓词语义核实（源码）：只有带**数字** version 且 `!== 1` 的记录才走原文；version 缺失 / 非数字 / 非记录 / 数组均返回 false，走 parse 的宽容解析（缺 version 即按 v1 处理，与 T06 既有行为一致）。`version: "99"`（字符串）同样走 parse——属既有宽容语义，未被本次修复改变。
- v1 路径不受影响（源码核实）：`version === 1` 时仍用 parse 后的值（需要其宽容解析语义：非法条目丢弃等）；三 helper 对未知版本均安全——`withTimestampRenderer` 只做 `{ ...fresh }` 展开替换（`valuePresentationSave.ts:65–77`）、`withValuePresentationRules` 对非 v1 原样返回 fresh（`:83–89`）、`withColumnOverride` 对非 v1 返回同一对象（`columnOverrides.ts:141`）。T06/T08/T10 的未知版本保留原文规则（parse 层与同步层）未动：`settings.ts:124–125` 原样保留，`loadSettings` 仍走 `normalizeLoadedSettings` 直读（`settingsStore.ts:342`），`readAuthoritativeSettings` 的唯一调用方是 `updateValuePresentationSettings`（`:414`），同步路径只读不写不受影响。
- 重读失败 / 权威响应无 valuePresentation 字段时回落不变：`authoritative` 为 null 时 `rawValuePresentation` 为 `undefined`，谓词返回 false，走本地基底的 parse，与修复前一致。
- 一处有意保留的既有行为（非回归缺口）：`persistSettings` 仍把保存响应经 parse 写回 store 内副本（`settingsStore.ts:317`），故 v99 下 store 内副本仍是 parse 后的 `{ version: 99, renderers }`，丢的是内存副本的未知字段，不是后端写回值；新 store 测试第一项已显式断言此点，与 T06 既有行为一致。

### R2：首轮失败探针 P7b/P7c/P7-panel 独立重跑 ✅ 通过

按本报告 §2.2 / §4 的复现步骤独立重写探针重跑（新临时项目 `/tmp/t11-reg-probe/`，`node_modules` 符号链接复用仓库依赖，`environment: jsdom`，被测源码绝对路径导入，`vi.mock` 同样用绝对路径拦截 `commands/settings` / `crossWindowBus` / `themePackApply` / `resolveUiLanguage` / `useI18n`，不污染仓库工作区；探针按惯例用后删除，日志 `~/workspace/t11-reg-p.log`），不只跑仓库新测试。夹具取自首轮 §4（v99：`unix-ms` 规则 + `conn-1/shop/public/orders/epoch_ms` raw 覆盖 + `futureField: { kept: true }`）。

- **P7b 逻辑层**：v99 权威值上经 store 保存全局格式 iso-tz / 上海 → 写回值 version 99、格式 iso-tz 正确，`rules` / `columnOverrides` / `futureField` 全部原样保留。✅ 通过。
- **P7c 逻辑层**：v99 权威值上经 store 保存规则列表（换名 `other-rule`）→ 写回值 version 99，`rules` 仍为原文 `[T11_RULE]`（`withValuePresentationRules` 对非 v1 不写规则，符合设计），`columnOverrides` / `futureField` 原样保留。✅ 通过。
- **P7b-col 逻辑层（附加）**：v99 权威值上经 store 保存一列覆盖 → 写回值与权威原文逐字相等（`withColumnOverride` 对非 v1 原样返回 fresh）。✅ 通过。
- **v1 对照（附加）**：v1 权威值上保存全局格式 → 写回值 version 1、格式 iso-tz 正确，`rules` / `columnOverrides` 保留，且无 `futureField`（parse 路径未变）。✅ 通过。
- **P7-panel 面板层**：真实 store + 真实 Section，v99 后端上改格式为 locale 并保存 → 写回值 version 99、格式 locale 正确，`rules` / `columnOverrides` / `futureField` 全部原样保留。✅ 通过。
- 探针合计 **5/5 通过**，退出码 0。探针过程说明：首版探针因 Vite fs allowlist 拒绝工作区外的 `setup.ts` 与驱动 svg（`Denied ID …postgresql.svg?url`，套件级失败、0 测试执行），属探针脚手架问题，非代码问题；在探针 vitest 配置加 `server.fs.allow` 指向工作区后通过（另 setup 文件改由探针内中转文件导入）。运行期 stderr 有一条 `common.close` 缺译警告，与首轮 O-13 同一既有模式，非本次引入。

### R3：G1 定向全集 ✅ 通过

`npx vitest run packages/frontend/src/lib/valuePresentation/__tests__/valuePresentationSave.test.ts packages/frontend/src/hooks/__tests__/valuePresentationSync.test.tsx packages/frontend/src/stores/__tests__/settingsStore.test.ts packages/frontend/src/hooks/__tests__/useSettingsSync.test.tsx packages/frontend/src/lib/valuePresentation/__tests__/columnOverrides.test.ts packages/frontend/src/components/DataTable/__tests__/ColumnPresentationDialog.test.tsx packages/frontend/src/components/DataTable/__tests__/ColumnPresentationEntries.test.tsx packages/frontend/src/windows/settings/__tests__/ValuePresentationSettingsSection.test.tsx packages/frontend/src/windows/settings/__tests__/ValuePresentationSettingsSectionStore.test.tsx --retry=0`（`valuePresentationSync.test.tsx` 取正确路径 `hooks/__tests__/`，与首轮 O-11 的笔误不同）：退出码 0，**9 文件 161/161 通过**（valuePresentationSave 9、valuePresentationSync 4、settingsStore 40、useSettingsSync 4、columnOverrides 32、对话框 25、入口 2、Section 44、新面板文件 1；日志 `~/workspace/t11-reg-g1.log`）。与修复人申报 161/161 一致：较首轮 157/157 净增 4 项，恰为 4 条新回归测试（settingsStore 37 → 40、新面板文件 1）。

### R4：双引擎类型门禁与命名门禁 ✅ 通过

- `npx -y pnpm@12.18.0 typecheck`（tsrs 0.18.0-ts7.1.0-dev.20260929）：退出码 0，`OK`（日志 `~/workspace/t11-reg-typecheck.log`）。
- `npx -y pnpm@12.18.0 typecheck:tsc`（tsc 7.0.2）：退出码 0，`OK`（日志 `~/workspace/t11-reg-typecheck-tsc.log`）。
- `node scripts/check-test-module-naming.mjs`：退出码 0，`ok`（1837 Rust 文件、256 cfg(test) 模块；2592 TS 文件、8 wdio spec 树；日志 `~/workspace/t11-reg-naming.log`）。TS 文件数较首轮的 2591 净 +1，恰为新面板测试文件。

**新回归测试「无修复时失败」的复核（未重做还原，代码审读方式）**：修复人称已临时还原验证 4 条新测试在无修复时失败。本次复核未重做还原，改为读测试代码确认其断言确会区分修复前后。Store 渲染器回归：无修复时 fresh 为 parse 后的 `{ version: 99, renderers }`，`withTimestampRenderer` 的展开只能保留 fresh 已有字段，写回值无 rules / columnOverrides / futureField，三处 `toEqual` 断言必失败（version 与新格式两处断言与修复无关、仍通过）。Store 规则回归：无修复时 `withValuePresentationRules` 原样返回 parse 后的残值 fresh，`rules` 为 `undefined`，`toEqual([T11_RULE])` 必失败。Store 列覆盖回归：无修复时 `withColumnOverride` 原样返回 parse 后的残值，`toEqual(authoritative)` 必失败（缺 rules / columnOverrides / futureField）。面板回归：与首轮 P7-panel 的失败是同一用例（真实 store + 真实 Section，v99 改格式保存），首轮实测已证无修复时三字段全部丢失，故其三处 `toEqual` 断言在无修复时必失败。4 条测试的关键断言均与修复行为直接挂钩，无修复时不可能通过；且首轮 §4/§7 的 P7b/P7c/P7-panel 实测（无修复时三字段全部丢失）是同一失败的独立证据。故修复人的临时还原说法可信。

### R5：Rust 侧局限核实 ✅ 属实，记为观察项（非 T11 新 bug）

修复人提到的 Rust 侧局限属实（源码核实，`crates/datazen/src/settings/domain.rs:66–77`）：`ValuePresentationSettings` 只有 `version` / `renderers` / `rules` / `column_overrides` 四个已知字段，`#[serde(rename_all = "camelCase")]`，**无 `#[serde(flatten)]` 兜底**（全文件 grep `flatten` 0 处）；serde 默认忽略未知字段、序列化时只写出已知字段。因此真正未知的顶层字段（如 `futureField`）在真实桌面路径上仍会被丢弃：前端 `toSharedSettings` 虽原样透传 valuePresentation（`packages/backend-client/src/settings.ts:295–296`，注释称未知版本原样保留），但 Tauri 命令反序列化进 Rust struct 时未知顶层字段即被剥离，写盘后再读回即消失。`rules` 与 `columnOverrides` 是已知字段（不透明 JSON），不受此限；受限的只是未来版本新增的顶层字段。另：该 struct 的文档注释称“未知版本原样保留”（`:40–44`），对未知渲染器/未知版本值本身成立，但对未知顶层字段的说法过宽，见下观察项。

不判为 T11 的新 bug：T11 无 Rust 改动；本次回归测试与 tester 探针用的都是 mock 后端，只验证写 payload（前端发出的 JSON 确已含 `futureField`，R2/R3 已证）；真实后端 round-trip 不在 T11 验收范围内。建议作为后续任务：给 `ValuePresentationSettings` 加 `#[serde(flatten)] extra: serde_json::Map<String, serde_json::Value>` 兜底（并同步 backend-client 的透传类型与文档注释），或在设计文档中明确“未知顶层字段不保证跨 Rust round-trip 保留”，二选一；当前版本恒为 1，野外无 v99 配置，无 urgency。

**T11-BUG-001 验证通过并关闭。**结论更新为「通过，可做收尾提交」（见 §6）。未发现修复引入的新问题；探针项目已按惯例删除（`/tmp/t11-reg-probe/`），日志见 `~/workspace/t11-reg-p.log` 与 `~/workspace/t11-reg-g1.log`、`~/workspace/t11-reg-typecheck*.log`、`~/workspace/t11-reg-naming.log`。
