# T03 测试报告：时间展示器贡献 datazen.timestamp（valuePresentation）

- 被测对象：`packages/frontend/src/lib/valuePresentation/timestampContribution.ts`（375 行，新文件）+ `packages/frontend/src/lib/valuePresentation/index.ts`（桶导出追加 6 行）+ `packages/frontend/src/lib/valuePresentation/__tests__/timestampContribution.test.ts`（46 项自带测试）
- 分支：`feature/issue-32-value-presentation`（HEAD `e05e50093`，作者 issue-32-agent；T03 两提交：`e21082069` feat + `e05e50093` test，基底 `6d62deb2c`；验证开始时工作区干净）
- 测试角色：tester 独立验证，未修改任何被测代码或测试
- 依据：`docs/archive/development/issue-32-value-presentation-design.md` §4 §5 §7 §8、`packages/extension-points/src/valuePresentation/contracts.ts`、T02 库（timestampConfig / timestampFormat / temporalMath）、`docs/development/issue-32/tasks.md`（T03）
- 日期：2026-11-12

## 1. 测试环境与命令

环境：Node v24.22.1，vitest 4.1.10（仓库自带），tsc 7.0.2 / tsrs 0.18.0，系统时区 UTC（`Intl.DateTimeFormat().resolvedOptions().timeZone` = UTC；全部探针均显式传入时区与语言，不依赖机器默认值）。对抗性探针运行于独立临时项目 `/tmp/probe-t03/`（通过 `node_modules` 符号链接复用仓库依赖，`environment: node`，被测源码以绝对路径导入，不污染仓库工作区；测试后 `git status` 复核仓库无任何测试方改动）。

环境说明：本机 `PATH` 中无 `pnpm` 二进制（与 T02 报告 O-5 相同情况）；G2 改用 `npx -y pnpm@12.18.0 <script>` 调用，版本号实测 12.18.0，属等价调用，不影响门禁结论。

| 命令 | 用途 |
|------|------|
| `npx vitest run packages/frontend/src/lib/valuePresentation --retry=0` | T03 包单测（排除根配置 retry 掩盖侥幸通过） |
| `npx -y pnpm@12.18.0 typecheck` / `npx -y pnpm@12.18.0 typecheck:tsc` | 双引擎类型门禁（tsrs / 权威 tsc；root / scripts / pack-ep 三程序） |
| `node scripts/check-test-module-naming.mjs` | 测试命名门禁 |
| `npx vitest run packages/frontend/src/lib/valuePresentation --coverage` | 新文件覆盖率（注意全局阈值，见 G4） |
| `/tmp/probe-t03` 探针项目（20 项，P1–P12 + 补充 S1–S4） | 对抗性行为验证（PRD 锚点、DST、门禁、覆盖、local 时区、永不抛异常、validateConfig 边界、注册中心集成、锚定裁决、原因码取法、Unix 精度、prepare 兜底） |
| `grep` 审读（`any` / `as` / `new Date(` / `Date.parse` / `TODO`） | G5 的源码级严格性审读与循环依赖核查 |

探针项目结构：`probes/t03-adversarial.test.ts`（P1–P12 共 16 项 + 补充 S1–S4 共 4 项）。探针属一次性验证资产，位于系统临时目录，不进入仓库；各项方法已在下表内描述到可复现程度。

## 2. 测试项清单

### 2.1 既有验证（仓库门禁）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| G1 | T03 包单测全量 | `npx vitest run packages/frontend/src/lib/valuePresentation --retry=0` | ✅ 4 文件 195/195 通过（temporalMath 91、timestampFormat 32、timestampConfig 26、timestampContribution 46，与申报数量一致；退出码 0） |
| G2 | 类型门禁（tsrs 单引擎 + 权威 tsc） | `pnpm typecheck` / `pnpm typecheck:tsc`（经 npx pnpm 等价调用） | ✅ 两者均 OK（root / scripts / pack-ep 三程序；tsrs 0.18.0-ts7.1.0-dev.20260929、tsc 7.0.2；退出码均为 0）。根 tsconfig `include` 含 `packages/frontend/src`，新文件实际被覆盖 |
| G3 | 测试命名门禁 | `node scripts/check-test-module-naming.mjs` | ✅ ok（1837 Rust 文件、256 cfg(test) 模块；2557 TS 文件、8 wdio spec 树；退出码 0） |
| G4 | 新文件覆盖率 | `npx vitest run packages/frontend/src/lib/valuePresentation --coverage` | ✅ 新文件四项指标全部 ≥80%（见下表）；进程退出码 1 **仅因全局阈值未达**，不是测试失败（195/195 全部通过）。**退出码 1 不得读作"测试失败"，也不得读作"全过"** |
| G5 | 源码严格性审读 + 桶导出循环依赖核查 | grep 新文件 `any` / `as` / `new Date(` / `Date.parse` / `TODO`；核对桶导出与注册中心导入 | ✅ 新文件 0 处 `any`、0 处 `as`、0 处 `new Date(`、`Date.parse` 0 处、`TODO`/`FIXME` 0 处；桶导出仅追加 `timestampContribution` 的具名导出，无循环依赖嫌疑（见 §2.3） |

