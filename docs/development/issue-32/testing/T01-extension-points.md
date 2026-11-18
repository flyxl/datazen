# T01 测试报告：扩展点契约与集合注册（valuePresentation）

- 被测对象：`packages/extension-points/src/valuePresentation/`（contracts / registry / lifecycle / index / 16 项自带测试）、`packages/extension-points/src/lifecycle.ts`（激活失败 LIFO 清理）、`packages/extension-points/src/index.ts`（追加导出）、根 `vitest.config.ts`（include 泛化）
- 分支：`feature/issue-32-value-presentation`（工作区未提交改动，coder 交付态）
- 测试角色：tester 独立验证，未修改任何被测代码
- 依据：`docs/archive/development/issue-32-value-presentation-design.md` §2 §3 §4 §10-A、`AGENTS.md`
- 日期：2026-11-11

## 1. 测试环境与命令

环境：Node v24.22.2，pnpm workspace，vitest 4.1.10（仓库自带），tsc 7.0.2 / tsrs 0.18.0。对抗性探针运行于独立临时项目 `/tmp/probe-t01/`（通过 `node_modules` 符号链接复用仓库依赖，`environment: node`，不污染仓库工作区；测试后 `git status` 复核仓库无任何测试方改动）。

| 命令 | 用途 |
|------|------|
| `npx vitest run packages/extension-points` | EP 包全部单测 |
| `npx vitest run packages/extension-points --retry=0` | 排除根配置 `retry: 2` 掩盖侥幸通过 |
| `pnpm typecheck` / `pnpm typecheck:tsc` | 双引擎类型门禁（root / scripts / pack-ep 三程序，均覆盖 `packages/extension-points`） |
| `node scripts/check-test-module-naming.mjs` | 测试命名门禁 |
| `npx vitest run scripts/__tests__/select-codegen-sensitive-tests.test.ts scripts/__tests__/probe-exclusion.test.ts scripts/__tests__/check-test-module-naming.test.ts scripts/__tests__/typecheck.test.ts` | 与 vitest 配置相关的门禁单测 |
| `node scripts/select-codegen-sensitive-tests.mjs` | include 泛化对 codegen 敏感选择的影响 |
| `/tmp/probe-t01` 探针项目（19 项，P1–P19） | 对抗性行为验证（注册中心语义 10 项、加载器失败清理 5 项、loadFromUrl 3 项、并发竞态 1 项） |

探针项目结构：`probes/registry-adversarial.test.ts`（P1–P10）、`probes/loader-cleanup.test.ts`（P11–P15）、`probes/load-from-url.test.ts`（P16–P18，配合 `fixtures/ep-fail.mjs`、`fixtures/ep-good.mjs` 本地 ESM 夹具，经 `loadFromUrl` 的原生动态 import 加载）、`probes/concurrency.test.ts`（P19）。探针属一次性验证资产，位于系统临时目录，不进入仓库；各项方法已在下表内描述到可复现程度。

## 2. 测试项清单

### 2.1 既有验证（仓库门禁）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| G1 | EP 包单测全量 | `npx vitest run packages/extension-points` | ✅ 7 文件 70/70 通过（含新增 valuePresentation.test.ts 16 项，与申报数量一致） |
| G2 | 非侥幸通过 | 同上，`--retry=0` 复跑 | ✅ 70/70 通过，全确定性 |
| G3 | 类型门禁（单引擎） | `pnpm typecheck` | ✅ OK（root / scripts / pack-ep） |
| G4 | 类型门禁（权威 tsc） | `pnpm typecheck:tsc` | ✅ OK；root tsconfig `include: packages/extension-points` 与包 tsconfig `include: src` 均实际覆盖新文件（包程序含 `verbatimModuleSyntax: true`，新代码 `import type` 使用正确） |
| G5 | 测试命名门禁 | `node scripts/check-test-module-naming.mjs` | ✅ ok（2546 TS 文件扫描，`__tests__/*.test.ts` 约定符合） |
| G6 | 配置一致性门禁单测 | 4 个 scripts 门禁测试文件 | ✅ 53/53 通过 |

