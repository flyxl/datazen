# T10 测试报告：表列覆盖持久化与来源隔离

- 被测对象：`packages/frontend/src/lib/valuePresentation/columnOverrides.ts`（新增，215 行：`validateStoredColumnOverrides`、`withColumnOverride` 纯函数、`saveColumnOverride` 保存链路、`MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES = 200`）+ `packages/frontend/src/lib/valuePresentation/settings.ts`（`StoredValuePresentationSettings` 加可选 `columnOverrides` 字段、parse 逐条校验、`buildValuePresentationSnapshot` 填 overrides，替代恒 `{}`）+ `packages/frontend/src/lib/valuePresentation/index.ts`（T10 导出）+ `packages/frontend/src/lib/valuePresentation/columnPresentationSession.ts`（文件头更新：会话仍是唯一合并点 + 乐观层）+ `packages/frontend/src/components/DataTable/ColumnPresentationDialog.tsx`（默认保存改走 `saveColumnOverride`，异步；失败就地报 saveFailed、面板不关闭、可重试；默认开启路径保留 columnOverrides）+ `packages/frontend/src/windows/settings/ValuePresentationSettingsSection.tsx`（全局保存时保留 columnOverrides）+ `packages/backend-client/src/settings.ts`（镜像 `SettingsValuePresentationColumnOverride` + `columnOverrides?`）+ Rust `crates/datazen/src/settings/domain.rs`（不透明 `column_overrides: serde_json::Map` + Default + JSON round-trip 测试）+ `crates/datazen-desktop/src/platform/settings.rs` 与 `crates/datazen-desktop/src/store/tests/settings_shared_tests.rs`（夹具补齐 + 新增桌面 shared round-trip 测试）+ 自带测试 `columnOverrides.test.ts`（新，30 项）、`settings.test.ts`（+7 净增）、`ColumnPresentationDialog.test.tsx`（+3 项）、`ColumnPresentationEntries.test.tsx`（改写 2 项）
- 分支：`feature/issue-32-value-presentation`（HEAD `5ca99ed0a`，作者 issue-32-agent；T10 两提交：code `256b7c251` + test `5ca99ed0a`，基底 `f3952812f`；验证开始时工作区干净，`git status` 无未提交差异）
- 测试角色：独立 tester。未修改任何被测源码或测试；唯一可写文件为本报告。
- 依据：`docs/archive/development/issue-32-value-presentation-design.md` §5（覆盖键结构化字段）、§6（配置加载、持久化与同步：表覆盖键、用户/组织作用域、加载状态、保存失败）、`docs/archive/development/issue-32-value-presentation-prd.md` §3.2、§5（列覆盖按用户/组织、连接、数据库、namespace、表、列隔离；面板 ID、会话 ID 不能作为持久化身份）、§6 验收 6（重启恢复配置；不同连接、数据库、schema 的同名表不串配置）
- 日期：2026-11-12

## 1. 测试环境与命令

环境：Node v24.22.1，vitest 4.1.10（仓库自带），tsrs 0.18.0-ts7.1.0-dev.20260929 / tsc 7.0.2，cargo 1.99.1（`~/.cargo/bin`），系统时区 UTC（全部探针均显式传入时区与语言，不依赖机器默认值）。对抗性探针运行于独立临时项目 `~/workspace/t10-probe/`（通过 `node_modules` 符号链接复用仓库依赖，`environment: jsdom`，被测源码以绝对路径导入，不污染仓库工作区；探针按任务要求用后删除，原始日志 `~/workspace/t10-p-logic.log`、`~/workspace/t10-p-panel.log`、`~/workspace/t10-p-all.log` 与 `~/workspace/t10-p6b*.log`；另有 G1–G7 日志 `~/workspace/t10-g{1,2,3a,3b,4,5,7-kernel,7-check,7-desktop}.log`、`~/workspace/t10-g2-plain.log`（去 ANSI 色码版）与 `~/workspace/t10-g6-{code-part1,code-part2,code-rust}.diff`、`~/workspace/t10-g6-{prettier,fmt}.log`）。

环境说明：本机 `PATH` 中无 `pnpm` 二进制（与 T02–T09 报告相同情况）；typecheck 改用 `npx -y pnpm@12.18.0 <script>` 调用，版本号实测 12.18.0，属等价调用，不影响门禁结论。

| 命令 | 用途 |
|------|------|
| `npx vitest run packages/frontend/src/lib/valuePresentation/__tests__/columnOverrides.test.ts packages/frontend/src/lib/valuePresentation/__tests__/settings.test.ts packages/frontend/src/components/DataTable/__tests__/ColumnPresentationDialog.test.tsx packages/frontend/src/components/DataTable/__tests__/ColumnPresentationEntries.test.tsx --retry=0` | G1：T10 定向（新 30 + settings 30 + 对话框 23 + 入口 2） |
| `npx vitest run packages/frontend/src/windows/settings packages/frontend/src/lib/valuePresentation packages/frontend/src/stores/__tests__/settingsStore.test.ts packages/frontend/src/components/DataTable --retry=0` | G2：接线回归（设置页全量 + valuePresentation 全库 + settingsStore + DataTable 全目录） |
| `npx -y pnpm@12.18.0 typecheck` / `npx -y pnpm@12.18.0 typecheck:tsc` | G3：双引擎类型门禁（tsrs / 权威 tsc） |
| `node scripts/check-test-module-naming.mjs` | G4：测试命名门禁 |
| `npx vitest run <G1 路径> --retry=0 --coverage --coverage.include='…columnOverrides.ts' --coverage.include='…settings.ts' --coverage.include='…ColumnPresentationDialog.tsx'` | G5：新文件覆盖率（v8 报告 + coverage-final.json） |
| `git show 256b7c251 / 5ca99ed0a` 全 diff + grep 新增行 `any` / 跳过断言 + Prettier + rustfmt + `git diff --check` | G6：改动范围审读 |
| `CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0' cargo test -p datazen --lib --features settings`；同参数 `cargo check -p datazen_desktop --lib --tests`；`USER=root cargo test -p datazen_desktop --lib settings` | G7：Rust（见 §2.1 与 §7） |
| 探针项目内 `./node_modules/.bin/vitest run --retry=0`（`probes/logic.test.ts` + `probes/panel.test.tsx`） | 对抗性探针 P1–P10 + P6b（BUG-001 两层复现） |

## 2. 测试项清单

