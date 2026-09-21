# Track: i18n-drivers — Bug 清单

> Tester 独立复测（commit `9d4016295`，基准 `d172476fc`；本轨 7 个 commit / 61 文件）一次性登记。
> **第 2 轮复测（commit `a61e42f14`，全新 Tester 实例）：BUG-001/002/003 全部闭环，状态置「已修复」；
> 反证 A/B/C、CLI md5 逐字对比、覆盖率与全套件复跑数字见 progress.md「Tester 第 2 轮复测记录」。
> 本轮无新增 Bug。**
> 结论：**运行时行为全部实测为真**（注册链路、bundle、tsc、四套件全绿、Grep 红线全零命中），
> 但**本轨核心变更「驱动词条自注册链路」在常驻测试与守门脚本里都没有任何拦截能力**，
> 且本轨新增守门脚本逻辑自身零单测。三项登记如下，均为「只测不修」。

---

## i18n-drivers-BUG-001：驱动词条自注册链路（`ui/**/meta.ts` 副作用挂载点）零常驻测试覆盖，删除该行不会被任何 CI 捕获

- **状态**: 已修复（Tester 第 2 轮复测通过，commit `a61e42f14`：harness 已改走 meta；redis/mongodb 各 4 例常驻注册链路套件经逐例审查为真断言；反证 A/B/C 由本轮 Tester 独立重跑，全部与预期一致——A：`6 failed | 235 passed (241)`（失败恰为两套件各 3 例，文件/用例名见 progress.md「Tester 第 2 轮复测记录」§2）；B：harness 清空后 241 全绿（不再依赖旁路）；C：游离 `locales/xx.ts` 使守门用例转红。另实测：harness 禁用下仅两个注册套件即把两 meta + 两 `locales/index.ts` 推到 100% 四维覆盖。建议 3（脚本层结构断言）取舍判定为**可接受**：常驻用例跑真模块、断真值，A/C 变异均被拦截，脚本字面量扫描无额外拦截力；留待 Wave 4 lint 不判 Bug。）
- **严重度**: 中（不阻断当前运行时行为——已实测 bundle 内生效；阻断本轨核心链路的回归防护与 tester.md §3「覆盖率硬标准」）
- **位置**:
  - 被测代码：`packages/drivers/redis/ui/shared/meta.ts:1-4`、`packages/drivers/mongodb/ui/meta.ts:1-4`（`import '../locales'` 副作用行）
  - 装配代码：`src/test/driverUiSetup.ts:16-18`（三行 import）、`vitest.drivers.config.ts:19`
  - 相关测试：`packages/drivers/redis/ui/__tests__/*`（26 files / 218 tests）、`src/locales/locales.test.ts:104-113`
- **描述（含量级）**:

  本轨的唯一实质行为是「驱动 UI 入口模块一经装载即完成词条注册」。实测该链路**机制为真**，
  但**没有任何常驻断言**保护它，原因有两层叠加：

  1. `src/test/driverUiSetup.ts` 直接 `import '../../packages/drivers/{redis,mongodb}/locales'`，
     **绕过了 `meta.ts` 挂载点**（真实装载路径是 `generated.ts → ui/**/meta.ts → locales/index.ts`）。
     因此 harness 一旦注入，meta.ts 的副作用行是否存活对测试完全不可见。
  2. 218 个 redis UI 用例**全部与语言无关**（8 个 `vi.mock('@datazen/ui')` 让 `t` 回显 key，
     其余不校验译文字符串）。把 harness 三行 import 全部清空后，218 仍**全绿**。

  量级：`packages/drivers/redis/ui/**`（26 files / 218 tests）+ `packages/drivers/mongodb/ui/**`
  对本轨核心链路的**有效断言数 = 0**；宿主侧 `src/locales/locales.test.ts` 新增的 1 例
  只覆盖「驱动包 `locales/index.ts` 被显式 import 后会注册」，同样不覆盖 meta.ts 挂载点。
  即：任何一次「顺手删掉/重构掉 `import '../locales'`」的改动，都会在**测试全绿、tsc 全绿、
  vite build 全绿、`i18n-sync-check` 也全绿**（它只校验 `index.ts` ↔ 语言文件，不校验
  `meta.ts` ↔ `index.ts`）的情况下静默上线，表现为 redis 工作台整体退化为 raw key。

