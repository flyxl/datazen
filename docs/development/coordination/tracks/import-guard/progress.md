# Track: import-guard — 驱动/扩展 import 边界与 setLocale 调用护栏 + CI 接入

- 分支: `feature/import-guard`（基准 `feat/driver-decoupling` @ 8b66586e4，Wave 1/2/3 全部已合并）
- 角色: Coder → Tester
- 波次: Wave 4-A（本轨合并后进入 R 阶段全量回归，任务 #32）

## 目标

Wave 1~3 已把驱动侧对宿主代码的引用清零（`grep ['"]\.\./.*src/` 在 `packages/drivers/**` 当前仅剩 2 处已裁定夹具，见下）。这类解耦**极易被新 MR 悄悄回退**，本轨落地静态护栏 + CI 阻断，把契约变成机器强制。

## 现状事实（已核对，勿重复调研）

- 允许 import 面与契约正文：`docs/development/driver-api-dependency-boundary.md`（Part 2 前端契约，2.1.2 允许/禁止面、2.2 落点决策表、2.4.3 语言集合三层、2.6「Wave 4 护栏预告」明确写着文件名与 CI 位置尚未确定 —— **本轨就是把它落实并回扫措辞的那一轨**）。
- 当前基线（基准 8b66586e4 实测）：
  - `packages/drivers/**` 中匹配 `['"]\.\./.*src/` 命中 **2**，均在 `packages/drivers/redis/ui/__tests__/redisKeyWebContextMenu.test.tsx:5`（`WebContextMenuHost`）与 `:9`（`useContextMenuStore`）—— 协调者已裁定为驱动↔宿主**集成夹具**，属唯一豁免。
  - Wave 3 已把驱动侧 8 处 `vi.mock('<rel>/src/hooks/useI18n')` 改为 partial-mock `@datazen/ui`，因此 **`vi.mock` 形态的宿主路径命中数应为 0**（若你实测非 0，说明基线漂移，须先报告再决定）。
  - `packages/ui/src/i18n.ts` 是 `setLocale` 的唯一定义处；唯一生产调用方应为宿主 `src/lib/localeSync.ts`。
  - 同类护栏脚本惯例（照抄其结构：可导出纯函数 + `runCli()` + `endsWith(...)` main 守卫 + 明确退出码 + `scripts/__tests__/*.test.mjs` 单测）：`scripts/check-id-terminology.mjs`、`scripts/check-module-layers.mjs`、`scripts/check-ci-docs-consistency.mjs`，以及 Wave 3 刚重构过的 `scripts/i18n-sync-check.mjs`（其 18 例 fixture 用例可当写法样板）。
  - 接入点：`package.json` 现有 `test:ids` / `test:layers` / `test:ci-docs` / `test:version` 脚本位；CI 步骤见 `.github/workflows/ci.yml` 的「Guard …」段（约 49~68 行）；本地等价位 `scripts/ci-local.sh`、`scripts/run-full-automation-test.sh`（Wave 3 Tester 已登记这三处的调用方清单）。

## 范围

1. **新增护栏脚本 `scripts/check-driver-import-boundaries.mjs`**（命名沿用 `check-*.mjs` 惯例），导出纯函数 + CLI 守卫，覆盖三条规则：
   - **R1 驱动禁引宿主源码**：扫描 `packages/drivers/*/ui/**`（含 `__tests__`、`locales/**`）内所有**说明符字面量**，凡匹配 `'^(\.\./)+.*src/'`（即相对上溯进宿主 `src/`）即违规。必须同时覆盖：`import`/`export ... from`、`import(...)` 动态形式、`vi.mock(...)` / `vi.doMock(...)` / `require(...)` 的字符串参数形态——**不能只 grep `from`**，这是上一轮实测暴露的 8 处漏网点。
   - **R2 非宿主禁调 `setLocale`**：扫描范围 = `packages/**`（除 `packages/ui/src/i18n.ts` 的定义与自身导出行），命中 `setLocale(` 调用即违规；须排除注释行与 `type`/接口成员声明（`setLocale(locale: string): void;` 这类契约声明不算调用）。
   - **R3 宿主不引驱动内部**（低成本对称护栏）：`src/**` 不得相对 import `packages/drivers/**`（当前合法出口是 codegen 的 `src/extensions/generated*.ts`，属 gitignored 产物，跳过；`src/locales/index.ts` 对 `packages/drivers/*/locales/en` 的 **type-only** import 若仍存在须先核实现状再决定放行或收紧）。**若 R3 现状即红，不要擅自扩大豁免**：先在本文件登记事实并与协调者裁定后再定。
   - 允许清单（allowlist）：外置为脚本内显式常量（文件 + 规则 + 原因 + 归属里程碑），初始仅 2 条 = 上述 fixture；禁止用目录级/通配级豁免把口子放大。
2. **测试**：`scripts/__tests__/check-driver-import-boundaries.test.mjs`，用内联 fixture（虚拟文件树 map）覆盖：干净树通过 / `from` 违规 / `vi.mock` 违规 / 动态 `import()` 违规 / `export from` 违规 / 非宿主 `setLocale` 调用违规 / 注释与契约声明不误报 / allowlist 命中放行 / allowlist 指向不存在文件时报「过期豁免」/ R3 分支。**本轨新增逻辑行覆盖 ≥80%（目标 100%）**。
3. **对真实仓库自证有效**：临时在某个驱动 ui 文件里加一行 `import { useI18n } from '../../../src/hooks/useI18n';` 与一行 `vi.mock('../../../../src/stores/settingsStore')`，跑脚本必须红；还原后必须绿。把两次输出贴进 progress（结束前 `git status` 干净）。
4. **接入 CI 与本地**：`package.json` 加 `test:boundaries`；`.github/workflows/ci.yml` 在其它 Guard 步骤后加「Guard driver/host import boundaries」；`scripts/ci-local.sh` 与 `scripts/run-full-automation-test.sh` 同步补一步（与既有 guard 步骤风格一致，失败即阻断）。
5. **文档回扫（把 Wave 3 留下的债一次清掉）**：
   - `docs/development/driver-api-dependency-boundary.md` 2.6：把「文件名与 CI 位置尚未确定」替换为真实脚本名 / npm script 名 / CI 步骤名。
   - 同文档 2.1.2 与 2.7 的过渡期基线数字（`32 useI18n + 2 夹具 + 8 vi.mock = 42`）**已因 Wave 3 合并而失效**：按你实测重写成当前真实基线（应为「生产码 0 / 夹具 2」），并注明护栏已阻断新增。
   - `decouple-docs` 轨登记的 3 条 Nit 一并处理：① `boundary.md:299` 单条 bullet 过载 → 拆 2~3 子条；② 两份指南 §6.3 分例补「（如 mongodb）」标注与契约写法对齐；③ 与 2.4.3 观察项同批回扫（保持两份指南标题 **21 : 21**、层级 `#`×1/`##`×13/`###`×7 不变）。
   - 文档改动必须逐处 Read 代码核实，禁止编造符号/路径/行号。