### 2.1 既有验证（仓库门禁）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| G1 | T10 定向单测 | 见 §1 表格 | ✅ 4 文件 **85/85 通过**（columnOverrides 30、settings 30、对话框 23、入口 2；退出码 0；与 coder 申报逐项一致，含拆分） |
| G2 | 接线回归 | 设置页全量 + valuePresentation 全库 + settingsStore + DataTable 全目录 | ✅ 97 文件 **1196/1196 通过**（退出码 0；回归全绿，无未解释失败；coder 的 704 之说与任务指定命令的实测范围不符，见 O-1） |
| G3 | 类型门禁（双引擎） | `pnpm typecheck` / `pnpm typecheck:tsc`（经 `npx -y pnpm@12.18.0` 等价调用） | ✅ 两者均 OK（tsrs 0.18.0-ts7.1.0-dev.20260929 与 tsc 7.0.2，退出码均为 0） |
| G4 | 测试命名门禁 | `node scripts/check-test-module-naming.mjs` | ✅ ok（1837 Rust 文件、256 cfg(test) 模块；2588 TS 文件、8 wdio spec 树；退出码 0） |
| G5 | 新文件覆盖率 | 见 §1 表格 | ✅ 全部 ≥80%（见下表明细；测试 85/85 通过，退出码 0；未出现 T06 报告的全局阈值退出码问题） |
| G6 | 改动范围与源码审读 | 两提交全 diff + grep + Prettier + rustfmt + diff-check | ✅ 通过（见 §2.3；但有一处申报数字出入，见 O-2） |
| G7 | Rust | 内核 lib 测试 + 桌面 check + 桌面 test 链接复核 | ⚠️ 见 §2.1：内核 57/57 与桌面 check 通过；桌面 test 本地无法链接（环境限制，留给 CI），含新增的 column_overrides round-trip 测试 |

> G5 明细（v8 报告；`% Stmts / % Branch / % Funcs / % Lines`）：
>
> | 文件 | % Stmts | % Branch | % Funcs | % Lines | 未覆盖行（语句行） |
> |------|---------|----------|---------|---------|-------------------|
> | `lib/valuePresentation/columnOverrides.ts`（新） | 100 | 100 | 100 | 100 | — |
> | `lib/valuePresentation/settings.ts` | 97.43 | 100 | 100 | 97.22 | 188 |
> | `components/DataTable/ColumnPresentationDialog.tsx` | 90.56 | 85 | 89.74 | 93.1 | 181, 183, 214, 249, 333, 402, 413–418, 446, 447, 493, 614–616, 718 |
>
> 与 coder 申报的对照：columnOverrides.ts 100/100/100/100 与 settings.ts 97.43/100/100/97.22 逐字一致；对话框 coder 申报 89.93/81.66/89.74/93.1，实测行 90.56 / 分支 85 均**高于**申报（函数 89.74 / 行 93.1 一致），见 O-2。未覆盖行均为防御分支，见 §5 O-6。

> G7 分项：
>
> | 分项 | 命令 | 结果 |
> |------|------|------|
> | 内核 lib 测试 | `cargo test -p datazen --lib --features settings`（低内存参数） | ✅ 退出码 0，**57 passed；0 failed**，含新增 `settings::domain::tests::value_presentation_column_overrides_round_trip_opaquely`（与 coder 申报 57 一致） |
> | 桌面 check | `cargo check -p datazen_desktop --lib --tests`（低内存参数） | ✅ 退出码 0（51 warnings，与 T08 相同数量，均为既有警告；T10 的 Rust 改动无新增警告） |
> | 桌面 test 链接 | `USER=root cargo test -p datazen_desktop --lib settings`（低内存参数） | ⚠️ 未能链接：`rust-lld: error: unable to find library -lwebkit2gtk-4.1 / -lgtk-3 / -lgdk-3 / -lgdk_pixbuf-2.0`——coder 申报属实（沙箱缺系统库），桌面测试（含新增的 `value_presentation_column_overrides_round_trip_through_shared_settings`）留给 CI；不是代码问题 |

