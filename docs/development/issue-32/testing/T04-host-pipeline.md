# T04 测试报告：宿主展示管线与列展示计划（valuePresentation）

- 被测对象：`packages/frontend/src/lib/valuePresentation/presentationPlan.ts`（465 行，新文件）+ `packages/frontend/src/hooks/useValuePresentation.ts`（104 行，新文件）+ `packages/frontend/src/lib/valuePresentation/index.ts`（桶导出 T04 追加 24 行）+ 自带测试 `presentationPlan.test.ts`（75 项）与 `useValuePresentation.test.tsx`（6 项）
- 分支：`feature/issue-32-value-presentation`（HEAD `4c7d97467`，作者 issue-32-agent；T04 两提交：`6c3c42a1b` feat + `4c7d97467` test，基底 `063996b42`；验证开始时工作区干净）
- 测试角色：tester 独立验证，未修改任何被测代码或测试
- 依据：`docs/archive/development/issue-32-value-presentation-design.md` §3 §5 §6 §7 §9 §10-A、`packages/extension-points/src/valuePresentation/contracts.ts` 与 `registry.ts`、T03 `timestampContribution.ts`
- 日期：2026-11-12

## 1. 测试环境与命令

环境：Node v24.22.1，vitest 4.1.10（仓库自带），tsc 7.0.2 / tsrs 0.18.0，系统时区 UTC（`Intl.DateTimeFormat().resolvedOptions().timeZone` = UTC；全部探针均显式传入时区与语言，不依赖机器默认值）。对抗性探针运行于独立临时项目 `/tmp/probe-t04/`（通过 `node_modules` 符号链接复用仓库依赖，`environment: jsdom`，被测源码以绝对路径导入，不污染仓库工作区；测试后 `git status` 复核仓库无任何测试方改动，探针用后删除）。

环境说明：本机 `PATH` 中无 `pnpm` 二进制（与 T02/T03 报告相同情况）；G2 改用 `npx -y pnpm@12.18.0 <script>` 调用，版本号实测 12.18.0，属等价调用，不影响门禁结论。

| 命令 | 用途 |
|------|------|
| `npx vitest run packages/frontend/src/lib/valuePresentation packages/frontend/src/hooks/__tests__/useValuePresentation.test.tsx --retry=0` | T04 相关包单测（排除根配置 retry 掩盖侥幸通过） |
| `npx -y pnpm@12.18.0 typecheck` / `npx -y pnpm@12.18.0 typecheck:tsc` | 双引擎类型门禁（tsrs / 权威 tsc；root / scripts / pack-ep 三程序） |
| `node scripts/check-test-module-naming.mjs` | 测试命名门禁 |
| `npx vitest run <同上路径> --retry=0 --coverage --coverage.include='…/presentationPlan.ts' --coverage.include='…/useValuePresentation.ts'` | 两新文件覆盖率（include 限定统计范围） |
| `/tmp/probe-t04` 探针项目（30 项，P1–P13 + P12b） | 对抗性行为验证（PRD 锚点、BUG-003 不回归、默认路由、三态缺失与顺序、覆盖三态、版本不一致、prepare 异常隔离、缓存键、覆盖键、查询结果、Hook 行为、默认快照/环境、全局配置体分工） |
| `grep` 审读（`any` / `as` / `new Date(` / `Date.parse` / `console.log` / `TODO`） | G5 的源码级严格性审读、分层与循环依赖核查 |

探针项目结构：`probes/t04-adversarial.test.ts`（P1–P13 共 29 项）+ `probes/t04-node-env.test.ts`（P12b，`@vitest-environment node`，1 项）。探针属一次性验证资产，位于系统临时目录，不进入仓库；各项方法已在下表内描述到可复现程度。

## 2. 测试项清单

### 2.1 既有验证（仓库门禁）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| G1 | T04 相关包单测全量 | `npx vitest run packages/frontend/src/lib/valuePresentation packages/frontend/src/hooks/__tests__/useValuePresentation.test.tsx --retry=0` | ✅ 6 文件 **283/283 通过**（presentationPlan 75、useValuePresentation 6、timestampContribution 46、timestampConfig 26、temporalMath 96、timestampFormat 34，与申报数量一致；退出码 0） |
| G2 | 类型门禁（tsrs 单引擎 + 权威 tsc） | `pnpm typecheck` / `pnpm typecheck:tsc`（经 npx pnpm 等价调用） | ✅ 两者均 OK（root / scripts / pack-ep 三程序；tsrs 0.18.0-ts7.1.0-dev.20260929、tsc 7.0.2；退出码均为 0）。根 tsconfig `include` 含 `packages/frontend/src`，新文件实际被覆盖 |
| G3 | 测试命名门禁 | `node scripts/check-test-module-naming.mjs` | ✅ ok（1837 Rust 文件、256 cfg(test) 模块；2561 TS 文件、8 wdio spec 树；退出码 0） |
| G4 | 两新文件覆盖率 | `npx vitest run <同上路径> --retry=0 --coverage`（双 `--coverage.include` 限定两新文件） | ✅ 两新文件四项指标全部 ≥80%（见下表）；进程退出码 0。本次因 include 限定统计范围，未触发 T03 报告中的全局阈值退出码 1 |
| G5 | 源码严格性审读 + 分层与循环依赖核查 | grep 两新文件 `any` / `as` / `new Date(` / `Date.parse` / `console.log` / `TODO`；核对 lib 不导入 components、hook 不读 Store/DOM/IPC/文件；核对桶导出导入边 | ✅ 通过（见 §2.3） |

