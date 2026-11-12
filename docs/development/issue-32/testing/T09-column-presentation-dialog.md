# T09 测试报告：列配置面板（ColumnPresentationDialog 与两处入口）

- 被测对象：`packages/frontend/src/components/DataTable/ColumnPresentationDialog.tsx`（新增，777 行：inherit / raw / custom 三模式面板、草稿校验、预览双锚点、总开关关闭提示与显式开启、无来源列禁用保存）+ `packages/frontend/src/lib/valuePresentation/columnPresentation.ts`（新增，292 行：覆盖校验 `validateColumnPresentationOverride`、预览 `computeColumnPresentationPreview`、样本 `fallbackSampleForColumn`）+ `packages/frontend/src/lib/valuePresentation/columnPresentationSession.ts`（新增，129 行：进程内会话覆盖存储 + 唯一合并点 `applySessionColumnOverrides`）+ `packages/frontend/src/hooks/useColumnPresentationSession.ts`（新增，39 行：`useSessionValuePresentationSnapshot` 合并 hook）+ 改 `DataTable.tsx`（表头菜单接线 + 会话快照合并）、`DetailPanel.tsx`（aside 字段入口 + 会话快照合并）、`useDataTableContextMenu.ts`、`lib/dataTableContextMenu.ts`（表头菜单新增 `column-presentation` 项）、`lib/valuePresentation/index.ts`（T09 导出）、`locales/en/query.ts` 与 `locales/zh-CN/query.ts`（各 +21 key）+ 自带测试 5 个新文件（45 项）与 `DataTable.contextMenu.test.tsx` 等价守卫更新
- 分支：`feature/issue-32-value-presentation`（HEAD `a29536621`，作者 issue-32-agent；T09 两提交：code `c8d0dbc3b8` + test `a29536621`，基底 `84a23e88f`；验证开始时工作区干净，`git status` 无未提交差异）
- 测试角色：独立 tester。未修改任何被测源码或测试；唯一可写文件为本报告。
- 依据：`docs/archive/development/issue-32-value-presentation-design.md` §5（ColumnPresentationOverride：inherit/raw/custom）、§7（共享展示管线）、§9（ColumnPresentationDialog.tsx、表头菜单接线、ContentViewDrawers、表与查询结果面板）、`docs/archive/development/issue-32-value-presentation-prd.md` §3.2（两处入口打开相同面板）、§3.3（生效反馈）、§4（覆盖 > 规则 > 默认；原始值阻断规则）、§5、§6 验收 2/3/4/7
- 日期：2026-11-12

## 1. 测试环境与命令

环境：Node v24.22.1，vitest 4.1.10（仓库自带），tsrs 0.18.0-ts7.1.0-dev.20260929 / tsc 7.0.2，系统时区 UTC（全部探针均显式传入时区与语言，不依赖机器默认值）。对抗性探针运行于独立临时项目 `~/workspace/t09-probe/`（通过 `node_modules` 符号链接复用仓库依赖，`environment: jsdom`，被测源码以绝对路径导入，不污染仓库工作区；探针按任务要求用后删除，原始日志 `~/workspace/t09-p-logic.log` 与 `~/workspace/t09-p-panel.log`；另有 G1/G2/G3/G4/G5/G6 日志 `~/workspace/t09-g{1,2,3a,3b,4,5}.log` 与 `~/workspace/t09-g6-prettier.log`，i18n 对照脚本 `~/workspace/t09-i18n.mjs`）。

环境说明：本机 `PATH` 中无 `pnpm` 二进制（与 T02–T08 报告相同情况）；typecheck 改用 `npx -y pnpm@12.18.0 <script>` 调用，版本号实测 12.18.0，属等价调用，不影响门禁结论。T09 为纯前端改动，未碰桌面 settings 链路与任何 Rust 代码，故未跑桌面 settings 套件（任务亦注明 T09 预计不需要）。

| 命令 | 用途 |
|------|------|
| `npx vitest run packages/frontend/src/components/DataTable/__tests__/ColumnPresentationDialog.test.tsx packages/frontend/src/components/DataTable/__tests__/ColumnPresentationEntries.test.tsx packages/frontend/src/hooks/__tests__/useColumnPresentationSession.test.tsx packages/frontend/src/lib/valuePresentation/__tests__/columnPresentation.test.ts packages/frontend/src/lib/valuePresentation/__tests__/columnPresentationSession.test.ts packages/frontend/src/components/DataTable/__tests__/DataTable.contextMenu.test.tsx --retry=0` | G1：T09 定向（新测试 45 项 + 既有 contextMenu 11 项） |
| `npx vitest run packages/frontend/src/components/DataTable packages/frontend/src/lib/valuePresentation packages/frontend/src/hooks/__tests__/useValuePresentationSnapshot.test.tsx packages/frontend/src/windows/settings/__tests__/ValuePresentationSettingsSection.test.tsx --retry=0` | G2：接线回归（DataTable 全目录 + valuePresentation 全库 + 快照 hook + 设置面板） |
| `npx -y pnpm@12.18.0 typecheck` / `npx -y pnpm@12.18.0 typecheck:tsc` | G3：双引擎类型门禁（tsrs / 权威 tsc） |
| `node scripts/check-test-module-naming.mjs` | G4：测试命名门禁 |
| `npx vitest run <G1 路径> --retry=0 --coverage --coverage.include='…ColumnPresentationDialog.tsx' --coverage.include='…columnPresentation.ts' --coverage.include='…columnPresentationSession.ts' --coverage.include='…useColumnPresentationSession.ts'` | G5：新文件覆盖率（v8 报告 + coverage-final.json） |
| `git show c8d0dbc3b8 / a29536621` 全 diff + grep 新增行 `any` / 跳过断言 + i18n key 对照脚本 + Prettier + `git diff --check` | G6：改动范围审读 |
| `node ~/workspace/t09-probe` 内 `npx vitest run --config vitest.config.ts probes/logic.test.ts probes/panel.test.tsx` | 对抗性探针 P1–P9（逻辑 14 项 + 面板 12 项） |
| `node ~/workspace/t09-i18n.mjs` + grep 逐 key 核对 | P10：en / zh-CN key 文件级逐一对照 |

## 2. 测试项清单