6. **R1 违规修复的边界**：本轨**只写护栏**，不重做解耦。若 R1/R2 因合并后的新漂移出现非豁免违规，先停下报告协调者裁定，不要自行搬迁代码。

## 禁止事项

- 不动 `packages/ui/src/i18n.ts` 的既有 API 行为；不引入第二套 i18n；不改驱动/宿主业务代码（除第 3 项临时自证，且必须还原）。
- 不动 `docs/development/coordination/hub.md`（协调者聚合专用）、其他轨 `progress.md` / `bugs.md`。
- 不新增/删除两份 `independent-driver-development.*.md` 的任何标题（21:21 是前几轮实测口径）。
- 不动 `src-tauri/**`、`Cargo.lock`、`packages/pro-extensions/**`；不提交 codegen 产物（`src/extensions/generated*.ts`、`src-tauri/src/driver_init.rs`、`capabilities/default.json`）。
- 禁止 `pnpm install`、裸 `pnpm build`（会自触发 install；用 `npx vite build`）；禁止真实 `pnpm e2e`。

## 环境注意

- 工作目录固定 `.worktrees/datazen-import-guard`；`pwd` 自检；搜索用 Grep 工具。
- worktree codegen 默认 `--drivers=basic`；redis 相关校验前：`node scripts/resolve-drivers.mjs --codegen-only --drivers=all`

## 验收标准

1. `node scripts/check-driver-import-boundaries.mjs` 在还原态基准分支上 **exit 0**，输出风格与其它 guard 脚本一致（含扫描文件数、豁免命中数、过期豁免检测）。
2. 故意注入 R1（`from` 与 `vi.mock` 两种）与 R2 违规各一处 → 脚本 **exit≠0 且逐条点名文件:行**；还原后再次 exit 0。
3. `npx vitest run scripts` 全绿，且本轨新增用例数与覆盖数字如实登记（新增逻辑行覆盖 ≥80%）。
4. allowlist 只含已裁定的 2 条 fixture；无目录级/通配豁免。
5. `package.json` + `.github/workflows/ci.yml` + `scripts/ci-local.sh` + `scripts/run-full-automation-test.sh` 四处接入齐备且互相一致（脚本名拼写完全相同）。
6. `npx tsc --noEmit -p tsconfig.json` = 0；`npx vite build` exit 0。
7. `node scripts/check-id-terminology.mjs`、`check-ci-docs-consistency.mjs`、`check-module-layers.mjs`、既有 `pnpm test:unit` / `pnpm test:unit:drivers` 不因新护栏破坏（新脚本是**新增**步骤，不得改动既有步骤语义）。
8. 文档回扫：2.6 不再含「尚未确定」；2.1.2/2.7 基线数字与实测一致；3 条 Nit 关闭；两份指南 21:21；文档内路径/符号抽验零失配。

## 状态

- [x] Coder 完成 → READY_FOR_TEST（commits `3e9014d91` 护栏+单测 / `b56058f46` 四处接入 / `d250e52de` 文档回扫 / `1a64ca904` 本实施记录 / `43e042341` 回扫口径修正，本报告末尾）
- [x] Tester 复测 → **TEST_DONE(PASSED)**（复测记录见文末，本 commit）
- [x] BUG-008 第 2 轮 Tester 复测（全新实例）→ **TEST_DONE(PASSED)**：该实例跑完全部核查后在写记录时遭网络中断，正文丢失；协调者从其 transcript 逐条恢复实测证据（见文末「BUG-008 第 2 轮复测记录」），并在合并后的主检出独立复跑关键门禁交叉核对。
- [x] 二次合流 → `b22b41ac8`（`git merge --no-ff feature/import-guard`），合流后主检出护栏 exit 0 / 12 advisory、`vitest run scripts` 23/244、`tsc --noEmit` 0。

## Coder 实施记录

Phase：**READY_FOR_TEST**。分支 `feature/import-guard`，worktree `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-import-guard`（开工自检 `pwd` / `--show-toplevel` 均落在本 worktree，分支非 main）。

### 1. 改动文件清单

| 文件 | 性质 |
| --- | --- |
| `scripts/check-driver-import-boundaries.mjs` | 新增（581 行）：导出 `scanCode` / `resolveSpecifier` / `rulesForFile` / `inspectSource` / `checkDriverImportBoundaries` / `runCli` + `RULES` / `ALLOWLIST` 常量；`process.argv[1].endsWith('check-driver-import-boundaries.mjs')` main 守卫（`/* istanbul ignore next */`）；退出码 0/1/2 |
| `scripts/__tests__/check-driver-import-boundaries.test.mjs` | 新增（386 行 / **31 例**）：内联虚拟文件树 fixture（`files` 注入，零临时目录）+ 真实仓库自证 2 例 |
| `package.json` | `test:boundaries` script（紧邻 `test:layers`） |
| `.github/workflows/ci.yml` | 步骤 `Guard driver/host import boundaries` → `pnpm test:boundaries`（位于 `Guard version consistency` 之后、`Guard i18n sync (warning only)` 之前） |
| `scripts/ci-local.sh` | `step "3.4/11 Guard: driver/host import boundaries …"` + `pnpm test:boundaries \|\| fail` |
| `scripts/run-full-automation-test.sh` | Stage 1 增 `pnpm test:boundaries \|\| fail …`，并同步覆盖范围注释第 2 条 |
| `docs/development/driver-api-dependency-boundary.md` | 2.1.2 / 2.4 引言 / 2.4.2 / 2.4.3 表 + 配套终态 + 长 bullet 拆分 / 2.4.4 / 2.5 / 2.6 / 2.7 回扫 |
| `docs/development/independent-driver-development.zh-CN.md`、`.en.md` | §6.2 尾句、§6.3 两条 bullet（zh/en 同步，标题零增删） |
| 本文件 | 实施记录 |

未触碰：`packages/ui/src/i18n.ts`、`src-tauri/**`、`Cargo.lock`、`packages/pro-extensions/**`、`hub.md`、他轨 progress/bugs、任何驱动/宿主业务代码（第 3 项自证除外，已还原）。

### 2. 三条规则的实现要点

