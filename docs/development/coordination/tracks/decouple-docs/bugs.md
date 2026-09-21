# Track: decouple-docs — Bug 清单

> Tester 独立复测（被复测 commit `6199d9d95`，基准 `d172476fc`）。只测不修；以下缺陷全部由 Tester 自己实测证实，出处均为本 worktree 实际文件内容。

## 汇总

| Bug ID | 严重度 | 文档位置 | 一句话 | 状态 |
| --- | --- | --- | --- | --- |
| `decouple-docs-BUG-001` | 中 | `driver-api-dependency-boundary.md` 2.1.2 过渡期例外 | 声明「除两点外不存在任何豁免」，但 sqlserver 驱动存在同类宿主 import | 已修复（第 2 轮复测通过） |
| `decouple-docs-BUG-002` | 低 | 同上 2.4.3 配套终态第 2 条 | 用 `BUILTIN_LOCALES` 佐证 `pt-BR` 连字符，但该常量不含 `pt-BR` | 已修复（第 2 轮复测通过） |
| `decouple-docs-BUG-003` | 低 | 同上 2.2 决策表行 1 先例列 | `src/lib/driverSettings.ts` 作为「薄再导出先例」引用，实际文件已不存在 | 已修复（第 2 轮复测通过；残留同类问题另立 BUG-005） |
| `decouple-docs-BUG-004` | 低 | 本 track `progress.md` Coder 自验第 6 条 / B 表 | 自报 zh/en 各 24 个标题，实测各 21 个（结构对应本身通过） | 已修复（第 2 轮复测通过） |
| `decouple-docs-BUG-005` | 低 | `independent-driver-development.zh-CN.md:196` / `.en.md:211`（§6.2） | 仍无条件声称「宿主原路径仅剩薄再导出」并把 `driverSettings`、`resolveEditorFontFamily` 列为该形态先例，实际二者宿主文件均已不存在，与修复后的契约 2.2/2.5 三分规则互相矛盾 | 已修复（第 3 轮复测通过） |
| `decouple-docs-BUG-006` | 低 | `driver-api-dependency-boundary.md:295-299`（2.4.3「真值分三层」） | 三层只描述宿主侧接线集（en/zh-CN）与「其余 8 语言生产路径无运行时 import」，未对表 O-1 裁定下「驱动自注册把全部 10 语言灌入共享注册表、启动即进 main chunk」的运行时集合，读者会误判全仓只有 2 语言进 bundle 或以为驱动应只注册 2 语言 | 已修复（第 4 轮复测通过） |
| `decouple-docs-BUG-007` | 低 | 同上 2.4.3 词条归属表「驱动词条」行 + 指南 §6.3（zh:214 / en:229） | 副作用行示例字面写 `import '../locales';`，却同时点名 redis 入口为 `ui/shared/meta.ts`（嵌套两层），该目录下正确写法只能是 `import '../../locales';` | 已修复（第 4 轮复测通过） |

> **Coder 第 1 轮修复说明（状态推进：待修复 → 待复测）**：4 条判定经本轮独立实测**全部成立，无反驳项**，已按建议方向修正。改法与实测事实逐条见 `progress.md`「Coder Bug 修复记录（第 1 轮）」。BUG-001 额外登记了 Tester 未列的 **8 处 `vi.mock` 宿主 `useI18n` 路径**（非 `from` 形态、不被重现命令命中），供 Wave 4 护栏白名单口径复核。状态由复测 Tester 推进为「已修复」，Coder 不自行判定。
>
> **Coder 第 2 轮修复说明（BUG-005，状态推进：待修复 → 待复测）**：判定经本轮独立实测**成立，无反驳项**（`src/lib/driverSettings.ts`、`src/lib/resolveEditorFontFamily.ts` 实测均不存在，SDK 侧两文件均在，宿主两个消费点均直连 `@datazen/driver-sdk`）。已把两份指南 §6.2 该条改为与契约 2.2/2.5 同一套三分规则，并换用实测存在的薄壳先例；改法与自验输出见 `progress.md`「Coder Bug 修复记录（第 2 轮）」。状态由第 3 轮复测 Tester 推进为「已修复」，Coder 不自行判定。
>
> **Coder 第 3 轮修复说明（BUG-006 / BUG-007，状态推进：待修复 → 待复测）**：两条判定经本轮独立实测**全部成立，无反驳项**。BUG-006 按用户 2026-09-21 裁定（驱动全量 10 语言自注册、接受包体代价）在 2.4.3 补写「宿主接线集 vs 驱动注册集」的不对称对表句并显式限定第 ② 层的适用范围，未提出任何惰性/按需注册建议；BUG-007 按并行轨实测 import 串改正为「相对层级随入口深度而定」并分别举例（`ui/meta.ts` → `'../locales'`、`ui/shared/meta.ts` → `'../../locales'`），契约与两份指南 §6.3 三处同改。改法、对照表与实测出处见 `progress.md`「Coder Bug 修复记录（第 3 轮）」。状态由第 4 轮复测 Tester 推进，Coder 不自行判定。
>
> **接管更正（同轮，第 3 轮原 Coder 中断后由新实例复核）**：现场遗留改动经逐行实测复核，`:289` / `:297` 分例与限定语、`:299` 不对称主体与 O-1 数字、两份指南 §6.3 同步、`bugs.md` 状态推进均**复核为真并保留**；但 `:299` 末句把「某语言运行时是否可达」的判据写成「宿主 `BUILTIN_LOCALES` **与 `src/locales/lazyPacks.ts` 的接线集合**」不精确——`lazyPacks.ts:21-34` 的类型键域只有 `en` / `zh-CN`、值只有宿主 4 个惰性域包（`src/locales/domains.ts:21`），驱动 eager 词条不经它装载，且该写法与同段「不做惰性/按需注册」自相矛盾；同时遗漏真实接入路径 `src/locales/index.ts:42-45 registerLocale()` → `:51-56 getExtensionLocales()` → 设置页 `src/windows/settings/SettingsContent.tsx:91-100`。已改为「`BUILTIN_LOCALES` 并上 `registerLocale()` 注册语言」并把 lazyPacks 单列为「只服务宿主词条」；另把「也不做惰性 / 按需注册」强化为「本契约不要求、不建议…O-1 裁定明令禁止引入该机制」，消除被反向读成建议的空间。全部改动仍在 2.4.3 纯文本范围内，未新增标题。取舍明细与自验输出见 `progress.md`「Coder Bug 修复记录（第 3 轮 · 接管）」§一/§三。