### 2.2 对抗性探针（~/workspace/t10-probe，独立于 coder 自带测试编写，逻辑 15 项 + 面板 9 项）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| P1 | 保存→重读 round-trip | 逻辑探针：`withColumnOverride` 写 custom（Unix 毫秒 / iso-tz / 上海）与 raw 各一例 → `JSON.stringify` → `parseStoredValuePresentationSettings` → 快照重建 → 计划重建；断言 JSON 原文含键与 `"mode":"custom"`/`"mode":"raw"`、custom 计划 present 为 `2024-01-15T19:00:00+08:00` 且 `matchedRuleName` 为空、raw 计划状态 `raw`、无覆盖的同名规则列仍命中规则 | ✅ 通过 |
| P2 | 重启恢复模拟 | 逻辑探针：假 store（persist 写进变量）→ 保存 custom → 以新 registry、新 revision 7 重建 store/快照/计划；断言覆盖仍在且优先于规则（`matchedRuleName` 为空），无覆盖列走默认（状态 `default`、present `unchanged/no-renderer`） | ✅ 通过 |
| P3 | 隔离矩阵 | 逻辑探针：同一快照内 8 条覆盖（不同 connection / database / namespace / 表大小写 / 列大小写 / 缺省 vs 空串 database），在**计划 present 文本层**逐一核对；另直调 `columnOverrideKey`，证实 panelId / sessionId / dbSessionId 等额外字段不进键（键恒为 `["conn-A","store","sales","events","ts"]`） | ✅ 通过。conn-A/store/sales/events/ts → `2024-01-15T19:00:00+08:00`；conn-B → raw；otherdb → 秒解释 `2024-01-15T11:00:00Z`；archive → raw；`Events` → 秒解释；`EVENTS`/`Ts` → 无覆盖（`no-renderer`）；缺省 database 与空串 database 各走各的覆盖 |
| P4 | inherit 删除持久条目 | 逻辑探针：假 store 演进序列 custom → inherit 保存 → 规则恢复（`matchedRuleName: 'probe-rule'`、秒解释 `2024-01-15T11:00:00Z`）→ 再保存 custom 重建；断言删除只删本列条目（另一列 raw 保留） | ✅ 通过 |
| P5 | 保存失败 | 面板探针（真实 ColumnPresentationDialog + mock store）：(a) raw 保存时 `updateValuePresentationSettings` reject 一次 → 就地报 `saveFailed`、面板不关闭、store 旧值不变（无 columnOverrides、基底快照计划为 `default`）、会话条目保留（合并快照计划为 `raw`，即乐观值仍可见）→ 重试成功后关闭、会话清除、store 写入；(b) inherit 保存失败 → 无会话条目可保留（inherit 在会话层即删条目）、store 的 raw 仍在且真实生效、靠面板草稿重试成功后 store 条目删除 | ✅ 通过。(a)(b) 均验证，(b) 为 coder 申报的已知行为 |
| P6 | 保存成功清除会话 | 逻辑 + 面板探针：逻辑层预置与待保存值**不同**的会话覆盖（秒）再保存 custom（毫秒）→ 成功后会话为空、`applySessionColumnOverrides(重建快照)` 返回同一对象、计划呈示毫秒文本；面板层 raw 保存成功后会话为空、合并快照即持久快照、计划为 raw | ✅ 通过 |
| P6b | 无变化保存的会话清除 | 逻辑 + 面板探针（BUG-001 复现，见 §4） | ❌ **失败，见 §4 T10-BUG-001**：保存与已存值相同的覆盖时成功返回但不清除会话条目 |
| P7 | 上限 200 条 | 逻辑探针：199 条 + 第 200 条新键保存成功（persist 1 次、写回 200 条）；200 条 + 第 201 条新键拒绝 `too-many-overrides`、persist 未调、原 200 条原样不动；满额时替换已有条目成功（仍 200 条、值已换）；`validateStoredColumnOverrides` 对 205 条记录截断保留前 200 条 | ✅ 通过 |
| P8 | 未知版本 | 逻辑探针：v99 payload **含** columnOverrides 字段与 rules → parse 保留 version 99、结果无 `columnOverrides`/`rules` 字段、快照 overrides/rules 均为空、计划不命中规则；`saveColumnOverride` / `withColumnOverride` 对 v99 不写（前者 `unknown-version` 且 persist 未调、会话 untouched；后者返回同一对象）；旧 payload 无 columnOverrides → parse 无该字段、快照 `{}` | ✅ 通过 |
| P9 | 无来源列禁用保存 + 非法值逐条校验 | 逻辑 + 面板探针：面板层无来源列保存按钮禁用、点击不产生任何写操作（persist 未调、会话为空）；逻辑层混合记录逐条丢弃（format `bogus`、未知 renderer、raw 多余字段、4/6 元键、null 表）并保留合法条目；`saveColumnOverride` 遇 format 非法 custom 返回 `invalid-override` 且不碰 persist/会话；格式合法但未知的 IANA（`Mars/Olympus`）在逻辑层通过（存在性是面板层职责，见 O-5） | ✅ 通过 |
| P10 | 全局保存保留覆盖 | 面板探针（真实 ValuePresentationSettingsSection + 对话框默认开启路径）：(a) 设置页改格式为 iso-tz 并保存 → `updateValuePresentationSettings` 的 next 含原样 columnOverrides（raw + custom 各一）与 rules、格式已改、重建快照后覆盖仍在；(b) 对话框默认开启路径 → 开启 timestamp 的同时保留 columnOverrides；(c) 默认开启路径 store 失败 → 就地报 saveFailed 且 store 未写回（覆盖 G5 未覆盖行 :493）；(d) v99 整份重写丢字段（见 O-3） | ✅ 通过（(d) 为观察项，非缺陷） |

> 探针过程说明：首轮 2 项失败均为**探针期望/脚手架错误**，代码未动，修正后除 P6b 外全部通过：(1) P1 的 JSON 原文断言：记录键在 `JSON.stringify` 后引号被转义，应断言 `JSON.stringify(key)` 而非裸键；(2) P10b（v99）首版未改草稿就点保存——设置页保存按钮为 `disabled={!isDirty}`，需先改格式使草稿变脏。另：探针配置初版两次失败亦为脚手架问题（setup 文件在 root 之外被 Vite 拦截，改 `server.fs.allow` 放行 worktree 后解决；root 指向 worktree 的一版误把 setup 路径解析为 `/packages/...`，已弃用）。
>
> 探针总计：**22/24 通过**，2 项失败均为同一 bug 的两处复现（逻辑 P6b 与面板 P6b，见 §4）。

### 2.3 契约类型严格性审读（G6）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| T1 | 改动范围 | `git show 256b7c251 --stat` / `git show 5ca99ed0a --stat` | ✅ 两提交共 14 文件、+1385/−49，全部在 T10 范围内：code 10 文件（新 1 + 改 9）、test 4 文件（新 1 + 改 3）。无无关改动；tasks.md 未动 |
| T2 | any 泄漏 / 断言跳过验证 | grep 两提交新增行 `: any`/`as any`/`<any>`/`any[]`、新测试 `it.skip/test.skip/.todo(/xit(/xdescribe`（含仅 `+` 行复核） | ✅ 新增行中 `any` 0 处；新测试 0 处跳过（30 项全数，见 G1） |
| T3 | backend-client 镜像与规范类型逐字段一致 | `SettingsValuePresentationColumnOverride`（backend-client/settings.ts）对照 `ColumnPresentationOverride`（extension-points/contracts.ts:106–113） | ✅ 通过。inherit / raw / custom 三分支、`rendererId: string`、`options: unknown` 逐字段一致（含 `readonly`）；`inherit` 保留在联合类型里以便 `toSharedSettings` 可赋值，解析层丢弃（与文件注释一致） |
| T4 | T09 入口测试改写合理 | `git show 5ca99ed0a` 的 Entries diff 全读 | ✅ 通过。保存变异步后：两处保存均 `waitFor` persist 调用 1/2 次 + 面板关闭，再用已持久化值经 `parseStoredValuePresentationSettings` → `buildValuePresentationSnapshot` 重建快照并 `rerender`，与生产 store → 快照链路一致；`persistedSnapshot` 辅助函数注释写明了原因 |
| T5 | settings.test.ts / Dialog 拆分数字 | 基底与 HEAD 的 `it(` 计数对照 | ✅ 通过。settings 23 → 30：新增 8 项、删除 1 项 T06 占位测试（`overrides is always empty in T06 (T10 fills it)`），净 +7，与申报一致；Dialog 20 → 23（+3），与申报一致 |
| T6 | ValuePresentationSettingsSection 全局保存保留 columnOverrides | diff 审读 + P10a 探针 | ✅ 通过。`handleSave` 的 `next` 在 `version === 1` 且字段存在时展开 `{ ...stored.columnOverrides }`；与 rules 的保留条件同一模式 |
| T7 | 对话框默认开启路径保留 columnOverrides | diff 审读 + P10b 探针 | ✅ 通过。`handleEnableTimeDisplay` 默认路径同条件展开；P10 探针 (b)(c) 覆盖成功与失败两路 |
| T8 | Rust 改动最小性与字面量同步 | `git show 256b7c251 -- crates/` 全 diff + 全仓 grep `ValuePresentationSettings {` | ✅ 通过。`domain.rs` 只加不透明字段 + `#[serde(default)]` + Default + 1 个新测试；全仓 7 处字面量（domain.rs :308/:330/:355、platform/settings.rs :141、settings_shared_tests.rs :80/:119/:168）全部含 `column_overrides`，无 T08-BUG-001 式遗漏；`cargo test --lib` 编译通过即是旁证；server 侧无该结构体字面量 |
| T9 | Prettier + rustfmt + 空白检查 | `npx prettier --check`（两提交全部 11 个 TS/TSX 改动文件）+ `rustfmt --check`（3 个 Rust 文件）+ `git diff --check f3952812f..HEAD` | ✅ 通过。Prettier `All matched files use Prettier code style!`；rustfmt 退出码 0；diff-check 无输出（注：首版 Prettier 命令误含 3 个 .rs 文件，报无解析器，属命令写法错误，非代码问题；重跑 TS 文件后通过） |
| T10 | code/test 提交分拆与 amend | 两提交 `--name-only` + author/committer 日期对照 | ✅ code 提交不含测试文件（`__tests__` 0 处），test 提交仅 4 个测试文件；两提交 author 与 committer 时间戳均相同，未 amend |
| T11 | T09 接缝四项逐条核验 | diff 与源码对照（见 §3） | ✅ 全部成立（除 §4 BUG-001） |
| T12 | `applySessionColumnOverrides` 仍是唯一合并点 | 全仓 grep 会话合并调用 | ✅ 通过。会话合并只经 `applySessionColumnOverrides`（`useColumnPresentationSession` hook 与 grid/aside 两处调用）；T10 未引入第二合并点 |