### 2.2 对抗性探针（/tmp/probe-t01，独立于 coder 自带测试编写）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| P1 | 重复 ID 抛错后注册表状态完整性 | 预注册 a、b + 双 listener 后以重复 id 调 register；断言原实例保留、列表顺序、revision 不变、listener 零触发、抛错后新 id 仍可注册 | ✅ 通过 |
| P2 | 重复 ID 失败不产生半写 | 重复注册失败后，原句柄仍具权威性（能移除原实例），id 释放后可重新注册 | ✅ 通过（throw 发生在 map 写入前） |
| P3 | 通知顺序与异常隔离 | 三个 listener 按订阅顺序调用；中间 listener 抛错后其余照常、后续轮次照常、注册流程不受影响 | ✅ 通过（顺序=订阅顺序；异常被 console.error 记录） |
| P4 | revision 失败路径零递增 | 覆盖：重复注册抛错、未知 id unregister、句柄双击、显式反注册后的陈旧句柄、get/list/subscribe/unsubscribe 读操作、空 reset；仅 3 次成功变更各 +1 | ✅ 通过 |
| P5 | reset 后陈旧句柄不误删 | reset → 同 id 重注册 → 旧句柄调用 no-op（新实例保留、revision 不变） | ✅ 通过 |
| P6 | notify 中自退订 | listener 内退订自身：本轮（快照）仍送达，后续轮次不再送达 | ✅ 通过 |
| P7 | listener 内有界再注册（重入） | listener 内注册链式新贡献 10 层：无崩溃、全部注册成功、revision=10、状态一致 | ✅ 通过 |
| P8 | listener 内反注册刚注册的 id | register(x) → notify → unregister(x) → notify → no-op：两跳终止，无死循环，revision +2 | ✅ 通过 |
| P9 | listener 内无界再注册（病态重入） | 不设上限的 listener 内 register：递归至栈耗尽终止（**非挂起**）；RangeError 被 notify 的 listener 隔离 catch 捕获并 console.error 记录，register 正常返回；注册表保持可查询、内部一致（list 长度=递归层数+1，revision 同步）、无跨实例污染 | ✅ 通过（记录为观察项 O-1，非缺陷：与既有 `ExtensionRegistry.notify` 行为逐字对齐，设计未要求重入防护） |
| P10 | listener 观察时机 | notify 期间 get/list 读到的是本次变更后的状态 | ✅ 通过（先写后通知） |
| P11 | 激活失败清理：半注册多资源 | activate 依序推入 [普通函数, 贡献A, Disposable, 贡献B, 普通函数] 后抛错：五资源严格 LIFO 释放（含两个贡献）、registry 清空、revision=4（2 注册 + 2 释放）、**deactivate 未被调用**、isLoaded=false、失败后 unload 同 id 无害、同 id 重载成功且贡献生效 | ✅ 通过 |
| P12 | 清理期 dispose 自身抛错 | 失败清理中某个 Disposable.dispose 抛错：错误被吞并 console.error、其余资源继续释放、load 仍以**原始激活错误**拒绝 | ✅ 通过 |
| P13 | 异步激活失败 | `async activate` 内注册贡献后 `await` 再抛错：清理照常、revision=2、未加载 | ✅ 通过 |
| P14 | 重载路径语义不变 | load 同 id 第二次：先对旧实例执行 deactivate + LIFO（既有语义），再激活新模块；卸载新实例正常 | ✅ 通过 |
| P15 | 失败隔离 | 一个扩展激活失败不影响另一已加载扩展及其贡献 | ✅ 通过 |
| P16 | loadFromUrl 失败路径 | 本地 ESM 夹具（activate 推 3 资源后抛错）经 `loadFromUrl` 原生动态 import 加载：拒绝、LIFO 清理、deactivate 未调用、未加载 | ✅ 通过（loadFromUrl 仅委托 load()，改动后的清理链路完整生效） |
| P17 | loadFromUrl 成功路径 | 正常夹具加载/卸载：deactivate 先行 + LIFO 释放 | ✅ 通过 |
| P18 | loadFromUrl 模块缺失 | 不存在的入口：以 import 错误拒绝，loader 状态干净 | ✅ 通过 |
| P19 | 同 id 并发 load 竞态 | 双 load 以受控 gate 交错：后完成者覆盖 activeExtensions 条目，先完成者 context 的订阅永久泄漏（unload 无法触达） | ⚠️ 复现成功 → **BUG-001（一般级，既有行为，非 T01 引入；改动前代码 `git show HEAD` 核实为同一形态）** |

