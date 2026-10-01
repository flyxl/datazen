# CI 与测试矩阵

> 与 [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)、[release.yml](../../.github/workflows/release.yml) 及 [AGENTS.md](../../AGENTS.md) 测试约定配套。  
> 目标：PR 流水线快且稳定；驱动 SKU 组合可扩展；**`all` 预设只进 Host 单测，不进编译矩阵**。

## 1. 总览

| 层级 | PR CI（`ci.yml`） | Release（`release.yml`） | 本地 / 维护者 |
|------|-------------------|--------------------------|---------------|
| 驱动选型 | **`basic` 固定**（postgres, mysql, sqlite, redis）；Host 单测另跑一遍 **`all`** | Basic / All × 四平台 + Akulaku × 三平台（Windows / macOS，无 Linux） | 任意 `--drivers=` / `DATAZEN_DRIVERS` |
| Host 前端单测 | ✅ `basic`：`pnpm test:unit`；`all`：`pnpm test:unit:driver-set`（软门禁，理由见 §2.1.1） | 构建前 `pnpm build`（含 typecheck） | `pnpm test:unit` / `pnpm test:unit:driver-set` |
| 驱动 UI 单测 | ✅ `pnpm test:unit:drivers` | 构建前 `pnpm build`（含 typecheck） | `pnpm test:unit:drivers` |
| TypeScript | ✅ `pnpm typecheck` | 同上 | `pnpm typecheck` |
| Host Rust lib | ✅ `cargo test -p datazen --lib`（basic features） | 完整 release 构建 | `cargo test -p datazen --lib` |
| driver-api | ✅ | 随构建链接 | `cargo test -p datazen-driver-api --lib` |
| ai-api | ✅ | 随构建链接 | `cargo test -p datazen-ai-api --lib` |
| Basic path 驱动 lib | ✅ 四 crate 并行 | Basic SKU 内嵌 | `cargo test -p datazen-driver-<id> --lib` |
| 可选 path 驱动 lib | ❌ | **All SKU** 构建时编译链接 | 改驱动 crate 时本地必跑 |
| Git 驱动（kiwi/superset） | ❌ | **Akulaku SKU**（需 Deploy Key） | 见 [ci-private-drivers.md](./ci-private-drivers.md) |
| Host E2E | ❌ | ❌（发版后手工 / R 阶段） | `pnpm e2e` / `pnpm e2e:minimal` |
| Host 契约矩阵 E2E | ❌ | ❌ | `pnpm e2e:contract:matrix` |
| 驱动专属 E2E | ❌ | ❌ | `packages/drivers/<id>/e2e/` |

**原则**

1. **Basic 必测**：每个 PR 与 `main` push 均跑 basic 四驱动 + Host 三件套（TS 单测 / Host lib / driver-api / ai-api）。
2. **All 只进 Host 单测，不进编译矩阵**：`resolve-drivers --drivers=all` 的 codegen 让 PR CI 跑一遍全部 path 驱动的 Host 前端单测——前端驱动形态是编译期 codegen，验证它不需要编译 Rust 驱动，因此 `all` 在 PR 上是廉价的；而 `cargo` 侧编译全部 path 驱动仍然只发生在 Release **All** SKU 与本地全量验证（`--drivers=all` 约 5 分钟重编译），不占 PR 流水线。
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
| 类型 | `pnpm typecheck` | `tsc --noEmit` |
| 守卫 | `check-managed-stubs.mjs`、`check-structure-editor-guardrails.mjs` 等 | 防止误提交 inject 产物 |
| Host 单测 | `pnpm test:unit` | Vitest。驱动集不是由 `pretest:unit` 定的：pnpm ≥ 7 默认不跑 `pre`/`post` 脚本（无 `.npmrc` 开 `enable-pre-post-scripts`），该 hook 实为惰性；真正把 codegen 落成 `basic` 的是 `pnpm install` 的 `prepare` → `ensure-generated-drivers.mjs` → `resolve-drivers.mjs` 默认值 |
| Host 单测（`all`） | `pnpm test:unit:driver-set` | `scripts/run-unit-driver-set.mjs`：`resolve-drivers.mjs --codegen-only --drivers=all` → `generate-builtin-locales.mjs` → `vitest run`。`all` = 全部 **path** 驱动（不含 kiwi/superset），故无需 Deploy Key 与网络。**软门禁**（`continue-on-error: true`），理由见 §2.1.1；转硬门禁时**必须同时**改 `scripts/__tests__/run-unit-driver-set.test.ts:149`（见 §2.1.1 末） |
| 用例计数守卫 | `pnpm test:unit-count` | `scripts/check-test-count.mjs`：`vitest list` 实测用例数，与本节三处引用比对。**硬门禁**，理由见 §2.1.3 |
| 驱动 UI 单测 | `pnpm test:unit:drivers` | `packages/drivers/*/ui/__tests__/`；与 Host 单测互不收集，两步都必须跑 |
| Site（条件） | `check-site-seo.mjs` | 仅当 diff 含 `site/`（`fetch-depth: 0` 仅此 job 需要） |