## 3. 设计符合性逐条结论

对照设计文档、PRD、T09 接缝与 coder 申报：

**Coder 申报核验**

| 条目 | 结论 |
|------|------|
| 1. 新 `columnOverrides.ts`（validateStoredColumnOverrides、withColumnOverride 纯函数、saveColumnOverride 保存链路、MAX = 200） | ⚠️ 大体符合，附 §4 BUG-001：保存链路的无变化早退路径漏清会话条目；其余语义（乐观写会话、失败保留、成功清除、inherit 删条目、未知版本拒绝、200 上限）均经 P4–P8 独立复核成立 |
| 2. `settings.ts` 加可选 columnOverrides 字段、parse 逐条校验、buildValuePresentationSnapshot 填 overrides（替代恒 `{}`） | ✅ 符合（P1/P8：旧 payload 无字段 → 缺省；v99 不解析；非法条目逐条丢弃；快照 overrides 不设渲染器启用门禁——raw 在总开关关闭时仍生效，P1 的 coder 自带测试与本报告 P2 均覆盖） |
| 3. `index.ts` 导出 | ✅ 符合（仅新增 T10 导出，无既有导出变动） |
| 4. `columnPresentationSession.ts` 文件头更新（会话仍是唯一合并点 + 乐观层） | ✅ 符合（四项接缝逐条见下；G6 T12：无第二合并点） |
| 5. `ColumnPresentationDialog.tsx` 默认保存改走 saveColumnOverride（异步；失败就地报 saveFailed、面板不关闭、可重试） | ✅ 符合（P5 两路独立复核；`saving` 态禁用保存按钮，防重复提交） |
| 6. `ValuePresentationSettingsSection.tsx` 全局保存时保留 columnOverrides（此前整份重写会丢弃该字段） | ✅ 符合（P10a：改格式保存后 columnOverrides 与 rules 原样保留） |
| 7. `backend-client/src/settings.ts` 镜像 SettingsValuePresentationColumnOverride + columnOverrides? | ✅ 符合（G6 T3 逐字段一致） |
| 8. Rust `domain.rs` 加不透明 column_overrides: serde_json::Map（+Default、+JSON round-trip 测试） | ✅ 符合（G7 内核 57/57；旧 payload 无字段加载为空，见新测试 `:369` 断言） |
| 9. `platform/settings.rs` 与 `store/tests/settings_shared_tests.rs` 夹具补齐 + 新增桌面 shared round-trip 测试 | ✅ 符合（G6 T8：7 处字面量全同步；桌面测试本地无法链接，留给 CI，见 G7） |

**T09 接缝四项（columnPresentationSession.ts 注释）**

| 条目 | 结论 |
|------|------|
| ① StoredValuePresentationSettings 加表列覆盖字段、buildValuePresentationSnapshot 填进快照 overrides | ✅ 符合（settings.ts diff；P1 端到端复核） |
| ② 保存经 updateValuePresentationSettings 写持久化，成功后清除同一键会话条目 | ⚠️ 基本符合，附 BUG-001：成功路径中**有持久写操作**时清除（P6）；**无变化早退**时不清除（P6b） |
| ③ 键与隔离继续用 columnOverrideKey + ColumnSource | ✅ 符合（P3 计划文本层 + 键直调复核；面板/会话/dbSession ID 不进键） |
| ④ applySessionColumnOverrides 是唯一合并点 | ✅ 符合（G6 T12） |

**设计 §5/§6（与 T10 相关项）**

| 条目 | 结论 |
|------|------|
| §5：表覆盖键结构化字段（用户/组织作用域 + connectionId + database + namespace + table + column）；保留大小写；缺省与空串不碰撞 | ✅ 符合（键为 `columnOverrideKey` 的 5 元 JSON 数组；用户/组织作用域由存储层负责，键内不含用户 ID；P3 逐项复核） |
| §5：覆盖只存 raw / custom，inherit 即无条目 | ✅ 符合（parse 丢弃 inherit 条目；保存 inherit 删条目；P4/P9） |
| §6：全局与表覆盖配置由宿主使用同一用户偏好存储端口管理；补齐 backend-client、Rust 共享设置 DTO、默认值与宿主映射 | ✅ 符合（backend-client 镜像 + Rust 不透明字段 + 双端 round-trip；Web 端复用同一 settingsStore 链路，未另建存储） |
| §6：保存失败：保持已生效快照，编辑面板保留草稿与错误 | ✅ 符合（P5：store 旧值不变、面板就地报错不关闭、草稿可重试；会话层保留的是用户刚改的乐观值，基底快照仍是旧值——两层语义见 P5 探针区分） |
| §6：未知配置版本保留原文，不用默认值写回覆盖 | ⚠️ 部分符合：`saveColumnOverride` 路径完全符合（P8：v99 返回 unknown-version、persist 未调、原文保留）；但**整份重写路径**（设置页全局保存、对话框默认开启）在 v99 下会丢弃 columnOverrides 与 rules 字段，见 §5 O-3（与 T08 O-3 同类已知局限，非新 bug） |
| §6：加载状态——初次加载默认快照、后续刷新保留最后有效快照 | ➖ 部分范围外：T10 只验证存储→快照→计划的同步重建；跨窗口/刷新时序属 T11 范围 |