---

## decouple-docs-BUG-001（中）— 过渡期例外清单漏登记 sqlserver

- **文档写**：`docs/development/driver-api-dependency-boundary.md:178-180`「**过渡期例外（截至本文件基准）**：1. `packages/drivers/redis/ui/**` 仍有部分文件经宿主相对路径 import `useI18n`…；2. `packages/drivers/redis/ui/__tests__/redisKeyWebContextMenu.test.tsx`…**除上述两点外不存在任何豁免。**」
- **代码实为**：`packages/drivers/sqlserver/ui/ConnectionFields.tsx:2` 同样从宿主相对路径 import `useI18n`：

  ```ts
  import { useI18n } from '../../../../src/hooks/useI18n';
  ```

  该文件是 git 跟踪的普通驱动 UI 源码（`git ls-files` 命中），既不在 `packages/drivers/redis/ui/**` 之下，也不是被豁免的测试夹具，因此「不存在任何豁免」为假。
- **重现命令**：

  ```bash
  grep -rn "from '\.\./.*src/" packages/drivers/*/ui/ | grep -v "^packages/drivers/redis/"
  # packages/drivers/sqlserver/ui/ConnectionFields.tsx:2:import { useI18n } from '../../../../src/hooks/useI18n';
  ```

- **交叉证据（正确事实的代码/文档出处）**：并行轨 `i18n-drivers` 自己的任务书就已把它计入换源范围——`.worktrees/datazen-i18n-drivers/docs/development/coordination/tracks/i18n-drivers/progress.md:28`：「已盘点范围（基准 d172476fc）：redis ui 31 处 + `packages/drivers/sqlserver/ui/ConnectionFields.tsx` 1 处」。实测 redis 侧 `useI18n` 宿主相对 import 命中 **31 个文件**，与并行轨盘点完全一致，故仅本契约文档少写一条。
- **影响范围**：本文件是「依赖边界的唯一规范落点」。2.7 自查清单写作「驱动 UI 无任何 `.../src/` 形态宿主 import（2.1.2 登记的过渡期例外除外）」——Reviewer 依据该基线会把 sqlserver 判为「新增违规」而阻断无关 MR；Wave 4 import 护栏若照 2.1.2 清单实现豁免白名单，则会漏配该文件（要么护栏落地即红，要么白名单与规范不一致）。
- **建议修复方向（供 Coder 参考，非 Tester 实施）**：把例外 1 的适用范围从 `packages/drivers/redis/ui/**` 扩为「`packages/drivers/*/ui/**` 现存 32 处宿主 `useI18n` 相对 import（redis 31 + sqlserver 1）」，或显式并列两处，并与 `i18n-drivers` 轨盘点口径对齐。
- **Coder 处理（第 2 轮复测通过 → 已修复）**：2.1.2 已改为「命令 + 计数」式基线（32 处 `useI18n`（redis 31 + `sqlserver/ui/ConnectionFields.tsx:2`）+ 2 处测试夹具 = 34 行命中）并同步 2.7 判据；实测 redis 31 处目录分布见 `progress.md` 修复记录，与 `i18n-drivers` 轨盘点口径一致。另补登记 Tester 未覆盖的 8 处 `vi.mock` 宿主 `useI18n` 路径（护栏按 mock 扫描时基线 42 处）。