- **重现步骤**（Tester 实测已执行并还原，`git status` 干净）:

  ```bash
  # 0) 基线（全绿）
  node scripts/resolve-drivers.mjs --codegen-only --drivers=all
  npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui   # 26 files / 218 passed

  # 1) 清空 i18n 装配（等价于「驱动词条完全没注册」的最坏情形）
  cp src/test/driverUiSetup.ts /tmp/d.ts.bak
  printf '// PROBE\nexport {};\n' > src/test/driverUiSetup.ts
  npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui   # ← 仍然 218 passed（缺陷点）
  cp /tmp/d.ts.bak src/test/driverUiSetup.ts

  # 2) 证明「机制为真、但仅由临时探针证明」：临时用例只 import meta
  cat > packages/drivers/redis/ui/__tests__/zzProbe.test.ts <<'EOF'
  import { expect, it } from 'vitest';
  import { t, getRegisteredTranslations } from '@datazen/ui';
  import '../shared/meta';
  it('meta registers the pack', () => {
    expect(t('redis.batchDelete')).toBe('Delete selected');
    expect(Object.keys(getRegisteredTranslations('en')).length).toBeGreaterThan(100);
  });
  EOF
  printf '// PROBE\nexport {};\n' > src/test/driverUiSetup.ts
  npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui/__tests__/zzProbe.test.ts  # pass（链路真实）
  # 3) 关掉 meta 的副作用行 → 该探针 FAIL（`expected 'redis.batchDelete' to be 'Delete selected'`），
  #    但同一次运行里 218 个常驻用例仍全绿 —— 即回归不可被拦截
  ```

- **实测日志**（节选）:

  ```text
  # (1) harness 清空后，redis UI 套件：
   Test Files  26 passed (26)
        Tests  218 passed (218)          ← 注册链路被完全切断，仍无一条用例失败

  # (2) 仅 import '../shared/meta'（harness 已清空）：
   ✓ packages/drivers/redis/ui/__tests__/zzProbe.test.ts (1 test)      ← 驱动包确实自注册（非假绿）
     t('redis.batchDelete') === 'Delete selected'，getRegisteredTranslations('en') > 100 key

  # (3) 再把 meta 的 `import '../../locales'` 注释掉（harness 保持清空）：
   FAIL ... zzProbe.test.ts > meta registers the pack
   AssertionError: expected 'redis.batchDelete' to be 'Delete selected' // Object.is equality
   Test Files  1 failed | 26 passed (27)
        Tests  1 failed | 218 passed (219)   ← 218 常驻用例对该破坏零感知

  # (4) mongodb 同款探针（harness 清空）：
   mongo.collections='Collections'；en/zh-CN/ja/pt-BR 各 15 key（10 语言全部注册）
  ```