### 2.1.1 为什么该步是软门禁，以及 `testTimeout` 为何取 10s

**软门禁的原因（与"已知红"无关）。** 早期版本的 `ci.yml` 注释记录了两个阻塞项，
二者都是基线陈旧造成的假象：它们测于 `7f35e1932`，该基线比 `d1edfa7ba` / `f4c2e5078`
早 9 个 commit。在 `feat/platform-p0` @ `7559759b7` 上，两者都不存在——该基线
`--drivers=all` + locales codegen 全绿：**555 文件 / 5677 用例，exit 0，140s**；
`tester_tunnelValidationMatrix.test.tsx` 单跑 **36 通过**。此前记为红的
`ContentView` / `ContentViewKv*` / `QueryPanel.executeCancel`，正是 `cbf87a4c0`
（修复 schemaStore/panelStore mock 漂移导致的 18 个前端单测失败）已修掉的那批。

保留 `continue-on-error: true` 的真实理由很窄：**这一步与 10s 的 `testTimeout`
都还没有在真实 GitHub runner 上跑过。** 本机 8 核开发机全绿，不能推断真实 runner
的全绿——本项目无法控制 runner 规格，仓库可见性也未定。因此本文档与
`vitest.config.ts` 中**不对 runner 的核数做任何断言**（下表第二列描述的是本机
实测行的形态，不是对 CI runner 规格的推测）。软门禁让首次运行可被观察而不阻塞主干；
连续若干次 CI 全绿后即可去掉该标记升为硬门禁，届时同步更新本节。

**⚠️ 转硬门禁是耦合改动，两处必须同时改。** 只从 `ci.yml` 删掉
`continue-on-error: true`，会让守卫 `scripts/__tests__/run-unit-driver-set.test.ts:149`
的 `expect(step?.['continue-on-error']).toBe(true)` 失败（`expected undefined to be true`），
即照着上面那段"去掉该标记升为硬门禁"执行会直接造出一个红 CI。该守卫对软门禁标记的
断言与 workflow 里的标记是**同一个契约的两端**：改 workflow 就必须同步改那行断言。
`continue-on-error: true` 本身保留不动——首次真实 runner 运行需可观察，先软后硬是
正确顺序。

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

**守卫的边界：防删依据，不防错依据。** 该守卫断言的是"这些实测数字串**仍然存在**"
（`1745ms` / `4392ms` / `11629ms` / `testTimeout: 10_000` / `continue-on-error: true` /
`5677 passed`），**不是**它们是否属实。实测确认：把 `4392ms` 改成 `4400ms` 会红（串
不匹配），但改成某个**仍然存在、只是不再真实**的数不会红。所以"守卫全绿"**不等于**
"文档里的数字或说法是对的"。数字的正确性只能靠重测确认。

它对 runner 核数的检查是**关键词启发式（防形不防语义）**，边界必须说清：把
`ci.yml` 注释里那句关于 runner 核数的话整体改写成另一个"仍然成立"的说法，
只要句中同时出现**阿拉伯数字 + `core`/`核`**与**runner**，且该句**没有**否定/免责标记
（not、never、不推断、假定……），就会红；但写成文字（"four cores"）、或把免责标记留在
同一段而不是同一句，都可能漏过。它拦的是最常见的复发形态，不是语义判定。

用例总数是本节唯一**不靠字符串存在性**的数字，另有独立机器测量，见 §2.1.3。

### 2.1.2 `check-ci-docs-consistency.mjs` 覆盖什么、不覆盖什么

`scripts/check-ci-docs-consistency.mjs`（别名 `pnpm test:ci-docs`）**已经由 `ci.yml`
的 "All strict guards" 步直接调用**（`pnpm test:ci-docs`，与 `test:ids` / `test:layers`
同一 fail-fast 列表内）。它只做三件事：

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
  的三条腿与软门禁标记按结构守；**不校验这些数字是否属实**。
