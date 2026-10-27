# CI 与测试矩阵

> 与 [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)、[release.yml](../../.github/workflows/release.yml) 及 [AGENTS.md](../../AGENTS.md) 测试约定配套。  
> 目标：PR 流水线快且稳定；驱动 SKU 组合可扩展；**`all` 预设只进 Host 单测，不进编译矩阵**。

## 1. 总览

| 层级 | PR CI（`ci.yml`） | Release（`release.yml`） | 本地 / 维护者 |
|------|-------------------|--------------------------|---------------|
| 驱动选型 | **`basic` 固定**（postgres, mysql, sqlite, redis）；Host 单测另跑一遍 **`all`** | Basic / All × 四平台 + Akulaku × 三平台（Windows / macOS，无 Linux） | 任意 `--drivers=` / `DATAZEN_DRIVERS` |
| Host 前端单测 | ✅ `basic`：`pnpm test:unit`；`all`：`pnpm test:unit:driver-set`（**收窄至 codegen 敏感文件**，硬门禁，理由见 §2.1.1） | 构建前 `pnpm build`（含 typecheck） | `pnpm test:unit` / `pnpm test:unit:driver-set` |
| 驱动 UI 单测 | ✅ `pnpm test:unit:drivers` | 构建前 `pnpm build`（含 typecheck） | `pnpm test:unit:drivers` |
| TypeScript | ✅ `pnpm typecheck`（**单引擎**：`tsrs` = TS 7.1-dev，见 §2.1.4） | 同上（`union-typecheck` **也是**只跑 `tsrs`，见 §6.2） | `pnpm typecheck` / `pnpm typecheck:tsc` / `pnpm typecheck:tsrs` |
| Host Rust lib | ✅ `cargo test -p datazen_desktop --lib`（basic features + `datazen/test-harness`，与平台腿 feature 对齐以共享 datazen 编译，见 §2.2） | 完整 release 构建 | 同 PR CI 形 |
| driver-api | ✅ `cargo test -p datazen-driver-api`（lib + tests/ 4 文件 + doc test，2026-10-27 由 `--lib` 扩充） | 随构建链接 | `cargo test -p datazen-driver-api` |
| modules（datazen）单 feature 腿 | ✅ `pnpm test:platform-crates` 的 `FEATURE_TESTS` 逐腿编译（ai 腿已覆盖 ai-api 退役前的那步组合闭包；原 `--features settings,ai,connection,named_views` 组合步于 2026-10-27 删除，理由见 §2.2） | 随构建链接 | `pnpm test:platform-crates` |
| 平台内核 crate（`modules` / `infra`；`server` 层已武装待主体） | ✅ `pnpm test:platform-crates`，集合由脚本发现 | 随构建链接 | `pnpm test:platform-crates` |
| 平台 crate 架构门禁（全部平台边界规则） | ✅ `pnpm test:platform-arch`，附变异自证 `pnpm test:platform-arch:mutations` | ❌ | `pnpm test:platform-arch` |
| Basic path 驱动 lib | ✅ 四 crate 由 `--tests` 步一并运行（lib + 集成 target，见下行） | Basic SKU 内嵌 | `cargo test -p datazen-driver-<id> --lib` |
| Basic path 驱动集成测试（`tests/`，32 个文件） | ✅ `--tests` 编译并运行（**含四驱动 lib 单测**）；CI 无数据库，live 层在命令里**显式豁免**（见 §2.2） | Basic SKU 内嵌 | `cargo test -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis --tests`；live 层**默认严格**（验不到即失败），本地无数据库时加 `DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1` 声明豁免，`DATAZEN_CONTRACT_REQUIRE_LIVE=1` 压过豁免、强制严格 |
| 非 basic path 驱动集成测试（19 个文件：sqlserver 11 / clickhouse 3 / duckdb 3 / mongodb 2） | ❌ 未进 PR 矩阵 | **All SKU** 构建时编译链接 | 改这些驱动时本地 `cargo test -p datazen-driver-<id> --tests` |
| 可选 path 驱动 lib | ❌ | **All SKU** 构建时编译链接 | 改驱动 crate 时本地必跑 |
| Git 驱动（kiwi/superset） | ❌ | **Akulaku SKU**（需 Deploy Key） | 见 [ci-private-drivers.md](./ci-private-drivers.md) |
| Host E2E | ❌ | ❌（发版后手工 / R 阶段） | `pnpm e2e` / `pnpm e2e:minimal` |
| Host 契约矩阵 E2E | ❌ | ❌ | `pnpm e2e:contract:matrix` |
| 驱动专属 E2E | ❌ | ❌ | `packages/drivers/<id>/e2e/` |

**原则**

1. **Basic 必测**：每个 PR 与 `main` push 均跑 basic 四驱动 + Host 三件套（TS 单测 / Host lib / driver-api；modules 单 feature 腿由 `pnpm test:platform-crates` 覆盖）。
2. **All 只进 Host 单测，不进编译矩阵**：`resolve-drivers --drivers=all` 的 codegen 让 PR CI 把 Host 前端单测中**codegen 敏感的那部分**（模块图会执行 `generated.ts` 的测试文件，运行期由 `select-codegen-sensitive-tests.mjs` 选出）在 `all` 下再判一遍——前端驱动形态是编译期 codegen，验证它不需要编译 Rust 驱动，因此 `all` 在 PR 上是廉价的；而 `cargo` 侧编译全部 path 驱动仍然只发生在 Release **All** SKU 与本地全量验证（`--drivers=all` 约 5 分钟重编译），不占 PR 流水线。
3. **Path 轮转（维护者策略）**：可选 path 驱动（mongodb、clickhouse、duckdb、sqlserver、elasticsearch 等）的 **Rust lib 不在 PR CI 矩阵内**；其 **UI 单测全部进 PR CI**（`pnpm test:unit:drivers` 按文件系统收集 `packages/drivers/*/ui/__tests__/`，与 `--drivers=` 选型无关，因此可选驱动同样被覆盖）。改 Rust 时作者仍须在 PR 说明中列出 `cargo test -p datazen-driver-<id> --lib`（及该 crate 内 E2E）。发版 **All** SKU 是对全部 path 驱动的集成校验。
4. **契约矩阵**：Host Connection Contract（`e2e/contract/`）验证 PG/MySQL/SQLite 上同一套 Host UI journey；**不进 PR CI**，由维护者在合并前或 R 阶段跑 `pnpm e2e:contract:matrix`；fixtures 单测 `pnpm test:unit:e2e-contract` 可在本地或后续 CI 扩展中启用。

## 2. PR CI 步骤（与 workflow 对齐）

触发：`pull_request` → `main`、`push` → `main`、`workflow_dispatch`。

环境：`ubuntu-latest`；`DATAZEN_KEYRING=file`（无 OS 钥匙串）。

结构：`frontend` 与 `rust` 两个**并行 job**（互不依赖，墙钟时间取较大者），外加聚合 job `ci`（`needs: [frontend, rust]`）作为分支保护的 required status check。

### 2.1 frontend job（前端段）

| 步骤 | 命令 / 动作 | 说明 |
|------|-------------|------|
| 依赖 | `pnpm install --frozen-lockfile` | Node **24**、pnpm **11**（与 workflow 一致） |
| 代码生成 | `node scripts/generate-builtin-locales.mjs` | `builtinLocales.ts` 为 gitignore codegen |
| 类型 | `pnpm typecheck` | `tsrs --noEmit`（单引擎，见 §2.1.4）。发布侧的 `union-typecheck` **也是只跑 `tsrs`**（§6.2），所以本地绿与发布绿是同一件事 |
| 守卫 | `check-managed-stubs.mjs`、`check-id-terminology.mjs`、`check-module-layers.mjs` 等 | 防止误提交 inject 产物、ID 术语回退、分层倒置。`check-structure-editor-guardrails.mjs` 已移至发布前，见 §2.1.3 |
| Host 单测 | `pnpm test:unit` | Vitest。驱动集不是由 `pretest:unit` 定的：pnpm ≥ 7 默认不跑 `pre`/`post` 脚本（无 `.npmrc` 开 `enable-pre-post-scripts`），该 hook 实为惰性；真正把 codegen 落成 `basic` 的是 `pnpm install` 的 `prepare` → `ensure-generated-drivers.mjs` → `resolve-drivers.mjs` 默认值 |
| Host 单测（`all`） | `pnpm test:unit:driver-set` | `scripts/run-unit-driver-set.mjs`：`resolve-drivers.mjs --codegen-only --drivers=all` → `generate-builtin-locales.mjs` → **运行期收窄** → `vitest run <选中文件>`。收窄自同一份 `include` glob：只有模块图**执行** `src/extensions/generated.ts` 的测试文件才可能观察到驱动集（判定口径与 fail-closed 清单见 `scripts/select-codegen-sensitive-tests.mjs`），其余文件在两轮跑的是同一份字节码，由上面的 `basic` 步判过。`all` = 全部 **path** 驱动（不含 kiwi/superset），故无需 Deploy Key 与网络。**硬门禁**（无 `continue-on-error`），理由见 §2.1.1；重新加回软门禁时**必须同时**改 `scripts/__tests__/run-unit-driver-set.test.ts`（见 §2.1.1 末） |
| 驱动 UI 单测 | `pnpm test:unit:drivers` | `packages/drivers/*/ui/__tests__/`；与 Host 单测互不收集，两步都必须跑 |
| Site（条件） | `check-site-seo.mjs` | 仅当 diff 含 `site/`（`fetch-depth: 0` 仅此 job 需要） |