> G4 明细（v8 报告，`--coverage` 单目录运行）：
>
> | 文件 | % Stmts | % Branch | % Funcs | % Lines | 未覆盖行 |
> |------|---------|----------|---------|---------|----------|
> | `timestampContribution.ts`（新文件） | 94.54 | 92.13 | 100 | 97.91 | 166、366 |
> | `timestampConfig.ts` | 100 | 100 | 100 | 100 | — |
> | `timestampFormat.ts` | 97.26 | 93.1 | 100 | 96.87 | 187、243 |
> | `temporalMath.ts` | 92.58 | 90.55 | 100 | 96.24 | … |
> | valuePresentation 目录合计 | 94.38 | 92.51 | 100 | 97.1 | — |
>
> 全局阈值报错原文：`Coverage for lines (2.88%) does not meet global threshold (80%)` 等四项——因覆盖配置统计整个前端（单目录运行下其余文件均为 0%），与本次新文件无关。未覆盖行 166（`wallParseToComponents` 的防御性 `return null`，switch 已穷尽三种 shape）与 366（`present` 内层 catch 的 `unexpected-error` 兜底）均为防御性分支，无法经公开 API 触发（见 O-5），不构成覆盖缺口。

### 2.2 对抗性探针（/tmp/probe-t03，独立于 coder 自带测试编写）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| P1 | PRD 锚点 1：`2026-01-15T14:30:00Z` 用 Asia/Shanghai 展示 | native + iso-tz / locale(en-GB) / iso 三格式分别断言全量文本 | ✅ 通过。iso-tz = `2026-01-15T22:30:00+08:00`（含 22:30 与 +08:00）；locale 含 `22:30`；iso = `2026-01-15T14:30:00Z`（保持 UTC） |
| P2 | PRD 锚点 2：Unix `1705316400000` → `2024-01-15T19:00:00+08:00` | 毫秒 number / 秒 number / 毫秒字符串 / 秒字符串 / 毫秒 bigint / 秒 bigint 六路径 | ✅ 通过。六路径均为 `2024-01-15T19:00:00+08:00`（毫秒余数为 0 的锚点值；余数非 0 的情形见 S3/BUG-003） |
| P3 | DST 双义时间被拒绝 | `2024-11-03 01:30` America/New_York，经 text 解释与 native 回退两路 | ✅ 通过。两路均为 `{ kind: 'unchanged', reason: 'dst-ambiguous' }`。对照：同日 00:30（EDT → 04:30Z）与 03:30（EST → 08:30Z）均唯一通过 |
| P4 | DST 不存在时间被拒绝 | `2024-03-10 02:30` America/New_York，经 text 解释与 native 回退两路 | ✅ 通过。两路均为 `{ kind: 'unchanged', reason: 'dst-invalid' }` |
| P5 | null / 非时间值回退与 reason 核对 | null / undefined / boolean / 数组 / 对象 + `'other'` 列的 null 与有效时间戳 | ✅ 通过。null/undefined → `'null'`；boolean/数组/对象 → `'not-applicable'`；`'other'` 列的 null 与有效时间戳均 → `'not-applicable'`，与 coder 申报一致。裁决评审见 O-3：合理 |
| P6 | 列覆盖三态 | raw / inherit（与缺省）/ custom 命中 / 错配 rendererId / custom 无效 options / 全局无效 + raw 覆盖 | ✅ 通过。raw → `'raw-override'`；inherit → 用全局；custom 命中生效（全局 iso/UTC 被覆盖为 iso-tz/上海）；错配 → `'override-target-mismatch'`；custom 无效 → `'invalid-config'`；全局无效 + raw → `'invalid-config'`，与申报一致。裁决评审见 O-2：可接受，非缺陷 |
| P7 | configVersion 与全局有效性 | 版本 0 / 2 / -1 / 99；全局配置无效 | ✅ 通过。四版本均 → `'unsupported-config-version'`（null 值亦同）；全局无效 → `'invalid-config'`（null 值亦同），且版本检查先于配置体校验 |
| P8 | semanticKind 门禁 | `'other'`、number 列 + native/text/unix、date/time/local-datetime/instant/text 列 + native | ✅ 通过。`'other'` 全拒；number 列 + native/text → `'not-applicable'`，+ unix → 正常出文本；其余五种 semanticKind 均按 interpretation 正常展示 |
| P9 | local 时区与 locale 取自环境快照 | 机器时区 UTC（实测 `Intl…timeZone = UTC`），`environment.localTimeZone='Asia/Shanghai'` + `displayTimeZone {kind:'local'}`；locale en-GB vs zh-CN 对照 | ✅ 通过。local 展示输出 `2026-01-15T22:30:00+08:00`（不依赖机器默认值）；en-GB 与 zh-CN 的 locale 文本不同且均含 `22:30`，证明 locale 取自 `environment.locale` |
| P10 | present 永不抛异常 | 57 个畸形值 × native/unix/text 三种解释逐一调用（含超大 number、10^30 bigint、10 万字符长串、NUL、emoji、Symbol、函数、Date、Map/Set、getter 对象） | ✅ 通过。171 次调用全部无 throw，返回均为 `text` 或 `unchanged`。注：unix 路径下 Symbol/函数值经 T02 `parseUnixValue` 的 else 分支返回 `'not-a-number-or-string'`，不抛异常 |
| P11 | validateConfig 边界 | unix+sourceTimeZone、未知字段 rules、空对象、7 种非对象输入、configVersion 字段混入输入（1 与 2）、合法配置 JSON 往返 | ✅ 通过。unix+sourceTimeZone → `sourceTimeZone` 路径化拒绝（message 与 T02 逐字一致）；rules → `path: 'rules', message: 'unknown field'`；空对象 → `ok: false` 且 problems 非空；7 种非对象均 → `[{ path: '', message: 'expected an object' }]`；混入 `configVersion`（1 或 2）均 → `path: 'configVersion', message: 'unknown field'` 且不返回部分配置；合法配置 JSON 往返无损，且缺省的 sourceTimeZone 不出现在返回的 config 中。结论：validateConfig 入参确为纯 TimestampOptions，版本字段属于外层 `VersionedPresentationConfig`，不在此处理——符合契约 |
| P12 | 注册中心集成 | 全新 `ValuePresentationRegistryImpl` + 假 ExtensionContext：注册 → get/prepare 可用 → 重复 id 拒绝且保留原实例 → 释放句柄反注册 → 重注册成功 | ✅ 通过。注册后 `registry.get('datazen.timestamp')` 为同一对象，`prepare` 输出 `2026-01-15T22:30:00+08:00`；重复注册抛 `duplicate contribution id 'datazen.timestamp'`，原实例保留、第二上下文 subscriptions 为空；释放句柄后 get 为 undefined，重注册成功，`list()` 仅含该一项。只验贡献对象可注册，未重测注册中心本身 |
| S1 | 补充：锚定裁决核查 | time-only 与 date-only 文本，经 text 解释与 native 回退两路 | ✅ 通过。`14:30:00` 两路均为 `1970-01-01T06:30:00Z`（锚定 epoch 日）；`2026-03-01` 两路均为 `2026-02-28T16:00:00Z`（当日午夜 +08:00），与文件头裁决 3 一致 |
| S2 | 补充：native 回退失败原因码取法 | 墙上解析失败（`not-a-date` / `2026-13-01` / `2026-02-30`）经 native 回退 | ✅ 通过。`not-a-date` → `invalid-format`；`2026-13-01` → `invalid-month`；`2026-02-30` → `invalid-day`，均为底层 T02 稳定码原样透传，未自造新码。同一畸形输入在有/无 sourceTimeZone 时原因码可不同（见 O-4），属已文档化的取法，非缺陷 |
| S3 | 补充：unix bigint 精度与毫秒余数 | 超大 bigint、MAX+1、超安全整数 number、毫秒余数非 0 的 number/bigint/string 三路径 × iso/iso-tz/locale | ❌ **1 项失败 → BUG-003（主要级）**。超大 bigint → `out-of-range` ✅；MAX+1（`3093559603200000n`）→ `out-of-range` ✅；`2**53` → `not-safe-integer` ✅；**但 `1705316400123`（毫秒余数 .123）在 iso/iso-tz 下输出 `…19:00:00+08:00` / `…11:00:00Z`，毫秒被静默丢弃**，而同值的 locale 输出 `15/01/2024, 19:00:00.123 GMT+8` 保留了 `.123`——同一时刻三格式不一致。`-1`（即 -1ms）同样丢失 `.999`。详见 §4 |
| S4 | 补充：prepare 结构异常兜底 | `prepare({})` 畸形快照后 present 多值 | ✅ 通过。present 不抛异常，一律返回 `{ kind: 'unchanged', reason: 'unexpected-error' }`（含 null 值） |