> G4 明细（v8 报告，`--coverage` + 双 include）：
>
> | 文件 | % Stmts | % Branch | % Funcs | % Lines | 未覆盖行 |
> |------|---------|----------|---------|---------|----------|
> | `presentationPlan.ts`（新文件） | 96 | 92.77 | 100 | 95.94 | 154、209、350 |
> | `useValuePresentation.ts`（新文件） | 100 | 83.33 | 100 | 100 | 54（分支） |
> | 两文件合计（All files 行） | 96.9 | 92.13 | 100 | 96.84 | — |
>
> 未覆盖行均为防御性分支，不构成覆盖缺口：154 为 `resolvePresentationEnvironment` 的 `catch` 内 `localTimeZone = 'UTC'`（Intl 不可用时回退，jsdom/Node 下不可达）；209 为 `fallbackPresentationReason('ready')` 的防御性 `'plan-error'`（ready 一定携带 prepared，构造上不可达）；350 为 custom 路径 `validateConfig` 抛异常的 catch（契约要求 validateConfig 不抛）；hook 的 54 行为 `input.registry ?? valuePresentationRegistry` 的默认分支（自带测试一律传入显式 registry，全局单例默认分支未被覆盖；分支 83.33% 仍高于 80% 门禁）。

### 2.2 对抗性探针（/tmp/probe-t04，独立于 coder 自带测试编写）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| P1 | PRD 锚点经 plan.present | native + iso-tz + Asia/Shanghai 下 `2026-01-15T14:30:00Z`；unix 毫秒 + iso-tz + Asia/Shanghai 下 `1705316400000` | ✅ 通过。锚点 1 = `2026-01-15T22:30:00+08:00`（含 22:30 与 +08:00）；锚点 2 = `2024-01-15T19:00:00+08:00`。附注：`dataType: 'int8'` 且无 cellType 的列经 `resolveCellType` 得 number，走 `default` 而非时间路由；unix 端到端复核改用显式 `cellType: 'datetime'` 的列（见 O-6） |
| P2 | BUG-003 不回归 | 同一 unix 毫秒计划下 `1705316400123` 与 `1705316400000` | ✅ 通过。余数非 0 → `2024-01-15T19:00:00.123+08:00`（含 `.123`）；余数 0 → `2024-01-15T19:00:00+08:00`，不含 `.000` |
| P3 | 默认路由 + semanticKind 全映射表 | datetime（zoned/非 zoned）、date、time 列默认走 timestamp；number/text/enum/uuid/bool/json/bytes/unknown 列 → default；15 组映射（含 cellType 缺省推导：`timestamptz`→instant、`timestamp`→local-datetime、无 dataType→other） | ✅ 通过。4 时间列均为 ready 且 rendererId 为 `datazen.timestamp`；8 非时间列均为 `default`，present 一律 `{ kind: 'unchanged', reason: 'no-renderer' }`（string/number/null 三值均同）；15 组映射全部符合申报 |
| P4 | 三态缺失与顺序 | 逐项只缺一个（enabled 空 / registry 空 / configs 空），另两项齐备；再测两两叠加的优先级 | ✅ 通过。enabled 缺失 → `disabled`；registry 未注册 → `missing-renderer`；configs 缺失 → `missing-config`；顺序与申报一致：disabled 优先于 missing-renderer，missing-renderer 优先于 missing-config |
| P5 | 覆盖三态 | 计数型假贡献（validate/prepare 计数）+ timestamp 计数包装：raw、custom 有效、custom 无效、custom 错配 rendererId、inherit；custom 有效/无效各补 enabled 为空的 bypass 对照 | ✅ 通过。raw → `raw-override` 且 prepare/validate 调用均为 0；custom 有效 → ready 且 bypass enabled（enabled 为空仍 ready，validate 1 次、prepare 1 次，present 得 `FAKE`）；custom 无效 options → `invalid-config`（prepare 0），enabled 为空时仍为 `invalid-config` 而非 `disabled`；custom 错配 rendererId → `missing-renderer`（rendererId 原样保留为 `not.registered`）；inherit → 走默认路由 ready |
| P6 | 版本不一致 | configs 中 configVersion 2（贡献为 1），timestamp 计数包装 | ✅ 通过。→ `invalid-config`，prepare 调用 0 次，present 为 unchanged `invalid-config` |
| P7 | prepare 抛异常隔离 | 抛错假贡献作一列 custom 目标，计数型假贡献作另一列 custom 目标 | ✅ 通过。抛错列 → `error` / `plan-error`，present 不抛异常；其余列仍 ready 且 present 正常；计数贡献 prepare 恰 1 次 |
| P8 | 缓存键 | 基准键逐项单因子变化：配置修订、贡献修订、locale、localTimeZone、来源身份（connectionId）、任一列 dataType、列顺序、columnsRevision；相同输入复算；只改列名大小写 | ✅ 通过。相同输入键稳定；所列任一因子变化键均变化；只改列名大小写（`created_at`→`Created_At`）键变化，大小写保留 |
| P9 | columnOverrideKey | database/namespace 缺省 vs 空串；database/表名/列名大小写 | ❌ **1 项与设计不符 → BUG-004（轻微级）**。大小写不同 → 键不同 ✅；但缺省与空串 → **同键** `["c","","","t","col"]`，与设计 §6「缺省字段与空字符串不能无意碰撞」直接冲突（coder 自带测试 `presentationPlan.test.ts:248` 将该归一行为锁定为期望行为；代码注释称「键形状固定，不碰撞」与事实不符）。详见 §4 |
| P10 | 无 source（查询结果） | overrides 中有同名列 raw/custom 键，但 `buildColumnPresentationPlans` 不传 source | ✅ 通过。两覆盖均不命中（`column.source` 为 undefined）；时间列仍走默认路由 ready，present 正常出 `2026-01-15T22:30:00+08:00` |
| P11 | Hook 行为 | renderHook + 全新 registry：注册/注销往返、snapshot revision 变化重建（prepare 计数可观测）、presentValue 查不到列、重名列 planByColumn 取第一项；另加 registry 实例替换探针 | ✅ 通过（注册后 missing-renderer→ready，prepare 计数 0→1；snapshot revision 1→2 重建，prepare 计数 2→3；unregister 后变回 missing-renderer；presentValue 查不到列 → `no-plan`；重名列 plans 各有一项，`planByColumn.get` 恒为 `plans[0]`）。另见 O-2：同 revision 不同内容的 registry 实例替换后 plans 不重建（stale 已复现，非缺陷） |
| P12 | 默认快照与环境解析 | `createDefaultValuePresentationSnapshot()` 默认快照 + 时间列；`resolvePresentationEnvironment()` 在 jsdom 下（显式/缺省/空串 locale） | ✅ 通过。默认快照 revision 0、三空集合，时间列一律 `disabled`，present 为 unchanged `disabled`；jsdom 下不抛异常，locale 与 localTimeZone 均非空，显式 locale 优先，空串 locale 回退不为空 |
| P12b | 环境解析（Node） | `t04-node-env.test.ts`，`@vitest-environment node`：`resolvePresentationEnvironment()` | ✅ 通过。不抛异常；locale 为 `en-US`，localTimeZone 非空（本机为 UTC）。注：首轮探针曾假设 Node 无 navigator 而失败，实测 Node v24 自带全局 navigator（language `en-US`）——为探针前提错误，已修正探针，代码的 `typeof navigator` 守卫本身正确 |
| P13 | 裁决 7：全局配置体无效时的分工 | configs 中 `configVersion: 1` 但 config 体为 `{ bogus: true }`（timestamp 计数包装） | ✅ 行为与申报一致（见 O-1 评审）。prepare 确被调用 1 次；计划状态为 `ready`（宿主无法区分 T03 的 fixedUnchanged stub 与真实 prepared）；present 回退 `{ kind: 'unchanged', reason: 'invalid-config' }`，无崩溃、无错误文本、无行值记录 |

