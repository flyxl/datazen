# T02 测试报告：时间数学、时间戳格式化与配置验证核心（valuePresentation）

- 被测对象：`packages/frontend/src/lib/valuePresentation/`（temporalMath / timestampFormat / timestampConfig / index，4 源码文件 + 3 自带测试文件，共 7 文件 +3452 行；自带测试 149 项）
- 分支：`feature/issue-32-value-presentation`（HEAD `75771753f`，作者 issue-32-agent；T02 代码提交，工作区在验证开始时干净）
- 测试角色：tester 独立验证，未修改任何被测代码
- 依据：`docs/archive/development/issue-32-value-presentation-design.md` §5 §8 §9 §10-B、`docs/development/issue-32/tasks.md`（T02）、提交信息 `75771753f`
- 日期：2026-11-12

## 1. 测试环境与命令

环境：Node v24.22.1，vitest 4.1.10（仓库自带），tsc 7.0.2 / tsrs 0.18.0，系统时区 UTC（`Intl.DateTimeFormat().resolvedOptions().timeZone` = UTC；全部探针均显式传入时区与语言，不依赖机器默认值）。对抗性探针运行于独立临时项目 `/tmp/probe-t02/`（通过 `node_modules` 符号链接复用仓库依赖，`environment: node`，被测源码以绝对路径导入，不污染仓库工作区；测试后 `git status` 复核仓库无任何测试方改动）。

环境说明：本机 `PATH` 中无 `pnpm` 二进制（`which pnpm` 无结果），任务书称"pnpm 12.18.0 已装好"与实测不符；G2/G4 改用 `npx -y pnpm@12.18.0 <script>` 调用，版本号实测 12.18.0，与任务书一致，属等价调用，不影响门禁结论。`cargo`/`rustc` 同样不在本机 `PATH`（见 G4 说明）。

| 命令 | 用途 |
|------|------|
| `npx vitest run packages/frontend/src/lib/valuePresentation --retry=0` | T02 包单测（排除根配置 retry 掩盖侥幸通过） |
| `npx -y pnpm@12.18.0 typecheck` / `npx -y pnpm@12.18.0 typecheck:tsc` | 双引擎类型门禁（tsrs / 权威 tsc；root / scripts / pack-ep 三程序） |
| `node scripts/check-test-module-naming.mjs` | 测试命名门禁 |
| `npx -y pnpm@12.18.0 test`（即 `vitest run`） | 全量回归（超 10 分钟按任务指令中断，见 G4） |
| `/tmp/probe-t02` 探针项目（24 项，P1–P10 + 补充 S1–S4） | 对抗性行为验证（PRD 锚点、DST、精度回归、配置拒绝、边界与稳定错误码） |
| `grep` 审读（`new Date(` / `Date.parse` / `any` / `Temporal.` / 驱动引用） | P5 的源码级佐证与契约严格性审读 |

探针项目结构：`probes/t02-adversarial.test.ts`（P1–P10 共 20 项 + 补充 4 项）。探针属一次性验证资产，位于系统临时目录，不进入仓库；各项方法已在下表内描述到可复现程度。

## 2. 测试项清单

### 2.1 既有验证（仓库门禁）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| G1 | T02 包单测全量 | `npx vitest run packages/frontend/src/lib/valuePresentation --retry=0` | ✅ 3 文件 149/149 通过（temporalMath 91、timestampFormat 32、timestampConfig 26，与申报数量一致；退出码 0） |
| G2 | 类型门禁（tsrs 单引擎 + 权威 tsc） | `pnpm typecheck` / `pnpm typecheck:tsc`（经 npx pnpm 等价调用） | ✅ 两者均 OK（root / scripts / pack-ep 三程序；tsrs 0.18.0、tsc 7.0.2；退出码均为 0）。根 tsconfig `include` 含 `packages/frontend/src`，新文件实际被覆盖 |
| G3 | 测试命名门禁 | `node scripts/check-test-module-naming.mjs` | ✅ ok（1837 Rust 文件、256 cfg(test) 模块；2554 TS 文件、8 wdio spec 树；退出码 0） |
| G4 | 全量回归 | `pnpm test`（`vitest run`） | ⚠️ 未跑完：运行约 14 分钟后按任务指令中断（18:23 启动，约 18:37 中断）。中断时已完成 **461 个测试文件**，其中 T02 的 3 个文件均在全量中通过（149 项）；唯一失败是 `scripts/__tests__/check-driver-type-isolation.test.ts` 的 1/36 项，原因是本机无 Cargo 工具链（`spawnSync cargo ENOENT`，见 §2.1 注）——环境缺失，非代码回归，与 T02 无关。**全量未完成，不得读作"全过"** |