### 2.1.1 该步为何是硬门禁，以及 `testTimeout` 为何取 10s

**该步曾是软门禁，理由与"已知红"无关。** 早期版本的 `ci.yml` 注释记录了两个阻塞项，
二者都是基线陈旧造成的假象：它们测于 `7f35e1932`，该基线比 `d1edfa7ba` / `f4c2e5078`
早 9 个 commit。在 `feat/platform-p0` @ `7559759b7` 上，两者都不存在——该基线
`--drivers=all` + locales codegen 全绿：**555 文件 / 5677 用例，exit 0，140s**；
`tester_tunnelValidationMatrix.test.tsx` 单跑 **36 通过**。此前记为红的
`ContentView` / `ContentViewKv*` / `QueryPanel.executeCancel`，正是 `cbf87a4c0`
（修复 schemaStore/panelStore mock 漂移导致的 18 个前端单测失败）已修掉的那批。

保留 `continue-on-error: true` 的理由只有一个，很窄：**这一步与 10s 的
`testTimeout` 都还没有在真实 GitHub runner 上跑过。** 本机 8 核开发机全绿，不能推断
真实 runner 的全绿——本项目无法控制 runner 规格，仓库可见性也未定。因此本文档与
`vitest.config.ts` 中**不对 runner 的核数做任何断言**（下表第二列描述的是本机
实测行的形态，不是对 CI runner 规格的推测）。

**现在它已是硬门禁：`continue-on-error: true` 已从 `ci.yml` 删除。** 上述观察窗口
由决策关闭，本节随之改写为已实现事实。

**遗留风险，明写而不是藏起来。** 首次在真实 runner 上执行时，**可能**因与本分支无关的
环境原因变红。吸收它的是下表的实测预算 + `vitest.config.ts` 的 `retry: 2`——两者都不是
继续放宽任一数值的理由。

**⚠️ 重新加回软门禁是耦合改动，两处必须同时改。** `ci.yml` 的 `continue-on-error`
与守卫 `scripts/__tests__/run-unit-driver-set.test.ts` 中断言其**不存在**的那条，
是同一个契约的两端：只改 workflow 就直接造出一个红 CI。

**这个契约的两端都不在 `ci.yml` 里。** 在 `ci.yml` 里搜 `testTimeout` 只会搜到**注释**，
搜不到任何配置——`testTimeout: 10_000` 实际写在 `vitest.config.ts` 根级 `test` 块的 `testTimeout` 上。
两个 `continue-on-error` 的一半在 workflow，另一半（断言它不存在的那条）也在
`scripts/__tests__/` 下。**只改 workflow 永远不够，只改守卫也永远不够。**

**⚠️ 这个硬门禁曾经有过一个例外，而且它已经被消除了。** 四个 workflow 里最后一处
`continue-on-error: true` 是 `release.yml` 的 i18n 翻译完整性步——它把"翻译不全"降级成
"黄色提醒"，于是 8 个宿主语言缺 **832 个 key** 长期无人理会，一个看得见但没人当回事的门禁
等于没有门禁。现已裁定**发布不允许只有英文**：该步改为硬阻断（§2.1.3），全仓
`continue-on-error: true` 归零。因此现在**不存在**"哪一步必须软"的例外清单；
若将来重新引入 `continue-on-error: true`，必须同时更新本节并说明为何该失败不该阻断。

`pnpm test:unit:driver-set` 步**必须没有** `continue-on-error`；上面这条已不适用。

**`testTimeout` 取值依据（实测，非拍脑袋）。** 对全部 5714 个 Host 用例逐条统计
墙钟耗时（驱动集 `all`，8 核 macOS，`--reporter=json`），三个竞争档位：

| 档位 | 运行前 1 分钟 load | p95 | p99 | p99.9 | max | 超 5s | 失败 |
|------|------|-----|-----|-------|-----|-------|------|
| `--maxWorkers=2`（本机低负载实测行） | 13.18 | 57ms | 170ms | 751ms | 1745ms | 0 | 0 |
| 默认 8 workers，轻度超订 | 7.04 | 118ms | 336ms | 2299ms | 4392ms | 0 | 0 |
| 默认 8 workers，6–7× 超订 | ~48 | 329ms | 918ms | 3436ms | 11629ms | 4 | 2 |

**上表原样保留，不做任何修饰。** 如实陈述三点：

1. 表中最坏实测值为 `max 11629ms`，而 `10_000` **低于**该值；
2. 11629 出现在 8 workers / load 48 档，该档另有 2 个用例失败；
3. 本机低负载（load 13 与 load 7 两档）从未观测到接近 10s 的单例，最慢分别为
   1745ms 与 4392ms。

**本文不再给出任何倍数推导。** 此前写的 "`10_000` ms ≈ 该实测最坏值的 2.3 倍"站不住：
它与本表自相矛盾（表里记的最坏值是 11629ms，而 10000 < 11629）。该推导已删除，
不要再按它的形式重建。

**作用范围：`testTimeout` 是根级 `test` 键，对 `include` 下全部 glob 生效。**
它同时作用于既有的 `pnpm test:unit`（basic 驱动集步）和新增的
`pnpm test:unit:driver-set`（`all` 步），**不是**只作用于 `all`。抬高它只会让真正
卡死的用例晚一点被报出来，不会把原本会过的用例变红。

**运行期抖动由 `retry` 兜底，不做 worker 数工程。** `vitest.config.ts` 的根级 `test`
块设了 `retry: 2`，同样对两个步全局生效。取 2 而非 1：要吸收的是竞争抖动，它可能
连续影响一段用例，立即重跑一次未必等得到抖动过去；成本只在**已经失败**时支付，
且有界——最坏情况单个卡死用例在 3 × 10s = 30s 后报错，不会无限重试。
**代价（明写，不可软化）：重试会让"首次失败、重试通过"的用例最终报为通过，因此
不能用"本轮全绿"反推"不存在间歇性失败"。** 判断抖动要看重试计数，不能只看退出码。

**保证范围的边界：`testTimeout` 只对会 yield 到事件循环的用例生效。**
load 48 那一档里 `DiffDetail`(11629ms)、`resolve-pro`(6540ms)、`tableDataStore`(5236ms)
都跑过 5s 却仍然 **passed**——它们是纯 CPU 占用、不让出事件循环，计时器无法插入；
真正超时的只有会 await 的 `DataTransferWindow`。因此：

- 抬高 `testTimeout` 只抬高**会 await 的那类超时**天花板，正是父代理在 load 55–80
  观察到的那批失败的成因；
- 6–7× 超订下仍会失败，这是**有意保留**的：真正的卡死必须继续与"只是慢"区分开。

**权衡（明写）：** 真正卡死的用例现在要烧 10s 而不是 5s 才被报出来，
在同一 job 里若有成片用例卡在未 resolve 的 `await` 上，报错会推迟。
接受该代价的前提是 10s 仍然有界且有实测支撑——p99.9 为 2299ms，
10s 远高于任何"仅仅是慢"的情形，而远低于"无限等待"。
若将来出现整片卡死，第一步应查具体用例，而不是继续抬高该值。

守卫见 `scripts/__tests__/run-unit-driver-set.test.ts`：它钉住 `testTimeout: 10_000`
这个值，并要求上述实测数据仍留在 `vitest.config.ts` 里（改值或删依据都会红）。

**守卫的依据分布跨两个文件，不要记混。** 实测归属如下：

| 断言的串 | 被断言的文件 |
|---------|------------|
| `testTimeout` 存在且 `= 10_000` | `vitest.config.ts` |
| `1745ms` / `4392ms` / `11629ms` | `vitest.config.ts`（`// Vitest's 5s default` 与 `testTimeout:` 之间）**和** `ci.yml` |
| `5677 passed` | **只有 `ci.yml`**（不在 `vitest.config.ts` 里） |

删 `vitest.config.ts` 的依据会红，删 `ci.yml` 的 `5677 passed` 同样会红——但这是两条
独立的断言，不是一条断言扫两个文件。