- **期望行为 / 建议补的用例**（Coder 落地，Tester 只登记）:
  1. **让 harness 走真实装载路径**：`src/test/driverUiSetup.ts` 改为 import 驱动 UI 入口
     （`packages/drivers/redis/ui/shared/meta` / `packages/drivers/mongodb/ui/meta`）而非
     `.../locales`，使全部 233 个驱动用例天然建立在「meta 副作用 → 注册」这条真实链路上；
     若担心驱动 UI 入口在装配期引入额外模块，可保留 `locales` import 但必须叠加下面第 2 条。
  2. **每个带词条包的驱动补 1 个常驻注册链路套件**（建议
     `packages/drivers/<id>/ui/__tests__/localePackRegistration.test.ts`，用例名前缀 `test_tester_`
     或描述块标注 `[tester]`）：断言（a）只 import `../shared/meta`（不 import `locales/index`）后
     `t('redis.batchDelete') === 'Delete selected'`；（b）`BUILTIN_LOCALES` 同款的 10 个 locale code
     在 `getRegisteredTranslations(code)` 中 key 数 > 0（把「locale code 字面量必须与宿主一致」
     这条约定也钉住，当前 `pt-BR` / `zh-CN` / `zh-TW` 拼写错误不会被任何测试发现）；
     （c）`en` 快照 key 数 ≥ 词条包 key 总数（redis 实测 340）。预计 3 例即可把该模块行覆盖推到 100%。
  3. **（可选，同一改动内完成）守门脚本补一环**：`scripts/i18n-sync-check.mjs` 的结构校验目前只到
     「`locales/index.ts` 是否 import 了每个语言文件」，未校验「驱动 UI 入口是否 import 了 `locales`」；
     可复用 `generated.ts`/`ui/**/meta.ts` 的字面量扫描补第 3 条结构断言（若判定越界，留给 BUG-002 一并处理）。
  - 覆盖率现状（v8 实测）：`packages/ui/src/i18n.ts` 100% 四维、
    `packages/drivers/{redis,mongodb}/locales/index.ts` 100%（仅因 `src/locales/locales.test.ts`
    显式动态 import 才被计入）、`src/locales/index.ts` 95.23% stmts / 83.33% branch（L99 未覆盖）；
    **`ui/**/meta.ts` 的副作用行与 `src/test/driverUiSetup.ts` 在常驻驱动套件中无任何有效覆盖。**

---

## i18n-drivers-BUG-002：`scripts/i18n-sync-check.mjs` 新增 ~100 行驱动词条扫描逻辑零单测（脚本无导出，与同目录 guard 脚本惯例不一致）

- **状态**: 已修复（Tester 第 2 轮复测通过，commit `a61e42f14`：5 个导出纯函数 + main 守卫成立，导入模块不再触发扫描；CLI 输出与首轮交付 `9d4016295` **逐字一致**（本人实测 `node scripts/i18n-sync-check.mjs`：55 行、md5 `c2ffde854c8b1dde7bd6fce1bfa28b44`、exit 1，与 Coder 自报相符；与 `fd23a66a8` 基线 diff 仅为本轨预期的驱动段 +18 行与汇总行措辞，宿主段逐字节一致）；18 例经逐例审查为真实分支断言（缺/多 key、缺 index.ts、漏 import、>10 截断、structural-only 致命、三种退出码组合，无以量补质的同义断言）；覆盖率实测：本轨新增/重构驱动扫描段（L60-209）行覆盖 100%，未覆盖仅剩宿主既有 `extractKeys` 段（L39-59，本轨未触碰代码、行为以 CLI 输出对比佐证）与 `runCli`/git-tag CLI 装配段（L210-329，含 `istanbul ignore` main 守卫）；main 守卫调用方核查：`.github/workflows/ci.yml:68`、`scripts/ci-local.sh:65`、`scripts/run-full-automation-test.sh:79`、i18n-sync skill、AGENTS.md 均为 `node scripts/i18n-sync-check.mjs` 直接调用 → 文件名结尾匹配命中，`package.json` 无脚本调用方，无破坏。）
- **严重度**: 低-中（脚本本身经 Tester 手工四分支实测正确；缺的是回归防护。该脚本是本轨「不再由宿主 codegen 聚合」后**唯一**的词条完整性守门，若被改坏无人发现）
- **位置**: `scripts/i18n-sync-check.mjs:56-72`（`extractPackKeys`）、`74-84`（`findDriverLocalePacks`）、`186-243`（驱动包结构 + 缺失/多余扫描、`totalStructural` 退出码）；对照 `scripts/__tests__/`（存在 `check-id-terminology.test.ts` / `check-managed-stubs.test.ts` / `check-module-layers.test.ts` 等同类目测试，**无 `i18n-sync-check` 任何测试文件**）
- **描述（含量级）**: 本轨为 `i18n-sync-check.mjs` 新增 97 行（含 4 条可判定分支：缺 index.ts / 漏 import / key 缺失 / key 多余），并改变汇总行与退出码语义（`totalStructural` 参与 `exitCode=1`）。实测该逻辑**当前行为正确**（见下日志），但 `scripts` 套件（21 files / 190 tests）里没有一条用例触达它——脚本全部逻辑在模块顶层裸执行、未 `export`，因此**技术上无法被单测导入**（`scripts/__tests__/check-*.test.ts` 的既有写法都是「脚本导出纯函数 + 测试断言」）。
- **重现步骤**:
  ```bash
  ls scripts/__tests__ | grep -c i18n            # 0
  grep -c '^export ' scripts/i18n-sync-check.mjs # 0（无可测导出面）
  npx vitest run scripts --coverage --coverage.include='scripts/i18n-sync-check.mjs'  # 该文件不进报告
  ```