- **R1（阻断）**：扫描范围是整个驱动包 `packages/drivers/**`（`ui/**` + `locales/**` + `e2e/**`，比任务书的 `ui/**` 略宽，仍只覆盖驱动侧）。核心是**先做一遍词法扫描收集全部字符串/模板字面量**（`scanCode`），再对每个字面量做相对路径解析，解析结果落进 `src/` 即违规——因此 `import`/`export … from`/动态 `import()`/`vi.mock`/`vi.doMock`/`require`/任何辅助函数取道同一条判定路径，**不存在「只匹配 `from`」的漏网形态**。注释与被注释掉的 import 在扫描阶段被空白化（保留行号），字符串内容同样空白化（R2 因此不会把 prose 当调用）。驱动包内部自身的 `../src/…`（解析后仍是 `packages/drivers/<id>/src/…`）明确不报，见单测「leaves driver-internal ../src/ trees alone」。模板字面量含 `${}` 者视为计算值、跳过（静态不可判）。
- **R2（阻断）**：范围 `packages/**`，豁免是脚本内 `R2_FILE_CARVEOUTS` 的**两个精确文件**——`packages/ui/src/i18n.ts`（`setLocale` 唯一定义处，`:34`）与它自己的单测 `packages/ui/src/__tests__/i18n.test.tsx`（实测 7 处调用是该运行时的唯一行为测试手段，任务书原文只写「定义与自身导出行」，若不豁免该测试则基准即红、又不允许新增 allowlist 条目）。**不是 `packages/ui/**` 整包/目录级豁免**：`@datazen/ui` 其它组件出现 `setLocale(...)` 调用照样红，符合任务书「禁止目录级/通配级豁免」的口径；驱动/扩展/driver-sdk/wapp-sdk/extension-points 全在覆盖内。判定式 `\bsetLocale\s*\(`，随后排除契约成员声明 `setLocale(locale: string): void;`、`function setLocale(`；`import { setLocale } from '@datazen/ui'` 无括号故不算调用。实测 `packages/**` 中 `setLocale(` 仅命中上述两文件，其余 0。（本条措辞由 `43e042341` 修正为已落地实现，此前 `d250e52de` 的文档误写成整包豁免。）
- **R3（advisory，暂不阻断）**：`src/**` 相对解析进 `packages/drivers/**` 即列出，跳过 codegen `src/extensions/generated{,-locales,-pro}.ts`。**现状实测非 0（4 处）**，按任务书要求未擅自加豁免、未搬迁代码，改为 `RULES.R3.blocking = false` 报告模式，交协调者裁定（见「开放项 B」）。
- **allowlist**：脚本内显式常量，2 条 = `redisKeyWebContextMenu.test.tsx` 的 `WebContextMenuHost`(:5) 与 `useContextMenuStore`(:9)，字段为 `rule + file + specifier + reason + milestone`，精确三元组匹配，**无目录级/通配豁免**（单测用正则断言条目里不许出现 `*` / `?`）。**过期豁免检测**两种：条目文件不存在 / 文件存在但条目未被命中，均 exit 1（含真实 fs 路径的单测各一条）。

### 3. 真实仓库自证（任务书范围第 3 条，原文输出）

注入形态：R1 `from`（`packages/drivers/sqlserver/ui/ConnectionFields.tsx`）、R1 `vi.mock`（`packages/drivers/redis/ui/__tests__/useRedisGate.test.tsx`）、R2 `setLocale`（`ConnectionFields.tsx`）各一处。

```console
$ # 注入三处违规后
$ node scripts/check-driver-import-boundaries.mjs; echo "EXIT=$?"
[check-driver-import-boundaries] 2 allow-listed reference(s) skipped
[check-driver-import-boundaries] R1 packages/drivers/redis/ui/__tests__/useRedisGate.test.tsx:157: resolves to host src/stores/settingsStore
    vi.mock('../../../../../src/stores/settingsStore');
[check-driver-import-boundaries] R1 packages/drivers/sqlserver/ui/ConnectionFields.tsx:109: resolves to host src/hooks/useI18n
    import { useI18nProbe } from '../../../../src/hooks/useI18n';
[check-driver-import-boundaries] R2 packages/drivers/sqlserver/ui/ConnectionFields.tsx:110: setLocale() may only be called from host src/** (src/lib/localeSync.ts)
    export const probeFrom = () => setLocale('zh-CN') || useI18nProbe;
[check-driver-import-boundaries] R3 (advisory) src/locales/locales.test.ts:107: reaches into driver internals (packages/drivers/redis/locales)
[check-driver-import-boundaries] R3 (advisory) src/test/driverUiSetup.ts:25: reaches into driver internals (packages/drivers/redis/ui/shared/meta)
[check-driver-import-boundaries] R3 (advisory) src/test/driverUiSetup.ts:26: reaches into driver internals (packages/drivers/mongodb/ui/meta)
[check-driver-import-boundaries] R3 (advisory) src/windows/connection/DocumentConnectionView.tsx:25: reaches into driver internals (packages/drivers/mongodb/ui/mongodbFind)
[check-driver-import-boundaries] FAILED: 3 violation(s) (1403 file(s) scanned · 4 advisory finding(s))
    see docs/development/driver-api-dependency-boundary.md §2.1.2
    see docs/development/driver-api-dependency-boundary.md §2.4.2
EXIT=1

$ git checkout -- packages/drivers/sqlserver/ui/ConnectionFields.tsx packages/drivers/redis/ui/__tests__/useRedisGate.test.tsx
$ git status --short
（无输出，注入已完全还原）
$ node scripts/check-driver-import-boundaries.mjs; echo "EXIT=$?"
[check-driver-import-boundaries] 2 allow-listed reference(s) skipped
[check-driver-import-boundaries] R3 (advisory) …（同上报 4 条）
[check-driver-import-boundaries] ok (1403 file(s) scanned · 0 blocking violation(s) · 4 advisory finding(s))
EXIT=0
```

要点：三种注入形态**逐条点名 `文件:行`** 且退出码非 0；`vi.mock` 形态被抓住，正是要补的旧口径漏洞。

### 4. 自测清单（真实命令与真实输出）

前置：`node scripts/resolve-drivers.mjs --codegen-only --drivers=all` → exit 0。

| 命令 | 结果 |
| --- | --- |
| `node scripts/check-driver-import-boundaries.mjs` | **exit 0**，`ok (1403 file(s) scanned · 0 blocking violation(s) · 4 advisory finding(s))` + `2 allow-listed reference(s) skipped` |
| `npx vitest run scripts` | **23 files / 239 tests 全绿**（本轨新增 1 file / **31 例**） |
| 本轨新增逻辑覆盖（`--coverage.include='scripts/check-driver-import-boundaries.mjs'`） | **Lines 100% · Functions 100% · Statements 99.26% · Branches 95.42%**（≥80% 门槛远超；行/函数双 100% 达成目标）。未触发的 7 个分支逐个核实为：`opts.log ?? console.log` / `opts.error ?? console.error` / `opts.argv ?? process.argv` 三处默认参数回退、转义符正好落在文件末尾的 `source[i + 1] ?? ''` 两处、`lines[line - 1] ?? ''` 取行文本的兜底，以及 fs `walk` 里真实扫描未命中的 `SKIP_DIR_NAMES.has(...)` / `!entry.isFile()` 两个 continue |
| `npx tsc --noEmit -p tsconfig.json` | **exit 0（0 错误）** |
| `npx vite build` | **exit 0**（仅既有 chunk >500 kB 提示，非本次引入） |
| `node scripts/check-id-terminology.mjs` | `ok (1719 files scanned)`，5 处既有豁免 |
| `node scripts/check-ci-docs-consistency.mjs` | drivers 11 ids / window boundaries / toolchain 三项 ok |
| `node scripts/check-module-layers.mjs` | `ok (3 rules)` |
| `npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui` | **27 files / 222 pass / 0 fail**（与基线一致，无回归） |
| `npx vitest run src packages/driver-sdk packages/ui` | **412 files / 4243 pass**（与基线一致，无回归） |
| `npx vitest run --config vitest.drivers.config.ts`（全驱动 UI，附加自查） | 33 files / 241 pass / 0 fail |
| 两份指南标题结构 | zh `#`×1 `##`×13 `###`×7 = 21；en 同 = 21（**21:21 未变**） |