> 探针过程说明：首轮 4 项探针失败均系**探针自身缺陷**（P4 期望值算术错误、P9 预设了错误的失败模式、P16/P17 夹具与测试的共享数组引用笔误），修正后 19/19 通过；被测代码在全部探针下无失败。

### 2.3 vitest.config include 泛化影响

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| C1 | 是否引入意外收集 | 用 `node:fs` globSync 对比新旧模式匹配集合：旧 `src/__tests__/**` 4 文件，新 `src/**/__tests__/**` 5 文件，差集仅 `src/valuePresentation/__tests__/valuePresentation.test.ts` | ✅ 无意外收集、无遗漏（旧 4 文件全部仍被覆盖） |
| C2 | codegen 敏感选择不受影响 | `node scripts/select-codegen-sensitive-tests.mjs`：正常运行，331/828→331/829（新文件计入分母但不在敏感列表——其模块图不触达 `packages/frontend/src/extensions/generated.ts`）；脚本对新 pattern 的 glob 解析正常，"零匹配即抛错"的 fail-closed 检查不触发 | ✅ 通过 |

### 2.4 契约类型严格性审读

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| T1 | any 泄漏 / 断言滥用 | 对 valuePresentation 全目录 grep `any`、`as`、`@ts-ignore`、`@ts-expect-error`、`eslint-disable` | ✅ 源码（contracts / registry / lifecycle / index）0 处；测试文件仅 2 处刻意 `as ValuePresentationContribution[]` 用于篡改返回快照以验证隔离性（合理用途，非跳过验证）；`validateConfig(input: unknown)` 以 unknown 入参 + 判别联合出参，无 any |
| T2 | 契约形状 vs 设计 §4 | 逐类型比对（见 §3 符合性） | ✅ 全部一致；`PresentationResult` 仅 `text`/`unchanged` 两种 kind，无 link/image/ReactNode 提前开放 |

## 3. 设计符合性逐条结论

对照设计文档（§2 / §3 / §4 / §10-A）：

**§2 分层与命名**

| 条目 | 结论 |
|------|------|
| 新增独立 `ValuePresentationRegistry`，不将多展示器注册为现有单实现 EP 的同名实现 | ✅ 符合：独立集合注册中心，未触碰 `extensionRegistry`；无并行的单实现真源 |
| 命名 `data.valuePresentation`（能力）/ `datazen.timestamp`（首个贡献） | ✅ T01 范围内符合：贡献 id 采用「提供方.展示器」并在契约文档注明；能力名 `data.valuePresentation` 尚未在代码中声明常量，contracts.ts 注明"由宿主侧声明"——宿主接线属后续任务（观察项 O-2，非缺陷） |

**§3 多贡献注册语义（逐条）**

| 条目 | 结论 |
|------|------|
| 重复贡献 ID 显式拒绝，不覆盖已有实现 | ✅ 符合（register 在写入前抛错；P1/P2 + 自带测试） |
| 卸载句柄绑定注册实例，不能移除之后重新注册的新实例 | ✅ 符合（内部 registration 记录按对象身份比对；同对象重注册亦安全；P5 + 自带测试） |
| 注册帮助函数将释放句柄加入 `ExtensionContext.subscriptions` | ✅ 符合（`registerValuePresentation` 对齐 `registerExtensionPoint`；unload 与激活失败清理两条释放路径均验证） |
| 注册、卸载触发订阅；增加贡献版本令相关计划失效 | ✅ 符合（subscribe + revision 原语齐备；"计划失效"的消费端属 T04，不在 T01 范围） |
| 贡献顺序不决定抢占字段，由配置指定 rendererId | ➖ 范围外（消费侧语义，T04）；`list()` 仅保插入顺序作枚举，不含抢占语义，不阻碍后续实现 |
| 贡献缺失时保留配置、默认展示、面板说明缺失 | ➖ 范围外（T04 宿主管线）；`get()` 返回 undefined 提供缺失检测原语 |
| 激活失败需释放本轮已注册资源；核查并补齐现有加载器失败清理 | ✅ 符合（`load()` 以 try/catch 包裹 activate，失败时按与 unload 相同的 LIFO 清理本轮 subscriptions 后重抛原始错误；deactivate 刻意不调用；P11–P16 行为级验证） |