> 探针过程说明：首轮 30 项中 1 项失败系**探针自身缺陷**（P12b 误以为 Node v24 无全局 navigator；实测 `typeof navigator === 'object'`，language 为 `en-US`，已修正探针前提），修正后 30/30 通过；除 P9 暴露的 BUG-004 外，被测代码在全部探针下无失败。最终探针 30/30 通过（含 P12b）。

### 2.3 契约类型严格性审读

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| T1 | any 泄漏 / 断言滥用 | 对两新文件 grep `any`、`as`、`@ts-ignore`、`@ts-expect-error`、`eslint-disable` | ✅ 基本通过。`presentationPlan.ts`：`any` 仅出现在注释「禁 any」字样，`as` 0 处；`useValuePresentation.ts`：`as` 1 处（`:89`，`map as ReadonlyMap<string, ColumnPresentationPlan>`——`Map` 本就可赋值给 `ReadonlyMap`，该断言多余但无害，未跳过任何验证；见 O-4）。两文件均无 `@ts-ignore` / `@ts-expect-error` |
| T2 | 隐式时间解析 | grep 两新文件 `new Date(` / `Date.parse` | ✅ 0 处。时间解析全部委托 T02/T03；`resolvePresentationEnvironment` 仅用 `Intl.DateTimeFormat().resolvedOptions().timeZone`（有 try/catch 回退 UTC）与 `navigator.language` |
| T3 | 分层：lib 不导入 components | 核对 `presentationPlan.ts` 导入边 | ✅ 无 components 导入。仅导入 `@datazen/extension-points`（类型 + 全局 registry）、同目录 `../cellTypes`（`resolveCellType` / `isZonedCellType`）、`./timestampContribution`（`TIMESTAMP_CONTRIBUTION_ID` 常量）；与文件头「自有最小类型，不从 components/TableHeader 导入 ColumnDef」的声明一致 |
| T4 | hook 不读 Store/DOM/IPC/文件 | 核对 `useValuePresentation.ts` 导入与调用 | ✅ 无 Store/DOM/IPC/文件读取。仅导入 react（`useCallback` / `useMemo` / `useSyncExternalStore`）、`@datazen/extension-points`（类型 + 全局 registry）、`../lib/valuePresentation/presentationPlan`；snapshot 由调用方传入 |
| T5 | 桶导出循环依赖 | 核对 `index.ts` T04 追加导出与两新文件的导入边 | ✅ 无循环依赖嫌疑。`presentationPlan.ts` 导入同目录 `timestampContribution`（不经过桶 index）；hook 直接导入 `presentationPlan`（不经过桶 index）；`index.ts` 仅追加具名类型与函数导出；T04 改动范围 `git diff 063996b42..HEAD --stat` 为 5 文件 +1615 行，无其他文件改动 |