---

## decouple-docs-BUG-002（低）— `pt-BR` 连字符规则的佐证出处对不上

- **文档写**：`driver-api-dependency-boundary.md:286`「语言 code 字面量与宿主保持一致（`zh-CN`、`pt-BR` 带连字符，对照 `src/locales/builtinLocales.ts` 的 `BUILTIN_LOCALES`）」。
- **代码实为**：`src/locales/builtinLocales.ts:9` 为 `export const BUILTIN_LOCALES = ['en', 'zh-CN'] as const;`，`BUILTIN_LOCALE_LABELS`（同文件 :26-29）只有 `'en'` / `'zh-CN'`；**整个文件不出现 `pt-BR`**。
- **重现命令**：

  ```bash
  sed -n '9p' src/locales/builtinLocales.ts        # ['en', 'zh-CN']
  grep -n "pt-BR" src/locales/builtinLocales.ts    # no matches
  ```

- **正确事实的代码出处**：`pt-BR` 的连字符约定实由以下两处支撑——`scripts/i18n-sync-check.mjs:23`（`const LOCALE_FILES = ['de','es','fr','ja','ko','pt-BR','ru','zh-TW']`）与各包文件名 `src/locales/` / `packages/drivers/redis/locales/pt-BR.ts`、`packages/drivers/mongodb/locales/pt-BR.ts`。
- **影响范围**：结论（用连字符）本身正确，但读者按文档去 `BUILTIN_LOCALES` 核对 `pt-BR` 会找不到，属引用错配。建议把 `pt-BR` 的出处改为 `i18n-sync-check.mjs` 的 `LOCALE_FILES` / 现有语言文件名，或把括号内的举例拆成「`zh-CN`（`BUILTIN_LOCALES`）与 `pt-BR`（`LOCALE_FILES`）」。
- **Coder 处理（第 2 轮复测通过 → 已修复）**：2.4.3 该条已拆为三层——接线层仅 `en`/`zh-CN`（`builtinLocales.ts:9` + 真值源 `builtin-locales.json`，并点明 `fullLocales.ts` 也只含这两个）、parity 校验层其余 8 语言（`i18n-sync-check.mjs:23` `LOCALE_FILES`）、命名约定层 `pt-BR` 以 `LOCALE_FILES` 与文件名为出处；并加写「有语言文件 ≠ 宿主已接线」与驱动包 10 个语言文件的实测覆盖，杜绝 10 种语言全在线的误读。

---

## decouple-docs-BUG-003（低）— 决策表先例引用了已不存在的宿主路径

- **文档写**：`driver-api-dependency-boundary.md:188`（2.2 决策表行 1）落点为「下沉 `@datazen/driver-sdk`（**移动实现，宿主原路径改薄再导出**）」，先例列「`src/commands/driver.ts` → …；**`src/lib/driverSettings.ts` → `packages/driver-sdk/src/driverSettings.ts`**；`src/lib/nativeContextMenu.ts` → …」。
- **代码实为**：SDK 侧 `packages/driver-sdk/src/driverSettings.ts` 存在✓，但宿主侧 `src/lib/driverSettings.ts` **已不存在**（`ls` 报 No such file；`find src -iname "*driverSettings*"` 只剩无关的 `src/windows/settings/DriverSettingsSection.tsx`；全仓无任何 import 该路径），该模块是在 `92a039383 refactor(driver-sdk): sink pure/IPC modules and add capability bridges` 中整体移走的。
- **重现命令**：

  ```bash
  ls src/lib/driverSettings.ts            # No such file or directory
  find src -iname "*driverSettings*"      # 仅 src/windows/settings/DriverSettingsSection.tsx
  git log --oneline -1 -- src/lib/driverSettings.ts   # 92a039383
  ```

- **影响范围**：作为「宿主路径改薄再导出」的先例举证失效（另两个先例 `src/commands/driver.ts`、`src/lib/nativeContextMenu.ts` 实测确为薄再导出，故表格主结论仍成立）。建议把该行先例改为实际留存薄再导出的 `src/lib/cn.ts`、`src/lib/nativeContextMenu.ts`、`src/commands/driver.ts`、`src/commands/file.ts`，并注明「无宿主消费方时可直接移走、不留薄再导出（driverSettings 即此例）」。
- **Coder 处理（第 2 轮复测通过 → 已修复）**：2.2 行 1 先例列已换为 `src/lib/cn.ts`（实测整文件 1 行）、`src/lib/nativeContextMenu.ts:7-15`、`src/commands/driver.ts:6-11` 三个真壳，外加 `src/commands/file.ts:2/9`（实测为与 SDK `fileCommands` **合并再导出**、宿主另留 host-only 命令，故按实际形态标注为「合并」而非纯薄壳）；`driverSettings` 从先例列改列为「整体移走不留壳」反例（`git log --oneline -1 -- src/lib/driverSettings.ts` = `92a039383`；全仓 `lib/driverSettings` 仅剩文档命中；旁证 `tracks/cap-bridge/progress.md:111` 本身即记载「移动 + 宿主消费点改为直接 import SDK」）。同一规则补齐到 2.1.2「唯一实现原则」与 2.5 流程第 2 步，避免只在表格里出现一次。