> 探针过程说明：首轮 20 项中 1 项失败（S3 毫秒余数）系**被测代码真实缺陷**（BUG-003），非探针错误；另 1 处探针期望值错误已在过程中修正并记录：S3 初版误以为 `MAX_SUPPORTED_EPOCH_MS = 3093559603199999` 对应 9999-12-31，实测该值对应 `+100001-01-01T07:59:59+08:00`（支持上限约 10 万年，见 T02 报告 P9），已将该断言改为 MAX+1 越界（实测 `out-of-range` ✅）。除 S3 外，被测代码在全部探针下无失败。最终探针 19/20 通过。

### 2.3 契约类型严格性审读

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| T1 | any 泄漏 / 断言滥用 | 对新文件 grep `any`、`as`、`@ts-ignore`、`@ts-expect-error`、`eslint-disable` | ✅ 新文件（timestampContribution.ts）0 处。测试文件有 7 处 `as`：`as const`（时区字面量）、`config as PresentationConfigValue` 与 `as unknown as …`（向贡献传入畸形输入以验证运行时校验），均在输入侧、不跳过任何验证；`validateConfig(input: unknown)` 以 unknown 入参 + 判别联合出参 |
| T2 | 隐式时间解析 | grep 新文件 `new Date(` / `Date.parse` | ✅ 0 处。时间解析全部委托 T02 的手工分段解析；目录内既有的 3 处 `new Date(epochMs)`（数值入参，temporalMath.ts:312、timestampFormat.ts:149/241）为 T02 既有实现，不在本次新文件内 |
| T3 | 桶导出循环依赖 | 核对 index.ts 追加的导出与 timestampContribution.ts 的导入边 | ✅ 无循环依赖嫌疑。timestampContribution.ts 只从 `@datazen/extension-points` 做 `import type`（编译期擦除）并相对导入同目录 T02 模块；extension-points 包内无任何对 `@datazen/frontend` 的引用；测试从 `@datazen/extension-points` 导入 `ValuePresentationRegistryImpl` / `registerValuePresentation`，两者均在 `packages/extension-points/src/valuePresentation/index.ts:24-25` 正常导出，导入正确 |