### 2.1 既有验证（仓库门禁）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| G1 | T09 定向单测 | 见 §1 表格 | ✅ 6 文件 **56/56 通过**（Dialog 19、Entries 2、hook 3、columnPresentation 15、session 6、contextMenu 11；退出码 0；总数与 coder 申报一致，拆分见 O-1） |
| G2 | 接线回归 | DataTable 全目录 + valuePresentation 全库 + 快照 hook + 设置面板 | ✅ 68 文件 **946/946 通过**（退出码 0；回归全绿，无未解释失败；coder 的「113 中 112 过」与任务指定命令的实测范围不符，见 O-2） |
| G3 | 类型门禁（双引擎） | `pnpm typecheck` / `pnpm typecheck:tsc`（经 `npx -y pnpm@12.18.0` 等价调用） | ✅ 两者均 OK（tsrs 0.18.0-ts7.1.0-dev.20260929 与 tsc 7.0.2，退出码均为 0） |
| G4 | 测试命名门禁 | `node scripts/check-test-module-naming.mjs` | ✅ ok（1837 Rust 文件、256 cfg(test) 模块；2586 TS 文件、8 wdio spec 树；退出码 0） |
| G5 | 新文件覆盖率 | 见 §1 表格 | ✅ 全部 ≥80%（见下表明细；测试 56/56 通过，退出码 0；未出现 T06 报告的全局阈值退出码问题） |
| G6 | 改动范围与源码审读 | 两提交全 diff + grep + i18n 对照 + Prettier + diff-check | ✅ 通过（见 §2.3；但有两处申报数字出入，见 O-1/O-2/O-3） |

> G5 明细（v8 报告；`% Stmts / % Branch / % Funcs / % Lines`）：
>
> | 文件 | % Stmts | % Branch | % Funcs | % Lines | 未覆盖行（语句行） |
> |------|---------|----------|---------|---------|-------------------|
> | `components/DataTable/ColumnPresentationDialog.tsx`（新） | 90.27 | 81.17 | 89.47 | 93.84 | 179, 181, 212, 247, 263, 330, 390, 401, 403, 406, 451, 572, 573, 676 |
> | `lib/valuePresentation/columnPresentation.ts`（新） | 93.9 | 91.13 | 100 | 94.8 | 103, 107, 129, 197, 265 |
> | `lib/valuePresentation/columnPresentationSession.ts`（新） | 100 | 100 | 100 | 100 | — |
> | `hooks/useColumnPresentationSession.ts`（新） | 100 | 100 | 100 | 100 | — |
>
> 与 coder 申报逐项一致（Dialog 行 93.89/分支 81.17/函数 89.47——实测 Lines 93.84，差 0.05 属测量噪声，见 O-3；columnPresentation.ts 行 94.8/分支 91.13 逐字一致；session 与 hook 均为 100%）。未覆盖行均为防御分支，见 §5 O-8。

### 2.2 对抗性探针（~/workspace/t09-probe，独立于 coder 自带测试编写，逻辑 14 项 + 面板 12 项）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| P1 | 两处入口同一配置，三处显示一致 | 面板探针：表头入口保存 custom（Unix 毫秒）→ aside 入口重开同一草稿 → 改格式为 iso → 保存；cell / aside / 预览三方逐字对照；反向（aside 保存 → 表头重开 → 选 inherit）用规则作区分器 | ⚠️ **1 项失败，见 §4 T09-BUG-001**：正向（表头→aside，含改格式）通过；反向在「选 inherit 后预览应为规则值」一步失败——预览仍显示旧 custom 值。保存本身正确（见 P1c）。其余断言均通过 |
| P1c | BUG 隔离：inherit 保存本身正确 | 面板探针：aside 保存 custom → 表头重开选 inherit（记录仍显示 custom 的预览与「列覆盖」来源标签）→ 保存 → cell/aside 恢复规则值；重开后面板预览与来源标签均正确 | ✅ 通过。缺陷只在保存前的预览/标签，不在保存 |
| P2 | 跟随规则（计划层） | 逻辑探针：规则（Unix 毫秒）→ 会话 custom（Unix 秒）胜出 → 保存 inherit 删除会话条目 → 规则重新生效；`matchedRuleName` 全程对照 | ✅ 通过。custom 计划无 `matchedRuleName`；inherit 后会话条目删除、合并快照与基底同一对象、规则重新命中 |
| P2b | inherit 预览的根因定位 | 逻辑探针：快照含规则 + 会话 custom 覆盖已合并时，直调 `computeColumnPresentationPreview({ candidate: { mode: 'inherit' } })` | ❌ **失败，T09-BUG-001 的根因**：返回 custom 文本 `2024-01-15T19:00:00+08:00`，而非规则文本 `2024-01-15T11:00:00Z` |
| P3 | 原始值阻断规则 | 逻辑探针：命中规则的列选 raw → 计划状态 `raw`、present 为 `{ unchanged, reason: 'raw-override' }`、预览为 `{ unchanged, reason: 'raw-override' }`；再切 inherit 恢复规则文本 | ✅ 通过（首轮探针把回退原因误写为 `'raw'`/`'default'`，代码实为 `'raw-override'`/`'no-renderer'`，见探针过程说明） |
| P4 | 时间 custom：Unix 秒 vs 毫秒显式选择 | 逻辑探针：同值 `1705316400000` 秒解释与毫秒解释的预览各与独立 Intl 重算逐字对照；10 位值 `1705316400` 在秒解释下为 2024 年、在毫秒解释下为 1970 年（含 `.400` 小数秒保留） | ✅ 通过。毫秒 `2024-01-15T19:00:00+08:00`、秒 `+56009-04-29T16:00:00+08:00`——coder 申报的秒级年份逐字核对无误（ISO 扩展年 `>9999` 前缀 `+`，与 `formatIsoYear` 一致） |
| P4b | BIGINT 默认草稿 | 面板探针：新开 bigint 列面板，inherit 下解释下拉隐藏；切 custom 后默认值为 `unix-milliseconds`，下拉含 native / unix-seconds / unix-milliseconds / text 四档可改；切秒后预览变 `+56009-04-29T16:00:00+08:00` 且徽标切为秒 | ✅ 通过。绝不按位数推断（10 位值在毫秒解释下仍为 1970 年，见 P4） |
| P5 | 双锚点逐字 | 逻辑 + 面板探针：原生 `2026-01-15T14:30:00Z` 与 Unix `1705316400000` 在 Asia/Shanghai/iso-tz 下，原值逐字 + 预览逐字；原生锚点与独立 Intl 重算（`Date.UTC(2026,0,15,14,30,0)`）一致 | ✅ 通过。原生 `2026-01-15T22:30:00+08:00`、Unix `2024-01-15T19:00:00+08:00` |
| P6 | 总开关关闭 | 面板探针：禁用快照下面板提示「尚未启用时间显示」；保存列配置不调 store、不调显式开启回调；显式开启回调只调用一次；默认开启路径只把 `datazen.timestamp` 置 enabled，其余 renderer 条目与规则原样保留；开启失败就地报错且 store 不变 | ✅ 通过。另见 O-4：总开关关闭时保存的 custom 覆盖仍会立即作用于网格（计划层 custom 绕过 enabled 门禁，T04 既有行为） |
| P7 | 无来源查询列 | 逻辑 + 面板探针：source 缺省时 `columnOverrideFromSnapshot` 返回 inherit；表级规则不命中无来源列（别名列同名亦不命中），仅列名/类型名规则可命中；面板保存按钮禁用 + 说明原因；预览仍走全局默认 | ✅ 通过。无来源 instant 列 inherit 预览 `2026-01-15T22:30:00+08:00`，生效来源为全局默认 |
| P8 | 会话生效与隔离 | 逻辑探针：两列覆盖互不干扰；会话为空时 `applySessionColumnOverrides` 返回同一快照对象、revision 不变；写入/删除各涨 1、合并 revision = 快照 + 会话、单调递增；无条目时保存 inherit 无 revision 涨幅、无通知；`resetColumnPresentationSession` 后覆盖全部丢失 | ✅ 通过。刷新后丢失为 T09 有意边界（文件头已写明），按任务指令记为已知局限，不判 bug |
| P9 | 非法 options 禁存就地报错 | 逻辑 + 面板探针：`format: 'bogus'`、未知 renderer、名内空白 rendererId 均拒存且带路径；IANA `Mars/Olympus`（格式合法但未知）与 `Not A Zone`（名内空白）在面板层禁存就地报错；空串禁存；`' Asia/Shanghai '` trim 后接受、存为干净名 | ✅ 通过。附分层说明：纯逻辑层 `validateTimestampOptions` 只校验 IANA 格式、不校验存在性（T02 既有分层，见 O-5）；存在性由面板 `validateColumnPresentationDraft` 把关 |
| P10 | i18n | 文件级脚本 + grep 逐 key 核对：提取 en/zh-CN 的 `columnPresentation.*` 与 `dataTable.columnPresentation` key 集、组件实际 `t()` 引用、逐 key 取值对照 | ✅ 通过。两文件各 21 key、key 集完全一致、无空值、zh 值与 en 无一相同；组件引用的 16 个静态 key 与 `mode.${mode}` / `mode.${mode}.hint` 动态模板在两文件均有对应 |