- `pnpm test:unit-count` —— 例外见 §2.1.3：只有用例总数是被机器实测比对的。

### 2.1.3 用例计数守卫：为什么它不在 vitest 里，而是 CI 的独立一步

`vitest.config.ts`、`ci.yml` 注释、本文档 §2.1.1 各写了一个 Host 用例总数。
手写三个数字互相比对是**同义反复**：三处同时写错，错得一样多，守卫全绿。
所以这个数字必须有一侧是机器产出的。

**做法：`pnpm test:unit-count` → `scripts/check-test-count.mjs`。**
它复用 `scripts/run-unit-driver-set.mjs` 的驱动集规划（同样先跑两条 codegen 腿，
把 `vitest run` 那条腿改成 `vitest list`），数出 stdout 的用例行，再和三处引用比对，
并要求三处彼此相等。任一处对不上或三处互相矛盾即 exit 1。

**为什么是 CI 的独立一步，而不是一个 vitest 用例。** 唯一机器产出的口径就是
`vitest list`，而它要**重新收集并转换全部文件**。本机实测（darwin-arm64 /
node v22.20.0 / 驱动集 `all`）：

| 口径 | 墙钟 | user CPU | 峰值 RSS | 产出 |
|------|------|----------|----------|------|
| `node scripts/check-test-count.mjs`（冷缓存，含两条 codegen 腿；CI 的情形） | 126.96s | 558.27s | 约 300 MB | 5714 个用例 + 557 个文件 |
| `vitest list`（热缓存） | 87.31s | 541.14s | 约 299 MB | 5714 行用例 + 557 个文件 |
| `vitest list --filesOnly`（热缓存） | 0.25s | 0.31s | 约 115 MB | 557 行，**只有文件数** |

> 三行同在 5714 用例 / 557 文件的基线上实测（darwin-arm64 / node v22.20.0 /
> vitest 4.1.10）。CI 是冷缓存，所以第一行才是要拿来做预算的那个数。

> 实测记录：冷缓存 126.96s（热缓存也还有 87.31s）超 20s 门槛，因此**否决**"写成一个
> vitest 用例"的方案。

把它塞进 vitest 用例，等于在跑套件的过程中再跑一遍完整收集：`vitest.config.ts`
自己声明的 `testTimeout` 是 10s，这个用例无论如何都超时，套件会因为一个文档守卫而红。
`--filesOnly` 够便宜，但它只给**文件数**，而三处引用的是**用例数**，口径不同，
拿它冒充实测值就是把"便宜"换成了"守的不是那件事"。因此它是独立一步，是**硬门禁**
（用例数陈旧属文档缺陷，不是抖动），排在 `pnpm test:unit:driver-set` 之后——
计数是 `all` 驱动集的属性，而那一步正是生成该状态的动作。

**已考虑并否决的省时方案**：直接用 `pnpm test:unit:driver-set` 自己的
`--reporter=json` 输出，几乎零边际成本。否决理由有两条：它把计数的来源绑在被检查的
那一步上，reporter 一改守卫就静默失效；且 `retry: 2` 与软门禁语义都可能让 json
里的总数与 `vitest list` 的收集口径不一致。两害相权，多花这 126.96s 更划算。

**这个守卫已经抓到过一次真问题。** 上线的第一次运行就红：三处引用的 5695 是本轮改动
之前的实测值，而本轮新增的守卫用例本身已经让套件变成 5714。旧数字在同一个提交内漂移
且无人察觉，正是因为它当时**只被"字符串还在不在"守着、没有任何机器测量**。

**能力边界（必须写明）：** `run-unit-driver-set.test.ts` 里那个便宜的 vitest 守卫
**只能**发现三处数字互相矛盾；它发现不了"三处一致地写错"。后者只有
`pnpm test:unit-count` 能发现，而它不在 `pnpm test:unit` 里跑。两者是互补的，
不要把前者当成真值检查。

**另一半边界：它也认不出"用例真的少了"。** 实测四组变异（每组跑完复原）：

| 变异 | 便宜的 vitest 守卫 | `pnpm test:unit-count` |
|------|------|------|
| 三处一致改成 5700 | **全绿**（37/37 通过） | **红**：三处各自报 5700 ≠ 5714 |
| 只改 `vitest.config.ts` 一处为 5700 | **红**：三处互相矛盾 | **红** |
| 删掉一个真用例文件，三处仍写 5714 | — | **红**：实测 5713 ≠ 5714 |
| 删掉一个真用例文件，三处同步改成 5713 | — | **全绿** |