### 5. 文档回扫落点（逐处 Read 代码核实，无编造符号/行号）

- **2.6**：`尚未确定` 措辞删除，替换为真实脚本名 `scripts/check-driver-import-boundaries.mjs`、`pnpm test:boundaries`、CI 步骤名 `Guard driver/host import boundaries`、两处本地等价位、单测文件与覆盖数字、R1/R2/R3 语义与 0/1/2 退出码。全文再 grep `尚未确定\|同期落地` → 0 命中。
- **2.1.2 / 2.7 基线数字**：`34 = 32 + 2` 与 `34 + 8 = 42` 的旧口径按实测改写为 **生产码 0 / 夹具 2**（`redisKeyWebContextMenu.test.tsx:5,9`，护栏输出 `2 allow-listed reference(s) skipped` 即机器口径），并显式说明**抛弃 `grep -rn "from '\.\./.*src/"` 作为清点口径**的原因（漏 mock/require 形态）。
- **顺带关掉 decouple-docs 3 条 Nit**：① 2.4.3 超长 bullet → 拆「不对称本体 / 代价与裁定出处 / 注册≠可达 / 评审口径」3 子条（结论未改）；② 两份指南 §6.3 分例补「（如 mongodb）」并给契约同款措辞 `（如 redis，嵌套两层）`；③ 观察项 1 与 4 归并同批：2.4.2 补 `src/locales/index.ts:69-84` 的 `getTranslation()` 临时换 locale 适配器说明（实测 `:78` 交换、`:82` 复位，仍在宿主内、不违反「只有宿主调用」），§6.2 尾句补「薄再导出壳也不得被驱动 import + 已由 R1/R2 阻断」。
- **额外事实修正（超出任务书列举，但属同一回扫的实测失配）**：
  1. 2.4.3 两处 `scripts/i18n-sync-check.mjs:23` 的 `LOCALE_FILES` 引用 → 实际在 **`:36`**（Wave 3 重构后行号漂移；`src/locales` 目录 `:33`、`packages/drivers` 目录 `:34`、`checkDriverLocalePacks()` 调用 `:318`）。
  2. 2.4.3 「`src/locales/index.ts:42-45` 写入 `extensionLocales`」→ 实际写入在 `registerLocale()` **`:47-50`**（`:48`）。
  3. 2.4 引言 / 2.4.3 表 / 2.4.4 的「`i18n-drivers` 轨**同期落地、截至本文件基准尚未合并**」→ 已交付事实核实为真：`packages/drivers/{redis,mongodb}/locales/index.ts` 均存在，入口副作用行分别是 `packages/drivers/redis/ui/shared/meta.ts:4`（`import '../../locales';`）与 `packages/drivers/mongodb/ui/meta.ts:4`（`import '../locales';`），`DRIVER_LOCALES` / `generated-locales` 在 `src/ scripts/ packages/ e2e/` **0 生产命中**（仅 `AGENTS.md`、`CONTRIBUTING.md`、`.gitignore` 文字残留，越权未动，维持他轨登记），`i18n-sync-check.mjs` 已扫驱动包。相应改为「已落地」并给出实测出处。
  4. 2.7 与两份指南中「护栏将强制 / 待落地」措辞 → 改为「已由 R1/R2 强制」。
- 文档内新写符号/路径抽验：`src/lib/localeSync.ts:20,24`、`src/main.tsx:66`、`packages/ui/src/i18n.ts:34`、`src/lib/cn.ts`（1 行 `export { cn } from '@datazen/ui';`）、`src/commands/driver.ts:6-11`、`SettingsContent.tsx:91-100`、`lazyPacks.ts:21-34`、`domains.ts:21`、`builtinLocales.ts:9` / `:26-29`、驱动 `locales/` 各 10 语言文件 + `index.ts` → 全部 Read/ls 命中，零失配。

### 6. 验收标准对照

| # | 标准 | 证据 |
| --- | --- | --- |
| 1 | 还原态 exit 0，输出含扫描数/豁免命中数/过期豁免检测 | 上表第 1 行；过期豁免检测见单测 3 例（含真实 fs 一例） |
| 2 | 注入 R1(from)+R1(vi.mock)+R2 → 非 0 且逐条点名 `文件:行`；还原后 exit 0 | 「真实仓库自证」原样输出（3 violation(s)、EXIT=1 → 还原 EXIT=0） |
| 3 | `npx vitest run scripts` 全绿 + 覆盖数字如实登记 | 23 files / 239 tests；本轨 31 例；Lines 100% / Stmts 99.26% / Branches 95.42% |
| 4 | allowlist 仅 2 条已裁定夹具、无通配 | `ALLOWLIST` 常量 + 单测「shipped allow-list is exactly …」断言长度 2 / 无 `*` `?` |
| 5 | 四处接入齐备且拼写完全一致 | `git show b56058f46`；四处均为 `test:boundaries` / `scripts/check-driver-import-boundaries.mjs` |
| 6 | `tsc --noEmit` = 0；`vite build` = 0 | 自测表 |
| 7 | 既有守卫与既有测试语义不被改动 | `check-id-terminology` / `check-ci-docs-consistency` / `check-module-layers` 全绿；redis 27/222、host 412/4243、drivers 33/241 与基线一致 |
| 8 | 文档回扫：2.6 无「尚未确定」、2.1.2/2.7 与实测一致、3 Nit 关闭、21:21、抽验零失配 | 第 5 节 + `grep -rn "尚未确定\|同期落地\|34 处\|42 处"` 0 命中 + 标题计数 |

### 7. 开放项（请协调者裁定）