> 探针过程说明：首轮逻辑探针 4 项失败均为**探针期望错误**，代码未动，修正后除 P2b 外全部通过：(1) 回退原因码：计划层 `raw` 状态的 present 原因是 `'raw-override'` 而非探针原写的 `'raw'`，`default` 状态是 `'no-renderer'` 而非 `'default'`（`fallbackPresentationReason` 的显式映射，`presentationPlan.ts:227–244`）；(2) 小数秒保留：`1705316400` 毫秒解释为 `1970-01-21T01:41:56.400+08:00`，探针独立重算初版漏了 `.400` 后缀，补足后一致（T07 P3a 已有同类注记）；(3) P9 纯逻辑层 `Mars/Olympus` 应为通过：`validateTimestampOptions` 只做 IANA 格式校验，存在性是面板层职责（`timestampConfig.ts:16–18` 文件头写明；面板探针 P9 已验证面板拒存）；(4) 面板 P1b 首轮失败另有一处探针脚手架缺陷：`RULE_ISO` 初版用 native 解释配 BIGINT 列，native 不解释数字、规则命中但呈示仍为原值——改为 Unix 毫秒 + iso + UTC 后规则才真正区分 custom 与 inherit；另补 `useI18n` mock 的参数插值（规则名需经 `{name}` 传入标签，key 直通 mock 原不插值）。
>
> 探针总计：**24/26 通过**，2 项失败均为同一 bug（逻辑 P2b 与面板 P1b，见 §4）。P1c（bug 隔离用例）通过，证明缺陷只在保存前的预览/标签，不在保存。

### 2.3 契约类型严格性审读（G6）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| T1 | 改动范围 | `git show c8d0dbc3b8 --stat` / `git show a29536621 --stat` | ✅ 两提交共 17 文件、+2703/−4，全部在 T09 范围内：code 11 文件（新 4 + 改 7）、test 6 文件（新 5 + 改 1）。无无关改动 |
| T2 | any 泄漏 / 断言跳过验证 | grep 两提交新增行 `: any`/`as any`/`<any>`/`any[]`、新测试 `it.skip/test.skip/.todo(/xit(/xdescribe` | ✅ 新增行中 `any` 0 处；新测试 0 处跳过（45 项全数，见 G1） |
| T3 | i18n key 数量与内容一致 | `git show` 计数 + P10 脚本 + grep 逐 key 核对 | ✅ 通过。en 与 zh-CN 各 +21 key（`dataTable.columnPresentation` 1 + `columnPresentation.*` 20），key 集完全一致，无缺 key、无多余、值非空 |
| T4 | 表头菜单新增项位置与等价守卫更新 | diff 审读 + `DataTable.contextMenu.test.tsx` | ✅ 通过。`column-presentation` 项插在 `columnStats` 之后、`copy-column-name` 之前（与「列检查项」相邻，commit message 一致）；等价守卫更新为 `['column-presentation', 'copy-column-name', 'copy-column-data', 'export']`，为有意新增；无 `onColumnPresentation` 时菜单保持原样（`headerPresentation` 与 handler 双条件） |
| T5 | aside 按钮 testid | diff 审读 | ✅ 通过。`detail-field-presentation-${column.name}`，见 `DetailPanel.tsx` FieldRow；按钮文案为 `t('dataTable.columnPresentation')`（「显示方式…」），样式为文字按钮（见 O-9） |
| T6 | Dialog 外层 testId 约定 | `packages/ui/src/tid.ts` + `packages/ui/src/Dialog.tsx:157` + `ColumnStatsDialog.tsx:69` 对照 | ✅ 通过。外层 `testId="column-presentation-dialog"` 经 `tid()` 只在 `VITE_E2E=1` 构建才渲染 `data-testid`，单元测试改用常驻的内层 `column-presentation-dialog-body` 定位——与既有 `ColumnStatsDialog`（`testId="column-stats-dialog"` + 内层 `data-testid`）同一约定；G1 stderr 的一条 `common.close` 缺译警告来自 `@datazen/ui` 自身注册表（见 O-7），非 T09 引入 |
| T7 | 申报语义逐条核验 | 逐条对照 diff 与源码（见 §3） | ✅ 全部成立（除 §4 BUG-001 与 O-1/O-2/O-3 的申报数字出入） |
| T8 | Prettier + 空白检查 | `npx prettier --check`（两提交全部 17 个改动文件）+ `git diff --check 84a23e88f..HEAD` | ✅ 通过。`All matched files use Prettier code style!`；diff-check 无输出 |
| T9 | code/test 提交分拆 | 两提交 stat 对照 | ✅ code 提交不含测试文件，test 提交不含源码；code 提交 author/committer 均为 04:06:18（未 amend），test 提交亦未 amend |

## 3. 设计符合性逐条结论

对照设计文档、PRD 与 coder 申报：

**Coder 申报核验**