> G4 注：失败用例的日志原文为 `[check-driver-type-isolation] could not run cargo metadata … spawnSync cargo ENOENT. This guard needs a Cargo toolchain; it is wired into the CI rust job for that reason.`；本机 `which cargo` / `which rustc` 均无结果。T02 提交只新增 `packages/frontend/src/lib/valuePresentation/` 下 7 个文件（`git show --stat HEAD` 核实，无其他改动），不触及驱动类型隔离的任何输入，该失败不可能由 T02 引入。建议协调方在有 Cargo 的环境或 CI 中复跑该单文件确认。

### 2.2 对抗性探针（/tmp/probe-t02，独立于 coder 自带测试编写）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| P1 | PRD 锚点 1：`2026-01-15T14:30:00Z` 用 Asia/Shanghai 展示 | 解析后分别以 iso-tz / locale(en-GB) / iso 输出并断言全量文本 | ✅ 通过。iso-tz = `2026-01-15T22:30:00+08:00`（含 22:30 与 +08:00）；locale = `15/01/2026, 22:30:00 GMT+8`（含 22:30）；iso = `2026-01-15T14:30:00Z`（保持 UTC） |
| P2 | PRD 锚点 2：Unix `1705316400000` → `2024-01-15T19:00:00+08:00` | number 毫秒、number 秒、string 毫秒三路径解析并以 iso-tz 输出 | ✅ 通过。三路径 epochMs 均为 1705316400000，iso-tz = `2024-01-15T19:00:00+08:00` |
| P3 | DST 双义时间被拒绝 | `wallClockToInstant({2024-11-03 01:30}, America/New_York)` | ✅ 通过。`{ ok: false, reason: 'dst-ambiguous' }`。对照：同日 00:30（EDT → 04:30Z）与 03:30（EST → 08:30Z）均唯一通过 |
| P4 | DST 不存在时间被拒绝 | `wallClockToInstant({2024-03-10 02:30}, America/New_York)` | ✅ 通过。`{ ok: false, reason: 'dst-invalid' }` |
| P5 | 无时区文本被拒绝，绝无 `new Date(string)` 隐式解析 | `parseZonedTimestamp('2024-01-15 14:30:00')` 与 `'2024-01-15T14:30:00'` 均应拒；`parseWallClockText` 同串只返回墙上分量；全目录 grep `new Date(` / `Date.parse` | ✅ 通过。两者均为 `{ kind: 'error', reason: { reason: 'invalid-format' } }`；`parseWallClockText` 返回 `{ shape: 'datetime', wall: {…} }`，序列化结果不含 `epochMs`；带偏移文本在 `parseWallClockText` 下同样 `invalid-format`（归属正确）。grep：全目录 5 处 `new Date(` 均为数值入参（`new Date(epochMs)` / `new Date(instant.epochMs)`，temporalMath.ts:312、timestampFormat.ts:149/241，另 2 处在测试内），`Date.parse` 0 处 |
| P6 | 微秒/纳秒精度保留，不静默截断（小数秒毫秒丢失 bug 回归） | `.123456789` / `.123` / `.100` / `.000000001` / `.999999999` / epoch 前 1ns / 墙上往返 / `compareInstants` 亚毫秒比较 / 10 位小数秒 | ✅ 通过，bug 回归点已验证关闭。`.123456789` → epochMs 含毫秒 123、subSecondNanos 456789、fractionDigits 9，iso 与 iso-tz 原样输出 9 位；`.123` epochMs 精确含 123ms（**提交信息申报的毫秒丢失 bug 已修复**）；`.100` 输出 `.100`（尾零不裁剪）；1ns → epochMs 0、subSecondNanos 1；epoch 前 1ns（`1969-12-31T23:59:59.999999999Z`）→ epochMs -1（floor）、subSecondNanos 999999，iso 往返一致；墙上往返保留 123456789ns；`.123456789` > `.123456788`（compare=1）；10 位小数秒报 `fraction-digits-out-of-range`，不截断 |
| P7 | unix + sourceTimeZone 被拒绝 | `validateTimestampOptions({ interpretation: { kind: 'unix', unit: 'milliseconds' }, …, sourceTimeZone: { kind: 'iana', name: 'Asia/Shanghai' } })`；对照 text + sourceTimeZone 合法 | ✅ 通过。`ok: false`，problems 含 `{ path: 'sourceTimeZone', message: 'unix interpretation must not carry a source time zone' }`；对照合法 |
| P8 | 未知配置字段被拒绝 | 顶层多余字段 / interpretation 内多余 `unit`（native）/ displayTimeZone 内多余 `name`（utc） | ✅ 通过。三处均 `ok: false`，路径分别为 `extraField`、`interpretation.unit`、`displayTimeZone.name`，message 均为 `unknown field`（含说明） |
| P9 | 超大精度 Unix 值 BigInt 精确解析不丢精度 | number 超安全整数 / 1e17 毫秒 number 与字符串 / 支持范围边界 MAX±1 / 9999-12-31 秒与毫秒字符串 | ✅ 通过。`2^53+1` 与 `1e17`（number）→ `not-safe-integer`；字符串 `'100000000000000000'`（毫秒与秒）→ `out-of-range`（BigInt 精确算出越界，不静默取整）；`MAX_SUPPORTED_EPOCH_MS = 3093559603199999`，该值字符串可解析、+1 越界，下界同理；`'253402300799'` 秒与 `'253402300799999'` 毫秒精确且数值/字符串路径一致。附注：支持范围内毫秒值恒 < 2^53，BigInt 与浮点的分歧只体现在越界/拒绝行为上，本探针正是验证该行为 |
| P10 | 稳定错误原因码 | 同类输入批量比对 + 同一输入 3 次重复 + Unix 错误码 + 时区名校验 | ✅ 通过。`''`→`empty-input`、`'x'`→`invalid-format`、`2024-13-01`→`invalid-month`、`2024-02-30`→`invalid-day`、`24:00`→`invalid-hour`；重复调用同一输入（含 dst-ambiguous）码不变；`'12.5'`/`'1e3'`→`invalid-integer-syntax`、`NaN`→`not-safe-integer`。时区名：`resolveTimeZoneName({ kind: 'iana', name: 'Not/AZone' })` 格式层放行（`ok: true`，见 O-2），`temporalMath.isValidTimeZoneName('Not/AZone')` 为 false，端到端 `formatTimestampOutput`（iso-tz）一致报 `invalid-time-zone` |
| S1 | 补充：locale 亚毫秒精度行为 | `.123456789` 以 locale（en-GB、UTC）输出 | ✅ 实测（记录为观察项 O-1，非缺陷）：输出 `15/01/2026, 14:30:00.123 GMT+0`——截断到毫秒，`.123` 保留、`456789` 丢失；文件头已文档化该决策 |
| S2 | 补充：0 与负数 Unix | `parseUnixValue(0 / -1 / '-86400', 'seconds')` | ✅ 通过。0 → epochMs 0；-1 → epochMs -1000；`'-86400'` 字符串正常解析 |
| S3 | 补充：历史亚分钟偏移 | `1850-06-15T12:00:00Z` 以 iso-tz（Asia/Shanghai）输出 | ✅ 通过。`1850-06-15T20:05:43+08:05:43`（LMT 偏移精确到秒） |
| S4 | 补充：iso 格式同样先校验时区 | iso + 无效 IANA 名 | ✅ 通过。`{ ok: false, reason: { reason: 'invalid-time-zone' } }`（与格式无关，预览与展示一致失败） |