---

## decouple-docs-BUG-004（低）— Coder 自报标题计数与实测不符

- **文档写**：`docs/development/coordination/tracks/decouple-docs/progress.md` B 表末行「标题结构：zh/en 均 **24** 个 `#` 级标题、顺序一一对应」与自验第 6 条「zh/en 标题对照：各 **24** 个标题」。
- **实测**：两份文档 `^#` 级标题各 **21** 个（层级分布完全相同：`#`×1 + `##`×13 + `###`×7），序号与顺序 1:1 对应，**验收标准 4 本身通过**，仅自报数字失真。
- **重现命令**：

  ```bash
  grep -c "^#" docs/development/independent-driver-development.zh-CN.md   # 21
  grep -c "^#" docs/development/independent-driver-development.en.md      # 21
  ```

- **影响范围**：零生产影响，但 progress.md 是 Tester/Reviewer 的核对基线，失真数字会让后续复测误判「漏了 3 个标题」。建议随本轮修正文中数字（或改为「21 个，zh/en 一一对应」）。
- **Coder 处理（第 2 轮复测通过 → 已修复）**：`progress.md` B 表末行与自验第 6 条的 24 已改为 **21**（`#`×1 + `##`×13 + `###`×7，`####`×0），并写明计数口径＝行首 `#` 的 ATX 标题行、含 H1 主标题、不计表格 `#` 列与代码块注释；本轮另以 `paste` 逐行比对两份标题序列，确认 21:21 且顺序 1:1（验收标准 4 结论不变）。同批把自验表第 13 条对 `BUILTIN_LOCALES` 的错误断言就地标注作废（指向 BUG-002）。

---

## decouple-docs-BUG-005（低，第 2 轮复测新发现）— 指南 §6.2 仍无条件声称「下沉后宿主原路径仅剩薄再导出」

- **文档写**：`docs/development/independent-driver-development.zh-CN.md:196`「**纯函数 / IPC 封装**：实现下沉 `@datazen/driver-sdk`（全仓单实现，**宿主原路径仅剩薄再导出**）。已有先例：**`driverSettings`、`resolveEditorFontFamily`**、`ipc/driverCommands`、`ipc/fileCommands`、`nativeContextMenu`。」`.en.md:211` 同义（"the host path keeps only a thin re-export … precedents: `driverSettings`, `resolveEditorFontFamily`, …"）。
- **代码实为**：所列 5 个先例中 2 个**宿主侧连薄再导出壳都不存在**——
  - `src/lib/driverSettings.ts`：No such file（`92a039383` 整体移走，仅存 `packages/driver-sdk/src/driverSettings.ts`）；
  - `src/lib/resolveEditorFontFamily.ts`：No such file（同一 commit `92a039383` 移走；宿主消费点 `src/components/sql-editor/editorExtensions.ts:36-38` 直接 `from '@datazen/driver-sdk'` import `resolveEditorFontFamily`）。
- **与修复后契约自相矛盾**：契约 2.2 行 1 / 2.5 第 2 步（`driver-api-dependency-boundary.md:197/:326`，BUG-003 修复后）已确立三分规则「**有存量消费方改薄再导出、无消费方连文件删除**」并把 `driverSettings` 明确标注为「整体移走不留壳」反例；指南却仍用无条件句式并把 `driverSettings`、`resolveEditorFontFamily` 归入「薄再导出先例」，属 BUG-003 同类残留在并行文档面的复现（修复只改了契约文档，未同步两份指南）。
- **重现命令**：

  ```bash
  grep -n "宿主原路径仅剩薄再导出" docs/development/independent-driver-development.zh-CN.md   # :196
  ls src/lib/driverSettings.ts src/lib/resolveEditorFontFamily.ts                              # 均 No such file
  git log --oneline -1 -- src/lib/resolveEditorFontFamily.ts                                   # 92a039383
  grep -rn "resolveEditorFontFamily" src/ --include='*.ts*' | grep "@datazen/driver-sdk"       # 消费点直连 SDK，无宿主壳
  ```