- **A. R2 对 `packages/ui/src/__tests__/i18n.test.tsx` 的文件级豁免**（原「整包 `packages/ui/**` 豁免」的表述已随 `43e042341` 收回，实现自始是文件级）：任务书原文把 R2 豁免写成「除 `packages/ui/src/i18n.ts` 的定义与自身导出行」，但实测该包单测 `packages/ui/src/__tests__/i18n.test.tsx` 有 7 处**必须存在**的 `setLocale` 调用（唯一 i18n 运行时的行为测试），按原文口径基准就会红 7 条，而又禁止新增 allowlist 条目。落地选择：把这**一个测试文件**与定义文件一起放进 `R2_FILE_CARVEOUTS`（精确文件清单，不是目录级豁免，也不占 allowlist 的 2 条额度）。若裁定连这一个文件也不该豁免，则需允许它进 `ALLOWLIST`（会突破「初始只允许 2 条」口径）或改写 `@datazen/ui` 的单测（属他轨交付面），请明示取舍。
- **B. R3 现状 4 处、暂为 advisory**（未擅自豁免、未搬迁代码）：
  - `src/test/driverUiSetup.ts:25,26` —— 驱动 UI 测试装配（经 meta 入口挂词条，注释即声明这是有意设计）；
  - `src/locales/locales.test.ts:107` —— 动态 `import('../../packages/drivers/redis/locales')` 验证驱动自注册；
  - `src/windows/connection/DocumentConnectionView.tsx:25` —— **生产宿主码** import 驱动内部 `packages/drivers/mongodb/ui/mongodbFind`（这一条是唯一涉及产品代码的，可能需要在 mongodb 侧另立公开入口或经 SDK 暴露）。
  另有 `src/styles/globals.css:10` 的 `@source '../../packages/drivers/*/ui';`（Tailwind 指令、非 import，R3 不覆盖，仅登记）。
  三种候选：(1) 保持 advisory 永久；(2) 三条测试/装配豁免进 allowlist（突破 2 条口径需你点头）+ 生产那条另立 Bug；(3) `RULES.R3.blocking=true`，同时开整改 Bug。当前脚本实现为 (1) 的形态，翻 `blocking` 一行即可切到 (3)。
- **C. 第 5 个接入点**：`scripts/run-regression.sh` 也单独跑了 `check-id-terminology.mjs`（第 218 行），任务书只要求 4 处，故未改；若希望回归脚本同步纳入 `pnpm test:boundaries`，一行即可，请裁定。
- **D. 提交方式说明**：`3e9014d91` / `b56058f46` 两次提交的 author 为仓库 git 配置、message 与实际改动一致（护栏+单测 / 四处接入），本文件与文档回扫在 `d250e52de`。工作区现无残留改动（`git status --short` 仅本文件）。**未 push。**

## 留待 R 回归

- **R1/R2 护栏纳入全量回归**：`pnpm test:boundaries` 已成 CI 阻断步骤，R 阶段无需额外手工验证；若 R 阶段任一轨触发它变红，属真实漂移，按本轨口径处理（不许扩豁免）。
- **R3 裁定后的收紧**：若裁定转 blocking，需回扫 2.6/2.7 措辞（把 advisory 改回阻断）并把 `RULES.R3.blocking` 翻 true，同时补 `DocumentConnectionView.tsx:25` 的整改轨道。
- **`scripts/run-regression.sh` 是否纳入新护栏**（开放项 C）。
- `AGENTS.md:31/232`、`CONTRIBUTING.md:91`、`.gitignore:64` 仍描述已退役的 `src/extensions/generated-locales.ts`（`i18n-drivers` 轨已登记给 `decouple-docs`/hub，本轨无权限改，维持原登记）。
- 两份驱动指南的 §6.2/§6.3 措辞本轮已随护栏落地更新；后续若 `WebContextMenuHost` 夹具豁免被移除（菜单挂载下沉进 SDK），须同步回扫 2.1.2 与 `ALLOWLIST`（预期从 2 条变 0 条，届时过期豁免检测会主动报错）。

## Tester 复测记录（Wave 4-A · 全新 Tester 实例）

Phase：**TEST_DONE(PASSED)**。复测于 worktree `.worktrees/datazen-import-guard`（分支 `feature/import-guard`，起始 HEAD `83434e73c`）。环境前置：`node scripts/resolve-drivers.mjs --codegen-only --drivers=all`（后切 `--drivers=basic` 复验）+ `node scripts/generate-builtin-locales.mjs`。

### 开工异常（已处置，不构成本轨缺陷）

开工 `git status --short` 非干净：存在**上一 Tester 会话中断残留**的未提交改动（`scripts/__tests__/check-driver-import-boundaries.test.mjs` +1 用例；复测中该会话残留进程还实时改写了 boundary.md/progress.md 的「31→32 例」口径）。逐项核验后**全部 reset 回 HEAD**：新增用例的断言（R2 精确文件豁免 + ui 包其它文件照样红）已由本人以**真实文件注入**独立验证（见下 R2/R3 段），不保留半成品用例，使复测口径与 Coder 提交（31 例/239 tests）严格对齐。

### 8 条验收标准逐条判定（全部实测）

| # | 标准 | 判定 | 实测证据 |
| --- | --- | --- | --- |
| 1 | 还原态 exit 0、输出含扫描数/豁免命中/过期豁免 | ✅ | `ok (1403 file(s) scanned · 0 blocking violation(s) · 4 advisory finding(s))` + `2 allow-listed reference(s) skipped`；`--drivers=basic` 档同样 exit 0 |
| 2 | 注入三形态 R1 + R2 必红点名 `文件:行`，还原必绿 | ✅ | **本人独立注入**（与 Coder 自证不同文件不同形态组合）：①`packages/drivers/redis/testerProbeR1a.ts:1` `from '../../../src/hooks/useI18n'` ②`packages/drivers/redis/ui/testerProbeR1b.ts:1` `vi.mock('../../../../src/stores/settingsStore')` ③`packages/drivers/testerProbeR1c.ts:1` 动态 `import('../../src/lib/cn')` ④同 ② 文件 `:2` 真实 `setLocale('en')` ⑤`packages/ui/src/testerCarveoutProbe.ts:1`（`@datazen/ui` **非豁免文件**调用 setLocale，裁定 A② 验证）——5 条全部点名报出，`FAILED: 5 violation(s)` EXIT=1；删除探针后 EXIT=0、`git status` 干净 |
| 3 | `npx vitest run scripts` 全绿 + 覆盖如实 | ✅ | **23 files / 239 tests**（与自报一致）；本轨文件单独覆盖 Lines 100 / Funcs 100 / Stmts 99.26 / Branch 95.42；未覆盖 **statement 仅 2 处**（json 解析实测：`walk()` `:430`/`:436` 目录过滤 continue），真实 fs 分支（默认 root `runCli` 扫全仓 + 真实 fs 过期豁免）已有 2 例单测覆盖，非「只测虚拟树」 |
| 4 | allowlist 恰好 2 条、无通配、过期豁免 exit 1 | ✅ | 常量逐字段核对 = `redisKeyWebContextMenu.test.tsx` 的 `WebContextMenuHost`（实测该文件 `:5`）与 `useContextMenuStore`（`:9`）说明符逐字节一致；**真实 fs 探针**（node -e 注入 2 条假条目）：文件不存在→`the file no longer exists`、文件存在未命中→`no matching violation was found`，各自报出且 PROBE_EXIT=1 |
| 5 | 四处接入一致、CI 失败即阻断 | ✅ | `package.json:94` `test:boundaries=node scripts/check-driver-import-boundaries.mjs`；ci.yml `:67-68` 步骤名与文档 2.6 表格逐字一致，位于 `Guard version consistency` 后、i18n warning 前，**无** `continue-on-error`（`:72` 属既有 i18n 步骤）；ci-local.sh `:64-65`、run-full-automation-test.sh `:78` 均 `pnpm test:boundaries \|\| fail` |
| 6 | tsc 双档 = 0；vite build = 0 | ✅ | `--drivers=all` 与 `--drivers=basic` 分别 `npx tsc --noEmit -p tsconfig.json` exit 0；`npx vite build` exit 0（仅既有 >500kB chunk 提示） |
| 7 | 既有守卫/测试零回归 | ✅ | redis UI **27/222/0**；全驱动 **33/241**；`src packages/driver-sdk packages/ui` **412/4243**；`check-id-terminology`（1719 files）/ `check-module-layers`（3 rules）/ `check-ci-docs-consistency` 全 exit 0，与本轨基线口径逐项一致 |
| 8 | 文档回扫事实性 | ✅ | 2.6 无「尚未确定」；2.1.2/2.7 基线 = **生产码 0 / 夹具 2**，旧 34/42 口径仅以「历史上登记过」措辞出现；42 处路径/行号引用逐条抽验零失配（含 `i18n.ts:34`、`localeSync.ts:20/24`、`main.tsx:66`、`meta.ts:4`×2、`i18n-sync-check.mjs:33/34/36/318`、`index.ts:47-50/78/82`、`lazyPacks.ts:21-34`、`domains.ts:21`、`SettingsContent.tsx:91-100`、O-1 三档数字与 i18n-drivers/bugs.md 一致）；3 条 decouple-docs Nit 全部关闭；两份指南标题 zh `#`1/`##`13/`###`7 = en 同 = **21:21**；R2 豁免措辞（2.4.2/2.6/两份指南 §6.2-6.3）与 `R2_FILE_CARVEOUTS` 实现一致，无「整包豁免」残留 |