第一行是"三处一致地写错"的确凿证据：便宜的守卫对它毫无反应。第二行说明便宜的守卫
并非全无作用——它守的正是三处**互相矛盾**。后两行是这个守卫**守不住**的地方：它守的是
"总数与引用相符"，不是"用例还在"。第三行它能发现；**第四行它发现不了**——删用例与
改引用落在同一个 commit 里时，计数、引用、机器实测三者自洽，没有任何一侧会说"不"。
因此它**不是丢用例检测器**；要检测丢用例得比对用例名单，而不是总数。

解析侧是 fail-closed 的：`vitest list` 输出里只要出现一行不像用例行，它就拒绝继续，
并把那行原样打出来（实测：用例名里带换行时触发），而不是少算一个然后照样通过。

### 2.2 rust job（Rust 段，浅克隆）

| 步骤 | 命令 / 动作 | 说明 |
|------|-------------|------|
| 依赖 | `pnpm install --frozen-lockfile` | 注入脚本链 import `fflate`，仍需 node_modules |
| 驱动解析 | `resolve-drivers.mjs --drivers=basic` | 写入 `.driver-features.json`、codegen |
| Rust | 见下表 | Rust **stable** |
| 清理 | `driver-file-stash.mjs restore` | 恢复被 inject 的 tracked 文件（`if: always()`） |
| ai-api | `cargo test -p datazen-ai-api --lib` | 在 restore **之后**执行（不依赖 inject 产物） |

Rust 测试顺序（与 `ci.yml` 一致）：

```bash
# driver-api 与四个 path 驱动合为一次 cargo 调用（不依赖 --features）
cargo test --lib -p datazen-driver-api -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis
# datazen 需要 features 选择注入的驱动
FEATURES=$(node -e "console.log(JSON.parse(require('fs').readFileSync('.driver-features.json','utf8')).features.join(','))")
cargo test -p datazen --lib --features "$FEATURES"
# 恢复被 inject 的 tracked 文件，确保后续步骤工作在干净状态
node scripts/driver-file-stash.mjs restore
# ai-api 不依赖注入产物，放在 restore 之后
cargo test -p datazen-ai-api --lib
```

## 3. 驱动预设与 SKU

| 预设 / SKU | Registry ids | PR CI | Release job |
|------------|--------------|-------|-------------|
| `basic`（默认） | postgres, mysql, sqlite, redis | ✅ | Basic 变体 |
| `all` | 全部 **path** 条目（不含 git 驱动） | ❌ | All 变体（`*-all` 后缀） |
| Akulaku 显式列表 | postgres,mysql,sqlite,redis,mongodb,kiwi,superset | ❌ | Akulaku 变体（`*-akulaku`）；`needs_git: true` |
| 自定义逗号列表 | 任意 registry id 组合 | ❌ | 仅本地 / 定制发版 |

详见 [optional-drivers.md](./optional-drivers.md)、[ci-private-drivers.md](./ci-private-drivers.md)。

## 4. 本地 PR 基线（与 CI 对齐）

贡献者在开 PR 前至少跑：

```bash
node scripts/generate-builtin-locales.mjs
pnpm typecheck
pnpm test:unit
pnpm test:unit:drivers
node scripts/resolve-drivers.mjs --drivers=basic
cargo test --lib -p datazen-driver-api -p datazen-driver-postgres -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis
FEATURES=$(node -e "console.log(JSON.parse(require('fs').readFileSync('.driver-features.json','utf8')).features.join(','))")
cargo test -p datazen --lib --features "$FEATURES"
node scripts/driver-file-stash.mjs restore
cargo test -p datazen-ai-api --lib
```

若改动 `site/`：`node scripts/check-site-seo.mjs`。

若改动可选 path 驱动的 **Rust**：追加 `cargo test -p datazen-driver-<id> --lib`（该 crate 的 UI 单测已由 PR CI 的 `pnpm test:unit:drivers` 覆盖，无需再手工跑）。

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
的超集）做**一次** `tsc --noEmit`。11 个 variant job 通过 workflow 级
`DATAZEN_CI_TYPECHECK_ONCE=1` 跳过自己的 tsc，改走 `pnpm build:bundle`（codegen + Vite）。

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