- **影响范围**：指南是 git 驱动作者的第一入口文档；按该句照做的作者会为「无宿主消费方」的下沉能力徒留空壳文件（与 2.5 流程第 2 步相反），或按先例反查宿主壳而不得、连带怀疑契约 2.2 的可信度——正是 BUG-003 登记的失效模式。
- **建议修复方向（供 Coder 参考，非 Tester 实施）**：将 zh:196 / en:211 该括注改为与契约 2.2 一致的三分表述（「有存量消费方→薄再导出；无消费方→整体移走不留壳，如 `driverSettings` / `resolveEditorFontFamily`」），**zh/en 两份必须同步**（维持验收 4 的 21:21 结构，不新增标题）。
- **Tester 判定依据（实测日志摘录）**：`find src -iname "*driverSettings*"` 仅剩 `src/windows/settings/DriverSettingsSection.tsx`；`find src -name "resolveEditorFontFamily*"` 0 命中；`grep -n "薄再导出" docs/**` 全量清点确认其余文档（契约 2.1.2/2.2/2.5、components.md:575）均已带限定语，唯指南 6.2 两处（zh/en）为无条件句式。
- **Coder 处理（第 2 轮修复完成 → 待复测）**：zh:196 / en:211 两处已改写为与契约 2.2 行 1、2.1.2「唯一实现原则」、2.5 第 2 步同一套三分规则（「**有存量消费方时**只保留薄再导出 / **无消费方时**整体移走、不留空壳，消费点改为直接 import SDK」，并在句内指向「契约文档 2.2 / 2.5」）；先例拆成两组——**留壳**：`src/lib/cn.ts`（整文件 1 行）、`src/lib/nativeContextMenu.ts:7-15`、`src/commands/driver.ts:6-11`、`src/commands/file.ts:2/9`（合并再导出，另留 host-only 命令），**不留壳**：`driverSettings`、`resolveEditorFontFamily`（并写明 SDK 唯一实现路径、`92a039383` 移走、宿主消费点 `DriverSettingsSection.tsx:3` 与 `editorExtensions.ts:36-38` 直连 SDK）。两份 1:1 同步（20 个 code span 逐项相同、`#` 级标题仍为 21 : 21，未新增/删除标题）；契约主文档经全量 grep 复核无同类无条件句式残留，**未改动**。详见 `progress.md`「Coder Bug 修复记录（第 2 轮）」。
- **Tester 第 3 轮复测判定（commit `595f106dd`）→ 已修复**，逐句实测均成立、无反驳项：
  1. 两份该段已与契约 2.2 行 1 / 2.5 第 2 步 / 2.1.2「唯一实现原则」三分规则一致（留壳条件、不留壳条件、合并再导出形态单列），并句内指向契约 2.2 / 2.5；
  2. `grep -rn "宿主原路径仅剩\|仅剩薄再导出\|keeps only a thin re-export"` 在 5 份文档中 **0 命中**（`../../../src/` 之外无残留无条件句式；`components.md:575` 另带「存量 import 兼容」限定语）；
  3. 该段点名的路径存在性逐个 `test -f`/`wc -l` 复核：`src/lib/cn.ts`（EXISTS，1 行且内容正是文中引句）、`src/lib/nativeContextMenu.ts`（EXISTS，15 行，:7-15 为值+类型再导出块）、`src/commands/driver.ts`（EXISTS，11 行，:6-11 再导出块）、`src/commands/file.ts`（EXISTS，92 行，:2 import SDK `fileCommands as dialogFileCommands`、:9 类型再导出、:17-18 与 host-only 命令合并）均为壳/合并形态；`src/lib/driverSettings.ts`、`src/lib/resolveEditorFontFamily.ts` **均 ABSENT**，且文中把二者正确归入「整体移走不留壳」组；
  4. 支撑事实复核：SDK 侧 5 个实现文件均在（`packages/driver-sdk/src/{driverSettings,resolveEditorFontFamily,nativeContextMenu}.ts`、`src/ipc/{driverCommands,fileCommands}.ts`）；两个宿主路径的最后改动 commit 均为 `92a039383`；宿主消费点 `src/windows/settings/DriverSettingsSection.tsx:3`、`src/components/sql-editor/editorExtensions.ts:36-38` 直连 `@datazen/driver-sdk`；全仓 `src/`+`packages/` 代码（含 tsx/ts）对 `lib/driverSettings` / `lib/resolveEditorFontFamily` 的 import **0 命中**；契约 2.2 另举的消费点 `src/windows/settings/JsonSchemaSettingsForm.tsx:1-6`（`applySchemaDefaults` / `listSchemaPropertyEntries` / `readBooleanField`）实测符号确住在 `packages/driver-sdk/src/driverSettings.ts:19/41/63`；
  5. zh/en 同步：两份该行 code span **20 : 20 逐项完全相同**、反引号各 40；按小节扩展比对（6.1/6.2/6.3 全段）→ 三份集合差仅 en 侧多出 `` `dialog` `` / `` `confirm` `` 两个代码块注释词（中文同行未加反引号，事实相同），**无任何单边新增事实**；标题结构仍 21 : 21（`#`×1 / `##`×13 / `###`×7，层级序列与编号前缀 `diff` 均为空）。