**PRD §3.2/§5/§6（与 T10 相关项）**

| 条目 | 结论 |
|------|------|
| §3.2：跟随规则清除本列显式覆盖 | ✅ 符合（P4：inherit 保存删持久条目、规则恢复、可再建） |
| §3.2：无可靠表列来源的查询结果禁用「保存为表列覆盖」 | ✅ 符合（P9：按钮禁用 + 点击零写操作；`saveColumnOverride` 的 `no-source` 防御复核亦通过） |
| §5：列覆盖按当前用户/组织、连接、数据库、namespace、表、列隔离；面板 ID、会话 ID 不能作为持久化身份 | ✅ 符合（P3：隔离矩阵在计划文本层成立；键直调证实面板/会话/dbSession ID 不进键） |
| §5：保存失败应明确提示，不能显示成已保存；旧配置继续生效 | ✅ 符合（P5：就地 saveFailed、无 saved 标记、store 旧值不变、草稿可重试） |
| §6 验收 6：重启恢复配置；不同连接、数据库、schema 的同名表不串配置 | ✅ 符合（P1/P2 重启恢复；P3 同名表隔离矩阵） |
| §6 验收 4：列覆盖优先于规则；「原始值」阻断规则；清除覆盖恢复继承 | ✅ 符合（P1/P2 覆盖 > 规则且 `matchedRuleName` 为空；P4 清除后规则恢复） |

## 4. Bug 列表

| 编号 | 标题 | 级别 | 状态 |
|------|------|------|------|
| T10-BUG-001 | 保存与已存值相同的列覆盖时，`saveColumnOverride` 无变化早退路径不清除会话条目，成功后同一键仍留一条与持久值相同的会话覆盖 | 低 | 已关闭（回归验证通过，见 §8） |

**T10-BUG-001 详情**

- 位置：`packages/frontend/src/lib/valuePresentation/columnOverrides.ts:201–206`（`saveColumnOverride` 的无变化早退分支；清除点在 `:213`，早退时跳过）。
- 引入：code 提交 `256b7c251`（T10 新建文件，基底 `f3952812f` 时 `saveColumnOverride` 不存在）。
- 现象：某列已存覆盖 custom（Unix 毫秒 / iso-tz / Asia/Shanghai），用户打开面板、不改动直接点保存（或逻辑层直调 `saveColumnOverride` 传入同一覆盖）：返回 `{ ok: true }`，persist 未调用（正确，避免冗余写），但会话存储里仍留有该键的 custom 条目；随后 `applySessionColumnOverrides(持久快照)` 不再返回同一对象，而是新合并快照且 revision 从 1 被抬到 2。
- 根因（源码核实）：`:201` 先乐观写会话（`setSessionColumnOverride`），`:202` 算 `next = withColumnOverride(...)`，`:203` 若 `next === stored` 直接 `return { ok: true }`，跳过了 `:213` 的 `clearSessionColumnOverride`。`:204` 注释称“会话已与持久值一致”，只在 inherit-删不存在条目时成立：此时会话条目本就被 inherit 删除、无残留；但 custom/raw 与已存值相同时，会话里刚写入的条目与持久值**内容相同但仍存在**，注释的前提不成立。
- 复现（逻辑层，探针 P6b）：
  ```ts
  const stored = { version: 1, renderers: {},
    columnOverrides: { [keyFor(SRC, 'ts_ms')]: CUSTOM_MS } };
  await saveColumnOverride({ stored, source: SRC, columnName: 'ts_ms',
    override: CUSTOM_MS, persist: vi.fn() });
  // 实际：getColumnPresentationSessionState().overrides 仍含该键 custom 条目；
  //      applySessionColumnOverrides(snapshotOf(stored)) !== snapshotOf(stored)，revision 2。
  // 预期：overrides 为 {}；合并返回同一快照对象。
  ```
  复现（面板层，探针 P6b）：store 与快照均含某列 raw 覆盖 → 打开面板 → 不改动点保存 → `onClose` 被调、persist 未调 → 会话仍含该键 raw 条目（预期 `{}`）。
- 影响范围：**不显示错、不丢数据、不崩溃**。残留条目与持久值内容相同，cell/aside 呈示结果不变；唯一可观测影响是合并快照不再是持久快照本身、revision 被多抬 1（下游计划多重建一次），以及会话里多一条冗余条目直至该列下次保存或刷新。理论上的跨窗口分歧（另一窗口改写了持久值后本窗口会话仍遮住旧值）属 T11 跨窗口同步的范围，本 bug 只是在无变化路径上凭空制造了这样一条残留条目。定为**低**：违背提交信息与文件头步骤 6「成功后清除同一键会话条目」的不变量，但无用户可见错误。
- 修复建议（未实施，tester 不改代码）：在 `:203–206` 的早退分支返回前同样调 `clearSessionColumnOverride(source, columnName)`（无变化时会话值即持久值，清除无副作用；inherit-删不存在条目时清除亦为空操作）；或把乐观写会话移到 `next !== stored` 的确认之后。修复后以探针 P6b 两处用例（逻辑层与面板层）回归，并把逻辑层用例吸收为仓库回归测试。
- 注意：失败路径不受影响（`save-failed` 时会话条目保留是有意行为，P5 已验证）；有持久写操作的成功路径已正确清除（P6 已验证）。

本次验证未发现其他 bug。既有门禁 G1–G6 全部通过，G7 内核腿与桌面 check 通过、桌面 test 腿因环境缺系统库留给 CI；对抗探针除 BUG-001 的两处复现外全部通过。

## 5. 观察项（非缺陷，供 coder / 后续任务参考）