| 条目 | 结论 |
|------|------|
| 1. 新 `ColumnPresentationDialog.tsx`（设计 §9 指定落点，复用既有 Dialog + 原生 select） | ✅ 符合（落点 `components/DataTable/`；`Dialog` + 原生 `<select>`/`<input>`；三模式 + 双锚点预览 + 总开关提示 + 无来源禁用保存齐全；附 BUG-001 一处预览缺陷，见 §4） |
| 2. 新 `columnPresentation.ts`（覆盖校验 + 预览，纯逻辑） | ✅ 符合（`validateColumnPresentationOverride`：inherit/raw 不接受多余字段、custom 只接受 `datazen.timestamp` 且 options 走 `validateTimestampOptions`、非法禁存；`computeColumnPresentationPreview` 复用运行时计划/贡献路径；附 BUG-001：inherit 分支未剥离仍存的覆盖，见 §4） |
| 3. 新 `columnPresentationSession.ts`（会话覆盖存储 + 合并点） | ✅ 符合（inherit 删条目、raw/custom 按 `columnOverrideKey` 存一条；会话为空原样返回快照、revision 不变；合并 revision = 快照 + 会话、单调递增；T10 口子写进文件头；刷新丢失为有意边界，P8 核实） |
| 4. 新 `useColumnPresentationSession.ts`（合并 hook） | ✅ 符合（`useSyncExternalStore` 订阅会话存储；DataTable 与 DetailPanel 各自调用同一合并函数；会话为空返回原快照本身） |
| 5. 改 `DataTable.tsx`、`DetailPanel.tsx`、`useDataTableContextMenu.ts`、`lib/dataTableContextMenu.ts` | ✅ 符合（表头菜单新增 Display 入口、aside 字段新增同名按钮、两处共用同一 `ColumnPresentationDialog` 与同一合并快照；菜单项可选——不传 handler 时菜单原样） |
| 6. 改 `lib/valuePresentation/index.ts` | ✅ 符合（仅新增 T09 导出，无既有导出变动） |
| 7. 改 `locales/en/query.ts` 与 `zh-CN/query.ts` | ✅ 符合（各 +21 key，P10 逐一对照一致；其余 9 个 locale 走 en fallback，注释已写明，与 `columnStats` 既有模式一致） |
| 8. 测试新 5 文件，改 `DataTable.contextMenu.test.tsx`（等价守卫加入有意新增的 `column-presentation` 项） | ✅ 符合（45 项新测试 + 守卫 1 处更新；守卫位置与菜单位置一致，见 G6 T4；拆分数字与申报不符见 O-1） |
| 9. BIGINT 默认草稿显式选 Unix 毫秒且下拉可见可改，绝不按位数推断 | ✅ 符合（`draftFromColumnOverride`：`semanticKind === 'number'` 时默认 `unix-milliseconds`；P4/P4b 独立复核秒/毫秒互不混淆） |
| 10. 总开关关闭时编辑保存仍可用、显式开启、不静默开启 | ✅ 符合（P6：保存不调 store、不调开启回调；默认开启路径只替换 `datazen.timestamp` 一项；开启失败就地报错；附 O-4 与 O-6 两处边界观察） |
| 11. 无来源查询列禁用保存、不从 SQL 别名猜来源 | ✅ 符合（`saveDisabled = source === undefined` + `handleSave` 首行守卫；P7：表级规则不命中无来源列、别名同名列亦不命中） |

**设计 §5/§7/§9（与 T09 相关项）**

| 条目 | 结论 |
|------|------|
| §5：`ColumnPresentationOverride` 三模式；`native` 不自动解释数字；Unix 必须显式指定单位 | ✅ 符合（类型与校验一致；P4/P4b 验证秒/毫秒显式区分、native 下 BIGINT 不动） |
| §5：表覆盖键结构化字段、保留大小写、缺省与空串不碰撞 | ✅ 符合（复用 T04 `columnOverrideKey`：connectionId/database/namespace/table/column 的 JSON 键；P8 逐 key 核对） |
| §7：共享展示管线，cell 与 aside 同一计划 | ✅ 符合（两处经同一合并快照建计划；P1a 三方逐字一致；P8 会话隔离） |
| §9：`ColumnPresentationDialog.tsx` 为统一列配置入口；需改表头菜单接线、DetailPanel、表与查询结果面板 | ✅ 符合（落点与改动面与 §9 一致；ContentViewDrawers 本身未改——入口经 DataTable/DetailPanel 接入，属 §9「需从核查后的当前代码确定最小改动范围」的裁量内） |

**PRD §3.2/§3.3/§4/§6（与 T09 相关项）**

| 条目 | 结论 |
|------|------|
| §3.2：两个入口打开相同面板，编辑同一份列配置 | ✅ 符合（P1a/P1c：两入口互通同一草稿与同一会话存储） |
| §3.2：跟随规则清除显式覆盖；原始值跳过自定义展示、不继续匹配全局规则；时间选来源/格式/时区 | ⚠️ 部分符合：保存语义全部正确（P2/P3/P1c）；但**选「跟随规则」时预览与生效来源标签仍显示旧覆盖**，见 §4 BUG-001 |
| §3.2：BIGINT 选时间后必须明确指定 Unix 秒或毫秒，不以位数自动决定 | ✅ 符合（P4/P4b） |
| §3.2：面板同时显示原值与结果（双锚点） | ✅ 符合（P5：原值逐字 + 预览逐字；`column-presentation-original` 与 `column-presentation-preview-result` 并列） |
| §3.2：总开关关闭时允许编辑与保存，明确提示「尚未启用时间显示」，提供显式开启，不静默打开总开关 | ✅ 符合（P6；附 O-4：关闭时保存的 custom 覆盖仍立即作用于网格） |
| §3.2：无可靠表列来源的查询结果禁用「保存为表列覆盖」并说明原因；仍可用全局默认和仅依赖已知元数据的规则；不从 SQL 别名猜表来源 | ✅ 符合（P7） |
| §3.3：Unix 来源显示「Unix 秒 / Unix 毫秒」标记；提示中可查看原值和配置来源（全局默认、规则名称、列覆盖） | ⚠️ 部分符合：Unix 徽标与原值/来源提示齐全；但**有覆盖待清除时来源标签不反映草稿**，见 §4 BUG-001 |
| §3.3：不支持或无法解析的值显示原值并提供简短原因；展示器缺失时保留配置并提示不可用 | ✅ 符合（`unchanged` + reason；贡献缺失时 `column-presentation-contribution-missing` 提示，仓库测试与 P5 覆盖） |
| §4：表列覆盖 > 全局规则 > 全局默认 | ✅ 符合（计划顺序 raw/custom → 规则 → 默认；P2/P3 独立复核） |
| §4：「原始值」表示绕过自定义展示 | ✅ 符合（P3：raw 计划状态 `raw`、原因 `raw-override`，不调 prepare、不匹配规则） |
| §6 验收 2：两个入口修改同一配置；cell、aside、预览显示结果一致 | ⚠️ 部分符合：cell/aside 一致（P1a）；预览在 custom/raw 下一致，在 **inherit 待清除覆盖时不一致**（BUG-001） |
| §6 验收 3：BIGINT 无显式配置时保持数字；配置 Unix 秒/毫秒后按指定单位展示 | ✅ 符合（P1a 初始态为原数字；P4 秒/毫秒各按指定单位） |
| §6 验收 4：列覆盖优先于规则；「原始值」阻断规则；清除覆盖恢复继承 | ⚠️ 部分符合：三条保存语义均符合（P2/P3/P1c）；「清除恢复继承」的**预览**不符合（BUG-001） |
| §6 验收 7：切换选中行、分屏、切换查询结果集时 aside 始终使用正确来源的格式计划 | ➖ 部分范围外：T09 只验证同一面板内两入口共用同一快照与来源；行切换/分屏/结果集切换的整页回归未在 T09 覆盖，建议 T12 全量回归时补 |