**守卫的边界：防删依据，不防错依据。** 该守卫断言的是"这些实测数字串**仍然存在**"，**不是**它们是否属实。实测确认：把 `4392ms` 改成 `4400ms` 会红（串
不匹配），但改成某个**仍然存在、只是不再真实**的数不会红。所以"守卫全绿"**不等于**
"文档里的数字或说法是对的"。数字的正确性只能靠重测确认。

它对 runner 核数的检查是**关键词启发式（防形不防语义）**，边界必须说清：把
`ci.yml` 注释里那句关于 runner 核数的话整体改写成另一个"仍然成立"的说法，
只要句中同时出现**阿拉伯数字 + `core`/`核`**与**runner**，且该句**没有**否定/免责标记
（not、never、不推断、假定……），就会红；但写成文字（"four cores"）、或把免责标记留在
同一段而不是同一句，都可能漏过。它拦的是最常见的复发形态，不是语义判定。

**用例总数：本文没有独立机器测量，不要假装有。** 此前此处写着"另有独立机器测量，见
§2.1.3"，而**本节没有 §2.1.3**——是一个从未写出来的指针，已删除，改为直说：本文记的
用例数全部是**当时那次运行的转述**，没有第二处机器测量可以交叉验证。

**并且本节内部有一个未对齐的差值，记在这里而不是抹平。** 全文出现两个用例总数：

- `5677 passed`——`7559759b7` 那次全绿运行的套件通过数（555 文件），被守卫钉在 `ci.yml`；
- `5714`——`testTimeout` 预算的测量基数（`ci.yml` 的 frontend job 注释 "Timeout budget, measured not guessed" 原文："all 5714 tests,
  `--reporter=json`"），**不被任何守卫断言**。

两者相差 **37**，且 `5714` 的来源（哪次运行、哪个驱动集）本文没有记录。**在补测之前，
这两个数只能各自当作"某次运行的记录"看，不能相减、不能互相印证。** 重测时应当重新
统计一次套件总数并统一两个数字，而不是挑一个信。

**2026-10-23 套件总数补测（CI 实跑口径）。** main @ `52cf9ca3e`（frontend job，全绿）
三步的实测计数：`test:unit` 步 **755 个测试文件，7,846 passed + 1 skipped，合计 7,847
用例**；`test:unit:drivers` 步 88 文件 / 1,007 用例；`driver-set` 步（收窄后）316 文件 /
3,146 用例。据此裁定：套件总数的**当前基准**为 7,847（归属 `52cf9ca3e`）；`5677` 与
`5714` 维持"各自某次运行的记录"地位，不改写、不相减。上文"重测时统一两个数字"的
处方针对的是 `--reporter=json` 墙钟分布的重跑——那是一次新的机器测量，本补测只是
计数，两者不是一回事；墙钟分布表继续绑定它自己的测量集，不做替换。另注：本 PR 同步
合并了 `resolve-drivers` 的两份重复测试文件（`.mjs` 并入 `.ts`，去重 7 个逐字重复
用例），下一个全绿运行的套件计数将比本记录少 1 个文件与约 7 个用例，属预期变化。

**2026-10 收窄：`all` 步不再整套重跑。** 该步此后只运行「codegen 敏感」的测试文件：
模块图会**执行** `src/extensions/generated.ts` 的用例才可能观察到驱动集，其余文件在
`basic` 与 `all` 两轮跑的是同一份字节码，由 `basic` 步判过。选择在运行期由
`scripts/select-codegen-sensitive-tests.mjs` 完成：以文本方式读取 `vitest.config.ts` 的
`include` / `alias` / `setupFiles`（配置仍是唯一 owner，重排格式会让选择器变红而不是
静默漂移），对**值边**（会执行目标模块的 import 形态）建反向图，从 codegen 目标反向
可达即入选；`import type` 与全标 `type` 的具名导入按 esbuild 语义擦除、不算边，仅类型
位置使用的普通导入因无法静态判定而保守计入——宁可多跑，不可漏跑。退化输入（配置不可
解析、include 空匹配、setupFiles 触达 codegen、选中为空）一律让该步变红，不退化为
全量或空跑；守卫见 `scripts/__tests__/select-codegen-sensitive-tests.test.ts`。

实测（双核 4GB 开发机，`--drivers=all`，收窄生效后）：550 个源文件传递执行 codegen；
755 个收集文件中 316 个敏感（41.9%）。本机受内存所限未能在单轮内完整跑完 316 个
文件（推进至 250/316 时被资源打断，已完成部分 0 失败），完整绿跑由 CI runner 验证。
历史依据不变：`7559759b7` 的 555 文件 / 5677 例 / 140s 全绿记录仍钉在 `ci.yml`，
作为该步晋升硬门禁时的证据。

### 2.1.2 `check-ci-docs-consistency.mjs` 覆盖什么、不覆盖什么

`scripts/check-ci-docs-consistency.mjs`（别名 `pnpm test:ci-docs`）**由 `release.yml`
的 `pre-release-guards` 步调用**——它是发布前门禁，不是每次 PR 的门禁（§2.1.3）。
它只做三件事：

1. **drivers** —— 本文档中提到的驱动 id 是否都存在于 `drivers-registry.json`；
2. **window boundaries** —— `windows.md` 记录的子窗口 kind 是否与 `windowKind.ts` /
   `windowManager.ts` 一致；
3. **toolchain** —— README / CONTRIBUTING / 本文档里的 Node / pnpm 版本是否与 `ci.yml`
   一致、是否提到 Rust stable。

**它不覆盖**：任何 runner 规格声明（核数），以及任何计时/计数声明——`testTimeout`、
`retry`、worker 数、用例总数、各档耗时百分比与 max 值。它只在三个维度上把关，
所以 §2.1.1 那些数字**不由它守**。别因为 CI 里跑了这个脚本，就以为 runner 规格
或计时数据已经有守卫。本轮不改动它的检查范围。

那些数字实际由两个守卫分担，能力边界不同，必须分开看：

- `scripts/__tests__/run-unit-driver-set.test.ts` —— §2.1.1 的
  `testTimeout` / `retry` / 耗时串按**字符串存在性**守，`run-unit-driver-set.mjs`
  的三条腿与「硬门禁标记必须缺席」按结构守；**不校验这些数字是否属实**。

### 2.1.3 per-PR 门禁与发布前门禁的分工

守卫按「什么时候出问题才算晚」分成两处，两处都由 workflow 强制，不是约定：

| 位置 | 守卫 | 判据 |
|------|------|------|
| `ci.yml` 每次 PR | stubs、IDs、layers、driver-protocol、boundaries、i18n key 冲突 | 改错了必须在**合并前**发现——每个 PR 都要付这个钱 |
| `release.yml` `pre-release-guards` | version、ci-docs、structure-editor guardrails、**i18n 翻译完整性（硬阻断）** | 前三条只在**发布这一刻**才有意义：版本号只在 `bump-version` 时变，CI 矩阵只随发布计划变，caps 注册表发布前查一次即可。第四条是**发布政策**而非代码卫生——发布出去的版本不允许只有英文 |

`pre-release-guards` 已进 `build` 的 `needs`，因此发布前这四条不过，发布构建就不启动——
这是 workflow 的硬依赖，不是文档承诺。四条都只用 Node 内置模块读 git 跟踪文件，
不碰 gitignored codegen，所以该 job 不需要 `resolve-drivers`、`pnpm install` 或任何 secret。

**i18n-sync-check 已是发布硬阻断。** 裁定：**发布出去的版本不能只有英文翻译**。它原先挂在
`ci.yml` 里却是 `continue-on-error: true`，常年是黄的、没人看，债累积到 **8 个宿主语言缺
832 个 key**——这正是"看得见但没人当回事"的门禁。现在删掉 `continue-on-error`，该步与
其余三条同级：翻译不全，`build` 根本不启动。

**已知后果，且是预期的**：版本已 bump 而翻译未补齐时，发布直接失败。这不是回归，
是把一笔被掩盖的债显性化。`bump-version` skill 触发重建时必须一并跑
`node scripts/i18n-sync-check.mjs`，缺失的 key 用 i18n-sync skill 补齐（只补翻译，不改 `en.ts`）。

### 2.1.4 类型门禁跑哪个编译器

`pnpm typecheck` 跑**一个** TypeScript 7 实现：

| 引擎 | 包 | 上游 | 角色 |
|------|----|------|------|
| `tsrs` | `tsrs@0.18.0-ts7.1.0-dev.20260929`（精确锁定） | TypeScript **7.1 开发线** | **门禁本体**。比 GA 领先一个版本，提前暴露下一版的新诊断；比 `tsc` 快约 2 倍、峰值内存少约 3.6 倍。 |
| `tsc` | `typescript@7.0.2`（精确锁定） | TypeScript 7 GA（Go 移植） | **逃生口 + 构建内的那道**。它不再是**类型检查环节**的一部分（本地与发布都用 `tsrs`），但 `pnpm build` 里仍有 `tsc --noEmit` ⇒ GA 独有的诊断仍会在发布构建里被抓住。`pnpm typecheck:tsc` 可在本地按需单跑它。 |