- **O-1（G2 申报 704 vs 实测 1196，范围口径不同）**：任务指定 G2 命令实测 97 文件 1196/1196（windows/settings 224 + valuePresentation 441 + settingsStore 31 + DataTable 500）。coder 的 704 恰为 224 + 441 + 31 + 入口 2 + useValuePresentation 6，即“设置页 + valuePresentation 全库 + settingsStore + 入口 + useValuePresentation”的窄口径；任务命令跑的是 DataTable **全目录**（500 项）而非仅入口与 hook。两数可互相算出，回归本身全绿，无未解释失败；后续申报写明口径即可。属 T09 O-2 同类出入。
- **O-2（G5 对话框覆盖率实测高于申报）**：coder 申报对话框 89.93/81.66/89.74/93.1，实测 90.56/85/89.74/93.1（行 +0.63、分支 +3.34，函数与行覆盖率一致）。方向为有利（实测更高），可能源于覆盖运行的 include/文件集不同；均 ≥80%，无结论影响。
- **O-3（v99 整份重写丢 columnOverrides 与 rules——T08 O-3 同类局限）**：探针 P10b 实测：store 为 v99 且含 columnOverrides/rules 时，设置页改格式保存后写回值保持 `version: 99`，但**不再含 `columnOverrides` 与 `rules` 字段**。根因是 `stored` 取自 `parseStoredValuePresentationSettings`，该函数对 v99 根本不解析这两个字段（P8 已验证），故整份重写路径看不到它们；与 T08 记录的 rules 丢失同一模式。`saveColumnOverride` 路径本身正确拒绝 v99（P8），列覆盖的保存入口不受影响。按 T08 O-3 的处理记为观察项/已知局限而非新 bug；建议 T11 跨窗口同步或文档中一并明确：未知版本配置经设置页保存后会丢失其 rules 与 columnOverrides 字段（其余 renderer 条目仍保留）。另：设置页保存按钮为 `disabled={!isDirty}`，v99 值在无改动时根本不会写回，丢失只发生在用户确实改了全局设置并保存时。
- **O-4（超 200 条记录的解析静默截断）**：`validateStoredColumnOverrides` 对超过 200 条的记录只保留前 200 条有效条目（P7 探针：205 条 → 200 条），不报错；保存路径则拒绝新键（`too-many-overrides`）而不截断。两者分工与 T08 的 rules（>20 条截断）同一模式，属设计内行为；但一份已存 201+ 条的记录经解析 → 面板保存后，第 201 条起会被丢掉。建议文档注明。
- **O-5（IANA 存在性校验的分层——T09 O-5 延续）**：纯逻辑层 `validateTimestampOptions` 只校验 IANA 格式，格式合法但未知的 `Mars/Olympus` 在 `validateStoredColumnOverrides` 中会被**保留**（P9 探针实测）；存在性由面板 `validateColumnPresentationDraft` 经 `isValidTimeZoneName` 把关（T09 P9 已验证面板拒存），渲染时另有 Intl try/catch 兜底。直接调纯逻辑层写入的调用方理论上可存入未知时区名，但唯一写入方是面板（经 `saveColumnOverride` → 面板草稿校验），且渲染会退回原值、不崩。属既有分层，非 T10 新问题。
- **O-6（未覆盖行清单）**：`settings.ts:188` 为 T06 既有防御分支（`validateConfig` 抛异常的 `continue`，契约要求贡献不抛）；对话框未覆盖行中 T10 新增路径只有两处：`:446–447`（`handleSave` 的 catch——`saveColumnOverride` 内部已捕获 persist 失败并返回 `save-failed`，外层 catch 仅防 `getState`/parse 等意外抛错）与 `:493`（默认开启路径的 catch；探针 P10c 已覆盖其行为——store 失败就地报错且 store 未写回，行覆盖缺口不代表行为缺口）；其余均为 T09 O-8 已列的防御分支（行号因 T10 插入 +10 左右平移，如 `:402` 的 `source === undefined` 守卫、`:413–418` 的保存时二次校验失败、`:614–616` 的 select 非法值守卫）。建议接受探针覆盖，不阻塞 T10。
- **O-7（G1 stderr 的 `common.close` 缺译警告）**：与 T09 O-7 同一既有模式（`@datazen/ui` 自身注册表，单元测试环境未注册该 key），非 T10 引入；不影响测试结果（退出码 0）。
- **O-8（探针脚手架教训两则）**：(1) P1 初版断言 JSON 原文含裸键失败——记录键在 `JSON.stringify` 后引号被转义，应断言 `JSON.stringify(key)`；(2) P10b 初版未改草稿就点保存失败——设置页保存按钮为 `disabled={!isDirty}`，v99 用例需先改格式。均为探针写法问题，代码未动。
- **O-9（Rust 测试需 CI 跑）**：桌面 `cargo test` 本地无法链接（缺 webkit2gtk-4.1/gtk-3/gdk-3/gdk_pixbuf-2.0，见 G7），新增的 `value_presentation_column_overrides_round_trip_through_shared_settings`（`settings_shared_tests.rs:161`）只能由 CI 验证；内核 `datazen` 的 settings 腿已本地验证（57/57，含新增 JSON round-trip 测试）。
- **O-10（G1 拆分数字与申报一致）**：columnOverrides 30、settings 30（含净增 7：新增 8、删除 1 项 T06 占位测试）、对话框 23（含新增 3）、入口 2，合计 85——与 coder 申报逐项一致，无 T09 O-1 式出入。
- **O-11（无变化保存跳过持久写是正确的）**：`withColumnOverride` 的 JSON 相等短路使 `saveColumnOverride` 在无变化时不调 persist（P6b 探针：persist 0 次调用），避免冗余写操作；BUG-001 只关乎同一路径漏清会话条目，不关乎是否写盘。
- **O-12（探针资产与回归建议）**：`~/workspace/t10-probe` 为一次性验证项目，已按任务要求删除（探针文件与 vitest 配置随之清除；仓库 `git status` 复核干净，除本报告外无新增文件）。本报告 §2.2 各项方法已描述到可复现程度，日志见 `~/workspace/t10-p-*.log` 与 `~/workspace/t10-g*.log`。建议将 P3（计划文本层隔离矩阵）、P4（inherit→规则→重建序列）、P5b（inherit 保存失败靠草稿重试）、P6（stale 会话值清除）与 P10a（设置页全局保存保留覆盖）吸收为仓库回归测试；P6b 两处用例在 BUG-001 修复后应转为通过。

## 6. 结论