## 4. Bug 列表

| 编号 | 标题 | 级别 | 状态 |
|------|------|------|------|
| T09-BUG-001 | 选「跟随规则」时预览与生效来源标签仍显示待清除的旧覆盖，而非规则/全局默认的结果 | 中 | 已关闭（2026-11-12 回归验证，见 §8） |

**T09-BUG-001 详情**

- 位置：`packages/frontend/src/lib/valuePresentation/columnPresentation.ts` 的 `computeColumnPresentationPreview`（inherit 分支）与 `packages/frontend/src/components/DataTable/ColumnPresentationDialog.tsx` 的 `currentPlan` / `effectiveSourceLabel`（约 `:340–360`）。
- 引入：code 提交 `c8d0dbc3b8`。基底 `84a23e88f` 时面板与预览函数均不存在。
- 现象：某列已有显式覆盖（例如 custom：Unix 毫秒 / iso-tz / Asia/Shanghai，cell 显示 `2024-01-15T19:00:00+08:00`），用户打开面板、把模式切到「跟随规则」（inherit），此时预览区仍显示旧 custom 值 `2024-01-15T19:00:00+08:00`，生效来源标签仍显示「列覆盖」；而按规则应显示的值是 `2024-01-15T11:00:00Z`（命中规则 `iso-rule`：Unix 毫秒 / iso / UTC），来源标签应为「规则：iso-rule」。只有保存并重新打开面板后，预览与标签才正确。
- 根因（源码核实）：面板收到的 `snapshot` 是已合并会话覆盖的快照（`presentationSnapshotResolved`）。`computeColumnPresentationPreview` 的 inherit 分支直接拿该快照建计划（`buildColumnPresentationPlans({ columns, source, snapshot })`），计划构建器按 `columnOverrideKey` 查到仍存的 custom 覆盖并优先采用，规则根本没机会命中；`currentPlan` 与 `effectiveSourceLabel` 用同一份合并快照，且标签还以**保存前**的 `effectiveCurrentOverride`（仍为 custom）判定为「列覆盖」，两处都不看草稿的 mode。custom/raw 预览走候选覆盖本身、不受此影响，故缺陷只出现在 inherit 草稿 + 仍存覆盖的组合——仓库自带测试的 inherit 用例均从无覆盖状态出发（`shows the matched rule name as the effective source while inheriting` 等），未覆盖该组合。
- 复现（逻辑层，探针 P2b）：
  ```ts
  // 快照含规则 iso-rule（Unix 毫秒 / iso / UTC）；会话已存 custom（Unix 毫秒 / iso-tz / Asia/Shanghai）
  computeColumnPresentationPreview({
    candidate: { mode: 'inherit' },
    column: { name: 'epoch_ms', dataType: 'bigint' },
    source: SOURCE, snapshot: mergedSnapshot, // 含会话 custom 覆盖
    environment: ENVIRONMENT, sampleValue: 1705316400000, registry,
  });
  // 实际：{ kind: 'text', text: '2024-01-15T19:00:00+08:00', ... }
  // 预期：{ kind: 'text', text: '2024-01-15T11:00:00Z', ... }
  ```
  复现（面板层，探针 P1b）：规则 `iso-rule` 下 cell 初显 `2024-01-15T11:00:00Z` → aside 入口保存 custom（iso-tz/Shanghai）→ cell 变 `2024-01-15T19:00:00+08:00` → 表头入口重开 → 点「跟随规则」→ 预览仍为 `2024-01-15T19:00:00+08:00`（预期 `2024-01-15T11:00:00Z`），来源标签仍为「列覆盖」（预期含 `iso-rule`）。
- 影响范围：仅预览与来源标签的**保存前显示**。保存本身正确：点保存后 inherit 删除会话条目，cell/aside 立即恢复规则值（探针 P1c 验证：保存后 cell/aside 均为 `2024-01-15T11:00:00Z`，重开面板后预览与标签均正确）。不写错数据、不丢配置、不崩溃；但用户在「清除覆盖」这一步看不到清除后的效果，PRD §3.2「跟随规则：清除本列显式覆盖」与 §3.3「提示中可以查看原值和配置来源」的预览承诺未兑现，且与 custom 模式「预览即草稿」不一致（custom 下改格式立即刷新预览，inherit 下不刷新）。
- 修复建议（未实施，tester 不改代码）：inherit 分支建计划时，用去掉本列覆盖的快照副本（`{ ...snapshot, overrides: { ...snapshot.overrides, [key]: undefined } }` 的等价写法——注意 `columnOverrideKey` 键需删除而非置 undefined，或在 `buildColumnPresentationPlans` 入口接受一个「忽略本列覆盖」的候选参数）；Dialog 的 `currentPlan` / `effectiveSourceLabel` 同样按草稿 mode 取值（draft 为 inherit 时剥离本列覆盖再建计划；draft 为 custom/raw 时来源即列覆盖）。修复后补一条仓库回归测试：有覆盖的列切 inherit，预览与来源标签立即变为规则值（即探针 P1b / P2b 的用例）。
- 注意：`columnOverrideFromSnapshot` 与 `draftFromColumnOverride` 不受影响（它们读的是保存前的当前覆盖，本就应反映现状）；`raw` 预览不走计划，不受影响。

本次验证未发现其他 bug。既有门禁 G1–G6 全部通过；对抗探针除 BUG-001 的两处复现外全部通过。

## 5. 观察项（非缺陷，供 coder / 后续任务参考）