- **残留（不阻断本条判定）**：该段把「条件」写在了「keeps only a thin re-export」之后（en:211 同句内以加粗 `when legacy host consumers still import it` 收口），属可读性小瑕疵而非事实错误；本轮不登记，建议随 BUG-006/007 的 2.4.3/§6.3 改动一并考虑是否调序。

---

## decouple-docs-BUG-006（低，第 3 轮复测新发现）— 2.4.3「真值分三层」未对表并行轨 O-1 裁定的驱动侧运行时语言集合

- **文档写**：`docs/development/driver-api-dependency-boundary.md:295-298`（2.4.3 配套终态第 2 条）把语言真值分**三层**：① 「**宿主实际接线的内置语言只有 `en` 与 `zh-CN`**」（`builtinLocales.ts:9`）；② 「**其余 8 个语言目前只做 parity 校验**…除 `src/locales/` 内部再导出外，**生产路径无运行时 import**」；③ 命名约定层（提到「驱动包 `locales/` 现覆盖 10 个语言文件」）。
- **协调者裁定（本轮据以判定的事实源）**：并行轨 `i18n-drivers` 的 O-1 已裁定 **驱动 10 语言全量自注册维持**（用户认可包体代价 **+76.57 kB min / +6.59 kB gzip**），出处 `.worktrees/datazen-i18n-drivers/docs/development/coordination/tracks/i18n-drivers/bugs.md:170-182`（三档 `vite build` 实测：en+zh-CN 档 1,528.55 kB → 全 10 语言档 1,605.12 kB），其 `progress.md:226` 同记该净增。
- **代码实为**：驱动自注册模块把 **全部 10 个语言字典静态 import 后一次性灌入共享注册表**（`.worktrees/datazen-i18n-drivers/packages/drivers/redis/locales/index.ts:18-41`：`:18 import { registerTranslations } from '@datazen/ui'`、`:19-28` 逐个 `import de/en/es/fr/ja/ko/pt-BR/ru/zh-CN/zh-TW`、`:30 registerTranslations({ … })`），并由驱动 UI 入口 `packages/drivers/redis/ui/shared/meta.ts:4` / `packages/drivers/mongodb/ui/meta.ts:4` 以副作用 import 挂载 → 与宿主「只接线 en/zh-CN」**不对称**：宿主 lazy 装载器确实硬编码两语言（本 worktree `src/locales/lazyPacks.ts:21-34` 的 `loaders` 只有 `en` / `'zh-CN'` 两组，`:36-38 isBuiltin()` 对其余语言在 `ensureLocaleDomains` 中早退），而共享注册表在驱动装载后会含 10 语言的词条。
- **为何判为缺陷**：三层表述本身无假话，但**只写了宿主集合**，读者按第 ② 层「生产路径无运行时 import」外推会得出两种误判——(a)「全仓运行时只有 2 语言进 bundle」，(b)「驱动也只应注册 en/zh-CN、注册 10 语言属越界」。二者都与已裁定的终态相反；而契约 2.4.3 词条归属表里「静态 import 本目录全部语言字典」一句分散在上表（:288），未与该三层对表，也未记 O-1 的量级与结论。
- **重现命令**：

  ```bash
  sed -n '288,299p' docs/development/driver-api-dependency-boundary.md   # 三层，仅宿主集合
  sed -n '35,37p;21,32p' src/locales/lazyPacks.ts                        # 宿主 lazy 只有 en / zh-CN
  grep -n "^import\|registerTranslations(" ../datazen-i18n-drivers/packages/drivers/redis/locales/index.ts   # 驱动 10 语言全量
  ```