**§4 首版公共契约（逐类型）**

| 类型 | 结论 |
|------|------|
| `PresentationValue` | ✅ 与设计逐字一致（含 bigint；readonly 数组/记录） |
| `ColumnSource` | ✅ 一致 |
| `PresentationColumnContext`（semanticKind 七值联合） | ✅ 一致 |
| `PresentationEnvironment` | ✅ 一致 |
| `PresentationResult`（仅 text/unchanged） | ✅ 一致；首版未开放 link/image/ReactNode（符合 §4/§11） |
| `PreparedPresentation` | ✅ 一致 |
| `ValuePresentationContribution`（id / configVersion / validateConfig(unknown) / prepare） | ✅ 一致 |
| `ConfigValidationResult` 判别联合（成功配置 / 路径化问题） | ✅ 一致，配套 `ConfigValidationProblem{path,message}` 与 `PresentationConfigValue` JSON 递归类型 |
| `PresentationPrepareInput`（版本化配置 + 列覆盖 + 列上下文 + 环境快照） | ✅ 一致，含 `VersionedPresentationConfig`、`ColumnPresentationOverride`（§5 形状） |
| 不使用 any、不以断言跳过验证 | ✅ 审读 + 双引擎门禁通过 |

接口面说明（非偏差）：实现相对设计 §3 的 4 方法草图增补 `unregister(id)`（显式反注册，未知 id no-op）、`revision`（承载"贡献版本"语义的必要读口）、`reset()`（测试/进程内清理，与 `ExtensionRegistry.reset` 先例一致）。均为加法且文档注明语义，判定为合理实现裁量。

**§10-A 验证清单**

| 条目 | 结论 |
|------|------|
| 验证重复 ID | ✅（自带测试 + P1/P2） |
| 验证旧卸载句柄 | ✅（自带测试 + P5） |
| 验证激活失败清理 | ✅（自带测试 + P11–P16） |
| cell/aside 默认一致和编辑原值 | ➖ 范围外：属阶段 A 后续任务（T04 宿主展示管线 / T05 网格与详情接线），T01 未涉及 frontend |

## 4. Bug 列表

| 编号 | 标题 | 状态 |
|------|------|------|
| BUG-001 | HostExtensionLoader 同 id 并发 load 覆盖致先完成实例订阅泄漏（既有行为，非 T01 引入） | 已关闭（2026-11-11 回归验证通过，详见 bug 文件「回归验证」段） |
| BUG-002 | activate 同步段内重入 load(same id) 绕过 FIFO 队列，实例订阅泄漏（BUG-001 同根因的更窄既有路径，回归期发现） | 已关闭（2026-11-11 聚焦回归验证通过，详见 bug 文件「回归验证」段） |

详情见 `../bugs/BUG-001-concurrent-load-overwrite-leak.md` 与 `../bugs/BUG-002-reentrant-load-bypasses-queue.md`。

## 5. 观察项（非缺陷，供 coder / 后续任务参考）

- **O-1（重入语义）**：listener 内无界再注册会以栈耗尽终止：RangeError 被 notify 的 listener 隔离 catch 吞掉（仅 console.error），registry 停留在一个内部一致但含大量垃圾贡献的状态。无死循环、无未捕获崩溃、无跨实例污染，且与既有 `ExtensionRegistry.notify` 行为逐字对齐（代码注释已声明对齐意图），设计未要求重入防护 → 不判缺陷。若未来宿主 listener 存在再注册可能，建议在宿主侧以微任务/队列化解。
- **O-2（能力名常量）**：`data.valuePresentation` 能力标识暂无代码级声明（contracts.ts 注明由宿主侧声明）。后续宿主接线 / EP manifest 任务落地时应补齐，避免字符串散落。
- **O-3（测试覆盖增强建议）**：coder 自带 16 项测试已覆盖 §10-A 三项必验语义；探针中的 P6/P12/P13/P16–P18（notify 中自退订、清理期 dispose 抛错、异步激活失败、loadFromUrl 链路）建议吸收为仓库回归测试。
- **O-4（探针资产）**：/tmp/probe-t01 为一次性验证项目，重启后失效；本报告 §2.2 各项方法已描述到可复现程度。