- **实测日志**（手工四分支，Tester 已还原，`git status` 干净）:
  ```text
  # 基线（HEAD）
  Summary: 2400 missing key(s), 1650 stale translation(s) across 8 host locales; 0 driver pack issue(s) across 2 driver locale pack(s).   EXIT=1
  # 删 redis/de.ts 的 'redis.batchDelete' → 2400 → 2401，并列出 [driver.redis/de] Missing 1 key(s)   EXIT=1
  # 给 redis/de.ts 加 'redis.testerExtraKey' → Extra 1 key(s): redis.testerExtraKey（多余不致命，与宿主一致）
  # 删 packages/drivers/redis/locales/index.ts → [driver.redis] locales/index.ts is missing… → 1 driver pack issue(s)   EXIT=1
  # index.ts 去掉 import de from './de' → [driver.redis] locales/index.ts does not import 1 locale file(s): de.ts   EXIT=1
  ```
- **影响范围**: 该脚本是 CI guard（`.github/workflows/ci.yml:66-68`，`continue-on-error: true`，
  故退出码语义变更**不会**打破 CI —— 已核实），同时是 i18n-sync skill 的输入；未来任何人调整
  key 抽取正则（例如词条包改用双引号 key 或模板字符串 key）都会静默把「缺失」算少甚至算成 0，
  从而给出虚假的「全绿」结论。
- **期望行为 / 建议**: 把 `extractPackKeys` / `findDriverLocalePacks` /（可选）`checkDriverPacks(dir)`
  提为 `export function`（CLI 行为与输出保持不变，沿用 `check-id-terminology.mjs` 的
  「导出纯函数 + `if (import.meta.url === …)` 守卫 main」写法），并新增
  `scripts/__tests__/i18n-sync-check.test.mjs`：用 `scripts/__tests__/fixture.ts` 同风格的
  内联样本至少覆盖 ①折行值 key 可见（`'k':` 换行取值）②单/双引号值 ③缺 key 计 missing
  ④多余 key 只列不计 ⑤缺 `index.ts` 计 structural ⑥`index.ts` 漏 import 单个语言文件。
  新增逻辑行覆盖目标 ≥80%（当前 0%）。

---

## i18n-drivers-BUG-003：三处宿主注释仍声明「`src/locales` 注册 host + driver dictionaries」，本轨后语义失真

- **状态**: 已修复（Tester 第 2 轮复测通过，commit `a61e42f14`：`git diff -U0 fd23a66a8..HEAD -- src/locales/t.ts src/hooks/useI18n.ts src/lib/localeSync.ts` 逐行核对，改动全为 `//` 注释行（含 localeSync 注释 2 行 → 3 行），import 语句与任何可执行语句零改动；文案与新契约一致。）
- **严重度**: 低（纯注释，但恰好描述的是本轨推翻的那条契约，误导后续读者）
- **位置**:
  - `src/locales/t.ts:1` — `import './index'; // side effect: register eager host + driver dictionaries`
  - `src/hooks/useI18n.ts:8` — `import '../locales'; // side effect: register host + driver dictionaries`（任务书「禁止事项」明令不得改动本文件）
  - `src/lib/localeSync.ts:3` — `// Side effect: registers the eager host dictionaries + driver locale packs into …`（任务书明令不得改动接线逻辑）