## 3. 设计符合性逐条结论

对照设计文档（§4 / §5 / §7 / §8 / §10-A 相关项与 §10-B）：

**§4 首版公共契约（T03 范围：贡献对象）**

| 条目 | 结论 |
|------|------|
| 贡献 id `datazen.timestamp`、configVersion 1 | ✅ 符合（timestampContribution.ts:354-355；P12 注册验证 id 一致；常量 `TIMESTAMP_CONTRIBUTION_ID` / `TIMESTAMP_CONTRIBUTION_CONFIG_VERSION` 正常导出） |
| `validateConfig(input: unknown): ConfigValidationResult`，不抛异常、失败返回路径化问题、不返回部分配置 | ✅ 符合（P11 全绿；成功返回重建的 JSON 配置，失败 path/message 原样透传） |
| `prepare(input): PreparedPresentation`，仅 text / unchanged 两种输出 | ✅ 符合（全部探针输出均为 `{kind:'text', text, interpretation}` 或 `{kind:'unchanged', reason}`；`interpretation` 字段与契约的可选字段一致） |
| 配置与公共契约 JSON 可序列化，不使用 any、不以断言跳过验证 | ✅ 符合（T1；`toPresentationConfigValue` 逐字段重建，不用断言） |
| 值输入允许 bigint | ✅ 符合（P2/S3：bigint 经 `String(value)` 精确传入 T02，不做 `Number()` 近似） |

**§5 配置模型与规则（T03 范围：prepare 的配置解析）**

| 条目 | 结论 |
|------|------|
| 列覆盖三态 inherit / raw / custom（§5 形状） | ✅ 符合（P6；custom 的 options 经 `validateTimestampOptions` 运行时校验，与契约"custom 的 options 是未经校验的原始 JSON"一致） |
| 源时区只出现一处；unix 携带 sourceTimeZone 即拒 | ✅ 符合（P11；拒绝由 T02 验证层执行，贡献层原样透传） |
| `native` 不把数字自动解释成 Unix；Unix 必须显式指定单位 | ✅ 符合（P8：number 列 + native → `not-applicable`；coder 自带测试覆盖 native 下 number/bigint 均 `not-applicable`） |
| 无时区文本与原生 local-datetime 可使用 sourceTimeZone；带偏移值以自身时刻为准 | ✅ 符合（P1/P3/P4/S1：带偏移文本经 `parseZonedTimestamp` 成功后直接格式化，不用 sourceTimeZone 重解释；无时区文本经墙上回退） |
| 规则数组 / 正则安全 / 规则数量与输入长度设限 | ➖ 范围外（规则系统属阶段 C / T05+，不在 T03；validateConfig 对 `rules` 未知字段拒绝已验证，P11） |

**§7 共享展示管线（T03 范围：prepare 阶段的工作）**