## 3. 设计符合性逐条结论

对照设计文档（§3 / §5 / §6 / §7 / §9 / §10-A 相关项）：

**§3 多贡献注册（T04 范围：宿主消费侧）**

| 条目 | 结论 |
|------|------|
| 新增独立 `ValuePresentationRegistry`，不将多展示器注册为现有单实现 EP 的同名实现 | ✅ 符合（T01 已交付；T04 只经 `registry.get` / `registry.revision` / `registry.subscribe` 消费集合注册中心，未触碰单实现 EP） |
| 注册、卸载触发订阅，增加宿主维护的贡献版本，令所有相关计划失效 | ✅ 符合（hook 经 `useSyncExternalStore` 订阅 revision，注册/注销触发重建；P11 行为级验证。贡献修订纳入缓存键，见 §7） |
| 贡献顺序不决定抢占字段，由配置指定 rendererId，原生时间默认路由至 `datazen.timestamp` | ✅ 符合（custom 覆盖直接指定 rendererId；默认路由时间列取 `TIMESTAMP_CONTRIBUTION_ID` 常量；P3/P5 验证） |
| 贡献缺失时保留配置，使用默认展示，配置面板说明缺失原因 | ✅ 基本符合（缺失时计划保留 rendererId 与 `missing-renderer` 原因，present 回退默认展示；P4 验证。"配置面板说明缺失原因"属 T05/T09 的 UI 消费，T04 已交付原因码这一原语） |
| 重复 ID 拒绝、卸载句柄绑定实例、激活失败清理 | ➖ 范围外（T01 已验证通过；T04 未重测注册中心本身） |

**§5 配置模型与规则（T04 范围：列覆盖三态与宿主校验）**

| 条目 | 结论 |
|------|------|
| 列覆盖三态 inherit / raw / custom（§5 形状） | ✅ 符合（P5/P10；custom 的 options 是未经校验的原始 JSON，由宿主交给目标 rendererId 的 `validateConfig` 验证，与 contracts.ts 一致） |
| 源时区只出现一处；unix 携带 sourceTimeZone 即拒；`native` 不把数字自动解释成 Unix；Unix 必须显式指定单位 | ➖ 范围外（T02/T03 已验证；T04 不解释 options，只选渲染器与路由） |
| 规则数组 / 正则安全 / 规则数量与输入长度设限 | ➖ 范围外（规则系统属阶段 C / T05+，不在 T04） |