**通过，可做收尾提交。**（原结论「不通过，待 T10-BUG-001 修复并回归验证」已被 §8 回归验证取代：唯一 bug T10-BUG-001 已修复并关闭，见 §8；其余首轮结论与数字不变。）

- 既有门禁：T10 定向 85/85（30+30+23+2）、接线回归 1196/1196（97 文件）、双引擎类型通过、命名门禁通过、Prettier 与 rustfmt 全过；新文件覆盖率全部 ≥80%（columnOverrides.ts 100/100/100/100、settings.ts 97.43/100/100/97.22、对话框 90.56/85/89.74/93.1），与 coder 申报逐项一致（附 O-1/O-2 的口径与数字出入）；Rust 内核 57/57（含新增 round-trip 测试）、桌面 check 通过，桌面 test 腿因环境缺系统库留给 CI。
- 对抗性探针 22/24 通过：P1 保存→重读 round-trip、P2 重启恢复（覆盖 > 规则 > 默认，`matchedRuleName` 为空）、P3 隔离矩阵（连接/数据库/namespace/表列大小写/缺省-vs-空串/面板-会话 ID 不进键）、P4 inherit 删除后规则恢复再重建、P5 保存失败（raw 与 inherit 两路：就地报错、面板不关闭、旧配置继续生效、会话/草稿保留、可重试成功）、P6 保存成功清除会话（含 stale 会话值）、P7 上限 200 条边界、P8 未知版本、P9 无来源列禁用保存与非法值逐条校验、P10 全局保存与默认开启保留覆盖，全部通过。2 项失败均为同一 bug 的两处复现（逻辑 P6b 与面板 P6b）。
- 设计符合性：设计 §5/§6 与 PRD §3.2/§5、验收 6 均符合；验收 4 的三条保存语义均符合；§6「未知版本保留原文」在 `saveColumnOverride` 路径符合，在整份重写路径有 T08 O-3 同类局限（O-3，非新 bug）。
- 唯一 bug 是 T10-BUG-001（低）：保存与已存值相同的列覆盖时，`saveColumnOverride` 的无变化早退路径（`columnOverrides.ts:203–206`）跳过 `:213` 的会话清除，成功后同一键仍留一条与持久值相同的会话覆盖，合并快照 revision 被多抬 1。不显示错、不丢数据；修复为早退前同样清除该键会话条目（一行），已由 §8 回归验证通过并关闭。
- 观察项 12 条均为非缺陷（O-1 G2 口径、O-2 覆盖率申报出入、O-3 v99 整份重写丢字段、O-4 超 200 条解析截断、O-5 IANA 存在性分层、O-6 未覆盖行清单、O-7 `common.close` 警告、O-8 探针脚手架教训、O-9 Rust 测试需 CI、O-10 拆分数字一致、O-11 无变化跳过写盘正确、O-12 探针资产与回归建议）。
- tasks.md 状态更新与 docs close commit 不在本次验证范围内，由协调方执行。

## 7. 专项：T10-BUG-001 根因、证据与影响面

### 7.1 现象

保存与已存值相同的列覆盖时返回成功且不写盘，但会话里残留一条与持久值相同的覆盖；随后合并快照不再是持久快照本身。

### 7.2 根因（源码核实）

`columnOverrides.ts` 的 `saveColumnOverride`（`:186–215`）：

- `:201` 先乐观写会话：`setSessionColumnOverride(source, columnName, validated.override)`；
- `:202` 算 `next = withColumnOverride(stored, source, columnName, validated.override)`；
- `:203–206` 若 `next === stored` 直接 `return { ok: true }`，跳过 `:213` 的 `clearSessionColumnOverride(source, columnName)`；
- `:204` 注释称“会话已与持久值一致”，只在 inherit-删不存在条目时成立（会话条目本就被 inherit 删除）；custom/raw 与已存值相同时，会话里刚写入的条目与持久值内容相同但**仍存在**，注释的前提不成立。

对照：有持久写操作的成功路径（`:207–214`）正确清除；失败路径（`:209–211`）有意保留。漏的只是无变化早退这一条分支。

### 7.3 影响面（实测分层）

| 层 | 行为 | 结果 |
|----|------|------|
| 保存返回值 | `saveColumnOverride` 返回 `{ ok: true }`，persist 未调 | ✅ 正确（避免冗余写盘） |
| 会话状态 | `getColumnPresentationSessionState().overrides` 仍含该键条目 | ❌ 错误（P6b：应为 `{}`） |
| 合并快照 | `applySessionColumnOverrides(持久快照)` 返回新对象、revision 1 → 2 | ❌ 错误（应返回同一对象；P6 的成功路径对照组返回同一对象） |
| cell / aside 呈示 | 残留条目与持久值内容相同，呈示文本不变 | ✅ 正确（无用户可见错误） |
| 面板 | 不改动点保存后 `onClose` 正常调用、persist 未调 | ✅ 正确（面板层复现） |
| inherit-删不存在条目 | 会话本就无条目，早退无残留 | ✅ 正确（注释的前提仅在此成立） |
| 下次同列保存 / 刷新 | 下次保存成功时清除；刷新后会话清空 | ✅ 自愈 |

结论：BUG-001 只断无变化成功路径的**会话清理不变量**，不影响保存结果、不写错数据、不影响呈示。定为低级：它违背提交信息与文件头步骤 6「成功后清除同一键会话条目」的不变量，且多抬的 revision 会引发一次多余的计划重建，但用户看不到错误，修复面为一行（早退前清除该键会话条目）。修复后以 P6b（逻辑层 + 面板层）两处探针用例回归，并建议把逻辑层用例吸收为仓库回归测试。

## 8. 回归验证记录（T10-BUG-001 修复）

2026-11-12，tester 对 T10-BUG-001 的修复做了聚焦回归验证。修复两笔提交：fix `96f8134b7`（`fix(issue-32): clear the session entry when saving an unchanged column override (T10-BUG-001)`）与 test `5ec0e0755`（`test(issue-32): cover unchanged-save session cleanup (T10-BUG-001 regression)`）。验证开始时工作区 HEAD 即 `5ec0e0755`（`git log -4`：`5ec0e0755` / `96f8134b7` / `5ca99ed0a` / `256b7c251`，分支 `feature/issue-32-value-presentation`），除本报告未提交外工作区干净；tester 未改任何被测代码，唯一可写文件为本报告。

### R1：修复 diff 审读 ✅ 通过