| 条目 | 结论 |
|------|------|
| 规则与 Intl 格式器在 prepare 阶段创建；不逐格编译 | ⚠️ 部分符合，见 O-1。配置解析与时区解析确在 prepare 阶段一次完成（local 时区按环境快照解析，P9）；但 `formatTimestampOutput` 的 locale 分支每格新建 `Intl.DateTimeFormat`（T02 既有实现），prepare 未缓存格式器。属 coder 自报的性能注记，非缺陷；宿主展示管线（T04）的计划缓存可缓解 |
| 计划缓存键至少包含来源身份、列元数据版本、配置修订、贡献修订、locale、localTimeZone | ➖ 范围外（宿主展示管线 T04 的职责；T03 只交付 prepare 本身） |
| 扩展异常由宿主统一隔离；普通非法值返回 unchanged，不作为扩展异常 | ✅ 符合（P10/S4：present/prepare 永不抛异常，非法值一律 unchanged；`unexpected-error` 仅为防御性兜底原因码） |
| 日志记录贡献与原因码，不记录原始行值 | ➖ 范围外（宿主日志行为，T03 模块内无日志调用） |

**§8 时间解析算法与精度（T03 消费侧）**

| 条目 | 结论 |
|------|------|
| 2. Unix 字符串先验证整数语法，再用 BigInt 精确解析；number 必须 Number.isSafeInteger | ✅ 符合（P2/S3：bigint 经 `String(value)` 交给 `parseUnixValue`，超安全整数 number 拒绝，不静默取整） |
| 3. 秒乘 1000 使用精确整数运算，检查支持的日期范围后才转用于格式器；0 与负数正常处理 | ✅ 符合（P2、S3 边界；0 与 -1 的 unix 值在 coder 自带测试与 S3b 对照中均为真实时刻） |
| 4. 带时区文本严格解析；无时区文本不交给隐式本地解析的 `new Date(string)` | ✅ 符合（T2：新文件 0 处 `new Date(string)`；P1/P3 行为级验证） |
| 5. 支持 IANA 与显式 DST 歧义策略；不能假定原生 Temporal 普遍可用 | ✅ 符合（P3/P4；实现只用 Intl + 字符串分段解析） |
| 6. 小数秒与时刻转换分开处理，保留原生微秒/纳秒精度；若依赖无法保证则返回 unchanged，不静默截断 | ❌ **不符合——BUG-003**。unix 毫秒路径在 iso/iso-tz 下静默丢弃毫秒余数（S3），与本条字面要求冲突；native/text 路径保留精度（T02 P6 与本报告 S1 验证），locale 分支亦保留（S3 对照） |
| 7. iso 输出 UTC；iso-tz 使用目标时刻对应的实际偏移；locale 固定传入语言和时区，不能依赖机器隐式默认值 | ✅ 符合（P1/P9：iso 恒为 UTC；iso-tz 含实际偏移；locale 取 `environment.locale`，local 时区取 `environment.localTimeZone`） |
| 8. 无时区源未配置、DST 歧义、范围错误使用稳定原因码；预览和实际展示调用同一解析实现 | ✅ 符合（P3/P4/S2：`dst-ambiguous` / `dst-invalid` / `missing-source-time-zone` 等稳定透传；present 与（未来）预览共享同一 `presentValue` 入口） |
| 1. 规范化类型判定原生时间，数据库特殊类型经 DatabaseTypeMeta/驱动声明，不在宿主硬编码驱动名称 | ➖ 范围外（驱动侧判定属后续任务；T03 的 semanticKind 门禁只消费宿主传入的 `semanticKind`，新文件 0 处驱动引用） |

**§10-A 验证清单（阶段 A：贡献与展示基础，与 T03 相关项）**

| 条目 | 结论 |
|------|------|
| 验证重复 ID、旧卸载句柄、激活失败清理 | ✅ 部分在 T03 范围：重复 ID 拒绝且保留原实例、释放句柄反注册后可重注册已验证（P12；激活失败清理属 T01，已验证通过） |
| cell/aside 默认一致和编辑原值 | ➖ 范围外：属 T04 宿主展示管线 / T05 网格与详情接线；T03 未接线 CellRenderer/DetailPanel（index.ts 注释已声明属 T05），不构成缺口 |

**§10-B 验证清单（阶段 B：时间与列配置）**