**§6 配置加载、持久化与同步（T04 范围：快照形状、默认快照、覆盖键）**

| 条目 | 结论 |
|------|------|
| 宿主负责加载配置，验证后以不可变快照传入 prepare | ⚠️ 部分符合，见 O-1。快照形状（revision / environment / configs / enabledRendererIds / overrides）符合；但全局配置体不在宿主边界验证——宿主只认版本与存在性，配置体有效性由 T03 prepare 内部校验（P13；coder 自带测试 `:543` 锁定该分工）。行为安全、回退正确，但与本条及 contracts.ts「JSON 输入在宿主边界交给 validateConfig 验证」的字面要求不一致 |
| 表覆盖键使用结构化字段（作用域 + connectionId + database + namespace + table + column）；保留标识符大小写语义，不自行统一小写 | ⚠️ 大小写符合（P8/P9），但缺省/空串处理不符合——见 BUG-004。作用域由存储层负责，宿主键不含作用域，符合「存储层负责用户/组织作用域」的分工（T06） |
| 加载态 1：初次加载中使用同一个默认快照 | ✅ 符合（`createDefaultValuePresentationSnapshot`：revision 0、三空集合，时间列一律 disabled；P12 验证） |
| 加载态 2–5（就绪发布、后续刷新、切换上下文、保存失败） | ➖ 范围外（快照发布与持久化属 T06/T11；T04 只定义快照形状与默认快照） |
| 历史配置缺字段默认关闭；未知配置版本或暂时缺失贡献时保留原文，不用默认值写回覆盖 | ✅ 部分符合（版本不一致 → `invalid-config` 且不调 prepare，P6；缺失贡献 → `missing-renderer` 且配置保留在快照内，P4。持久化"不写回"属 T06） |

**§7 共享展示管线（T04 范围：计划、缓存、隔离）**

| 条目 | 结论 |
|------|------|
| 面板层共享 controller/hook，列计划经 props/context 注入 DataTable 与 DetailPanel，组件不直接读 Store 或 IPC | ✅ 符合（hook 已交付，T4 审读确认不读 Store/DOM/IPC/文件；向 DataTable/DetailPanel 的注入属 T05 接线） |
| 计划缓存键至少包含来源身份、列元数据版本、配置修订、贡献修订、locale、localTimeZone | ✅ 符合（P8：所列因子逐项验证；另含 columnsRevision。附注：覆盖内容与 enabled 集合只经 snapshot.revision 间接覆盖，见 O-5） |
| 规则与 Intl 格式器在 prepare 阶段创建；不逐格编译、不在 render 调 IPC | ⚠️ 部分符合，见 O-3。prepare 确为每计划一次（P5/P11 计数验证）、present 不调 IPC；但 locale 分支的 `Intl.DateTimeFormat` 仍为每格新建（T02/T03 既有实现，T03 报告 O-1），T04 的计划缓存只避免跨渲染重复 prepare，未改变每格 Intl 构造 |
| 系统时区变化通过环境快照更新触发失效 | ✅ 机制具备（localTimeZone 纳入缓存键，P8；快照更新本身属 T06/T11 的发布链路） |
| 扩展异常由宿主统一隔离；普通非法值返回 unchanged，不作为扩展异常 | ✅ 符合（prepare 抛异常 → `error` / `plan-error`，其余列不受影响，present 不抛；P7 验证） |
| 日志记录贡献与原因码，不记录原始行值 | ⚠️ 未接线（T04 模块内无日志调用，G5 确认 0 处 `console.log`；计划本身携带 rendererId + reason，可供未来日志消费。不记录行值一点符合——present/prepare 调用不记录原始行值） |
| 编辑分支继续接收原值；NULL 优先；BIGINT 在默认数字分支返回前可被显式时间计划处理 | ➖ 范围外（CellRenderer/DetailPanel 接线属 T05；T04 只交付计划本身。附注：BIGINT unix 列默认语义为 number，需驱动/规则将其判定为 datetime 后才走时间路由，见 O-6） |

**§9 文件改动边界**

| 条目 | 结论 |
|------|------|
| 拟新增 `packages/frontend/src/lib/valuePresentation/`（配置快照适配、列计划、默认回退与统一展示）与 `packages/frontend/src/hooks/useValuePresentation.ts` | ✅ 符合（两新文件 + 桶导出追加，落点与设计清单一致；T04 范围 `git diff 063996b42..HEAD --stat`：5 文件 +1615 行，零新依赖、零旧文件修改） |
| 需修改的既有位置（CellRenderer、DetailPanel、ContentViewDrawers、settingsStore、设置 DTO 等） | ➖ 范围外：均未改动，属 T05+，不构成缺口 |