**一个编译器，全流程一致。** 本地门禁（`pnpm typecheck`）与发布门禁（`union-typecheck`）
**都只跑 `tsrs`**（用户裁定）：这样「本地绿」与「发布绿」指的是同一件事，不需要在两处维护
两套语义。`tsrs` 快约 2 倍、且唯一会报出**下一个 TS 版本**诊断的那个。

**代价（明写，不藏）**：`tsrs` 领先 GA 一个版本 ⇒ **它只会比 GA 多报，不会少报；反过来不成立**
——GA 独有的诊断 `tsrs` 可能没有。这道缺口由 `pnpm build` 里的 `tsc --noEmit` 兜住（发布构建
仍跑它），本地需要权威结论时跑 `pnpm typecheck:tsc`。

**实测（本仓库，1845 个 TS 文件，三个 program）**：

| 引擎 | 三个 program 墙钟 | 峰值 RSS |
|------|-------------------|----------|
| `tsc` 5.9.3（迁移前） | 24.8–27.0 s | 1.31 GB |
| `tsc` 7.0.2 | 2.4–2.6 s | 1.25 GB |
| `tsrs` 0.18.0 | 1.0–1.3 s | 0.35 GB |

5.9.3 → 7.0.2 的 10 倍来自编译器重写本身，不是本次改动。单引擎门禁现在取的是 `tsrs`
那一档：**约 1 秒、约 0.35 GB**。

**两个引擎的输出实测逐字节相同。** `tsc 7.0.2` 与 `tsrs 0.18.0` 在三个门禁 program 上的
诊断输出**逐字节相同**（不是"错误数相同"，是 diff 为空）。这是把 `tsrs` 单独当门禁的
前提：两者在当时的源码上分歧为零，所以换引擎不会让门禁变松也不会变紧。

**Windows 上 `tsrs` 跑不了，门禁回退到 `tsc` 并明确告知。** `tsrs@0.18.0` 没有发布
win32 二进制（上游列为"下次发布配置"）。`scripts/typecheck.mjs` 用 `--version` 探测
启动器能否真正运行——这是唯一诚实的探测方式，因为它走的是真实运行时的同一条解析路径。
探测失败就回退到 `tsc`，并打印 `[typecheck] tsrs is unavailable here (...) — falling
back to tsc`，**绝不静默**。发版矩阵要构建并签名 Windows 产物，一个在出货平台上跑不起来
的类型门禁是更差的门禁。该探测同时是自愈的：上游一旦发布 Windows 版，下次
`pnpm install` 装上即自动启用，无需改代码。

**版本必须精确锁定，不能用 `^`。** `tsrs` 的版本号里编码了上游 TypeScript 的 commit
（`0.18.0-ts7.1.0-dev.20260929` → `7.1.0-dev.20260929`）。`^7.0.2` 会在 7.1.0 转
stable 时自动升上去，门禁语义在无人察觉时漂移；`^0.18.0-...` 同理会让 0.19.x 带着另一个
TS 快照进来。两处都用精确版本。

**迁移到 TS 7 需要的源码改动只有两处**（外加五个包级 tsconfig 的配置修复），都已入库：

1. **显式 `types`**。TS 7 不再自动发现 `node_modules/@types`。迁移前 `tsc 5.9.3` 静默
   吃进了全部四个包（dompurify / node / react / react-dom）；`tsconfig.json` 与五个
   `packages/<id>/tsconfig.json` 现在显式列出**同一组**，两个编译器下语义一致。
   不写这一处的实测代价：TS 7 报 **109 个错误**，全部是 `@types/node` 没被加载的形状。
   维护规则：新增 `@types/*` 依赖要同步加进这些列表；失败方向是安全的——漏了会**多**错误，
   不会少错误。

   五个 `packages/<id>/tsconfig.json` 另外做了两件事，因为它们各自是 **standalone
   program**，看不到根配置的上下文：

   - **`extends: "../../tsconfig.json"`**。根配置的 `paths` 定义了 `@datazen/*` 别名，
     standalone program 拿不到，于是每一个 `@datazen/*` import 都报 TS2307。实测：
     driver-sdk 的 318 个错误里 84 个是 "Cannot find module '@datazen/ui'"、56 个是
     "'@datazen/driver-sdk'"、23 个是 "'@datazen/backend-client'"——全是别名解析失败，
     与代码质量无关。加上 `extends` 后这五个 program 全部归零。
   - **补两份环境类型**。`vite/client` 提供 `import.meta.env` 与 `*.svg` / `*.svg?url`
     模块声明（根 program 从 `src/vite-env.d.ts` 拿到同一个契约，但那个文件在 `src/`
     里面，standalone program 看不见）；`@testing-library/jest-dom/vitest` 提供 DOM
     matcher（根 program 从 `src/test/setup.ts` 拿到同一份增强）。实测：ui 的 147 个
     错误里，139 个是 matcher 缺失（`toHaveAttribute` 等），8 个是 Vite 环境类型缺失。

   结果：五个 program 在 TS 5.9.3 / TS 7.0.2 / tsrs 三个编译器下**都是 0 错误**，
   修复前分别是 147 / 318 / 245 / 120 / 0。

   `e2e/tsconfig.json` 的 120 个错误**不是**同一类问题，没有一并修：其中 30 个是同样的
   `@datazen/*` 别名解析失败（`extends` 可修），另外 90 个是 E2E spec 里的真实代码缺陷
   （`Promise<number>` 与 `number` 比较、`await` 缺失一类），需要改 spec 代码并跑 E2E
   才能验证，不属于配置修复的范围。
2. **`scripts/with-driver-inject.mjs` 的 JSDoc 拆 `@overload`**。TS 7 把 weak-type 检查
   （TS2559）扩展到了 union 目标，原先 `{...} | (() => boolean)` 的联合参数被拒。这是全仓
   唯一的 TS 7 语义报错，改完三个编译器都干净，15 个运行时测试仍全绿。

### 2.2 rust job（Rust 段，浅克隆）

| 步骤 | 命令 / 动作 | 说明 |
|------|-------------|------|
| 依赖 | `pnpm install --frozen-lockfile` | 注入脚本链 import `fflate`，仍需 node_modules |
| 驱动解析 | `resolve-drivers.mjs --drivers=basic` | 写入 `.driver-features.json`、codegen |
| 格式门禁 | `cargo fmt --all -- --check` | **硬门禁**；必须排在「驱动解析」**之后**——fmt 要加载 Cargo workspace，而 `lib.rs` 的 `mod driver_init` 指向 gitignored codegen，只由 resolve-drivers 生成，排前面会 `failed to resolve mod 'driver_init'` 直接 EXIT=1。rustfmt 由 `dtolnay/rust-toolchain` 的 `components: rustfmt` 显式安装（该 action 固定 `--profile minimal`，而 rustfmt 属 `default` profile） |
| Rust | 见下表 | Rust **stable** |
| 清理 | `driver-file-stash.mjs restore` | 恢复被 inject 的 tracked 文件（`if: always()`） |
| 架构门禁 | `pnpm test:platform-arch` | 全部平台边界规则检查，**阻塞** |
| 内核 crate 单测 | `pnpm test:platform-crates` | 集合由脚本按 LAYERS 发现 |
| 门禁自证 | `pnpm test:platform-arch:mutations` | 变异证明，**job 内最后一步** |

Rust 测试顺序（与 `ci.yml` 一致）：