> 探针过程说明：首轮 20 项中 1 项失败系**探针自身缺陷**（P10 误以为 `resolveTimeZoneName` 应拒绝格式合法但不存在的时区名 `Not/AZone`；实测返回 `ok: true`，与 timestampFormat.ts 文档"iana 只做格式校验，此处不查询 Intl，完整校验由调用方决定是否执行"逐字一致），修正探针为"格式层放行、Intl 层与端到端一致拒绝"的文档化分工断言后 20/20 通过；另 1 处探针期望值错误（MAX 边界初值误写 3155760959999999，实测 3093559603199999，已修正）。被测代码在全部探针下无失败。最终探针 24/24 通过（含补充 S1–S4）。

### 2.3 契约类型严格性审读

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| T1 | any 泄漏 / 断言滥用 | 对 valuePresentation 全目录 grep `any`、`as`、`@ts-ignore`、`@ts-expect-error`、`eslint-disable` | ✅ 源码 0 处 `any`（命中均为注释中"禁 any"字样）；`as` 仅出现在类型收窄的常规位置，未发现以断言跳过验证；`validateTimestampOptions(input: unknown)` 以 unknown 入参 + 判别联合出参 |
| T2 | 原生 Temporal 依赖 | grep `Temporal\.`（全局对象） | ✅ 0 处；实现只用 `Intl.DateTimeFormat` + 字符串分段解析，符合设计 §8.5"不能假定原生 Temporal 普遍可用" |
| T3 | 驱动耦合 | grep 驱动名 / `DatabaseTypeMeta` | ✅ 0 处；T02 纯时间数学核心不含驱动类型判定（设计 §8.1 的驱动侧判定属后续任务，见 §3） |