**§10-A 验证清单（阶段 A：贡献与展示基础，与 T04 相关项）**

| 条目 | 结论 |
|------|------|
| 验证重复 ID、旧卸载句柄、激活失败清理 | ✅（T01 已验证通过；T04 未重测） |
| cell/aside 默认一致和编辑原值 | ➖ 范围外：属 T05 网格与详情接线；T04 未接线 CellRenderer/DetailPanel，index.ts 注释已声明属 T05，不构成缺口 |

## 4. Bug 列表

| 编号 | 标题 | 状态 |
|------|------|------|
| BUG-004 | `columnOverrideKey` 将缺省字段与空字符串归一为同一键，与设计 §6「不能无意碰撞」不符 | 已关闭（2026-11-12 回归验证通过，详见 bug 文件「回归验证」段及本报告 §7） |

**BUG-004 — 覆盖键缺省/空串碰撞（轻微级）**

- 位置：`packages/frontend/src/lib/valuePresentation/presentationPlan.ts:122-132`（`columnOverrideKey`，归一在 `:125-126` 的 `?? ''`）；同一归一亦见于缓存键来源段 `:447-448`（`computePresentationPlanCacheKey` 的 `sourcePart`，两者一致，故缓存键与覆盖键之间无错位）。
- 复现输入：`columnOverrideKey({ connectionId: 'c', table: 't', column: 'x' })` 与 `columnOverrideKey({ connectionId: 'c', database: '', namespace: '', table: 't', column: 'x' })`。
- 实际输出：两者均为 `'["c","","","t","x"]'`（探针 P9 以列名 `'col'` 复核，同形状输出为 `'["c","","","t","col"]'`），即缺省与空串产生同一键。
- 正确应为：设计 §6 要求「缺省字段与空字符串不能无意碰撞」，两者应得不同键。
- 根因：`database ?? ''` / `namespace ?? ''` 把"字段缺省"与"字段为空串"映射为同一空串分量；文件头注释（`:118-121`）称「键形状固定，不碰撞」与事实不符——键字符串本身就是碰撞的证据。
- 为什么 coder 自带测试没抓住：该归一行为被自带测试显式锁定为期望（`presentationPlan.test.ts:248`，`it('缺省字段与空字符串归一为同一键')`），即有意设计而非疏漏；但该有意设计与设计文档 §6 的句子直接冲突，测试锁定的是一条与设计不符的行为。
- 影响：现实中可忽略——驱动返回的 database/namespace 要么缺省、要么为真实标识符，空字符串标识符在受支持数据库中不出现；且同一归一同时用于覆盖键与缓存键，两者无错位。风险窗口在 T06：覆盖键一旦成为持久化身份，若某驱动返回空串，覆盖会在"无数据库"表与"空串数据库"表之间串用。
- 修复建议（供 coder / 协调方定夺，不改被测代码）：首选修 `columnOverrideKey` 与缓存键 `sourcePart`，缺省用 `null` 而非 `''`（`JSON.stringify([connectionId, database ?? null, namespace ?? null, table, column])`），使缺省与空串得不同键；同步把自带测试 `:248` 的期望由"归一为同一键"改为"缺省与空串键不同"，并补一条缓存键来源段的对照断言。或由协调方裁决修订设计 §6（明确缺省与空串归一为可接受的有意裁决），则本 bug 关闭为"设计修订"，代码与测试不动。
- 归属说明：缺陷在 T04 提交（`6c3c42a1b`）引入；T01–T03 的键/注册逻辑不受影响。

## 5. 观察项（非缺陷，供 coder / 后续任务参考）