- **描述**: 自 `81c407ca4` 起，宿主 `src/locales/index.ts` 不再聚合任何驱动词条（`registerTranslations(DRIVER_LOCALES)` 已删），driver 词条改由驱动包自注册。上述三行注释与新契约直接矛盾，其中 `t.ts` 属宿主 locales 目录（本轨职权范围，可直接改），另两处需协调者裁定（改注释不改编排，风险为零）。
- **重现步骤**: `grep -n "driver" src/locales/t.ts src/hooks/useI18n.ts src/lib/localeSync.ts`
- **实测日志**:
  ```text
  src/locales/t.ts:1:import './index'; // side effect: register eager host + driver dictionaries
  src/hooks/useI18n.ts:8:import '../locales'; // side effect: register host + driver dictionaries
  src/lib/localeSync.ts:3:// Side effect: registers the eager host dictionaries + driver locale packs into
  ```
- **影响范围**: 仅阅读体验/认知一致性；不影响任何断言与运行时。
- **期望行为**: 三处注释改为「注册宿主 eager 字典（驱动/扩展包各自自注册，宿主不聚合）」。
  `src/locales/t.ts` 由本轨 Coder 直接修；`src/hooks/useI18n.ts` / `src/lib/localeSync.ts`
  若裁定为越界，则移交 `decouple-docs` 轨（该轨正在写解耦文档，可顺带清），
  同时提醒：`AGENTS.md:31/232`、`CONTRIBUTING.md:91`、`.gitignore:64`、
  `docs/architecture/**` 的 `generated-locales.ts` 残留描述 Coder 已如实登记给 `decouple-docs`。

---

## 观察项（非 Bug，不要求修复，供协调者/Wave 4 决策）

- **O-1 包体代价复核（Coder 自报数字成立，方向已被任务书认可）**：
  **【协调者裁定 · 第 1 轮修复循环】维持 10 语言全量自注册**，+76.57 kB min / +6.59 kB gzip 的代价由用户
  明确认可；本轨禁止改回 en+zh-CN、禁止引入惰性/按需注册机制。裁定与下表三档数字已归档到 progress.md
  「Coder Bug 修复记录（第 1 轮）」§0，R-8 的第二选项（退回两档）作废。`npx vite build` 三档实测同一 config、
  仅切换 locale 装配：
  | 装配 | main chunk (min) | gzip |
  | --- | --- | --- |
  | meta 副作用注释掉（驱动词条完全不进图） | 1,501.93 kB | 452.27 kB |
  | 仅 `en` + `zh-CN`（= 本轨前 codegen 实际接线集合） | 1,528.55 kB | 460.56 kB |
  | 全部 10 语言（HEAD） | **1,605.12 kB** | **467.15 kB** |
  → 本轨相对**真实改动前基线**（en+zh-CN 已被接线）的净增为 **+76.57 kB min / +6.59 kB gzip**，
  与 Coder 自报一致（其「1,528.55 → 1,605.12」的对照值经复算恰等于 en+zh-CN 档，数字与归因均可信）。
  驱动词条确经 `ui/**/meta.ts` 进入 **main chunk**（`dist/assets/main-*.js` 内可 grep 到 `redis` 的
  ja 值「バイト」与 zh-TW 值「批次 TTL」），即应用启动即加载；`--drivers=basic` 档实测 1,574.66 kB
  （mongodb 词条不进图）→ 包体随选型伸缩，无跨驱动浪费。
  **被忽略的低成本替代（仅登记事实）**：宿主自身的纪律是 `BUILTIN_LOCALES = ['en','zh-CN']`
  （`src/locales/builtinLocales.ts`，其余 8 个语言文件在宿主侧同样未被 eager 注册），
  而设置页语言下拉只有 en / zh-CN 两个可选值（`BUILTIN_LOCALE_LABELS`）——
  因此驱动 8 个新增语言在**当前宿主 locale 集合下运行时不可达**，这 76.6 kB 是为「未来某扩展经
  `registerLocale()` 引入第 3 语言」预付的。宿主 `lazyPacks.isBuiltin()` 同样硬编码 en/zh-CN，
  故「按键命中时再注册」需要新增共享机制（不属于本轨越界遗漏）。建议协调者二选一：
  (a) 认可 76.6 kB 作为「消灭 8 个死文件」的代价并在 progress 记录；(b) 退回 en+zh-CN 两档，
  把其余 8 语言连同宿主可选语言集合作为 Wave 4 的同一里程碑一起做（两者都不该维持现状的沉默）。