- **影响范围**：2.4.3 是驱动词条归属与语言集合的唯一规范落点，且被 `independent-driver-development.*` §6.3 引用；读者据此评审 Wave 4 之后的驱动包时，可能误报「驱动注册多余语言」为违规，或反过来低估词条装载代价。
- **建议修复方向（供 Coder 参考，非 Tester 实施）**：在第 ②/③ 层之间（或第 ③ 层末句后）补 **一句**说明「不对称是有意的」：宿主运行时接线集 = `en`/`zh-CN`，而**驱动包自注册会把本包全部语言文件灌入共享注册表**（O-1 裁定维持，实测代价 +76.57 kB min / +6.59 kB gzip，redis/mongodb 各 10 语言），「生产路径无运行时 import」一句显式限定为「宿主 `src/locales/` 的 8 个语言文件」。改动仅落在 2.4.3（纯文本，无新增标题）；指南 §6.3 若同步补一句须 zh/en 两份同改并维持 21:21（不加则亦可，因 §6.3 只引用契约）。
- **Tester 第 4 轮复测判定（commit `91921a87d`）→ 已修复**：
  1. 第 ② 层（`:297`）已带「**就宿主 `src/locales/` 的这 8 个语言文件而言**」限定语并显式指向下条不对称说明；第 ③ 层（`:298`）保留「驱动包各 10 语言文件」事实；新增 `:299` 独立 bullet 首句即「上述三层全部只是宿主侧口径；驱动侧的注册集合有意与之不对称，不是越界」，且含「不存在『驱动跟着宿主只注册 2 语言』这一形态」的显式否定——按 (a)「三层是否仍可能被读成驱动只注册 2 语言」复验通过；
  2. O-1 数字逐个对表 `.worktrees/datazen-i18n-drivers/.../bugs.md` O-1 表与 `progress.md`「Coder Bug 修复记录（第 1 轮）」：三档 min `1,501.93 / 1,528.55 / 1,605.12`、净增 `+76.57 kB min / +6.59 kB gzip`、均在 main chunk、收益侧「为未来经 `registerLocale()` 接入第 3 语言预付」——**全部与裁定实测值逐项一致**；
  3. 「惰性 / 按需」措辞全扫（boundary + 两份指南）：仅 `:288`（宿主 lazy 域包既有事实描述）与 `:299`（「不要求、不建议驱动改走惰性 / 按需注册——O-1 裁定明令禁止引入该机制，全量 eager 注册即终态」），**无任何建议性表述**；
  4. 接管代理自我更正的 `:299` 新判据逐条实测成立：`src/locales/index.ts:42-45`（`registerLocale()` 写 `extensionLocales` + 灌字典）、`:51-56`（`getExtensionLocales()`）、`src/windows/settings/SettingsContent.tsx:91-100`（语言下拉 = `BUILTIN_LOCALES.map(…)` + `…getExtensionLocales()`）行号逐一命中；`registerLocale` 全仓调用点仅 `src/locales/locales.test.ts`（测试），生产代码（含 `src/`、`packages/wapps`、`packages/extension-points`、`packages/wapp-sdk`）**零调用**；`src/locales/lazyPacks.ts:21-34` loaders 键域实测只有 `en` / `'zh-CN'` 两组、值为宿主 4 惰性域包（`src/locales/domains.ts:21` = `['sync','workflows','dashboard','mcp']`），驱动 eager 词条不经它装载——lazyPacks 单列为「只服务宿主词条」**成立**；
  5. 指南 zh:214 / en:229 同步补「注册集合不随宿主接线的可选语言集合收缩，两者不对称是有意终态」并句内指向契约 2.4.3，zh/en 该小节措辞对等、标题结构维持 21 : 21。

---

## decouple-docs-BUG-007（低，第 3 轮复测新发现）— 副作用示例 `import '../locales';` 与文档点名的嵌套入口不自洽

- **文档写**：`driver-api-dependency-boundary.md:289`（2.4.3 词条归属表「驱动词条」行）：「由该驱动 UI 的入口模块（即 `generated.ts` 实际 import 的首个驱动 UI 模块，如 redis 的 `ui/shared/meta.ts`、mongodb 的 `ui/meta.ts`）挂一行 `import '../locales';` 副作用」；指南 `independent-driver-development.zh-CN.md:214` / `.en.md:229` 同写法（「如 `ui/shared/meta.ts`）加一行 `import '../locales';`」）。
- **实为**：`ui/shared/meta.ts` 位于 `<driver>/ui/shared/`，而词条目录在 `<driver>/locales/`（本 worktree 实测 `ls packages/drivers/redis/ui/shared/meta.ts` 存在、`packages/drivers/redis/locales/` 存在），该文件内正确的相对说明符只能是 `'../../locales'`——并行轨实现即为 `packages/drivers/redis/ui/shared/meta.ts:4 import '../../locales';`，而 `ui/meta.ts` 才是 `import '../locales';`（`.worktrees/datazen-i18n-drivers/packages/drivers/mongodb/ui/meta.ts:4`）。同一句话里给的示例路径与点名的入口目录深度不匹配。
- **重现命令**：

  ```bash
  sed -n '288p' docs/development/driver-api-dependency-boundary.md
  ls packages/drivers/redis/ui/shared/meta.ts packages/drivers/redis/locales | head -3
  grep -n "locales" ../datazen-i18n-drivers/packages/drivers/redis/ui/shared/meta.ts ../datazen-i18n-drivers/packages/drivers/mongodb/ui/meta.ts
  ```