- **O-1（裁决评审：全局配置体无效时宿主只认版本与存在性）**：申报行为实测成立（P13；coder 自带测试 `presentationPlan.test.ts:543` 锁定该分工）。评审：分工可接受，非缺陷。最终行为安全——无效全局配置经 T03 prepare 返回 `invalid-config` 回退，无崩溃、无错误文本、无行值记录；且 custom 覆盖的 options 已在宿主边界验证，宿主并非完全不验证。但三点与设计字面要求有张力：(a) 设计 §6「宿主负责加载配置，验证后以不可变快照传入 prepare」与 contracts.ts「JSON 输入在宿主边界交给 validateConfig 验证」要求宿主边界验证，而全局配置体实际由贡献 prepare 内部验证；(b) 计划状态失真：全局无效时计划状态为 `ready`，但该计划永远只返回 unchanged `invalid-config`，T05 若以 `status === 'ready'` 判断"可渲染"会被误导（宿主无法区分 T03 的 fixedUnchanged stub 与真实 prepared，`PreparedPresentation` 无状态字段）；(c) 不对称：custom options 在宿主边界验证，全局配置不在。建议 T06 持久化链路落地时，由宿主在快照组装处对全局配置调一次 `validateConfig`（失败则保留旧快照或产 `invalid-config` 计划），或由协调方裁决修订设计表述；在此之前不阻塞收尾。
- **O-2（registry 身份不在缓存键内，替换实例同 revision 时 plans 不重建）**：探针复现（P11 附加项）：registry A（注册后注销，revision 2，空表）→ plans 为 missing-renderer；rerender 换 registry B（注册 timestamp + 另一假贡献，revision 同为 2，有 timestamp）→ `contributionRevision` 不变、缓存键不变，plans 仍为 missing-renderer（stale）。根因：`useValuePresentation` 的 `useMemo` 依赖只有缓存键结果，而缓存键不含 registry 实例身份；`useSyncExternalStore` 虽重订阅，但 revision 值相同不触发重建。评审：理论问题，非缺陷——实际使用中 registry 是全局单例、从不替换；hook 的 registry 入参主要为测试注入。若未来存在热替换 registry 的场景，应在缓存键或 memo 依赖中加入 registry 身份，或在文档中声明 registry 入参必须稳定。
- **O-3（prepare 每计划调一次属实；Intl 每格新建仍在）**：coder 申报的"prepare 每计划调一次"经计数探针核实：每列建计划恰调 1 次 prepare（P5：custom 有效列 validate 1 次、prepare 1 次；P11：注册后 1 次、snapshot revision 变化重建再 1 次）。但设计 §7「规则与 Intl 格式器在 prepare 阶段创建」只实现一半：prepare 确在建计划时一次解析配置与时区（T03），而 locale 分支的 `Intl.DateTimeFormat` 仍为每格新建（T02/T03 既有实现，T03 报告 O-1 的遗留）；T04 的计划缓存只避免跨渲染重复 prepare，未改变每格 Intl 构造。属 T02/T03 遗留，非 T04 引入，不判缺陷；T05 接线后若出现可测性能回归，再于贡献层缓存格式器。
- **O-4（未覆盖行均为防御性分支；hook 有一处多余 `as`）**：G4 未覆盖行 154/209/350 均为防御性兜底（见 §2.1 注），不构成覆盖缺口；hook `:54` 的默认 registry 分支未被自带测试覆盖（分支 83.33% 仍高于门禁），建议补一条"不传 registry 时用全局单例"的回归断言（需小心不污染全局单例，或以只读断言验证）。另 `useValuePresentation.ts:89` 的 `map as ReadonlyMap<…>` 断言多余（`Map` 本就可赋值给 `ReadonlyMap`），无害，可顺手去掉。
- **O-5（缓存键经 snapshot.revision 间接覆盖覆盖与 enabled）**：缓存键不直接包含 overrides 内容与 enabledRendererIds，而以 `snapshot.revision`（hook 传入 `configRevision`）为代理。评审：符合快照契约——快照不可变，任何内容变化都应产生新 revision；但若某调用方在不 bump revision 的情况下换 overrides/enabled 对象，plans 会静默过期。T06 落地快照发布时须保证"内容变则 revision 变"的不变量（设计 §6 的"成功保存后发布作用域与配置修订通知"即此意）。
- **O-6（BIGINT unix 列默认不走时间路由）**：探针 P1 附注：`dataType: 'int8'` 且无 cellType 的列经 `resolveCellType` 得 number → semanticKind number → `default`，不走 timestamp。评审：符合设计分工——T04 只消费 cellType 推导的 semanticKind，驱动特殊类型判定属 §8.1（DatabaseTypeMeta/驱动声明）与阶段 C 规则系统；BIGINT 毫秒列需由驱动 hints 或规则将其判定为 datetime 后才走时间路由。T05 接线与规则任务需注意该前置条件，非缺陷。
- **O-7（设计 §7 的日志行未接线）**：T04 模块内无日志调用（G5 确认 0 处 `console.log`），「日志记录贡献与原因码，不记录原始行值」尚未落地为调用点；但计划本身携带 rendererId + reason，具备日志所需的全部字段，且"不记录行值"已由 present/prepare 的实现保证。建议 T05 接线或 T11 回退任务补日志消费点时复用 plan.reason，非缺陷。
- **O-8（探针资产）**：/tmp/probe-t04 为一次性验证项目，重启后失效；本报告 §2.2 各项方法已描述到可复现程度。建议将 P9 的键碰撞对照、P11 的 registry 替换 stale 探针与 P13 的全局无效分工断言核对是否值得吸收为仓库回归测试（P9 的期望方向取决于 BUG-004 的修复/裁决结果）。

## 6. 结论