```bash
# driver-api：无 target 选择器 = lib 单测 + tests/ 下 4 个集成文件（26 个测试，
# 含 driver_env_injection_discipline 守卫型测试）+ doc test，与
# publish-driver-api.yml 同形。2026-10-27 由 `--lib` 扩充（当时的
# 「没有 tests/ 目录」注释已过时；那 4 个文件此前只在发布流水线执行）。
# 四个 basic path 驱动的 lib 单测不在这里跑：下一条 `--tests` 按 cargo 目标选择
# 语义同样会编译并运行 lib 的单测 target（去重无信号损失，详见 ci.yml 注释）。
cargo test -p datazen-driver-api
# 驱动集成测试 + 四驱动的 lib 单测：`--tests` 覆盖 test=true 的全部 target
# （含 lib，不含 doc test）。live 契约层默认严格（验不到即失败）。CI 全文件
# 没有数据库容器，也没有任何 TEST_PG_* / TEST_MYSQL_*，所以显式声明豁免——
# 去掉前缀就会红（实测 42 个）。要在 CI 里真验 live：先 provision 数据库并
# 注入 TEST_PG_* / TEST_MYSQL_*，再删掉这个前缀；DATAZEN_CONTRACT_REQUIRE_LIVE=1
# 压过豁免。（实测：不加前缀 EXIT=101，42 个 live 层测试翻转；命令不带
# --no-fail-fast，cargo 在第一个失败的 binary 就停。）
DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis --tests
# datazen 需要 features 选择注入的驱动；2026-10-27 起追加 datazen/test-harness
# （限定语法，只开 datazen 的同名 feature）：宿主腿与 platform-crates 的无选择器
# 腿由此编译同一 feature 集的 datazen（全 feature 减 default/webdriver），lib 从
# 每轮两份编译共享为一份。等式由 run-platform-crate-tests 的测试对两份真实
# manifest 钉死，任何一侧单独增删 feature 会先红测试而不是悄悄退回双份编译。
FEATURES=$(node -e "console.log(JSON.parse(require('fs').readFileSync('.driver-features.json','utf8')).features.join(','))")
cargo test -p datazen_desktop --lib --features "$FEATURES,datazen/test-harness"
# 恢复被 inject 的 tracked 文件，确保后续步骤工作在干净状态
node scripts/driver-file-stash.mjs restore
# 此处曾有 `cargo test -p datazen-ai-api --lib`，后又改为
# `cargo test -p datazen --lib --features settings,ai,connection,named_views`；
# 2026-10-27 删除：该组合的 feature 闭包 = platform-crate runner `ai` 腿 ∪
# `named_views` 腿，modules/src 无 cfg(all(feature)) 组合门控，全部 ~1,317 个
# 测试函数都在那两条腿里再跑一遍——零覆盖增益，却多付一次完整 datazen lib
# 编译。恢复前先读 ci.yml 同位置的墓碑注释。
# 此处还曾有「CM-60 latency benchmark (release)」步（cargo run --release …
# --bin cm60-bench -- --vcpus 4 …）及其 artifact 上传；2026-10-27 删除
# （维护者决策：CI 不做性能测试）。它是 rust job 唯一按 release profile
# （fat LTO）编译 datazen 全部 dev-dependencies 的腿；cm60-bench 的 53 个
# 单元测试是合成算术断言，已由 platform-crates 无选择器腿在 debug 下覆盖。
# 判据口径见 connection-management.md CM-60，需要实测时本地复跑同一命令。
pnpm test:platform-arch
pnpm test:platform-crates
pnpm test:platform-arch:mutations
```

### 2.3 平台 crate 架构门禁（全部平台边界规则）

规则正文见 [shared-boundaries-and-ports.md](../architecture/platform/shared-boundaries-and-ports.md) §2.4，
执行器是 `scripts/check-platform-crate-boundaries.mjs`。

**为什么落在 rust job 而不是 frontend 的严格守卫步**：门禁要读 `cargo metadata` 的
feature-resolved 图，而 `dtolnay/rust-toolchain@stable` 只装在 rust job；frontend job
（`ci.yml` §2.1）没有 Rust 工具链。并进那一步只会得到一个必然失败的步骤。

**为什么这里没有任何 crate 名单**：`scripts/lib/cargoWorkspace.mjs` 的 `LAYERS` 表把
`manifest_path` 映射到层，规则的主体由 §2.4 的路径推出。因此 `modules`、
`modules/src/platform_api`、`server/` 一旦被加进 workspace `members`，立刻自动入网——
**新增内核 crate 不需要改 `ci.yml`，也不需要改 `package.json`**。列不出来的 member 是
**error** 不是 pass：门禁无法分类的成员，就是没有任何规则覆盖的成员。

`pnpm test:platform-crates`（`scripts/run-platform-crate-tests.mjs`）用同一张 `LAYERS` 表
发现 `TESTED_LAYERS` 各层里真实存在的 crate，并跑**不带目标选择器**的
`cargo test --no-default-features --features <显式清单> -p …`（lib 单测、bin 单测、
全部 `tests/` 集成 target 与 doc test 一次覆盖）。2026-10-27 两处变化：

1. **无选择器腿由 `--all-features` 改为与宿主腿对齐的显式清单**（全 feature 减
   `default` 与 `webdriver`）：宿主腿同日追加了 `datazen/test-harness`，两腿由此
   编译同一份 datazen——~20.9 万行 lib 从每轮两份编译共享为一份。等式（宿主闭包
   ∪ {test-harness} = 全 feature 减 default/webdriver）由 runner 的测试对两份真实
   manifest 钉死；`webdriver` 排除的前提（空 cfg 开关、E2E 专用缝，门控在 cfg(test)
   下恒开）由 runner 运行时校验，feature 变非空即红。
2. **`EXTRA_TARGETS` 的 CM-60 release 条目删除**（维护者决策：CI 不做性能测试）：
   cm60-bench 的 53 个单测是合成算术断言，已由无选择器腿在 debug 下覆盖；
   被删的是 release 重跑与全 job 唯一的 release-profile dev-deps 编译（fat LTO）。

更早删除的那次 `--lib` 是无选择器调用的严格子集（实测让 2,537 个测试函数每轮
执行两遍）。它补上的是计划 :269 的空缺：`datazen` 当时带着 130 个通过的 Rust
单测，而 CI 里没有任何一步执行它们。`--require-layers=<ids>` 可在某个阶段把
「层存在但没被测到」从静默变成报错。

**变异自证**：`scripts/platform-arch-selfcheck.mjs` 临时改写 `Cargo.toml` / 新增探针 crate，
逐条运行门禁并断言退出码非零 + 输出点名了预期规则与 crate，再逐条还原；末尾断言
`git status --porcelain` 为空。它放在 job 的**最后一步**，因为它会动工作树——放在这里，
被强杀也不会污染任何后续步骤。10 个变异约 5s；它失败说明门禁本身失去了鉴别力，比门禁变红更早报警。

**判定范围**：门禁的 **Cargo 闭包判定**只读 crate 依赖闭包；**声明边**（manifest 里写了什么）
此前只产 advisory，**现已按规则的主体范围参与阻断判定**（F-01 的驱动侧），见 §2.3.1。
`@tauri-apps/api` / `fetch(` / `XMLHttpRequest` 这类前端字面量由
`shared-boundaries-and-ports.md` §7 记的源码扫描（F-07）负责，不属于 Cargo 闭包判定，但由同一门禁执行；
`packages/backend-client` 的 pnpm 接线（tsconfig paths / include / vite alias）见
`shared-boundaries-and-ports.md` §2.5，同样不在本门禁内。

### 2.3.1 F-01 的 `tauri*` 已变成阻断门禁

§2.4 的 F-01 禁止列写的是 `tauri*`，**门禁侧现在看得见了**：`F-01` 的 `forbiddenCrates`
是 `['tauri']`（`scripts/check-platform-crate-boundaries.mjs` 的 F-01 规则），闭包侧与声明边侧
任一出现 `tauri*` 都是 **violation**，退出码非 0。

> 这一段此前记的是"怎么把它变成阻断门禁"，不是"已经变了"。**三条前置条件现已全部完成**，
> 下面的推导保留下来，因为"为什么当时不能武装"正是今天能武装的依据。

当前实测产出 **1 条** advisory（退出码 0）——它来自**宿主**，不在 F-01 的主体范围内：