- **O-2 宿主仍持有驱动前缀 key（既有耦合，非本轨引入，但本轨使其更显性）**：
  **【协调者裁定 · 第 1 轮修复循环】与改动前同集、非本轨回归 → 本轨不修**，保持登记在 R-6 / Wave 4 lint 里程碑。
  `src/windows/connection/DocumentConnectionView.tsx` 有 20 处 `t('mongo.*')`，而
  `src/lib/connectionViews/index.ts:16` 对未知 mode 兜底返回该视图；`--drivers=basic`
  （不含 mongodb）时这些 key 会退化为 raw key。改动前后行为一致（旧 codegen 同样只合并
  已选型驱动），**判定：非本轨回归**，但需在 R 与 Wave 4 的 lint 规则里显式禁止宿主硬编码驱动前缀 key。
- **O-3 驱动 UI 并不自足**：`packages/drivers/*/ui/**` 仍消费宿主 key —— `common.*` 32 处、
  `newConn.*` 22 处、`sqlserver.*` 4 处（宿主 `src/locales/en/connection.ts` 持有）、
  `query/settings/menu/connWin.*` 若干。即「驱动包自带词条」只覆盖 `redis.*` / `mongo.*`；
  这些宿主 key 依赖 `main.tsx → src/locales` 已注册（两个 HTML 入口共用 `src/main.tsx`，
  实测无窗口漏注册）。非违规，登记为后续下沉里程碑输入。
- **O-4 注册顺序语义变更实为惰性**：驱动由「宿主之前注册」变为「随 UI 装载注册」，
  理论上是「驱动同名 key 反超宿主」。独立实测宿主 2,181 key × 驱动 355 key **交集为 0**，
  且驱动 key 100% 带 `redis.` / `mongo.` 前缀 → Coder 的「实际无碰撞」结论成立。
  建议 Wave 4 用 lint/`registerTranslations` 优先级机制固化，而非依赖前缀约定。
- **O-5 新增快照 API 复核**：`getRegisteredTranslations()` 为 `{ ...registry[locale] }` 浅拷贝，
  字典值均为字符串 → 外部写回/增删均不影响注册表（Tester 实测：篡改快照后重读仍为 `'Snap'`）；
  未知 locale 返回 `{}`（**与 `t()` 的「回落 en」不同**，勿混用）。唯一残余风险是被误用为
  「第二查表入口」（快照无插值/回落且不订阅），当前生产消费点只有 `src/locales/index.ts`
  的 `getAllTranslations()`（其生产消费者仅测试/工具路径）；建议 Wave 4 lint 禁止组件内
  `getRegisteredTranslations(...)[key]` 形态取值，本轨不必动。
- **O-6 遗留 gitignored 产物**：仍持有旧 `src/extensions/generated-locales.ts` 的开发机不会报错
  （该文件仅 import `../../packages/drivers/*/locales`，而 `RedisTranslationKey` /
  `MongoTranslationKey` 与默认导出均未删除 → 仍可通过 tsc），只是无人引用的死文件；
  `tsconfig.include` 含 `src`，故它仍会被类型检查一遍。清掉即可，无兼容性风险。