## 3. 设计符合性逐条结论

对照设计文档（§5 / §8 / §9 / §10-B）：

**§5 配置模型与规则（T02 范围：TimestampOptions 形状与验证）**

| 条目 | 结论 |
|------|------|
| `TimestampOptions`：interpretation（native / unix{seconds\|milliseconds} / text）+ format（iso / iso-tz / locale）+ displayTimeZone + 可选 sourceTimeZone | ✅ 符合（timestampConfig.ts:22–31；探针 P7/P8 验证语义） |
| 源时区只出现一处，禁止同时在 interpretation 与顶层重复存储 | ✅ 符合：interpretation 内无时区字段，sourceTimeZone 仅顶层可选；unix 携带即拒（P7） |
| `native` 不把数字自动解释成 Unix；Unix 必须显式指定单位 | ✅ 符合：interpretation 为显式判别联合，unix 缺 unit 报 `interpretation.unit` 错误；native 无 unit 字段（多余 unit 即拒，P8） |
| 无时区文本与原生 local-datetime 可使用 sourceTimeZone；带偏移值以自身时刻为准 | ✅ 符合（类型与验证层面；带偏移值的重解释拒绝属 T03+ 消费侧，T02 只交付验证原语） |
| 规则数组 / 正则安全 / 规则数量与输入长度设限 | ➖ 范围外（规则系统属阶段 C / T05+，不在 T02） |

**§8 时间解析算法与精度（逐条）**