- **O-1（G1 拆分数字不准）**：coder 申报「56 通过，其中 T09 新增 41、既有 contextMenu 15」；实测新测试 45 项（Dialog 19 + Entries 2 + hook 3 + columnPresentation 15 + session 6）、既有 contextMenu 11 项，合计 56 一致。基底 `84a23e88f` 的 contextMenu 文件同样为 11 个 `it(`，故 15 之说在基底即不成立。属申报数字不准，非代码问题；后续申报以实测为准。
- **O-2（G2 申报范围与任务指定命令不符）**：任务称 coder 申报「回归套件 113 中 112 过、唯一失败为等价守卫」；任务指定的 G2 命令实测为 68 文件 **946/946 全部通过**，退出码 0，无任何失败（含等价守卫在内的 contextMenu 11 项亦全过）。113 之说与该命令的范围对不上——可能指另一更小的回归子集；以本次实测为准：回归全绿，无未解释失败。
- **O-3（G5 Dialog 行覆盖率 93.84 vs 申报 93.89）**：差 0.05，测量噪声；分支 81.17 / 函数 89.47 逐字一致。无结论影响。
- **O-4（总开关关闭时保存的 custom 覆盖仍立即作用于网格——T04 既有行为）**：探针 P6 的 OBSERVE 用例实测：总开关关闭的快照下保存 custom（Unix 毫秒）后，cell 立即显示 `2024-01-15T19:00:00+08:00`。根因是计划层步骤 (c)：custom 覆盖绕过 enabled 门禁（`presentationPlan.ts` 注释写明），属 T04/T05 既有设计，非 T09 引入。但面板提示只说「尚未启用时间显示，仍可编辑并保存本列配置」，未说明保存的 custom 会立即生效——用户可能误以为要先点「启用时间显示」才看得到效果。建议后续在提示文案或设计文档中明确：显式列覆盖不受总开关门控（或由协调方裁决是否应受门控）；不判 bug。
- **O-5（IANA 存在性校验的分层——T02 既有设计）**：纯逻辑层 `validateTimestampOptions` 只校验 IANA 格式（`timestampConfig.ts:16–18` 文件头写明），格式合法但未知的 `Mars/Olympus` 在该层通过；存在性由面板 `validateColumnPresentationDraft` 经 `isValidTimeZoneName` 把关（面板探针 P9 已验证拒存），渲染时另有 Intl try/catch 兜底。直接调纯逻辑层写入会话的调用方理论上可存入未知时区名，但唯一写入方是面板，且渲染会退回原值、不崩。属既有分层，非 T09 新问题；T10 接持久化时保持同一分层即可。
- **O-6（默认开启路径在非 v1 版本下丢 rules——T08 O-3 同类模式）**：`handleEnableTimeDisplay` 默认路径只在 `stored.version === 1` 时写回 `rules`；若已存版本为 99 且含 rules，点「启用时间显示」会丢 rules 字段（其余 renderer 条目仍保留）。与 T08 面板保存同一模式，属已知局限，记为观察项而非新 bug；建议 T11 跨窗口同步或文档中一并明确。
- **O-7（G1 stderr 的 `common.close` 缺译警告）**：定向运行中出现一条 `[i18n] Missing translation for key "common.close": not registered for "en"`，源自 `@datazen/ui` 自身的 i18n 注册表（`UiDialog` 用自己的 `t('common.close')` 取关闭按钮文案，单元测试环境未注册该 key）。`packages/ui` 已有 `dialogCloseLabelI18n` / `dialogCloseLabelUnregistered` 专项测试覆盖该行为，属既有警告模式，非 T09 引入；不影响测试结果（退出码 0）。
- **O-8（未覆盖行清单）**：除 G5 表列出的行外，值得注意的几处均为防御分支：Dialog `:179–181`（IANA 检查通过后 `validateTimestampOptions` / `validateColumnPresentationOverride` 再失败——经 UI 无法构造，因 IANA 是唯一用户可控非法输入）、`:212`（`draftFromColumnOverride` 遇非法已存 custom options 的回落）、`:247`（`isOutputFormat` 假分支，select 只发出合法值）、`:263`（快照 configs 含非法 timestamp 配置的回落）、`:330`（`useEffect` 的 `!open` 早退）、`:390`（`handleSave` 的 `source === undefined` 守卫，保存按钮已禁用）、`:401–406`（保存时二次校验失败）、`:451`（开启失败 catch）、`:572–573` / `:676`（select onChange 的非法值守卫）；`columnPresentation.ts:103/107/129`（rendererId 非字符串/名内空白/custom 多余字段——面板硬编码 rendererId，用户不可达）、`:197`（`JSON.stringify` 抛错回退）、`:265`（`plans[0]` 缺失的 `no-plan` 防御）。建议接受探针覆盖，不阻塞 T09。
- **O-9（aside 入口为纯文字按钮）**：`DetailPanel` 的字段 Display 按钮只有文字「显示方式…」（`opacity-0 group-hover:opacity-100`，悬停才显），无图标；coder 申报中提到的「aside 文字按钮视觉建议」即指此处。功能正确（P1 验证），视觉是否加图标/常驻显示由后续任务定，不判缺陷。
- **O-10（Unix 秒级年份断言已独立核对）**：coder 申报的秒解释预览 `+56009-04-29T16:00:00+08:00` 经探针独立 Intl 重算逐字确认（`formatIsoYear` 对 `>9999` 年前缀 `+`，与 ISO 8601 扩展年一致）；10 位值在毫秒解释下带 `.400` 小数秒（`1970-01-21T01:41:56.400+08:00`），属精度保留行为，非缺陷。
- **O-11（探针资产与回归建议）**：`~/workspace/t09-probe` 为一次性验证项目，已按任务要求删除（探针文件与 vitest 配置随之清除；仓库 `git status` 复核干净，除本报告外无新增文件）。本报告 §2.2 各项方法已描述到可复现程度，日志见 `~/workspace/t09-p-logic.log`、`~/workspace/t09-p-panel.log` 与 `~/workspace/t09-g*.log`。建议将 P2b（inherit 预览剥离覆盖）、P1b（两入口 inherit 端到端）、P4（秒/毫秒独立重算）与 P6 默认开启写回断言吸收为仓库回归测试；其中 P2b/P1b 在 BUG-001 修复后应转为通过。
- **O-12（code 提交单独立即可构建性未单独验证）**：结构上 code 提交 `c8d0dbc3b8` 不依赖 test 提交内容（测试文件不被生产代码引用），但未在独立 worktree 中单独跑 typecheck（需另装依赖，成本高）；G3 在 HEAD 通过。记为未验证项而非缺陷；后续任务保持 code/test 分拆时留意夹具类依赖（T06 O-1 / T08-BUG-001 的教训）。

## 6. 结论

**通过，可做收尾提交。**

2026-11-12 回归验证：T09-BUG-001 已修复并关闭（见 §8）。以下为首轮验证的原始结论（保留不动，供对照）：