| 条目 | 结论 |
|------|------|
| 时区跨日 | ✅（P1/P2 验证 +08:00 换算；S1 验证 date-only 跨日） |
| 正负偏移 | ✅（+08:00 由 P1/P2/S1 验证；负偏移由 America/New_York 源时区换算验证，P3 对照 00:30 EDT → 04:30Z） |
| DST | ✅（P3/P4 双路验证） |
| NULL | ✅（P5：null → `'null'`；`'other'` 列 null → `'not-applicable'`，裁决见 O-3） |
| 0、负时间 | ✅（coder 自带测试覆盖 unix 0 / -1；S3b 对照验证 -1ms 的时刻正确，仅精度丢失属 BUG-003） |
| 不安全整数 | ✅（P10/S3） |
| 数字字符串 | ✅（P2/S3） |
| 越界 | ✅（S3：超大 bigint 与 MAX+1 均 `out-of-range`） |
| 非法文本 | ✅（S2/P10） |
| 无时区值 | ✅（coder 自带测试：text 无 sourceTimeZone → `'missing-source-time-zone'`；native 无源时区墙上文本 → 底层 `invalid-format`） |
| 高精度 | ❌ **部分失败——BUG-003**。zoned 文本 9 位精度保留（T02 P6）；unix 毫秒余数在 iso/iso-tz 下丢失 |

## 4. Bug 列表

| 编号 | 标题 | 状态 |
|------|------|------|
| BUG-003 | Unix 毫秒值的毫秒余数在 iso / iso-tz 输出下被静默丢弃（locale 保留，三格式不一致） | 已关闭（2026-11-12 回归验证通过，详见 bug 文件「回归验证」段及本报告 §7） |

**BUG-003 — Unix 毫秒余数静默截断（主要级）**

- 复现输入（prepare：`interpretation {kind:'unix', unit:'milliseconds'}`，`format 'iso-tz'`，`displayTimeZone Asia/Shanghai`；其余同 P2）：`present(1705316400123)`、`present(1705316400123n)`、`present('1705316400123')`。
- 实际输出：`{ kind: 'text', text: '2024-01-15T19:00:00+08:00', interpretation: 'unix' }`；iso 格式同理为 `2024-01-15T11:00:00Z`。正确应为 `…19:00:00.123+08:00` / `…11:00:00.123Z`。
- 对照证据：同一输入 locale 格式输出 `15/01/2024, 19:00:00.123 GMT+8`（保留 `.123`）；`present(-1)` 的 iso 输出为 `1969-12-31T23:59:59Z` 而 locale 为 `…07:59:59.999 GMT+8`（`.999` 丢失）。毫秒余数为 0 时三格式一致，余数非 0 时 iso/iso-tz 系统性丢失。
- 根因（两处配合，均在 T02 库）：
  1. `packages/frontend/src/lib/valuePresentation/temporalMath.ts:503`——`parseUnixValue` 成功时恒返回 `{ epochMs, subSecondNanos: 0, fractionDigits: 0 }`。毫秒余数实际存在于 `epochMs`（如 1705316400123）中，但 `fractionDigits: 0` 把源精度谎报为"无小数秒"。
  2. `packages/frontend/src/lib/valuePresentation/timestampFormat.ts:114-121`——`resolveFractionDigits` 在 `fractionDigits` 显式为 0 时直接返回 0，不看 `nanos`；而 `utcWallOf`（:149）与 `instantToWallParts` 已正确算出 `nanos = 123_000_000`。于是 `formatIsoBody`（:132-139）丢弃了已算出的毫秒。
  3. locale 分支（timestampFormat.ts:233-238）按 `wall.nanos > 0` 决定是否带毫秒，不依赖 `fractionDigits`，故保留——三格式分歧由此产生。
- 为什么 T02 与 T03 自带测试都没抓住：T02 的精度回归（P6）只覆盖带时区文本路径（其 `fractionDigits` 取自源文本，正确）；T02 的 `parseUnixValue` 断言只覆盖毫秒余数为 0 的值（如 `1705316400000`，temporalMath.test.ts:359）；T03 自带测试与 PRD 锚点 2 用的也是余数为 0 的 `1705316400000`。unix 毫秒值经 `formatTimestampOutput` 的端到端路径从未以余数非 0 的值断言过。
- 影响：unix 毫秒是最常见的 BIGINT 时间戳形态（DB 毫秒时间戳列正是 Issue #32 的目标场景之一）；显示值与真实值相差可达 999ms 且无任何提示，违反设计 §8.6"不静默截断"。PRD 锚点 2 本身不受影响（其余数为 0），故门禁 G1 全绿、P2 全绿。
- 修复建议（供 coder / 协调方定夺，不改被测代码）：首选修 `parseUnixValue`，按毫秒余数声明源精度——毫秒单位且 `epochMs % 1000 !== 0` 时返回 `fractionDigits: 3`（秒单位恒为 0）；或修 `resolveFractionDigits`，在显式 `fractionDigits === 0` 但 `nanos > 0` 时按 nanos 推导（防御性兜底）。修复后须同步核对 T02 自带测试中对 unix instant 的 `fractionDigits: 0` 断言（temporalMath.test.ts:339/343/350/359/378/382 均为余数 0 值，不受影响；但需确认无其他断言依赖恒 0），并补一条 unix 毫秒余数端到端回归断言（iso/iso-tz/locale 三格式一致）。
- 归属说明：BUG-003 的缺陷代码在 T02 提交（`75771753f`）引入，T03 的贡献模块（`presentValue` 的 unix 分支）只是忠实透传 T02 的 instant 与格式化结果，本身无错。登记为 T03 验证发现的 Bug，由修复方决定在 T02 回归任务或 T03 收尾前修复；修复后须用本报告 S3 探针做回归验证。