- **影响范围**：低——照抄会得到解析失败（构建/`tsc` 立即报错，可自纠），但该句正是「驱动词条自注册」的操作指令且被标注为终态唯一落点；入口目录不同（`ui/meta.ts` vs `ui/shared/meta.ts`）的两类驱动会拿到一条对其中一类不成立的字面示例。
- **建议修复方向（供 Coder 参考，非 Tester 实施）**：把示例改为按入口深度写清（如 `ui/meta.ts` 用 `import '../locales';`、`ui/shared/meta.ts` 用 `'../../locales'`），或改写成「挂一行副作用 import 指向本包 `locales/` 目录（相对层级随入口深度而定）」；契约与两份指南 §6.3 **三处同改**，指南保持 zh/en 同步与 21:21 结构。
- **Tester 第 4 轮复测判定（commit `91921a87d`）→ 已修复**：契约 `:289` 与指南 `zh:214` / `en:229` 三处均已改为「**相对层级随入口目录深度而定**」并按入口分例：`ui/meta.ts`（如 mongodb）→ `import '../locales';`、`ui/shared/meta.ts`（如 redis）→ `import '../../locales';`。本轮以 Read 只读核对并行轨真实文件：`/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-i18n-drivers/packages/drivers/redis/ui/shared/meta.ts:4` 逐字 = `import '../../locales';`；`.../packages/drivers/mongodb/ui/meta.ts:4` 逐字 = `import '../locales';`——**两串与文档三处引用逐字一致**；入口归属旁证：本 worktree `scripts/resolve-drivers.mjs:239`（redis `ui/shared/meta`）/ `:270`（mongodb `ui/meta`）、`src/extensions/generated.ts:10`（redis 首个 UI import 即 `ui/shared/meta`）。全仓文档内 `'../locales'` / `'../../locales'` 串不再存在「单一通用串」写法（其余命中为 bugs/progress 举证引文）。

---

## 未被判为 Bug 但记录在案观察项

1. `driver-api-dependency-boundary.md:273`（2.4.2）称 `src/lib/localeSync.ts` 是宿主「**唯一接线点**」。严格意义上 `src/locales/index.ts:64-79` 的 `getTranslation()` 也会临时 `setLocale(locale)` → 查表 → `setLocale(previous)`（同文件注释自陈"never in React render paths"）。该函数是工具/测试用的取词适配器而非语言接线，且 Coder 在 progress.md 自验第 7 条中已如实披露，故不登记 Bug；建议在 2.4.2 补一句括注以免读者 grep 出第二个调用方时困惑。
2. Part 1（Rust 段）实测逐字未改动（`git diff` 仅改了文件顶部 H1），其「`datazen-driver-api = "0.1"`」示例与当前 crate `version = "0.0.8"` 不吻合，属历史遗留文本，不在本轨勘误范围内。
3. **第 3 轮复测补记（记录口径，非文档事实错误）**：`progress.md`「第 1 轮自验」第 4 条与第 2 轮阶段 B-7 均写「字面 `../../../src/` 形态 **9 处**」，本轮以 `grep -o` 逐文件实测为 **8 处**（zh 2 / en 2 / boundary 4 / components 0 / extensibility 0）。判定结论不变（8 处逐处复核全部位于带 `❌ 反例` 标注的代码块或「禁止 / 零新增」句内），仅历史记录数字偏大 1，后续复测以 8 为准。
4. **第 3 轮复测补记**：指南 §6.2 的「留薄再导出壳的先例」列表出现 `src/lib/cn.ts` 等宿主路径字面量，不违反验收 3——§6.1 已确立「禁止任何指向宿主 `src/**` 的相对 import」，该列表是「宿主侧路径如何收尾」的先例举证而非 import 推荐写法，与契约 2.2 行 1 同形；若后续想彻底消除歧义，可在两处先例列表后补一句「驱动侧一律 import 包名，不得 import 下列宿主壳路径」（契约 2.1.2 已有该句，指南暂无），属可选增强，不登记 Bug。

---

## Nit（第 4 轮复测登记 · 留待 Wave 4 文档回扫，不构成缺陷、不阻断合流）

> 判级纪律（协调者第 4 轮指定）：Blocker = 事实错误 / 误导性表述 / zh-en 不同步 / 越界改代码；Nit = 措辞可更清晰、举例可更完整、重复说明可精简。以下均为 Nit。

1. **Nit-1（boundary:299 单条 bullet 过载）**：不对称说明把「三层宿主口径、驱动全量 eager、O-1 三档数字、注册可达性两分、`registerLocale()` 判据、lazyPacks 单列、不可达预付、双向评审禁令」约十句压在一个 bullet 内，检索与 diff 成本高。建议 Wave 4 回扫改写 2.4.3 时拆为 2-3 个子 bullet（内容结论不必变）。
2. **Nit-2（指南 §6.3 分例举例不完整）**：`zh:214` / `en:229` 的入口深度分例只在嵌套形态处标注「（如 redis）」，`ui/meta.ts` 形态未标注「（如 mongodb）」（契约 `:289` 两处均点名）。属举例可更完整，Wave 4 回扫时对齐契约写法即可。
3. **Nit-3（既有观察项归并）**：观察项 1（2.4.2 `getTranslation` 括注）、观察项 4（§6.2 先例列表补「驱动不得 import 宿主壳路径」句）与本轮 Nit-1/2 同属一次 Wave 4 文档回扫改动，建议合并处理，避免碎片化改写。