### 覆盖越界与开放项核验

- `git diff -M 8b66586e4..HEAD --stat`：仅 10 个文件（护栏+单测+四处接入+3 文档+本 progress），逐 commit 核对**无**驱动/宿主业务码、无 `packages/ui/src/i18n.ts`、无 `src-tauri/**`、无 codegen 产物、无 hub.md/他轨文件。
- 裁定 A：`R2_FILE_CARVEOUTS` 实测为两个精确文件（非 `packages/ui/**`）；注入验证 ui 包其它文件照样红（上表 #2⑤）。契约 2.4.2 措辞一致。
- 裁定 B：R3 实测恰报 4 处（`locales.test.ts:107`、`driverUiSetup.ts:25,26`、`DocumentConnectionView.tsx:25`）且 advisory 不影响 exit 0；`DocumentConnectionView.tsx:25` 在**基准 `8b66586e4`** 用 `git cat-file` 核实真实存在；2.6/2.7 与本文件开放项 B 均明示其为待整改项，未隐藏。
- 裁定 C/D 按协调者指示不纳入判定。

### Tester 结论与遗留

- **无 Blocker，未登记 bugs.md**（本轨目录仅 progress.md）。
- E2E 登记表：本轨纯静态脚本/CI，无 UI 交互路径，**不适用**。
- **Nit（可留 Wave 4-B，不阻断）**：
  1. 契约 2.6 单测格「未覆盖部分全部位于 `walk()` 的目录遍历过滤器」——对 **statement** 成立（实测未覆盖语句恰为 `:430/:436`），对 **branch**（95.42%）不完整：另有 `:261/:291/:365/:476/:477/:570` 六处 `??`/默认参数兜底分支未触发，建议措辞补全（Coder progress §4 本身口径是对的）。
  2. `ci-local.sh` 新步 `3.4/11` 排在了既有 `3.3/11`（i18n warning）之前，执行顺序与 ci.yml 一致但编号观感倒置，纯排版。
- **环境限制备忘（非缺陷）**：本 worktree 无独立安装态，任何 `pnpm test:*` 均触发 deps-check 且无 TTY 报 `ERR_PNPM_ABORTED_REMOVE_MODULES_DIR_NO_TTY`（既有 `pnpm test:ids` 复现完全相同）；新接线与既有 guard 同构，CI 首步 `pnpm install` 后即正常。

Tester 复测 commit：见本文件所在提交（`git log -1 --format=%h`）。**未 push。**

## BUG-008 接管实施记录（Takeover / Rescuer，Wave 4-A）

Phase：**READY_FOR_TEST**（待全新 Tester 复测）。分支 `feature/import-guard`，开工 HEAD `17e51023c`；修复 commit：`17c3d5cc7`（脚本+单测）/ `485d75797`（契约文档回扫+2 Nit）/ 本 commit（本轨记录）。**未 push。**

### 0. 现场盘点判定（前任崩溃代理遗留）

`git status --short` 仅两个未提交文件，`git diff` 逐行复核与协调者盘点结论**一致，全部保留**：

- `scripts/check-driver-import-boundaries.mjs`（+66/−3）：`createGitIgnorePredicate`（`git check-ignore -q --` 逐**违规文件**调用、Map 缓存、任何 git 失败按 tracked = fail-closed）、`EXTERNAL_ADVISORY_NOTE` 导出、`opts.isIgnored` 注入点（虚拟树默认全 tracked）、分类插入点 `finding.rule.blocking && isIgnored(finding.file)`、advisory 输出后缀、头注释与退出码语义重写。**`ALLOWLIST`（2 条）/ `R2_FILE_CARVEOUTS`（2 精确文件）/ `RULES.R3.blocking=false` 逐字节未动**（裁定第 2、3 条满足）。无截断函数、无夹带改动。
- `scripts/__tests__/check-driver-import-boundaries.test.mjs`（+67）：`describe('tracking-scope classification (BUG-008)')` 恰 5 例（①ignored 树 R1/R2 降 advisory+exit 0 ②tracked 下同违规 blocking+exit 1 ③虚拟树默认全 tracked ④真实仓 predicate 命中并缓存 ⑤git 不可用 fail-closed）。
- 主检出运行证据本人独立复跑一次，与协调者预采集输出逐字一致（见 §2）。

### 1. 改动文件清单（本轮接管新增部分）

| 文件 | 性质 |
| --- | --- |
| `scripts/check-driver-import-boundaries.mjs` + `scripts/__tests__/…test.mjs` | 保留前任实现，`17c3d5cc7` 提交 |
| `docs/development/driver-api-dependency-boundary.md` | 2.1.2 末段 / 2.4.2 新段 / 2.6（本地等价行、单测格、新增「阻断范围（BUG-008）」bullet、退出码行）/ 2.7 第一条，`485d75797` |
| `scripts/ci-local.sh` | Nit②：boundaries 步 `3.4/11→3.3/11`、i18n warning `3.3/11→3.4/11`（仅编号对调，执行顺序不变、与 ci.yml 一致），`485d75797` |
| 本文件 + 本轨 `bugs.md` | BUG-008 状态 → 待复测，本 commit |

未触碰：两个外部仓（`packages/drivers/superset`、`packages/pro-extensions/**` 全程只读）、`packages/ui/src/i18n.ts`、`src-tauri/**`、`Cargo.lock`、hub.md、他轨台账；codegen 产物未提交（`git status` 全程仅预期文件）。

### 2. 验收增补三条的真实输出