- 既有门禁：T09 定向 56/56（新 45 + 既有 contextMenu 11）、接线回归 946/946（68 文件）、双引擎类型通过、命名门禁通过、Prettier 全过；新文件覆盖率全部 ≥80%（Dialog 90.27/81.17/89.47/93.84、columnPresentation.ts 93.9/91.13/100/94.8、session 与 hook 均为 100%），与 coder 申报逐项一致（附 O-1/O-2/O-3 的申报数字出入）。
- 对抗性探针 24/26 通过：P1 正向三方一致、P2 跟随规则恢复、P3 原始值阻断、P4 秒/毫秒显式区分且经独立 Intl 重算逐字一致、P4b BIGINT 默认草稿、P5 双锚点逐字、P6 总开关关闭不静默开启且仅 timestamp 置 enabled、P7 无来源列禁用保存且不猜别名、P8 会话生效隔离与 revision 单调、P9 非法 options 禁存就地报错、P10 两语言 21/21 key 一致。2 项失败均为同一 bug 的两处复现（逻辑 P2b 与面板 P1b）。
- 设计符合性：设计 §5/§7/§9 与 PRD §3.2/§3.3、§4、验收 3 大体符合；验收 2 与验收 4 的「清除覆盖恢复继承」在**保存语义**上符合、在**保存前预览**上不符合（BUG-001）；验收 7 的行切换/分屏部分属 T12 范围。
- 唯一 bug 是 T09-BUG-001：有显式覆盖的列切「跟随规则」时，预览与生效来源标签仍显示待清除的旧覆盖（逻辑层 `computeColumnPresentationPreview` 的 inherit 分支未剥离仍存覆盖；面板层 `currentPlan` / `effectiveSourceLabel` 同样用合并快照且不看草稿 mode）。保存本身正确，缺陷只在保存前的预览/标签；修复面小（inherit 建计划时剥离本列覆盖；标签按草稿 mode 取值），修复后以探针 P1b / P2b 的用例回归即可。修复前不建议做收尾提交。
- 观察项 12 条均为非缺陷（O-1/O-2/O-3 申报数字出入、O-4 关闭时 custom 仍生效、O-5 IANA 存在性分层、O-6 非 v1 版本开启丢 rules、O-7 `common.close` 警告、O-8 未覆盖行清单、O-9 aside 纯文字按钮、O-10 秒级年份已核对、O-11 探针资产与回归建议、O-12 code 提交未单独验证）。
- tasks.md 状态更新与 docs close commit 不在本次验证范围内，由协调方执行。

## 7. 专项：T09-BUG-001 根因、证据与影响面

### 7.1 现象

有显式覆盖的列在面板内切「跟随规则」后，预览区与生效来源标签仍显示旧覆盖的结果，直到保存并重新打开才正确。保存本身正确。

### 7.2 根因（源码核实）

`columnPresentation.ts` 的 `computeColumnPresentationPreview`：

- inherit 分支直接以调用方传入的 `snapshot` 建计划；调用方（Dialog）传入的是**已合并会话覆盖**的快照 `presentationSnapshotResolved`，其中仍含本列的 custom 覆盖；
- 计划构建器 `buildOneColumnPlan`（`presentationPlan.ts:355–368`）按 `columnOverrideKey` 查到 custom 覆盖即优先采用（步骤 c），规则步骤 (d) 未被执行。

`ColumnPresentationDialog.tsx`：

- `preview`（`useMemo`）调 `computeColumnPresentationPreview({ candidate, ..., snapshot })`——candidate 为 inherit，但 snapshot 仍含覆盖，故预览为覆盖值；
- `currentPlan`（同文件另一处 `useMemo`）同样用合并快照建计划；
- `effectiveSourceLabel` 以**保存前**的 `effectiveCurrentOverride.mode`（仍为 custom）直接判定为「列覆盖」，不看草稿 mode。

三处共享同一根因：inherit 草稿的预览/标签按「当前已存状态」计算，而非按「草稿生效后的状态」计算。custom/raw 草稿的预览走候选覆盖本身，不受影响；无覆盖时的 inherit 草稿也不受影响（快照里无覆盖可查）——这正是仓库自带测试未发现该 bug 的原因：其 inherit 用例均从无覆盖状态出发。

### 7.3 影响面（实测分层）

| 层 | 行为 | 结果 |
|----|------|------|
| 预览（inherit 草稿，有覆盖） | `computeColumnPresentationPreview` 返回旧 custom 文本 | ❌ 错误（P2b：`2024-01-15T19:00:00+08:00`，应为规则值 `2024-01-15T11:00:00Z`） |
| 生效来源标签（同上） | `effectiveSourceLabel` 显示「列覆盖」 | ❌ 错误（P1b/P1c：应为「规则：iso-rule」） |
| 保存（inherit） | `handleSave` 调 `setSessionColumnOverride(source, column, { mode: 'inherit' })`，删除会话条目 | ✅ 正确（P1c：保存后 cell/aside 立即恢复规则值） |
| 重开面板（保存后） | 草稿由已清除的覆盖重建，预览与标签均正确 | ✅ 正确（P1c） |
| custom / raw 草稿的预览 | 走候选覆盖本身 | ✅ 正确（P1a/P3/P4） |
| 无覆盖时的 inherit 草稿 | 快照无覆盖可查，规则/默认正常命中 | ✅ 正确（仓库自带测试） |

结论：BUG-001 只断 inherit 草稿在有覆盖待清除时的**保存前预览与标签**，不影响保存结果、不写错数据、不影响其他模式。定为中级：它违背 PRD §3.2/§3.3 的预览承诺，且是面板核心交互路径（清除覆盖是验收 4 的第三条），但用户仍可通过保存看到正确结果，修复面小。修复后以 P1b（面板端到端）与 P2b（逻辑层）两处探针用例回归，另建议把 P1c 的「保存后重开」断言一并吸收为仓库回归测试。

## 8. 回归验证记录（T09-BUG-001 修复）

2026-11-12，tester 对 T09-BUG-001 的修复做了聚焦回归验证。修复两笔提交：fix `7524a424`（`fix(issue-32): preview the rule value when inherit clears a stored column override (T09-BUG-001)`）与 test `6e259c96`（`test(issue-32): cover inherit preview from a stored column override (T09-BUG-001 regression)`）。验证开始时工作区 HEAD 即 `6e259c960`（`git log -4`：`6e259c960` / `7524a424` / `a29536621` / `c8d0dbc3b8`），除本报告未提交外工作区干净；tester 未改任何被测代码，唯一可写文件为本报告。

### R1：修复 diff 审读 ✅ 通过