```
ADVISORY F-01?  datazen (src-tauri/Cargo.toml) declares tauri-plugin-webdriver in
                [dependencies], which the resolved graph does not contain. Not blocking:
                F-01's subjects are `packages/drivers/*`, and `datazen` is not one of them.
```

`datazen-driver-redis` 已不在任何 tauri advisory 里：它的 `[dependencies] tauri`、
`[build-dependencies] tauri-plugin`、`build.rs` 与插件 ACL 目录已随「Redis Pub/Sub 事件出口
改走形态中立的 `inventory` 注册」一并删除（契约见[驱动层 §1](../architecture/backend/drivers.md)）。
此前实测的 3 条（redis 闭包侧 `tauri-plugin`、`tauri-utils`，redis 声明侧 `tauri`，
`datazen` 声明侧 `tauri-plugin-webdriver`）因此降为 1 条。

**武装的前置条件共三条，归属分属两个写范围：**

| # | 前置条件 | 位置 | 归属 | 状态 |
|---|---------|------|------|------|
| 1 | redis 去掉 `tauri` 声明（`packages/drivers/redis/Cargo.toml` 的 `[dependencies] tauri`）与 `[build-dependencies] tauri-plugin` | `packages/drivers/**` | 驱动轨 | ✅ 已完成 |
| 2 | manifest 读取失败路径由 advisory 改为 error（§2.3.2） | `scripts/**` | 门禁脚本轨 | ✅ 已完成 |
| 3 | `F-01` 的 `forbiddenCrates` 由 `[]` 改为 `['tauri']` | `scripts/**` | 门禁脚本轨 | ✅ 已完成 |

**"任何阻断式的 tauri 检查今天都是红的"这句话已不成立。**
此前之所以写死"没有不动驱动 manifest 就能先武装的中间态"，是因为 redis 不只**声明**了
tauri，它**真的解析到** `tauri-plugin` 与 `tauri-utils`——无论把检查挂在声明边还是解析边
上，阻断式检查都会当场把 CI 转红。redis 的声明边与解析闭包都不含 tauri 家族之后，
武装本身是绿的；剩下的两条前置条件都只动 `scripts/**`，与驱动侧的前置条件**同一次落地**。

**武装后不会误伤宿主**：F-01 的 `subjects` 是 `['driver']`（`packages/drivers/*`），**闭包侧与
声明边侧都按这个主体范围判定**。`src-tauri` 那 9 条 tauri 声明（宿主本来就该依赖 tauri）
不在该规则主体内：闭包侧本来就不查宿主，声明边侧把宿主那条报成 advisory 而**不是** violation。
若按"工作区里所有成员"判，宿主自己声明的 `tauri-plugin-webdriver`（Tauri 自己的 webdriver
插件，宿主本来就该有）会把 CI 变红——这就是主体范围必须两侧一致的原因。

**豁免必须显式**：规则表的 `exemptions` 列表（成员 + 家族 + 理由）是唯一的放行通道，空即无豁免；
过期条目（成员不是本规则主体、或家族不是本规则禁的）报 **error**。**没有**"降级成 advisory
就放过"这种口子——真要豁免一个驱动，只能把它和理由一起写进规则表，并让它每次跑都打印出来。

**自证**：`scripts/platform-arch-selfcheck.mjs` 增加了两条 F-01 变异——M9（驱动**依赖**
`tauri*`，打闭包侧）与 M10（驱动**声明**但解析图里没有的 `tauri*`，打声明边侧）。两条必须同时
存在：F-01 有两侧，只有 M9 时声明边那一侧可以悄悄坏掉而自证仍然全绿。

### 2.3.2 manifest 读不出来现在是 error

`scripts/check-platform-crate-boundaries.mjs` 的声明边扫描（逐 member 读 manifest）：
`readFileSync(member.manifestPath)`，抛错时压一条 `read?` **error** 然后 `continue`。
退出码数 `violations + errors`，所以读不到的成员会把闸门打成红的——这曾是整套门禁里
唯一一处非 fail-closed 的不对称，现在没有了。

**这条翻转与 F-01 武装是耦合的，所以必须同一次落地。** 理由三条：

1. **它曾经大声 fail-open。** 翻转前打印的是 `read? <crate> (<dir>/Cargo.toml) could not be read for
   declared dependencies (<err.code>)` advisory，然后 `continue`——点名了成员和错误码，但不影响退出码。
2. **触发面窄。** `member.manifestPath` 来自 `cargo metadata` 刚成功读过的同一批路径，
   同进程、同用户、微秒级。要它失败需要 TOCTOU 竞态，不是配置能造出来的。
3. **损失原本有界，今天不再有界。** 翻转前解析边那一路读的是已经在内存里的 metadata，全程不碰文件，
   该成员照样被覆盖，丢的只是声明边那一路——而那时声明边还不阻断任何东西。**F-01 武装之后，
   同一个口子会导致漏掉一条真 violation**：这条路径是"声明扫描整体被跳过"的唯一入口。

所以上表前置条件 2 不是洁癖，是武装的组成部分——**两者必须同一次落地**，不能只武装第 3 条。

### 2.3.3 声明边扫描器的已知边界

`scripts/lib/cargoWorkspace.mjs` 的 `declaredDependencies` 是手写行扫描器，不是 TOML
解析器。理论盲点有两个，实测（22 个 manifest / 254 条依赖声明）：

| 盲点 | 实测命中 | 当前能否藏住 tauri 声明 |
|------|---------|----------------------|
| 多行 inline table（`tauri = {` 换行才闭合） | **0** | 否 |
| `workspace = true` 继承 | **4**，全在非 tauri crate | 否 |
| 根 `[workspace.dependencies]` 含 tauri | **0**（只有 3 条内部 path crate） | 否 |

结论：**两个盲点在当前树上都是空的**，且各自还有一层独立理由兜住：

- 多行 inline table 计数为 0，不存在这种写法；
- `workspace = true` 影响的是版本/feature 的**归因**，而 `D∖R` 判据只用 crate 的**名**，
  名是行扫描器一定拿得到的。所以即使某个 crate 写成 `tauri = { workspace = true }`，
  这条 dormant 边照样会被报出来。

**什么时候会失效**：有人往根 `[workspace.dependencies]` 加 tauri 并在成员里写
`tauri = { workspace = true }`，或把 tauri 写成多行 inline table。两者都会让声明扫描
静默漏掉一条 dormant 边。

**处置：不排期修，改为记录触发条件。** 理由是修它需要引入 TOML 解析依赖，而门禁当前是
零依赖的；为一个实测命中数为 0 的盲点换掉零依赖不划算。等触发条件真的出现时再改。

> **武装后这份清单的分量变了**：F-01 武装之前，声明扫描漏掉一条边只少一条 advisory；武装之后，
> 同一个盲点会漏掉一条**真 violation**（驱动侧）。两个盲点今天仍然都是空的，且各有一层独立理由
> 兜住，所以武装当天不引入新的漏报面；但触发条件一旦出现，处理优先级从"记录"升为"修"——
> 那时漏掉的是闸门的一半。

## 3. 驱动预设与 SKU

| 预设 / SKU | Registry ids | PR CI | Release job |
|------------|--------------|-------|-------------|
| `basic`（默认） | postgres, mysql, sqlite, redis | ✅ | Basic 变体 |
| `all` | 全部 **path** 条目（不含 git 驱动） | ❌ | All 变体（`*-all` 后缀） |
| Akulaku 显式列表 | postgres,mysql,sqlite,redis,mongodb,kiwi,superset | ❌ | Akulaku 变体（`*-akulaku`）；`needs_git: true` |
| 自定义逗号列表 | 任意 registry id 组合 | ❌ | 仅本地 / 定制发版 |

详见 [optional-drivers.md](./optional-drivers.md)、[ci-private-drivers.md](./ci-private-drivers.md)。

## 4. 本地 PR 基线（与 CI 对齐）

推荐直接 `pnpm ci:local`（等价于下面全部步骤）。贡献者手工分步跑时至少：

```bash
node scripts/generate-builtin-locales.mjs
pnpm typecheck
pnpm test:unit
pnpm test:unit:drivers
node scripts/resolve-drivers.mjs --drivers=basic
cargo test -p datazen-driver-api
DATAZEN_CONTRACT_ALLOW_UNVERIFIED=1 cargo test -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis --tests
FEATURES=$(node -e "console.log(JSON.parse(require('fs').readFileSync('.driver-features.json','utf8')).features.join(','))")
cargo test -p datazen_desktop --lib --features "$FEATURES"
node scripts/driver-file-stash.mjs restore
pnpm test:platform-arch
pnpm test:platform-crates
```

与 `ci.yml` 的 workflow 级 env 一致，这些步骤默认应带 `DATAZEN_KEYRING=file`（`pnpm ci:local` 已自动导出）。cargo 单测在 `cfg(test)` 下强制走文件后端，不访问系统钥匙串——macOS 上钥匙串搜索列表异常时，Keychain Services 调用会弹「找不到用于储存 app-encryption-key 的钥匙串 login」模态框并**阻塞**整条测试。只有显式设 `DATAZEN_TEST_KEYRING=1` 才启用钥匙串用例，见 [store.md — 单元测试与钥匙串弹窗](../architecture/backend/store.md#单元测试与钥匙串弹窗)。

若改动 `site/`：`node scripts/check-site-seo.mjs`。

若改动可选 path 驱动的 **Rust**：追加 `cargo test -p datazen-driver-<id> --lib`（该 crate 的 UI 单测已由 PR CI 的 `pnpm test:unit:drivers` 覆盖，无需再手工跑）。

若改动 `Cargo.toml`、workspace `members` 或任何内核 crate 的依赖：追加 `pnpm test:platform-arch`。
`pnpm test:platform-arch:mutations` 每次不必跑（它在 PR CI 里），但**改门禁本身**
（`check-platform-crate-boundaries.mjs` / `lib/cargoWorkspace.mjs`）时必须本地跑通——
否则你无法区分「新规则抓到了真问题」和「新规则坏了」。

若改动 Host UI 交互路径：同 PR 更新 E2E（见 [e2e-testing.md](./e2e-testing.md)）；全量 E2E 耗时长，**不要求**与 PR CI 同跑，但须在 PR test plan 说明。

## 5. E2E 与契约矩阵（CI 外）

| 命令 | 驱动 | 用途 | CI |
|------|------|------|-----|
| `pnpm e2e:minimal` | basic | 快速 Host E2E | ❌ |
| `pnpm e2e` | 当前 inject 选型 | 全量 Host E2E | ❌ |
| `pnpm e2e:contract:matrix` | PG + MySQL + SQLite | Host UI 契约 × 驱动 | ❌ |
| `pnpm test:unit:e2e-contract` | — | contract fixtures 单测 | ❌ |
| `packages/drivers/<id>/e2e/` | 单驱动 | 方言 / 专属 UI | ❌ |

契约 journey 列表见 [e2e-coverage.md](./e2e-coverage.md) §「Host Connection Contract × Driver」。

### 5.1 CI 完全不跑 E2E（不要把绿灯当覆盖）

`.github/workflows/` 下只有 `ci.yml` / `pages.yml` / `publish-driver-api.yml` / `release.yml` 四个文件，
**没有任何一个引用 wdio、webdriver 或 `pnpm e2e`**：

```bash
grep -rliE "wdio|webdriver|pnpm e2e" .github/   # 无输出
grep -rnE "e2e:ci:" .github/workflows/            # 无输出
```

`ci.yml` 实际执行的只有：`pnpm typecheck`、strict guards（stubs / IDs / layers /
driver-protocol / boundaries / i18n keys）、`pnpm test:unit`、
`pnpm test:unit:drivers`、Rust `fmt --check` 与各 crate 单测、平台架构门禁。

**结论：PR 绿灯与 E2E 覆盖无关。** 上表所有行的 `CI` 列都是 ❌，不是「暂时没接」，
而是这套 WDIO 从来就没有进过任何 workflow。**这是取舍，不是缺口**：CI 只要一台
普通 runner，E2E 需要带 WebDriver 与真实数据库服务的 runner（还要窗口系统），
两者成本不同量级，所以 E2E 留在 CI 之外跑，不在本文档承诺范围内。
任何「E2E 全绿」的结论只对本地那次运行有效，不得当作合并门禁。

**2026-10-27 起这是长期政策，不再是待议取舍**：维护者决策——CI 不做性能测试、
不需要 WebDriver、不跑 E2E（同日据此删除了 rust job 的 CM-60 基准步与
platform-crates 的 release 条目，并把无选择器腿改为排除 `webdriver` 的对齐清单）。
任何把 WDIO 或基准接回 workflow 的提议默认与本政策冲突；要推翻它，需在 PR 里
说明 runner 成本与窗口系统 / 性能噪声的获取与控制方式，并更新本节。

### 5.2 `pnpm e2e` 实际跑什么、跳过什么

`pnpm e2e` → `node e2e/run.mjs`（不带 `--suite`）→ WDIO 使用 `e2e/wdio.conf.ts` 的默认
`specs: ['./specs/**/*.ts']`。因此**默认全量运行 = `e2e/specs/**` 下的全部 spec**：