**通过，可做收尾提交。**

- 既有门禁全绿：相关包 283/283（`--retry=0` 复跑确认，含新增 presentationPlan 75 项与 useValuePresentation 6 项）、`pnpm typecheck` 与 `pnpm typecheck:tsc` 双引擎通过、命名门禁通过；两新文件覆盖率四项指标全部 ≥80%（presentationPlan 96 / 92.77 / 100 / 95.94；useValuePresentation 100 / 83.33 / 100 / 100；本次进程退出码 0）。
- 对抗性探针 30 项（P1–P13 + P12b）中 29 项通过：PRD 双锚点经 plan.present 精确命中、BUG-003 不回归（`.123` 保留、余数 0 不补 `.000`）、默认路由与 15 组 semanticKind 映射全部正确、三态缺失与顺序（disabled → missing-renderer → missing-config）逐项验证、覆盖三态（含 raw 不调 prepare、custom bypass enabled、inherit 走默认路由）全部正确、版本不一致不调 prepare、prepare 抛异常隔离且其余列仍 ready、缓存键全部失效因子逐项验证、查询结果下覆盖不可用、Hook 注册/注销/重建/no-plan/重名列全部正确、默认快照时间列一律 disabled、环境解析在 jsdom 与 Node 下均不抛异常。
- 唯一记录的 **BUG-004（轻微级）**：`columnOverrideKey` 将缺省字段与空字符串归一为同一键，与设计 §6「缺省字段与空字符串不能无意碰撞」不符（§4）。现实中无用户可见影响（空串标识符不出现；覆盖键与缓存键归一一致、无错位），修复为 1 行级改动（缺省改 `null`）加 1 条自带测试期望修正；**不阻塞 T04 收尾提交**，但须在 T06 持久化链路落地前修复或由协调方裁决修订 §6 表述（该键届时将成为持久化身份）。
- 观察项 8 条均为非缺陷（O-1 全局配置体分工、O-2 registry 身份不在缓存键内、O-3 Intl 每格新建遗留、O-4 防御性未覆盖行与多余 `as`、O-5 revision 代理不变量、O-6 BIGINT 列路由前置条件、O-7 日志未接线、O-8 探针资产），其中 O-1 与 O-2 建议在 T06 设计评审时一并定夺。
- T04 交付的宿主展示管线本身（465 + 104 行）与自带 81 项测试未发现阻塞性缺陷；tasks.md 状态更新与 docs close commit 不在本次验证范围内，由协调方执行。

## 7. 回归验证记录（BUG-004 修复）

2026-11-12，tester 对 BUG-004 的修复（`7f083faa2`：`columnOverrideKey` 与缓存键来源段各两处 `?? ''` → `?? null` + 注释修正）+ 回归测试（`1efa2cd41`）做了独立回归：门禁复跑 `npx vitest run packages/frontend/src/lib/valuePresentation packages/frontend/src/hooks/__tests__/useValuePresentation.test.tsx --retry=0` **283/283**（presentationPlan 75、useValuePresentation 6、timestampContribution 46、timestampConfig 26、temporalMath 96、timestampFormat 34；退出码 0）、`pnpm typecheck` 与 `pnpm typecheck:tsc` 双引擎均 OK（tsrs 0.18.0 / tsc 7.0.2，root / scripts / pack-ep 三程序）、命名门禁 `node scripts/check-test-module-naming.mjs` ok；代码审读确认修复 diff 最小（仅 `presentationPlan.ts`，7 行增 / 6 行删）、columns 段 `dataType ?? ''` 未被动（fix diff 中 0 行 `dataType` 变化）、注释与事实一致（缺省用 `null`、空串保留为空串、二者键不同，仍不做大小写归一）；`:248` 的期望修正正确（旧期望「归一为同一键」+ `toBe` 在修复前代码上成立、修复后必失败，编码的正是与设计 §6 冲突的 bug 行为；同文件其余既有断言一字未动）；`/tmp/probe-bug004` 独立探针 11 项（R1–R6：原始复现缺省键 `["c",null,null,"t","x"]` vs 空串键 `["c","","","t","x"]`、单字段 database / namespace 对照、缓存键对照与稳定性、覆盖查找端到端双向不串用、大小写语义与 PRD 锚点及 BUG-003 不回归、`:248` 裁决复核）全绿，退出码 0，探针用后已删除。**BUG-004 验证通过并关闭**（详见 bug 文件「回归验证」段）。结论更新：§6「通过，可做收尾提交」不变——BUG-004 首轮已判定不阻塞收尾，修复与回归均已完成，**T04 可做收尾提交**（tasks.md 状态更新与 docs close commit 不在本次验证范围内，由协调方执行）。