| 条目 | 结论 |
|------|------|
| 1. 规范化类型判定原生时间，数据库特殊类型经 DatabaseTypeMeta/驱动声明，不在宿主硬编码驱动名称 | ➖ 范围外：T02 为纯时间数学核心，不做类型判定；审读确认 0 处驱动引用、无宿主硬编码（T3） |
| 2. Unix 字符串先验证整数语法，再用 BigInt 精确解析；number 必须 Number.isSafeInteger | ✅ 符合（P9；`parseUnixValue`：字符串经 `INTEGER_STRING_RE` 后 BigInt，number 先 `isSafeInteger`） |
| 3. 秒乘 1000 使用精确整数运算，检查支持的日期范围后才转用于格式器的数值；0 与负数正常处理 | ✅ 符合（P2、P9 边界、S2；秒×1000 在 BigInt 域完成，范围检查在转 InstantParts 前） |
| 4. 带时区文本严格解析时刻与小数秒；无时区文本不交给隐式本地解析的 new Date(string) | ✅ 符合（P5、P6；手工正则分段解析，全目录无 `new Date(string)`、`Date.parse` 0 处） |
| 5. 使用支持 IANA 与显式 DST 歧义策略的时间实现；不能假定原生 Temporal 普遍可用 | ✅ 符合（P3/P4；Intl + 双向回读法，无 `Temporal.` 全局引用；DST 歧义/不存在均拒绝不猜测） |
| 6. 小数秒与时刻转换分开处理，保留原生微秒/纳秒精度；若依赖无法保证则返回 unchanged，不静默截断 | ✅ 符合（iso / iso-tz：P6 全绿；10 位小数秒拒绝不截断）。⚠️ 例外见 O-1：locale 格式截断到毫秒，属文件头文档化的二选一决策，与本条字面"不静默截断"存在张力，判定为观察项而非缺陷（见 §5） |
| 7. iso 输出 UTC；iso-tz 使用目标时刻对应的实际偏移；locale 固定传入语言和时区，不能依赖机器隐式默认值；locale 精度需显式保留，无法实现时回退 | ✅ 符合（P1/P2、S3 历史偏移、S4；locale 显式传入 locale + timeZone，`fractionalSecondDigits: 3` 显式设置；"回退"实现为截断并文档化，见 O-1） |
| 8. 无时区源未配置、DST 歧义、范围错误使用稳定原因码；预览和实际展示调用同一解析实现 | ✅ 符合（P10；原因码为字符串字面量判别联合，同类输入稳定可重复。预览/展示同一实现属 T03+ 消费侧，T02 交付了唯一的解析入口 `parseZonedTimestamp` / `parseUnixValue` / `parseWallClockText`） |

**§9 文件改动边界**

| 条目 | 结论 |
|------|------|
| 拟新增 `packages/frontend/src/lib/valuePresentation/` | ✅ 符合：T02 只新增该目录下 7 文件（`git show --stat HEAD`：4 源码 + 3 测试，+3452 行，0 修改旧文件），零新依赖 |
| 时间实现落点 | ✅ 符合：落在设计 §9 列出的 `packages/frontend/src/lib/valuePresentation/` 模块内；"独立 EP 包"的表述与同节模块清单并存，T02 任务范围即此目录，不判偏差 |
| 需修改的既有位置（CellRenderer、DetailPanel、ContentViewDrawers、settingsStore、设置 DTO 等） | ➖ 范围外：均未改动，属 T03+ 宿主管线任务；T02 未提前接线，不构成缺口 |

**§10-B 验证清单（阶段 B：时间与列配置）**

| 条目 | 结论 |
|------|------|
| 时区跨日 | ✅ 自带测试覆盖（temporalMath.test.ts:617：`2026-01-16 07:30 +08:00 → 2026-01-15T23:30Z`，UTC 日期回滚）；探针 P1/P2 验证 +08:00 换算，S3 验证历史偏移 |
| 正负偏移 | ✅（P1/P2 为 +08:00；自带测试覆盖负偏移与 +05:45 非整分钟偏移，提交信息申报；S3 覆盖 +08:05:43 亚分钟偏移） |
| DST | ✅（P3/P4 + 自带测试） |
| NULL | ➖ 范围外：NULL 语义在 `present()` 层（T03+ 展示器贡献），纯数学核心无 NULL 输入 |
| 0、负时间 | ✅（S2；自带测试覆盖负时刻 floor 分解，P6 epoch 前 1ns） |
| 不安全整数 | ✅（P9） |
| 数字字符串 | ✅（P2/P9 字符串路径） |
| 越界 | ✅（P9 MAX±1 / MIN-1；自带测试覆盖 ±100000 年边界） |
| 非法文本 | ✅（P5/P10） |
| 无时区值 | ✅（P5） |
| 高精度 | ✅（P6） |

## 4. Bug 列表