## 6. 结论

**通过。**

- 既有门禁全绿：EP 包 70/70（零重试复跑确认）、`pnpm typecheck` 与 `pnpm typecheck:tsc` 双引擎通过、命名与配置门禁通过。
- 对抗性探针 19 项：T01 范围内的全部核心语义（重复 ID 拒绝、句柄实例绑定、订阅通知与异常隔离、revision 失败路径稳定、激活失败 LIFO 清理、deactivate 不调用、可重载、loadFromUrl 不受影响、有界重入安全）验证正确，未发现 T01 引入的任何缺陷。
- 唯一记录的 BUG-001 为一般级且系改动前即存在的既有行为（已用 `git show HEAD` 核实），不构成 T01 交付缺陷，建议单独排期处理。
- 范围外事项（§3 消费侧两条、§10-A cell/aside 一致性）已明确归属后续任务，T01 交付了正确的原语。

## 7. 回归验证记录（BUG-001 修复）

2026-11-11，tester 对 BUG-001 的 FIFO 队列修复做了独立回归：门禁复跑 79/79（`--retry=0` 同样 79/79）、typecheck 双引擎 OK；代码审读确认队列实现五个审查点（尾 promise 身份删除无竞态、失败不毒化链、无私自我入队路径、reset 并行排空 + 首错重抛、空闲条目清理）均成立；/tmp/probe-bug001 对抗探针 19 项（原始复现场景、五种子随机交错压力、队列毒化、reset 竞态、unhandled rejection 探测、重入与内存）× 3 轮全绿，且 T01 首轮探针套件复跑 18/19——唯一失败正是 P19 泄漏金丝雀（行为如预期改变）。**BUG-001 验证通过并关闭**（详见 bug 文件「回归验证」段）。回归中发现并登记同根因的更窄既有路径 **BUG-002**（activate 同步段内重入 load 绕过队列，一般级、打开，经 T01 基线复核证实先于修复存在、非修复引入），另记录一条非缺陷观测：fire-and-forget 失败 load 不再触发 unhandledRejection（修复前 1 次 / 修复后 0 次，await 调用方语义不变）。

## 8. 回归验证记录（BUG-002 修复）

2026-11-11，tester 对 BUG-002 的占位尾修复做了聚焦回归：`npx vitest run packages/extension-points --retry=0` **81/81**（79 既有 + 2 新增）、typecheck 双引擎 OK；代码审读确认 (a) signal 释放无遗漏分支（`run.then(settleQueueEntry, settleQueueEntry)` 双臂 + 空闲分支同步抛错实际不可达——load/unload 闭包均为 async 体）、(b) 身份比对删除在占位尾语义下仍正确（重入已覆盖 Map 时外层跳过删除、由重入尾自负责）、(c) `signal()` 续体排在下一微任务而 `activeExtensions.set` 在 run settle 前已同步完成，重入操作启动时外层状态已完整落盘、串行语义不破坏；/tmp/probe-bug002 聚焦探针 6 项（原始复现无泄漏、重入 + 外层失败、三层嵌套重入、同占位尾挂 load+load+unload、窗口内重入 reset、Map 残留清理）× 3 轮全绿；BUG-001 期全套 19 项探针对新代码复跑 16/19——3 项失败恰为特征化探针 C1/C3/C4 的「断言泄漏存在」断言按预期翻转（泄漏已消除），无修复引入的回归。**BUG-002 验证通过并关闭**（详见 bug 文件「回归验证」段）；观察项一句话带过：类文档已如实声明「activate 内 await 同 id 重入 load 为构造性自死锁（重入调用应 fire-and-forget）」，属接口契约而非缺陷。