- `git show 7524a424`：3 文件、+43/−7，仅 `packages/frontend/src/components/DataTable/ColumnPresentationDialog.tsx`（+18/−7 行）、`packages/frontend/src/lib/valuePresentation/columnPresentation.ts`（+31 行）、`packages/frontend/src/lib/valuePresentation/index.ts`（+1 行：新增 `snapshotWithoutColumnOverride` 导出）。无无关文件。
- `snapshotWithoutColumnOverride`（`columnPresentation.ts:164–178`）：`source === undefined` 时返回原快照本身；键不在 `snapshot.overrides` 中时返回原快照本身；否则复制 `overrides` 后 `delete overrides[key]` 再返回 `{ ...snapshot, overrides }`——**删键而非置 undefined，副本操作不改原快照**，与任务书的修复描述逐字一致。
- inherit 分支两处均用剥离后的快照建计划：`computeColumnPresentationPreview`（`:285`）与 Dialog 的 `currentPlan`（`ColumnPresentationDialog.tsx:376`，仅 `draft.mode === 'inherit'` 时剥离，`draft.mode` 加入 `useMemo` 依赖）；`effectiveSourceLabel`（`:387` 起）改按草稿 mode 取值——custom/raw 为「列覆盖」，inherit 用剥离后计划的 `matchedRuleName` 判定规则/全局默认，不再看保存前的 `effectiveCurrentOverride`。
- `git show 6e259c96`：2 文件、+88/−0，仅两处测试文件新增回归测试，无生产改动：`columnPresentation.test.ts` +45 行（P2b 逻辑回归）、`ColumnPresentationDialog.test.tsx` +43 行（P1b 面板回归）。
- 两提交均无 `any` 新增（diff 全文审读），无无关改动。

### R2：首轮失败探针 P1b / P2b 独立重跑 ✅ 通过

按本报告 §2.2 的复现步骤独立重写探针重跑（新临时项目 `~/workspace/t09r-probe/`，`node_modules` 符号链接复用仓库依赖，`environment: jsdom`，被测源码绝对路径导入，不污染仓库工作区；探针按惯例用后删除，日志 `~/workspace/t09r-p.log`；另有 G1/G3/G4 日志 `~/workspace/t09r-g*.log`），不只跑仓库新测试。探针 i18n mock 返回中文真实文案（来源标签两 key），其余 key 直通。

- **P2b（逻辑层，`probes/logic.test.ts`，1 项）**：规则 `iso-rule`（Unix 毫秒 / iso / UTC）+ 会话已存 custom（Unix 毫秒 / iso-tz / Asia/Shanghai），经 `setSessionColumnOverride` + `applySessionColumnOverrides` 得到合并快照后直调 `computeColumnPresentationPreview({ candidate: { mode: 'inherit' } })`，返回 `{ kind: 'text', text: '2024-01-15T11:00:00Z', interpretation: 'unix', sample: 1705316400000 }`；断言同时核实传入的合并快照未被改动（覆盖条目仍在，会话状态仍含该键）。✅ 通过。
- **P1b（面板层，`probes/panel.test.tsx`，1 项）**：规则下初显预览 `2024-01-15T11:00:00Z`、标签「规则：iso-rule」→ 选 custom 保存后会话持有覆盖 → 重开（合并快照）草稿为 custom，预览 `2024-01-15T19:00:00+08:00`、标签「列覆盖」→ 点「跟随规则」后**保存前**预览立即为 `2024-01-15T11:00:00Z`、标签立即为「规则：iso-rule」→ 保存后会话条目删除，`applySessionColumnOverrides` 原样返回基底快照（同一对象），重开面板预览与标签均为规则值，计划层 `computeColumnPresentationPreview` 复核亦为规则值。✅ 通过。
- 探针合计 **2/2 通过**，退出码 0。探针过程说明：首版面板探针 1 处期望写法有误——生效来源元素的 `textContent` 含 `columnPresentation.effectiveSource: ` 前缀（元素内标签+值），已改为 `toContain` 断言，实际标签值确为「规则：iso-rule」/「列覆盖」；属探针脚手架问题，非代码问题。另：运行期 stderr 有一条 `common.close` 缺译警告，与首轮 O-7 同一既有模式，非本次引入。

### R3：G1 定向全集 ✅ 通过

`npx vitest run packages/frontend/src/components/DataTable/__tests__/ColumnPresentationDialog.test.tsx packages/frontend/src/components/DataTable/__tests__/ColumnPresentationEntries.test.tsx packages/frontend/src/hooks/__tests__/useColumnPresentationSession.test.tsx packages/frontend/src/lib/valuePresentation/__tests__/columnPresentation.test.ts packages/frontend/src/lib/valuePresentation/__tests__/columnPresentationSession.test.ts packages/frontend/src/components/DataTable/__tests__/DataTable.contextMenu.test.tsx --retry=0`：退出码 0，**6 文件 58/58 通过**（Dialog 20、Entries 2、hook 3、columnPresentation 16、session 6、contextMenu 11；日志 `~/workspace/t09r-g1.log`）。修复人申报「5 文件 47/47」与实测一致：5 个新测试文件 20+2+3+16+6 = 47/47，另加既有 contextMenu 11 项，合计 58。较首轮 56/56 净增 2 项，恰为两条新回归测试；`--reporter=verbose` 复核确认两条新测试均在列且通过（日志 `~/workspace/t09r-g1v.log`）。

### R4：双引擎类型门禁与命名门禁 ✅ 通过

- `npx -y pnpm@12.18.0 typecheck`（tsrs 0.18.0-ts7.1.0-dev.20260929）：退出码 0，`OK`（日志 `~/workspace/t09r-g3a.log`）。
- `npx -y pnpm@12.18.0 typecheck:tsc`（tsc 7.0.2）：退出码 0，`OK`（日志 `~/workspace/t09r-g3b.log`）。
- `node scripts/check-test-module-naming.mjs`：退出码 0，`ok`（1837 Rust 文件、256 cfg(test) 模块；2586 TS 文件、8 wdio spec 树；日志 `~/workspace/t09r-g4.log`）。

**新回归测试「无修复时失败」的复核（未重做 stash，代码审读方式）**：修复人称已用 `git stash` 验证两条新测试在无修复时失败。本次复核未重做 stash，改为读测试代码确认其断言确会区分修复前后：逻辑回归的已存覆盖用 `UNIX_MS_OPTIONS`（iso-tz / Asia/Shanghai → `2024-01-15T19:00:00+08:00`），与规则值 `2024-01-15T11:00:00Z` 逐字不同——无修复时 inherit 分支拿合并快照建计划，覆盖优先于规则，预览断言必失败；面板回归同理，选 inherit 后预览与来源标签两处断言在无修复时均仍为覆盖值。两处断言均与修复行为直接挂钩，无修复时不可能通过，故修复人的 stash 说法可信。另：首轮 §4/§7 的 P2b 实测（无修复时返回 `2024-01-15T19:00:00+08:00`）与此一致，是同一失败的独立证据。

**T09-BUG-001 验证通过并关闭。**结论更新为「通过，可做收尾提交」（见 §6）。未发现修复引入的新问题；探针项目已按惯例删除，日志见 `~/workspace/t09r-p.log` 与 `~/workspace/t09r-g*.log`。