无。T02 引入的缺陷 0 项；提交信息申报的"小数秒毫秒丢失 bug"经 P6 回归探针验证已修复（`.123` epochMs 精确含 123ms，`.123456789` 九位全保留）。

## 5. 观察项（非缺陷，供 coder / 后续任务参考）

- **O-1（locale 格式亚毫秒截断）**：`formatTimestampOutput` 的 locale 分支对微秒/纳秒源精度截断到毫秒（实测 `.123456789` → `…14:30:00.123 GMT+0`，S1）。文件头已明确文档化该决策（"Intl 的 fractionalSecondDigits 最高 3 位……需要完整精度时 iso / iso-tz 均无损"），且与设计 §8.7"无法实现时回退"的二选一相容 → 不判缺陷。但与 §8.6"不静默截断"的字面表述存在张力：输出本身不提示精度丢失。建议 T03 消费侧在 locale 模式下遇到亚毫秒源值时考虑在界面注明精度（如 tooltip 显示完整值），或由协调方确认该取舍。
- **O-2（时区名校验分工）**：`isValidIanaTimeZoneName` / `resolveTimeZoneName` 只做格式校验（正则），格式合法但不存在的时区名（如 `Not/AZone`）在该层放行，完整存在性由 `temporalMath.isValidTimeZoneName`（Intl 构造）与 `formatTimestampOutput` 入口兜底（P10、S4 验证端到端一致拒绝）。分工已在 timestampFormat.ts 文档中写明，行为正确；后续调用方若直接使用 `resolveTimeZoneName` 的结果而不经过 `formatTimestampOutput`，需自行补 Intl 校验。
- **O-3（DST 探测边界已文档化）**：`wallClockToInstant` 取 ±24h 三个探测点的偏移作候选集，文件注释声明"若某历史时区在 24h 内发生两次偏移变更，可能漏判有效解释（误报 dst-invalid）——已知 tzdata 无此情形"。属文档化边界，非缺陷；覆盖 2011 Pacific/Apia 整日跳变在候选集内。
- **O-4（探针资产）**：/tmp/probe-t02 为一次性验证项目，重启后失效；本报告 §2.2 各项方法已描述到可复现程度。建议将 P6 的 `.123`/`.123456789` 回归断言与 P3/P4 的 DST 拒绝断言核对是否已在自带测试中存在（提交信息称 bug"由新测试套件发现"，自带 149 项应已覆盖；探针为独立复核）。
- **O-5（本机 pnpm/cargo 缺失）**：本机 PATH 无 pnpm 与 cargo 二进制（见 §1 与 G4 注），G2/G4 改用 npx pnpm 等价调用，G4 的 1 项失败纯属环境缺失。后续 tester 在本机复跑全量回归前需先补齐工具链，或以 CI 结果为准。

## 6. 结论

**通过，可做收尾提交。**

- 既有门禁：T02 包 149/149（`--retry=0` 复跑确认）、`pnpm typecheck` 与 `pnpm typecheck:tsc` 双引擎通过、命名门禁通过。
- 对抗性探针 24 项（P1–P10 + 补充 S1–S4）全部通过：PRD 双锚点精确命中、DST 双义/不存在均拒绝、无时区文本拒绝且源码级确认无 `new Date(string)`、小数秒毫秒丢失 bug 回归验证已修复（9 位精度全保留）、unix+sourceTimeZone 与未知字段均路径化拒绝、BigInt 边界精确、错误原因码稳定可重复。
- 设计符合性：§8 八条中七条符合、一条（§8.1）范围外；§5 配置模型符合；§9 文件边界符合（仅新增 7 文件，零新依赖）；§10-B 清单除 NULL（范围外，属 T03+）外全部覆盖。
- Bug 0 项；观察项 5 条均为非缺陷（O-1 locale 截断取舍建议协调方确认，其余为文档化分工/边界与环境说明）。
- 唯一未完成项：G4 全量回归因超 10 分钟按任务指令中断（461 文件完成，唯一失败为本机缺 Cargo 的环境问题，与 T02 无关），**不得读作"全量通过"**；建议收尾提交前由协调方在有 Cargo 的环境或 CI 中复跑全量确认。