- `git show 96f8134b7`：1 文件、+5/−1，仅 `packages/frontend/src/lib/valuePresentation/columnOverrides.ts`。无变化早退分支（`:203–206`）返回前加 `clearSessionColumnOverride(source, columnName)`（`:208`），并把误导注释（旧 `:204`「会话已与持久值一致」）修正为说明残留条目会遮住持久值、抬高合并快照 revision；其余行零改动。
- `git show 5ec0e0755`：2 文件、+54/−0，仅两处新增回归测试，无生产改动：`columnOverrides.test.ts` +21 行（逻辑层回归）、`ColumnPresentationDialog.test.tsx` +33 行（面板层回归）。
- 两提交全文审读无 `any` 新增，无无关文件。
- inherit-删不存在条目时清除为空操作（源码核实）：`clearSessionColumnOverride` 即 `setSessionColumnOverride(..., { mode: 'inherit' })`（`columnPresentationSession.ts:103–107`），而 inherit 分支在键不存在时直接返回，不写、不涨 revision、不通知（同文件 `:81–101`）。另：无变化 custom/raw 用例中清除发生在乐观写之后，条目存在故删除时 session revision +1，但 `applySessionColumnOverrides` 在 overrides 为空时原样返回传入快照（`:121–128`），该 +1 不影响合并快照——R2 逻辑探针的 `toBe(snapshot)` 断言即覆盖此点。

### R2：首轮失败探针 P6b 两处独立重跑 ✅ 通过

按本报告 §2.2 / §4 的复现步骤独立重写探针重跑（新临时项目 `/tmp/t10-reg-probe/`，`node_modules` 符号链接复用仓库依赖，`environment: jsdom`，被测源码绝对路径导入，不污染仓库工作区；探针按惯例用后删除，日志 `~/workspace/t10-reg-p6b.log`），不只跑仓库新测试。逻辑夹具取自首轮报告 §4（`conn-A/store/sales/events/ts_ms` + Unix 毫秒 / iso-tz / Asia/Shanghai custom 覆盖），面板夹具取自首轮 §4（`conn-1/shop/public/orders/epoch_ms` raw 覆盖）。

- **P6b 逻辑层（`probes/logic.test.ts`，2 项）**：(a) 保存与已存值相同的 custom 覆盖 → 返回 `{ ok: true }`、persist 未调、会话 overrides 为 `{}`、以 revision 1 重建快照后 `applySessionColumnOverrides(snapshot)` 返回同一对象、revision 仍为 1；(b) inherit 删除不存在的条目 → 返回 `{ ok: true }`、persist 未调、会话为空且 session revision 仍为 0（清除为空操作）。✅ 2/2 通过。
- **P6b 面板层（`probes/panel.test.tsx`，1 项）**：store 与快照均含某列 raw 覆盖 → 打开面板 → 不改动点保存 → `onClose` 被调 1 次、persist（`updateValuePresentationSettings`）未调、会话 overrides 为 `{}`。✅ 通过。
- 探针合计 **3/3 通过**，退出码 0。探针过程说明：首版面板探针在 `vi.mock` 路径里用了模板字符串变量 `WT`，因 `vi.mock` 提升在变量初始化前执行而报 `Cannot access 'WT' before initialization`（套件级失败，0 测试执行）；改为字面量绝对路径后通过。属探针脚手架问题，非代码问题。另：运行期 stderr 有一条 `common.close` 缺译警告，与首轮 O-7 同一既有模式，非本次引入。

### R3：G1 定向全集 ✅ 通过

`npx vitest run packages/frontend/src/lib/valuePresentation/__tests__/columnOverrides.test.ts packages/frontend/src/lib/valuePresentation/__tests__/settings.test.ts packages/frontend/src/components/DataTable/__tests__/ColumnPresentationDialog.test.tsx packages/frontend/src/components/DataTable/__tests__/ColumnPresentationEntries.test.tsx --retry=0`：退出码 0，**4 文件 87/87 通过**（columnOverrides 31、settings 30、对话框 24、入口 2；日志 `~/workspace/t10-reg-g1.log`）。与修复人申报 87/87 一致：较首轮 85/85 净增 2 项，恰为两条新回归测试（columnOverrides 30 → 31、对话框 23 → 24）。

### R4：双引擎类型门禁与命名门禁 ✅ 通过

- `npx -y pnpm@12.18.0 typecheck`（tsrs 0.18.0-ts7.1.0-dev.20260929）：退出码 0，`OK`（日志 `~/workspace/t10-reg-typecheck.log`）。
- `npx -y pnpm@12.18.0 typecheck:tsc`（tsc 7.0.2）：退出码 0，`OK`（日志 `~/workspace/t10-reg-typecheck-tsc.log`）。
- `node scripts/check-test-module-naming.mjs`：退出码 0，`ok`（1837 Rust 文件、256 cfg(test) 模块；2588 TS 文件、8 wdio spec 树；日志 `~/workspace/t10-reg-naming.log`）。

**新回归测试「无修复时失败」的复核（未重做还原，代码审读方式）**：修复人称已临时还原验证两条新测试在无修复时失败。本次复核未重做还原，改为读测试代码确认其断言确会区分修复前后。逻辑回归：无修复时 `:201` 的乐观写把 custom 条目写进会话，早退分支不清除，随后 `expect(getColumnPresentationSessionState().overrides).toEqual({})` 必失败（会话仍含该键），且 session revision 为 1、overrides 非空，`applySessionColumnOverrides` 返回新合并对象，`toBe(snapshot)` 断言亦必失败；`persist` 未调与快照 revision 为 1 两处断言则与修复无关、通过与否不区分。面板回归：对话框默认保存经 `saveColumnOverride`（`ColumnPresentationDialog.tsx:434`），无修复时早退不清除，`onClose` 被调与 persist 未调两处断言仍通过，但末尾 `expect(getColumnPresentationSessionState().overrides).toEqual({})` 必失败。两处测试的关键断言均与修复行为直接挂钩，无修复时不可能通过；且首轮 §4/§7 的 P6b 实测（无修复时逻辑层会话残留 custom 条目、revision 1 → 2；面板层会话残留 raw 条目）是同一失败的独立证据。故修复人的临时还原说法可信。

**T10-BUG-001 验证通过并关闭。**结论更新为「通过，可做收尾提交」（见 §6）。未发现修复引入的新问题；探针项目已按惯例删除（`/tmp/t10-reg-probe/`），日志见 `~/workspace/t10-reg-p6b.log` 与 `~/workspace/t10-reg-g1.log`、`~/workspace/t10-reg-typecheck*.log`、`~/workspace/t10-reg-naming.log`。