## 5. 观察项（非缺陷，供 coder / 后续任务参考）

- **O-1（性能注记：每格新建 Intl，未缓存格式器）**：coder 申报属实。`formatTimestampOutput` 的 locale 分支每调用新建 `Intl.DateTimeFormat`（timestampFormat.ts:241）；iso-tz 路径经 `instantToWallParts` 同样每格构造格式器；`prepare` 只解析配置与时区，未缓存任何格式器。当前 T03 为纯逻辑贡献、无宿主接线，尚无可测性能回归；不判缺陷。建议 T04 宿主展示管线落地计划缓存时复核此点（设计 §7 要求"规则与 Intl 格式器在 prepare 阶段创建"）。
- **O-2（裁决评审：全局无效 + raw 覆盖 → `invalid-config`）**：申报行为实测成立（P6）。评审：可接受，非缺陷。两路最终行为相同（present 一律 unchanged），差异仅在诊断原因码；且设计 §6 要求宿主在 prepare 前验证配置，全局无效本不应到达 prepare，贡献层按"版本 → 全局有效性 → 列覆盖"顺序先报 `invalid-config` 是防御性的一致选择。反方观点（raw 意味着整列退出时间展示，全局有效性与之无关，应报 `raw-override`）同样合理；若协调方倾向后者，改动为调整 `resolveEffectiveOptions` 的检查顺序，但须同步更新 coder 自带测试的对应断言。
- **O-3（裁决评审：`'other'` 列的 null 得 `not-applicable` 而非 `null`）**：申报行为实测成立（P5）。评审：合理。semanticKind 门禁是"列是否适用本贡献"的判定，先于值级 null 检查；`other` 列根本不是时间列，其 null 不是"时间值的缺失"，贡献以 `not-applicable` 拒绝整列比报 `'null'` 更准确，也与 §7"NULL 优先"（指时间列内的 NULL 优先于展示尝试）无冲突。DetailPanel/CellRenderer 的 NULL 渲染仍由宿主默认分支处理，不受影响。
- **O-4（native 回退原因码随 sourceTimeZone 有无而变化）**：实测确认，非缺陷。同一畸形输入的原因码取决于走了哪条解析路径：`'2026-13-01T00:00:00Z'` 无源时区 → `invalid-month`（zoned 解析），有源时区 → `invalid-format`（墙上解析，Z 后缀不在墙上语法内）；反之 `'2026-13-01'` 无源时区 → `invalid-format`，有源时区 → `invalid-month`。两者均为底层 T02 稳定码，取法与文件头裁决 4（"墙上解析失败取墙上码，转换失败取转换码"）逐字一致；原因码标识的是"哪一层解析拒绝了它"，信息量反而更大。宿主若按原因码做用户提示，需注意同一输入在不同配置下码不同。
- **O-5（覆盖率未覆盖行 166/366 均为防御性兜底）**：`timestampContribution.ts:166`（`wallParseToComponents` switch 穷尽后的 `return null`）与 `:366`（`present` 内层 catch 的 `unexpected-error`）经公开 API 不可达——前者要求 `parseWallClockText` 返回 switch 之外的 shape，后者要求 T02 纯函数抛异常。两者均有注释说明防御意图，不构成覆盖缺口；不建议为凑覆盖率写人造测试。
- **O-6（validateConfig 对 `configVersion` 的处理）**：P11 确认：把 `configVersion` 混入 validateConfig 入参会被当作未知字段拒绝。正确：validateConfig 的入参是纯 `TimestampOptions`，版本字段属于外层的 `VersionedPresentationConfig`（由 prepare 的 `input.config.configVersion` 校验，P7）。宿主边界须注意不要把整包版本化配置直接喂给 validateConfig。
- **O-7（探针资产）**：/tmp/probe-t03 为一次性验证项目，重启后失效；本报告 §2.2 各项方法已描述到可复现程度。建议将 S3 的毫秒余数端到端断言（BUG-003 回归）与 P3/P4 的 2024 年 DST 拒绝断言核对是否已在自带测试中存在（自带测试用的是 2026 年 DST 日期；探针为独立复核）。

## 6. 结论

**不通过——暂缓收尾提交，待 BUG-003 修复并回归验证后方可关闭。**