**(a) 主检出（--drivers=all + Pro 已 stage）→ exit 0、12 advisory**（前置 `node scripts/resolve-drivers.mjs --codegen-only --drivers=all` 于 worktree，CODEGEN_EXIT=0）：

```console
$ node scripts/check-driver-import-boundaries.mjs --root=/Users/wuxiaolong/code/rust-projects/datazen
[check-driver-import-boundaries] 2 allow-listed reference(s) skipped
[check-driver-import-boundaries] R1 (advisory) packages/drivers/superset/ui/SupersetConnectionFields.tsx:3: resolves to host src/hooks/useI18n · external (untracked) repo — contract drift to be fixed in that repo, not here
[check-driver-import-boundaries] R1 (advisory) packages/drivers/superset/ui/SupersetSchemaTree.tsx:19: … · external …
[check-driver-import-boundaries] R2 (advisory) packages/pro-extensions/sql-editor-pro/src/intentions/__tests__/intentionCodeActions.test.ts:119: … · external …
[check-driver-import-boundaries] R2 (advisory) …/intentionCodeActions.test.ts:141 / locales/__tests__/locales.test.ts:28,32,36,39（共 6 处，逐条点名，形态同上）
[check-driver-import-boundaries] R3 (advisory) src/locales/locales.test.ts:107 / src/test/driverUiSetup.ts:25,26 / src/windows/connection/DocumentConnectionView.tsx:25（4 处，无 external 后缀）
[check-driver-import-boundaries] ok (1489 file(s) scanned · 0 blocking violation(s) · 12 advisory finding(s))
MAIN_CHECKOUT_EXIT=0
```

R1×2 + R2×6 + R3×4 = **12 advisory**，与协调者订正口径一致。worktree 自身默认 root：`ok (1403 file(s) scanned · 0 blocking · 4 advisory)` EXIT=0（worktree 无外部树，属预期盲区，见记忆 worktree-external-tree-blindspot）。

**(b) 反证：降级没把口子开进本仓**——在 `packages/drivers/redis/ui/testerProbe.ts`（本仓跟踪目录、未被 ignore）注入 R1：

```console
$ git check-ignore -q -- packages/drivers/redis/ui/testerProbe.ts; echo $?   # 1（非 ignored）
$ node scripts/check-driver-import-boundaries.mjs
[check-driver-import-boundaries] R1 packages/drivers/redis/ui/testerProbe.ts:1: resolves to host src/hooks/useI18n
    import { useI18n } from '../../../../src/hooks/useI18n';
[check-driver-import-boundaries] FAILED: 1 violation(s) (1404 file(s) scanned · 4 advisory finding(s))
    see docs/development/driver-api-dependency-boundary.md §2.1.2
PROBE_EXIT=1
$ rm packages/drivers/redis/ui/testerProbe.ts && node scripts/check-driver-import-boundaries.mjs; echo $?
[check-driver-import-boundaries] ok (1403 file(s) scanned · 0 blocking violation(s) · 4 advisory finding(s))
0    # git status 恢复仅预期文件
```

**任务书勘误（需协调者知悉）**：任务书示例探针写 `from '../../../src/hooks/useI18n'`（三级），从 `packages/drivers/redis/ui/` 出发解析落点是 `packages/src/…`，**不构成 R1 违规**（首跑确实 exit 0，属正确行为而非降级漏洞）；反证必须用四级 `../../../../src/hooks/useI18n`。已按四级深度完成反证并在此登记。

**(c) `npx vitest run scripts` 全绿 + 数字如实**：**23 files / 244 tests**（原 239 + BUG-008 新增 5）；本轨单测文件单独跑 **36 passed**（31→36）。覆盖（`--coverage.include='scripts/check-driver-import-boundaries.mjs'`，json 逐条解析）：**Lines 100% / Stmts 99.31% / Branch 95.72% / Funcs 100%**。未覆盖语句恰 2 处 = `walk()` 目录过滤器 `:444`/`:450`；未触发分支共 8 处 = 上述 2 continue + 6 处 `??`/默认参数兜底（`:275`/`:305` 转义符在文件末尾、`:379` 取行文本、`:524`/`:525` `opts.log`/`opts.error`、`:630` `opts.argv`）——**Tester Nit① 的行号已按改动后实测重定位**（旧 `:261/:291/:365/:476/:477/:570` 系脚本变更前坐标）。

### 3. 其余自测清单（真实命令与结果）

| 命令 | 结果 |
| --- | --- |
| `npx tsc --noEmit -p tsconfig.json` | **exit 0** |
| `npx vite build` | **exit 0**（`✓ built in 5.07s`，仅既有 chunk 体积提示） |
| `npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui` | **27 files / 222 pass / 0 fail**（= 基线） |
| `npx vitest run --config vitest.drivers.config.ts`（全驱动） | **33 files / 241 pass / 0 fail**（= 基线） |
| `npx vitest run src packages/driver-sdk packages/ui` | **412 files / 4243 pass / 0 fail**（= 基线） |
| `node scripts/check-id-terminology.mjs` | ok（1719 files，5 处既有豁免） |
| `node scripts/check-module-layers.mjs` | ok (3 rules) |
| `node scripts/check-ci-docs-consistency.mjs` | drivers / window boundaries / toolchain 三项 ok（步骤编号对调不影响其校验口径，已在改后复跑） |

### 4. 文档回扫核实纪律

契约新登记的每个路径/行号均为实测：外部漂移 8 处文件:行来自上表 (a) 的真实脚本输出（`--root` 主检出扫描）；覆盖未触发行号来自 coverage-final.json 解析并对当前脚本逐行 Read 比对；`ci-local.sh` 编号引用与实文件一致（grep `3.3/11|3.4/11` 全仓复扫零残留旧口径，历史 progress 章节按「只追加不覆盖」原则保留旧文字）。

### 5. 需协调者裁定的新发现

1. **任务书探针深度示例错误**（见 §2(b) 勘误）：不影响修复正确性，但后续 Tester 复测请按四级 `../` 注入，否则会误判「降级开了口子」。
2. **fail-closed 语义延伸确认**：`git check-ignore` 判定的是「是否被 ignore」，故**新建未跟踪但未被 ignore** 的文件（如探针）同样按本仓源码 blocking——与裁定「作用域按是否被本仓跟踪判定」的字面（tracked-vs-untracked）略有出入，但方向更严格、不放水，判为符合裁定精神，单测例③④⑤已固化该行为。
3. `ci-local.sh` 既有 `3.25/11`（version consistency）奇数编号系本轨之前遗留风格，未动。

## BUG-008 第 2 轮复测记录（Tester 实例 `be50383b…` 实测 · 协调者自 transcript 恢复）

**为什么这一节由协调者执笔**：该 Tester 是 BUG-008 修复后的全新独立实例，08:5x 起在 worktree
`fda61f690` 上跑完 **66 次工具调用、10 项核查全部执行完毕**，最后一步（把记录写进 `progress.md`）
时遭遇网络中断（`Unable to connect to the service, or the connection was interrupted.`，09:06），
正文随之丢失，工作区只留下一行指向不存在章节的 `TEST_DONE(PASSED)` 勾选。协调者当时的处置是
「无正文的结论不采信」并重派全新实例；重派实例只跑到读文档阶段（6 次工具调用，同样中断），
于是改为**从该 Tester 的会话 transcript 逐条恢复其原始命令与输出**——下表每一项都是它本人的实测，
不是协调者重跑，也不是它继承的上一轮结论。