| 项 | 数值 | 依据 |
|----|------|------|
| `e2e/specs/**` spec 文件总数 | 153 | `find e2e/specs -name '*.ts'` |
| 默认排除（截图/录屏类） | 9 | `wdio.conf.ts` 根级 `exclude`（`E2E_CAPTURE` 时放开为空数组）：7 个 `*screenshot*`（含 `zz-screenshots`）+ `demo-recording` + `zz-diag` |
| **默认实际运行** | **144** | 153 − 9 |

跳过条件分三层：

1. **截图类**：`--capture` 未开启时统一排除（`pnpm e2e:shots` 才跑）。
2. **数据库类**：`e2e/run.mjs` 的 `runEnvSetup()` 启动前跑 `e2e/setup-e2e-env.sh`，**失败只告警不中断**
   （"DB specs may fail; UI-only specs can still run"）。54 个 spec 读
   `E2E_PG_*` / `E2E_MYSQL_*`；预置库缺失时这些 spec 失败，UI 类仍可跑完。
3. **脚本级**：`E2E_SKIP_WORKER_DATABASE=1`（`e2e:schema-tree-objects`）由 `wdio.conf.ts` 读取并
   跳过依赖 worker 数据库的 spec；其余开关（如 `E2E_SKIP_SQLSERVER=1`）由各 spec 自查。

### 5.3 预置数据库链路（已核实）

`pnpm e2e` 与 `pnpm e2e:data-transfer` 等脚本的预置库都经同一条链路：

```text
e2e/run.mjs:50-54          → bash e2e/setup-e2e-env.sh
  setup-e2e-env.sh:113     → bash e2e/setup-sync-dbs.sh
  setup-e2e-env.sh:116     → bash e2e/setup-demo-data.sh（失败仅告警）
```

`e2e/setup-sync-dbs.sh` 实际创建的库（已逐行核对，非推断）：

| 数据库 | 脚本位置 | 用途 |
|--------|----------|------|
| `datazen_sync_src` / `datazen_sync_tgt` | `setup-sync-dbs.sh`「PostgreSQL setup」段：`for db in datazen_sync_src datazen_sync_tgt` 建库循环（其 `CREATE DATABASE $db` 分支），同段第二个同名循环负责重置 fixture 表并授权 | Data Sync 同族双库 |
| `datazen_sync_mysql_src` / `datazen_sync_mysql_tgt` | `setup-sync-dbs.sh`「MySQL setup」段三条 `CREATE DATABASE IF NOT EXISTS` 中的前两条 | Data Sync 跨方言 |
| `$MYSQL_DB`（契约 fixture 库） | `setup-sync-dbs.sh`「MySQL setup」段第三条 `CREATE DATABASE IF NOT EXISTS`；表结构由 `setup-e2e-env.sh` 的 `e2e_contract_conn` 建表语句创建 | 契约矩阵 + 截图 |

`datazen_sync_tgt` 另在 `setup-sync-dbs.sh`「PostgreSQL setup」段末尾的只读授权块（`GRANT CONNECT ON DATABASE datazen_sync_tgt TO ${PG_READONLY}` 起）授只读给 `$PG_READONLY`，用于权限用例。

### 5.4 契约矩阵（`pnpm e2e:contract:matrix`）覆盖什么

`--suite contract` → `e2e/specs/host-contract-matrix.ts` → 对 `DEFAULT_MATRIX_DRIVERS`
（**postgres / mysql / sqlite**，`fixtures.ts` 的 `DEFAULT_MATRIX_DRIVERS`）逐个驱动套用
`planJourneys()`，journey 集合取 `ALL_CONTRACT_JOURNEYS`（`journeys/plan.ts`，**10 条全跑**，
不是 core 3 条；core 3 条 `HC-DATA`/`HC-FILTER`/`HC-QUERY` 只是 F2 历史子集）。

| Journey | 所需能力 | PG | MySQL | SQLite |
|---------|----------|----|-------|--------|
| HC-CONN / HC-QUERY | `hasSqlEditor` | ✅ | ✅ | ✅ |
| HC-DATA / HC-FILTER | `hasTableData` | ✅ | ✅ | ✅ |
| HC-EDIT | `hasInlineEdit` + `hasTableData` | ✅ | ✅ | ✅ |
| HC-STRUCT | `hasStructure` | ✅ | ✅ | ✅ |
| HC-INDEX | `hasIndexes` | ✅ | ✅ | ✅ |
| HC-EXPORT | `hasExport` + `hasTableData` | ✅ | ✅ | ✅ |
| HC-OBJ | `hasObjects` | ✅ | ✅ | ❌（`fixtures.ts` 的 `SQLITE_CAPABILITIES` 置 false） |
| HC-EXPLAIN | `hasExplain` + `hasSqlEditor` | ✅ | ✅ | ✅ |

即 **3 × 10 = 30 格，29 跑 / 1 跳过**（SQLite 的 HC-OBJ）。

**矩阵只含这 3 个驱动。** SQL Server、ClickHouse、DuckDB、MongoDB、Redis 均不在
`DRIVER_FIXTURES` 中，它们的「Host 通用 UI 契约」目前没有任何自动化验证。

### 5.5 驱动 E2E 接线现状

`packages/drivers/<id>/e2e/` 按 `AGENTS.md` 本就「显式脚本，不进默认 `pnpm e2e`」。
但「显式」不等于「可跑」——实测接线情况：

**已接入 suite（可用 `pnpm e2e:<group>` 跑）：**

| 驱动 | 接线位置 | 覆盖 spec |
|------|----------|-----------|
| redis | `wdio.conf.ts` 的 `redis` suite（glob `../packages/drivers/redis/e2e/*.ts`） | 2 / 2 |
| mysql + postgres | `wdio.conf.ts` 的 `schema-tree-objects` suite | 各 1 / 7、1 / 6 |