- 既有门禁：T03 包 195/195（`--retry=0` 复跑确认）、`pnpm typecheck` 与 `pnpm typecheck:tsc` 双引擎通过、命名门禁通过；新文件覆盖率四项指标 94.54 / 92.13 / 100 / 97.91（G4 进程退出码 1 仅因全局阈值，见 §2.1 注）。
- 对抗性探针 20 项中 19 项通过：PRD 双锚点精确命中、DST 双义/不存在两路均拒绝、local 时区与 locale 均取自环境快照（不依赖机器 UTC 默认值）、present 在 171 次畸形调用下永不抛异常、列覆盖三态与 semanticKind 门禁全部正确、validateConfig 边界与注册中心集成（重复 id 拒绝、释放后重注册）全部正确、四项裁决记录逐条核验成立。
- 唯一失败项 S3 暴露 **BUG-003（主要级）**：Unix 毫秒值余数非 0 时 iso/iso-tz 静默丢弃毫秒（`1705316400123` → `…19:00:00+08:00`，而 locale 保留 `.123`），违反设计 §8.6。根因在 T02 库（`parseUnixValue` 恒报 `fractionDigits: 0`，temporalMath.ts:503；`resolveFractionDigits` 显式 0 优先，timestampFormat.ts:114-121），T03 贡献模块本身无缺陷——但该缺陷正落在 T03 交付的 unix 展示路径上，且是 Issue #32 的目标场景（BIGINT 毫秒时间戳列），不能随 T03 收尾提交一并带过。
- 建议后续步骤：(1) 在 T02 库修复 BUG-003（§4 修复建议二选一），补 unix 毫秒余数端到端回归断言；(2) tester 持本报告 S3 探针做回归验证（iso/iso-tz/locale 三格式一致含 `.123`，且 -1ms 含 `.999`），并复跑 G1 195/195；(3) 回归通过后由协调方做 T03 收尾提交（tasks.md 状态更新与 docs close commit 均不在本次验证范围内）。
- T03 贡献模块本身（375 行）与自带 46 项测试未发现缺陷；四项裁决评审意见（O-2/O-3）与观察项 O-1/O-4/O-5/O-6 均为非缺陷，不阻塞收尾。

> 2026-11-12 回归验证更新：BUG-003 已修复并验证通过（见 §7），本节「不通过——暂缓收尾提交」的结论更新为**通过，T03 可做收尾提交**；上文其余文字为 2026-11-12 首轮验证的原始记录，保留不动。

## 7. 回归验证记录（BUG-003 修复）

2026-11-12，tester 对 BUG-003 的修复（`7a7091629`：`parseUnixValue` 按毫秒余数声明 `fractionDigits`，`positiveMod(epochMs, 1000) === 0 ? 0 : 3`）+ 回归测试（`3b7fede54`）做了独立回归：门禁复跑 `npx vitest run packages/frontend/src/lib/valuePresentation --retry=0` **202/202**（temporalMath 96、timestampFormat 34、timestampContribution 46、timestampConfig 26；195 既有 + 7 新增，退出码 0）、`pnpm typecheck` 与 `pnpm typecheck:tsc` 双引擎均 OK（tsrs 0.18.0 / tsc 7.0.2，root / scripts / pack-ep 三程序）、命名门禁 `node scripts/check-test-module-naming.mjs` ok；代码审读确认修复 diff 最小（仅 `temporalMath.ts`，1 行逻辑 + 4 行注释）、`resolveFractionDigits` 未被改动（`timestampFormat.ts` 在 `e05e50093..HEAD` 间 0 行 diff）、`:421` 的 `+12` 断言修正正确（余数确为 12，即 0.012 秒；旧期望 0 编码了 bug 行为，修复后端到端实测 iso 输出 `1970-01-01T00:00:00.012Z`；同 case 其余断言未动，全文件仅 1 行既有断言被删改）；`/tmp/probe-bug003` 独立探针 17 项（R1–R6：原始复现三路径三格式一致含 `.123`、负值 `-1` 三格式含 `.999`、余数 0 不补 `.000`、秒单位无小数秒、999/1/`-1000`/`-1001` 边界、9 位文本精度原样且 locale 亚毫秒截断不变）全绿，退出码 0，探针用后已删除。**BUG-003 验证通过并关闭**（详见 bug 文件「回归验证」段）。回归中发现 1 处记录勘误（非代码缺陷）：bug 文件「回归测试」段称「未改动任何既有断言」不准确——`3b7fede54` 按协调方裁决修正了 1 条既有断言（`:421` 的 `+12` 期望 0 → 3），属授权修正而非放宽门禁，已在 bug 文件回归段中更正。结论更新：§6「不通过——暂缓收尾提交」改为**通过，T03 可做收尾提交**（tasks.md 状态更新与 docs close commit 不在本次验证范围内，由协调方执行）。