**协调者独立交叉核对**（合并到基准分支 `b22b41ac8` 之后，在主检出重跑最承重的三条门禁，10:15）：
主检出 `node scripts/check-driver-import-boundaries.mjs` → **EXIT=0 / 1489 files / 0 blocking / 12 advisory
逐条点名**；`npx vitest run scripts` → **23 files / 244 tests 全绿**；`npx tsc --noEmit -p tsconfig.json` → **0 error**。
三条与被恢复的记录完全一致，故接受其 PASSED 判定。

### 10 项核查逐条（恢复自 transcript 的原始输出）

| # | 核查 | 该 Tester 实测（原文摘要） | 判定 |
| --- | --- | --- | --- |
| 1 | 主检出全量复现（BUG-008 症状消失） | `--root` 指主检出 → `ok (1489 file(s) scanned · 0 blocking violation(s) · 12 advisory finding(s))`，EXIT=0；advisory 构成 = superset R1×2 + editor-pro R2×6 + R3×4，逐条带 `external (untracked) repo — contract drift to be fixed in that repo, not here`（R3 四条为既有 advisory，无该后缀） | ✅ 8 条红 → 0 阻断 / 12 advisory，与验收附加条款「advisory 总数 12」一致 |
| 2 | worktree 侧不回退 | basic codegen 下 `ok (1403 file(s) scanned · 0 blocking violation(s) · 4 advisory finding(s))`，EXIT=0 | ✅ |
| 3 | **反证：降级没有放水**（四级 `../` 注入） | 本仓跟踪文件注入后 `R1 packages/drivers/redis/ui/tester2Forms.ts:1/2/3`（`from` / `vi.mock` / 动态 `import()` 三形态同批命中）照样 blocking、EXIT=1；`tester2ProbeDeep.ts`（四级 useI18n）+ `tester2ProbeDeep2.ts`（五级 cn）→ `shallow_ignored_exit=1 (1=not ignored/tracked-scope)`；还原后 `git status` 空、EXIT=0 | ✅ 阻断仍生效；采纳接管记录的「探针必须四级」口径，三级 `packages/src/...` 反例已单独验过为 0 命中 |
| 4 | 分类按「是否被 ignore」而非目录白名单 | 在 gitignored 外部树里现造探针：`olapclone_ignored=0 proext_ignored=0` → `packages/drivers/olapclone/ui/tester2Ext.ts` R1、`packages/pro-extensions/tester2probe/src/tester2Ext.ts` R2 **都只报 advisory 且点名**；未跟踪但未被 ignore 的临时文件按 tracked 处理 | ✅ 探针全部清理（协调者复核：`packages/drivers/olapclone`、`packages/pro-extensions/tester2probe` 均已不存在，全仓 `tester2*` 零残留） |
| 5 | fail-closed 语义 | 主检出把 `git` 从 PATH 摘掉 → `EXIT(git missing, main checkout) = 1`（无法判定即按 tracked → blocking，门禁不被环境损坏放松）；对非 git 目录跑 → `fatal: 不是 git 仓库` + EXIT=1 | ✅ |
| 6 | `git check-ignore` 不在遍历热路径 | PATH 垫片计数：一次干净的全量扫描 **total git invocations: 4**，参数逐条为 4 个*违规文件*的 `check-ignore -q -- <file>`（superset×2 + editor-pro×2 文件），1489 文件耗时 1.756s | ✅ 违规数级别而非法文件数级别，符合裁定 |
| 7 | 豁免额度未膨胀 | 断言 `ALLOWLIST` 恰 2 条且全为 R1 + `redisKeyWebContextMenu.test.tsx`、无 glob；`R2_FILE_CARVEOUTS` 恰 2 个精确文件 | ✅ superset / editor-pro 未占用任何豁免位（与裁定一致） |
| 8 | 过期豁免双向检测仍红 | `file-missing EXIT=1`（`gone.test.tsx` 指向不存在文件）；`no-match EXIT=1`（`decoupledAlready` 已无对应违规） | ✅ 第 1 轮能力未回退 |
| 9 | 单测与覆盖如实 | 本轨文件单独跑 **36 passed**；`npx vitest run scripts` **23 files / 244 tests** 全绿；覆盖 json 逐条解析：**未覆盖语句恰 2 处 = `:444`/`:450`（`walk()` 目录过滤 continue）**，未触发分支 8 处 = 上述 2 + `:275`/`:305`/`:379`/`:524`/`:525`/`:630` 的 `??`/默认参数兜底 | ✅ 与契约 2.6 单测格登记的行号**逐字一致**（接管记录重定位的坐标成立） |
| 10 | 基线零回归 | `tsc --noEmit` 0（`--drivers=all` 与 `basic` 两档）；redis UI **27 files / 222**；全驱动 **33 / 241**；宿主 **412 / 4243**；`npx vite build` EXIT=0；`check-id-terminology` / `check-module-layers` / `check-ci-docs-consistency` 全 EXIT=0 | ✅ |

### 越界与只读性核验

- `git diff -M 8b66586e4..HEAD` 仅 11 个文件（护栏脚本 + 其单测 + 5 个接入/文档 + 本轨 progress/bugs），
  `packages/ui/src/i18n.ts` 与 `src-tauri/**` 未出现在 diff 中；未提交任何 codegen 产物；未触碰 `hub.md`。
- 两份驱动指南标题数 `zh-CN 21 / en 21`，逐行 diff 完全一致（结构未偏斜）。
- 外部仓只读：探针只建在 gitignored 目录且事后删除；主检出 `git status` 全程仅既有脏文件。

### 遗留观察（登记事实，不构成本轨缺陷，交协调者裁定）

1. **`packages/drivers/superset` 自身有未提交改动**：`M ui/SupersetConnectionFields.tsx`、`M ui/SupersetSchemaTree.tsx`、
   `M ui/plugin-meta.ts`、`?? .DS_Store` —— 前两个正是 2 条 R1 advisory 的所在文件。属该外部仓自己的漂移，
   按裁定由该仓整改，本仓不改（移交项见 2.4.2 / `bugs.md` 裁定第 6 条）。
2. **`npx prettier --check` 对护栏脚本与其单测报 warn**，但同批对既有 `check-id-terminology.mjs` 等仓库内多个
   脚本一并报 warn，且 CI 无 prettier 门禁 → 判定为全仓既有风格欠账，**非本轨回归**，未登记 Bug。

### 结论

**TEST_DONE(PASSED)**：`import-guard-BUG-008` 的三条验收增补与第 1 轮 8 条验收全部实测通过，
无新增 Bug，本 Tester 未改 `bugs.md`（状态流转归协调者）。协调者据此完成第二次合流 `b22b41ac8`。