**完全没有任何接线**（未进 `wdio.conf.ts` 任何 suite、CI 也不引用，需手动
`pnpm e2e:skip-build -- --spec <path>`）：

| 驱动 | spec 数 | 备注 |
|------|--------|------|
| **sqlserver** | 8 | 全部需 `E2E_SQLSERVER_*` + `E2E_SKIP_SQLSERVER!=1`；见该目录 README 的「These specs never run in CI」 |
| mysql | 5 | `sync-plan` / `sync-wave-one` / `wave1-isolated` / `ack-loss` / `rollback-continue` |
| postgres | 6 | 同上 + `schema-primary-key-nullability` |
| clickhouse | 1 | `clickhouse-smoke.ts` |
| duckdb | 1 | `duckdb-smoke.ts` |
| mongodb | 1 | `mongodb-smoke.ts` |

**另有 4 个孤儿 WDIO 配置**，仓库内 0 处引用（`grep -rn` 无输出），只能手工
`--config e2e/<name>.conf.ts` 跑，其对应的 spec 也不在任何 suite 内：

```text
e2e/wdio.migration-transfer-ack-loss.conf.ts
e2e/wdio.migration-transfer-rollback-continue.conf.ts
e2e/wdio.migration-transfer-structure-mapping-r3-tester.conf.ts
e2e/wdio.sync-wave-one.conf.ts
```

**读法**：驱动 E2E 目前是一套**纯手工回归集**。它有价值（方言、ACL、DDL 渲染只能这么测），
但不具备任何自动化保护——改动 SQL Server 驱动后 CI 全绿是**正常**的，不代表 SQL Server 没坏。

## 6. Release 流水线（摘要）

`release.yml` 在 tag `v*` 或手动 dispatch 时构建安装包；**不**替代 PR CI 的单测矩阵。

- **Basic**：四平台 × basic 驱动（与 PR CI 同套核心驱动，但做完整 `tauri build`）。
- **All**：四平台 × 全部 path 驱动（**不进 PR CI** 的集成验证点）。
- **Akulaku**：三平台（Windows / macOS）× 含 git 私有驱动；Secrets 在 GitHub Environment `release`。
- **更新通道（每个 SKU 各自独立）**：三个 SKU 都产出签名 updater 产物并各自发布清单
  `latest.json` / `latest-all.json` / `latest-akulaku.json`；构建时按 `matrix.variant`
  注入该 SKU 自己的 endpoint（`ci-tauri-build.mjs` 以 JSON Merge Patch 覆盖
  `plugins.updater.endpoints`）。Tauri updater 只按**平台**在清单里查条目、不认 SKU，
  因此共用一份清单就等于把变体更新成 Basic（丢掉 Basic 不含的驱动）。SKU 名单、清单名
  与平台集合的唯一来源是 `scripts/release-variants.mjs`；变体清单缺平台即失败，Basic
  仅告警。发布前用 `pnpm test:release-variants` 校验矩阵 / `tauri.conf.json` / 清单步骤 /
  打包模板四方一致（`scripts/check-release-variants.mjs`）。详见 [updater.md](./updater.md)。

### 6.1 driver union 预热：已撤销（实测）

**不要**在变体矩阵前加 `warm-driver-deps` 预热门。

曾存在这样一个 job：在 4 个 target 上各编一次 driver union（`all,kiwi,superset`）写进共享
rust-cache，11 个 variant job 通过 `needs` 等它完成。技术前提是成立的——driver crate 和
第三方依赖在所有 variant 中源码与 features 完全一致，只有宿主 `datazen` lib 随 driver
feature 集变化。但实测证明它一分钱不省：

| 指标 | 撤销前 `36286459429` | 撤销后 `36300831455` | 变化 |
| --- | --- | --- | --- |
| **总墙钟** | **34:47** | **56:12** | **+21:25 (+62%)** |
| runner-minutes（所有 job 之和） | 186:50 | 251:54 | +65:04 (+35%) |
| 11 个 build job 重活步骤合计 | 155:42 | 156:37 | +0:55 (+0.6%) |
| build 阶段最长 leg | 33:45 | 31:52 | −1:53 |

三条原因：

1. **`needs` 是 job 级硬屏障。** 预热的 23:48 完全串行地加在关键路径上，没有任何东西与之
   并行。`+23:48 − 1:53 = +21:55`，与实测的 `+21:25` 吻合：build 阶段本身的长度几乎没变，
   预热是**净增**而非替换。
2. **预热要暖的依赖闭包本来就是热的。** 两个 run 里 11 个 build job 的
   `Cache Rust compilation` 恢复耗时**全部 > 5s**（命中；miss 约 1~2s）——`swatinem/rust-cache`
   自 `9eb095728` 起已在工作。预热没有把任何一个 miss 变成 hit，因为没有 miss 可转。
3. **真正占时间的部分预热在结构上碰不到。** 本地实测单个 variant 在热缓存之上的边际成本是
   **lib codegen 5m29s + fat LTO 链接 12m07s ≈ 17m36s**。lib 按各自的 `--features driver-*`
   编译；链接要把该 variant 的 `dist/` 嵌进二进制，且 fat LTO 要对所有 rlib 重跑一遍全程序
   优化。两者都是 variant 特有的，预热只能命中「第三方依赖 + driver crate」这一段，而那一段
   本就已经 100% 命中。

> **教训**：评估任何缓存/预热优化之前，先量被优化那一侧在优化**之前**的 cache 恢复耗时。
> 用「应该会 miss」代替「量一下是不是 miss」，会做出一个 21 分钟的负优化。

撤销预热时必须**同时**去掉 build job 上的 `save-if: false`：预热曾是该 cache key 的唯一写入者，
留着这行就变成无人写入，条目会在 7 天闲置后被 GitHub 逐出，之后每次发版都静默退回冷编译。

### 6.2 union 类型检查：保留

`union-typecheck` 是独立的一个 job，对 union（`all,kiwi,superset`，即 basic / all / akulaku
的超集）做**一次**类型检查。11 个 variant job 通过 workflow 级
`DATAZEN_CI_TYPECHECK_ONCE=1` 跳过自己的类型检查，改走 `pnpm build:bundle`（codegen + Vite）。

**它只跑 `tsrs`**（用户裁定），与本地门禁 `pnpm typecheck` **同一个引擎**。
`scripts/ci-union-typecheck.mjs` 调 `node_modules/tsrs/bin/tsrs --noEmit`，跑在同一份 union 上。

为什么本地与发布用同一个编译器：「本地绿」与「发布绿」指的是**同一件事**，不必在两处维护两套
语义，也省掉一次「为什么本地绿、发布红」的争论。`tsrs` 领先 GA 一个版本（它只会比 GA 多报，
不会少报；GA 独有的诊断它可能没有）⇒ 这道缺口由 `pnpm build` 里的 `tsc --noEmit` 兜住，
发布构建仍会抓住 GA 独有的诊断。

`tsrs` 没有 win32 二进制：该 job 跑不到它时**回退到 `tsc` 并明确打印原因**（绝不静默）
⇒ 门禁背后**始终有编译器**。

单列一个 job 的理由是它与 target 无关：一条 runner 就能覆盖全部 4 个
`(platform, target)` 组合，放在预热里则要么重复 4 次，要么落在最慢那条的关键路径上。

**它不拖慢整条 run。** 把 tsc 放回 11 个 variant job 看似「去掉一个串行前缀」，但那样每个
job 都长 `T_tsc`，而结束时间取最长那条 leg：墙钟同样 +`T_tsc`，runner 时间反而多
`10 × T_tsc`，且覆盖面从 union 缩回各自的 variant。保留是严格更优。唯一前提是它必须继续
`needs` 进 `build`——否则等于给发版摘掉了类型检查。

### 6.3 不要用环境变量加回 thin LTO

没有 `--profile` 不代表改不动：`tauri build` 会把环境传给它的 cargo 子进程，而 cargo 认
`CARGO_PROFILE_RELEASE_LTO=thin` / `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`（已在隔离工程上
验证 rustc 确实收到 `-C lto=thin`）。这是一条真实存在的逃生口，本仓库**主动选择不走**：
`[profile.release]` 的 `opt-level = "z"` + fat LTO 是为压体积设的，thin LTO 会让发版二进制
变大且幅度未实测。需要时先量体积。

## 7. 相关文档

- [e2e-testing.md](./e2e-testing.md) — WebDriver 构建与跑法
- [e2e-coverage.md](./e2e-coverage.md) — Host 路径覆盖矩阵
- [optional-drivers.md](./optional-drivers.md) — 可选 path 驱动说明
- [ci-private-drivers.md](./ci-private-drivers.md) — Git 驱动 Deploy Key
- [packaging.md](./packaging.md) — 发版渠道与 SKU 命名
