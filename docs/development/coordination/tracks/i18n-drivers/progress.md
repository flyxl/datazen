# Track: i18n-drivers — 驱动词条自注册 + 驱动 UI 统一使用 @datazen/ui i18n

- 分支: `feature/i18n-drivers`（基准 `feat/driver-decoupling` @ d172476fc，已含 Wave 1 全部 + i18n-core + cap-bridge）
- 角色: Coder → Tester
- 上游依据: 用户 2026-09-20 批准的 i18n 终态方案（见本文件「目标设计」），**不得重新提出备选方案**

## 目标设计（既定 spec，逐条落实）

1. `@datazen/ui` 是**唯一** i18n 运行时（`packages/ui/src/i18n.ts`：`setLocale` / `getLocale` / `registerTranslations` / `t` / `useI18n`）。无 bridge、无第二套查表引擎（i18n-core 轨已交付）。
2. **词条由各 package 自己提供并自注册**：驱动不再依赖宿主 codegen 聚合，改为驱动包自带 `locales/index.ts` 副作用模块，调用 `registerTranslations` 把自己全部语言的字典灌入共享注册表。
3. **只有宿主调用 `setLocale`**（`src/lib/localeSync.ts` 已接线）。驱动/扩展侧出现任何 `setLocale` 调用即违规。
4. `@datazen/extension-points` 不 re-export i18n（i18n-core 已删除 `src/i18n.ts`）；扩展与驱动一律直接 import `@datazen/ui`。
5. 已接受的权衡：驱动侧 `t()` 的 key 是普通 `string`，不再有编译期 `I18nKey` 校验；词条完整性由扩展后的 `scripts/i18n-sync-check.mjs` 扫描保证。

## 范围

### A. 驱动词条自注册

1. 新增 `packages/drivers/<id>/locales/index.ts`（当前有词条包的驱动：`redis`、`mongodb`）：
   - `import { registerTranslations } from '@datazen/ui';`
   - 静态 import 本目录**全部**语言文件（`en`、`zh-CN`、`zh-TW`、`de`、`es`、`fr`、`ja`、`ko`、`pt-BR`、`ru`）并按 locale code 组装后一次性 `registerTranslations(...)`；locale code 必须与宿主 `src/locales/builtinLocales.ts` 的 `BUILTIN_LOCALES` 字面量完全一致（`pt-BR` 带连字符）。
   - 纯副作用模块：不导出运行时值（可 `export {}`），保证多次 import 幂等（`registerTranslations` 本身是 Object.assign 语义，重复注册无副作用）。
2. 在驱动 UI 的入口模块挂副作用 import：`packages/drivers/redis/ui/shared/meta.ts`、`packages/drivers/mongodb/ui/shared/meta.ts`（若某驱动入口文件路径不同，取 `src/extensions/generated.ts` 实际 import 的该驱动 UI 首个模块，并在 progress 记录落点）。理由：`generated.ts` 每次构建都会 import meta，故驱动 UI 一经装载即完成词条注册，宿主无需知道驱动有哪些语言包。
3. 驱动 `locales/` 目录结构、命名与 key 前缀（`redis.*` / `mongo.*`）保持不变；**本轨不新增/不重命名任何 key**。

### B. 驱动 UI 改用 @datazen/ui 的 useI18n

1. 全量替换驱动 UI 中的宿主 import：`import { useI18n } from '../../../../../src/hooks/useI18n'` → `from '@datazen/ui'`。已盘点范围（基准 d172476fc）：redis ui 31 处 + `packages/drivers/sqlserver/ui/ConnectionFields.tsx` 1 处（行号见 `grep` 结果，务必以实际检索为准，勿遗漏嵌套目录如 `value-editors/valueView/render.tsx`、`console/consoleCompletion/CompletionPopup.tsx`）。
2. 替换后驱动侧不得残留任何 `.../src/` 形态的宿主 import（含 `src/locales`、`src/hooks`）。允许残留：`packages/drivers/*/ui/__tests__/redisKeyWebContextMenu.test.tsx` 的 `WebContextMenuHost` + `contextMenuStore`（协调者已裁定：这是宿主集成 fixture，非解耦违规，留待后续里程碑）。
3. 若驱动组件对 `t(...)` 有 `as I18nKey` 之类的宿主类型断言，一并去除（改用 `string`）。

### C. 宿主与 codegen 收口

1. `src/locales/index.ts`：删除 `import { DRIVER_LOCALES, type DriverTranslationKey } from '../extensions/generated-locales'` 与 `registerTranslations(DRIVER_LOCALES)`；删除对 `MongoTranslationKey` 的 type import 及 re-export；`I18nKey` 收敛为 `TranslationKey | (string & {})`（保持向后兼容的字面量类型联合可删）。
2. `getAllTranslations(locale)`：改为从 `@datazen/ui` 暴露的只读快照 API 取字典。**必须在 `packages/ui/src/i18n.ts` 新增该 API**（建议 `getRegisteredTranslations(locale: string): Record<string, string>`，返回只读拷贝），避免宿主自持第二份字典。注意语义变化并在 progress 记录：驱动词条只有在驱动 UI 模块被 import 后才在注册表内；`src/locales/locales.test.ts` 若因此断言失败，允许在测试内显式 `import '../packages/drivers/redis/locales'`（或等价方式）后断言，**禁止放宽断言到"不校验驱动 key"**。
3. **删除驱动 locale 聚合 codegen**（第 2 步的 `generated-locales.ts` 随之消失）：
   - `scripts/resolve-drivers.mjs`：删 `DRIVER_LOCALE_CONFIG`（~368 行）、`generateLocales` 及其调用点、`HOST_LOCALES`（若无其他用途）；
   - `scripts/driver-deinject.mjs`：删 clean 模板与 `generated-locales.ts` 相关分支（~17/25/221/226/255）；
   - `scripts/driver-stash-precommit.mjs`：删 `hasInjectedGeneratedLocales` / 相关清理分支（~63/71/94/134/226/250/269/296/312/332）；
   - `scripts/ensure-generated-drivers.mjs`：删该文件的 clean 生成（~39）；
   - `scripts/check-id-terminology.mjs`：`SKIP_FILES` 移除该项（文件已不存在）；
   - `scripts/__tests__/`：同步 `fixture.ts`（~13/21）、`driver-deinject.test.ts`（~37/44/49）、`driver-stash-precommit.test.ts`（~63/64/81）。
   - 本地已存在的 gitignored `src/extensions/generated-locales.ts` 属产物，删除或不再引用即可，**不要提交**。
4. `scripts/i18n-sync-check.mjs`：新增扫描 `packages/drivers/*/locales/`——以各驱动 `en.ts` 为 source，校验同目录其他语言文件 key 集合一致（缺失/多余逐项列出，非零退出）。保持对宿主 `src/locales` 的既有行为不变。
5. 文档：`packages/drivers/*/README.md`（或既有驱动开发文档）补一段"词条自注册"约定；若无 README 则在 progress 记录并留待 `decouple-docs` 轨（该轨并行在写解耦指南，勿重复创建同名文档）。

## 禁止事项（防跨轨冲突 & 防越界）

- **不动** `docs/development/coordination/hub.md`（协调者聚合脚本专用）、其他轨道的 `tracks/*/progress.md`。
- **不动** `packages/ui/src/i18n.ts` 的既有 5 个 API 行为（只允许**新增**只读快照 API）；不引入第二套 `t`/字典。
- **不改** `src/hooks/useI18n.ts` 的宿主别名定位（宿主 300+ 文件仍走它），不得删除该文件。
- **不动** `settingsStore`、`localeSync.ts` 的接线逻辑（语言切换 → `setLocale` 已由 i18n-core 完成）。
- 不新增 i18n key、不翻译词条（开发期只允许改 `en.ts` 的纪律仍适用，本轨连 `en.ts` 都不该动）。
- 禁止 `pnpm install`；搜索用 Grep 工具；不提交 codegen 产物与 `Cargo.lock` / `src-tauri/Cargo.toml` 注入段。

## 环境注意

- worktree 初始化 codegen 为 `--drivers=basic`；验证 redis UI 前执行：
  `node scripts/resolve-drivers.mjs --codegen-only --drivers=all`
- 驱动 UI 单测配置：`vitest.drivers.config.ts`（alias 已含 `@datazen/ui` → `packages/ui/src/index.ts`）。

## 验收标准

1. `grep` 检索 `packages/drivers/*/ui/**` 中 `from ['"].*src/` 命中数 = **0**（除禁止事项中登记的 `redisKeyWebContextMenu.test.tsx` 两处宿主 fixture）。
2. 全仓无 `DRIVER_LOCALES` / `generated-locales` 引用残留（`scripts/__tests__` 与生产代码均清）。
3. `node scripts/resolve-drivers.mjs --codegen-only --drivers=all` 成功且 `src/extensions/generated.ts` / `driver_init.rs` 正常产出（不再产出 `generated-locales.ts`）。
4. `npx tsc --noEmit -p tsconfig.json` = 0 error。
5. `npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui` = **218 pass / 0 fail**（基线，且**不得通过修改断言来维持绿灯**；若某用例因宿主字典不再被 `src/hooks/useI18n` 间接注册而失败，用显式注册驱动/宿主字典的方式修复测试装配）。
6. `npx vitest run --config vitest.drivers.config.ts` 全绿（= `pnpm test:unit:drivers`；驱动 UI 全量入口。原文 `npx vitest run packages/drivers` 为任务书笔误：默认 `vitest.config.ts` 的 include 不含 `packages/drivers/**`，该命令按设计收集 0 文件并以 1 退出 —— Tester 已更正，本行按更正后口径执行）。
7. `npx vitest run src packages/driver-sdk packages/ui` 全绿（基准 412 文件 / 4241 用例，允许因 locales.test 装配调整产生的最小改动）。
8. `npx vitest run scripts` 全绿（codegen 相关脚本测试）。
9. `node scripts/i18n-sync-check.mjs` 能报出驱动词条缺失（可临时删一个 key 自证后还原），且当前仓库全绿。
10. 运行时自检（可用 vite build 或 `pnpm build` 代替手工）：切到 `zh-CN` 后 redis 工作台文案为中文、切回 `en` 生效——登记为「留待 R 回归」E2E 项。

## 状态

- [x] Coder 完成 → READY_FOR_TEST
- [x] Tester 复测 → TEST_DONE：**PASSED（第 2 轮复测，commit `a61e42f14`；首轮 TEST_FAILED 登记的 BUG-001/002/003 全部闭环，反证三组独立重跑通过，全量套件与红线复检零回归；详见文末「Tester 第 2 轮复测记录」与 bugs.md 各条状态。首轮判定原文：Phase = `FAILED`；无功能/红线违规，运行时链路实测为真，但本轨核心注册链路零常驻覆盖 → 见 `bugs.md` BUG-001/002/003 + 观察项 O-1~O-6；复测记录见文末）**
- [x] Coder Bug 修复循环（第 1 轮）→ READY_FOR_TEST：BUG-001 / BUG-002 / BUG-003 状态推进为**待复测**（不自标已修复），实施与反证见文末「Coder Bug 修复记录（第 1 轮）」

## Coder 实施记录

Phase = `READY_FOR_TEST`（本小节末尾为自验实测数字；未做 `PASSED` 自标）。

### 1. 改动落点表

| 主题 | 文件 | 改动 |
| --- | --- | --- |
| 唯一运行时新增只读快照 | `packages/ui/src/i18n.ts` | 新增 `getRegisteredTranslations(locale): Record<string,string>`（浅拷贝、未知 locale → `{}`）；既有 5 个 API 行为零改动 |
| | `packages/ui/src/index.ts` | 导出该 API |
| | `packages/ui/src/__tests__/i18n.test.tsx` | 新增 1 例：快照可读、写回不影响注册表、未知 locale 为空 |
| 驱动词条自注册（A.1/A.2） | `packages/drivers/redis/locales/index.ts`（新）<br>`packages/drivers/mongodb/locales/index.ts`（新） | 静态 import 本包 **全部 10 个**语言文件 → 一次 `registerTranslations({...})`；纯副作用模块（`export {}`），locale code 与 `BUILTIN_LOCALES` 字面量一致（`pt-BR`/`zh-CN`/`zh-TW` 带连字符） |
| | `packages/drivers/redis/ui/shared/meta.ts`<br>`packages/drivers/mongodb/ui/meta.ts` | 顶部 `import '../../locales'` / `import '../locales'` 副作用挂载点（两者正是 `generated.ts` import 的该驱动 UI 首个模块，与任务书预估一致） |
| 驱动 UI 取 `@datazen/ui`（B） | 32 个文件（redis ui 31 + `packages/drivers/sqlserver/ui/ConnectionFields.tsx`） | `import { useI18n } from '<rel>/src/hooks/useI18n'` → `from '@datazen/ui'`（逐文件单行替换，保持既有“每组件多行 @datazen/ui import”风格） |
| 宿主收口（C.1/C.2） | `src/locales/index.ts` | 删 `DRIVER_LOCALES` / `MongoTranslationKey` import 与 re-export、删 `registerTranslations(DRIVER_LOCALES)`；`I18nKey = TranslationKey \| (string & {})`；`getAllTranslations()` 改为 `getRegisteredTranslations()` 快照（非内置 locale 回落到 `en`，与原契约一致）；注释同步 |
| 测试装配 | `src/test/driverUiSetup.ts`（新）+ `vitest.drivers.config.ts` | 见下节 |
| | 8 个 redis UI 测试文件 | `vi.mock` 落点从宿主别名改挂到 `@datazen/ui`（partial mock，见下节） |
| | `src/locales/locales.test.ts` | 新增 1 例：驱动包自注册后 `getAllTranslations('en')` / `getTranslation()` 可见驱动 key，且宿主快照不含它 |
| codegen 删码（C.3） | 5 个脚本 + 3 个脚本测试 | 见下节清单 |
| 词条完整性扫描（C.4） | `scripts/i18n-sync-check.mjs` | 驱动包扫描 + 结构校验 |

`src/hooks/useI18n.ts` 宿主薄别名按要求原样保留（未删、未改）。`AGENTS.md` / `CONTRIBUTING.md` / `.gitignore` 中的 `generated-locales.ts` 文字残留属文档/忽略规则，本轨文档权限不含 `AGENTS.md`、`CONTRIBUTING.md`（`.gitignore` 条目留着无害：文件不再被产出，仅作防御性忽略），登记给 `decouple-docs` / hub。

### 2. 脚本删码清单（C.3）

| 脚本 | 删除内容 |
| --- | --- |
| `scripts/resolve-drivers.mjs` | `DRIVER_LOCALE_CONFIG`、`HOST_LOCALES`、整个 `generateExtensionLocales()`（原 368~441 行）、其调用点；`--codegen-only` 提示文案与文件头注释去掉 locales |
| `scripts/driver-deinject.mjs` | `FULLY_GENERATED_MANAGED` 与 `isFullyGeneratedManagedFile()` 中的该条目、`cleanGeneratedLocalesContent()` 整个模板、`cleanFullyGeneratedContent()` 分支、头注释 |
| `scripts/driver-stash-precommit.mjs` | `hasInjectedGeneratedLocales()` 整个函数 + `fileHasInjection()` 路由分支 |
| `scripts/ensure-generated-drivers.mjs` | 文件头清单条目（列表本身由 `FULLY_GENERATED_MANAGED` 派生，无需另改） |
| `scripts/check-id-terminology.mjs` | `SKIP_FILES` 中该项 |
| `scripts/__tests__/fixture.ts` | `CLEAN_CONTENTS` / `INJECTED_CONTENTS` 两条样本 |
| `scripts/__tests__/driver-deinject.test.ts` | `cleanGeneratedLocalesContent` import 与断言、路径分类断言（注释说明“驱动包自注册、旧聚合文件已不存在”，不落字面量） |
| `scripts/__tests__/driver-stash-precommit.test.ts` | `hasInjectedGeneratedLocales` import、`fileHasInjection` 路由用例、空/桩 codegen 两条断言 |
| 本地 gitignored 产物 | `src/extensions/generated-locales.ts` 已删除（未提交） |

`scripts/`、`src/`、`packages/`、`e2e/` 内 `DRIVER_LOCALES` / `generated-locales` / `DriverTranslationKey` / `hasInjectedGeneratedLocales` / `cleanGeneratedLocalesContent` 全部 0 命中。

### 3. 词条扫描脚本行为（C.4）

- 发现式扫描：任何 `packages/drivers/<id>/locales/en.ts` 即视为一个词条包（当前 redis、mongodb）。
- key 抽取用独立 `extractPackKeys()`：除宿主同款内联正额外，额外识别驱动文件里的**换行折行值**写法（`'key':` 结尾换行），否则这类 key 对完整性校验不可见；宿主 `src/locales` 路径与输出保持原样。
- 缺失 key → 计入 `totalMissing` → 退出码 1；多余 key 只列出不致命（与宿主一致）。
- 额外结构校验（`totalStructural`）：词条包必须有 `index.ts`，且 `index.ts` 必须 import 目录内每个语言文件（漏 import → 列出并退出 1），把“自注册”约定本身也纳守门。
- 汇总行改为同时报告宿主 locale 数与驱动词条包数。

### 4. 测试装配说明（不改任何断言）

- 驱动组件不再经宿主 `src/hooks/useI18n` 间接触发 `src/locales` 副作用 ⇒ 驱动 UI 测试若不改装配会拿到空注册表。装配点收在**宿主测试侧**新文件 `src/test/driverUiSetup.ts`：`import '../locales'`（宿主自己那份，等价 app 启动）+ `import '../../packages/drivers/{redis,mongodb}/locales'`（驱动自注册，等价 `generated.ts → meta`），由 `vitest.drivers.config.ts` 的 `setupFiles` 追加。这样 `packages/drivers/*/ui/**` 内不出现任何 `src/` import。
- 8 个原本 `vi.mock('<rel>/src/hooks/useI18n')` 的 redis 用例改为 partial-mock `@datazen/ui`（`importOriginal` 展开后只覆盖 `useI18n`），保留其“断言与语言无关”的原意；断言文本一字未动。
- 有效性反证（临时探针，跑完即删）：在驱动测试环境内 `t('redis.batchDelete')==='Delete selected'`、`t('common.cancel')` 为真实英文、`getRegisteredTranslations('zh-CN')` > 100 key、`t('mongo.collections')==='Collections'` 全部通过，证明注册表确实被灌满（绿灯不是“查不到就回显 key”的假绿）。
- `src/locales/locales.test.ts` 新增用例显式 `await import('../../packages/drivers/redis/locales')` 后断言驱动 key 出现在共享快照里；未放宽任何既有断言。

### 5. 自验命令与实测数字

| # | 命令 | 实测 |
| --- | --- | --- |
| 1 | `node scripts/resolve-drivers.mjs --codegen-only --drivers=all` | 成功；提示改为 `codegen-only: wrote generated.ts / driver_init.rs`；`generated.ts`（15 驱动）/`driver_init.rs` 正常产出；`src/extensions/` 只剩 `generated.ts` + `generated-pro.ts` |
| 2 | `npx tsc --noEmit -p tsconfig.json` | **0 error** |
| 3 | `npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui` | **26 files / 218 passed / 0 failed**（= 基线，断言未动） |
| 4 | `npx vitest run packages/drivers` | 默认 `vitest.config.ts` 的 include 不含 `packages/drivers/**`（该轨任务书这条命令按设计收集 0 文件并退出 1）；驱动 UI 全量的正确入口 `npx vitest run --config vitest.drivers.config.ts`（即 `pnpm test:unit:drivers`）= **31 files / 233 passed / 0 failed** |
| 5 | `npx vitest run src packages/driver-sdk packages/ui` | **412 files / 4243 passed / 0 failed**（基线 4241 + 本轨新增 2 例） |
| 6 | `npx vitest run scripts` | **21 files / 190 passed**；`npx vitest run scripts packages/ui` = 24 files / 215 passed |
| 7 | `node scripts/i18n-sync-check.mjs` | 见下「偏离」：宿主部分与基线逐项一致（`1216 missing / 1650 stale`，**基线本就是退出 1**）；驱动部分新报 `redis/{de,es,fr,ja,ko,pt-BR,ru,zh-TW} 各 139 missing`、`redis/zh-CN 72 missing`、mongodb 全同步；合计 `2400 missing`，`0 driver pack issue(s) across 2 driver locale pack(s)`，退出码 1。<br>负向自证 A（删 `mongodb/locales/ja.ts` 的 `mongo.applyFilter`）→ `[driver.mongodb/ja] Missing 1 key(s): mongo.applyFilter`、总数 2400→2401；<br>负向自证 B（删 `index.ts` 里 `import de from './de'`）→ `[driver.mongodb] locales/index.ts does not import 1 locale file(s): de.ts`；两者均已还原（`git status` 干净） |
| 8 | 验收 Grep 断言 | ① `packages/drivers/**` 中 `from ['"].*src/` → **2 命中**，全在已裁定的 `redis/ui/__tests__/redisKeyWebContextMenu.test.tsx`（宿主 `WebContextMenuHost` + `contextMenuStore`）；② `DRIVER_LOCALES\|generated-locales\|DriverTranslationKey` 全仓 → 生产代码/`scripts/`/`src/`/`packages/`/`e2e/` **0 命中**（仅 `AGENTS.md` 2 处、`CONTRIBUTING.md` 1 处、`.gitignore` 1 处、`docs/**` 5 处，越界不动）；③ 驱动/扩展侧 `setLocale` → **0 调用**（仅 `packages/ui` 实现+导出、两处文档注释） |
| 9 | `npx vite build`（代替 `pnpm build`，见「环境」） | 退出 0；产物内可 grep 到 `redis.bytes` 的日文值「バイト」与 `redis.console` 的 zh-TW 值「批次 TTL」⇒ 驱动词条经 `meta.ts` 副作用进入主 chunk，运行时自注册链路在真实 bundle 中成立 |

### 6. 取舍与偏离（重要）

1. **全部 10 个语言都注册**（按任务书 A.1 与建议）：实测代价为 `main-*.js` 1,528.55 kB → 1,605.12 kB（**+76.6 kB minified / +6.6 kB gzip**），并非“成本为零”。收益是消除 codegen 瓶颈、修复 8 个存量语言文件永不接线的缺口。若协调者认为包体优先，退回 `en`/`zh-CN` 只需删两处 import 列表（脚本会同步报 structural 之外的信息，不会静默）。
2. **`i18n-sync-check` 无法做到“当前仓库全绿”**：基线（改动前 `git stash` 前实测）宿主自身就是 `1216 missing / 1650 stale` 且退出 1；驱动 redis 的 8 个存量语言文件相对 `en.ts` 缺 139/72 个 key（同一批存量债）。本轨明令“不新增 key、不翻译词条”，因此**只如实上报、不刷绿**。要清零需一次专门的翻译回合（i18n-sync skill），登记给 R/hub。
3. **`npx vitest run packages/drivers`（任务书第 4 步）按当前 include 设计收集 0 文件并以 1 退出**——非回归；驱动 UI 单测入口是 `vitest.drivers.config.ts`（`pnpm test:unit:drivers`），已按 31/233 全绿执行。
4. **未跑 `pnpm build`**：pnpm 的 deps-status 检查会自行触发 `pnpm install`（本工作区硬禁止），首次尝试即以退出码 1 失败（已确认 `node_modules` 软链与 `pnpm-lock.yaml` 未被改动，`find node_modules -newermt '-20 minutes'` = 0 命中），改用等价 `npx vite build`（含 `tsc --noEmit` 单独执行）完成打包验证。
5. **key 优先级语义微调**：原先宿主 `registerTranslations(DRIVER_LOCALES)` 在宿主 eager 字典之前注册（宿主 key 覆盖驱动同名 key）；现在驱动包随 UI 装载注册，故**同名 key 由驱动胜出**。驱动 key 全部带 `redis.` / `mongo.` 前缀，实际无碰撞；如需“宿主永远压制驱动”，后续可在 `@datazen/ui` 加注册优先级（越界，未做）。
6. **语义可见性变化**：`getAllTranslations(locale)` 现在是共享注册表快照 ⇒ 含已加载的 lazy 域包、且驱动 key 只在驱动 UI 装载后出现（任务书 C.2 已预告）；宿主内建 locale 之外的 code 仍回落 `en`，`locales.test.ts` 相应断言不变。
7. **C.5 文档**：`packages/drivers/*/README.md` **一个都不存在**（18 个驱动目录全无 README）。按任务书指令不新建与 `decouple-docs` 轨重复的文档，约定改由代码内注释承载（`locales/index.ts` 头部写明自注册契约 + `i18n-sync-check` 守门），文档落点登记给 `decouple-docs`。

### 7. 未尽事项 / 风险

- 驱动词条翻译债：`redis` 9 个非 en 文件缺 139（zh-CN 缺 72）个 key，需要独立翻译回合（i18n-sync skill）补齐后 `i18n-sync-check` 才会绿。
- `AGENTS.md` / `CONTRIBUTING.md` / `.gitignore` / `docs/architecture/**` 仍描述 `generated-locales.ts`（越权，交 `decouple-docs`）。
- `i18n-core` 轨 progress.md 里对 `DRIVER_LOCALES` 的历史描述未改（他轨文件，禁改）。
- 后续 Wave 4 的 lint 规则需覆盖：驱动/扩展侧 `setLocale`、驱动侧 `src/**` import 两类违规（本轨靠人工 Grep 自证）。

## 留待 R 回归

- **R-1（E2E，验收 #10）**：`zh-CN` 下 redis 工作台/键浏览器/控制台文案为中文，切回 `en` 立即生效；顺带覆盖 mongodb 文档视图 `mongo.*`。构建须 `pnpm tauri:build:webdriver`（或 `pnpm e2e`），本轨未跑 GUI。
- **R-2**：`--drivers=basic`（不含 mongodb）与 `--drivers=all` 两种选型下 `pnpm build` / `tauri:dev` 均无缺失词条导致的 raw-key 泄漏（重点看 redis 工作台标题、控制台提示）。
- **R-3**：`npx vitest run --config vitest.drivers.config.ts`（31 files / 233）+ `npx vitest run src packages/driver-sdk packages/ui`（412 / 4243）+ `npx vitest run scripts`（21 / 190）在 R 环境复跑一致。
- **R-4**：`node scripts/i18n-sync-check.mjs` 输出结构与本轨实测逐项一致（宿主段数字不变、驱动段 8×139 + 1×72），并在翻译回合后转为退出 0。
- **R-5**：Pro/EP 与 wapp 侧若开始自带词条，验证其 `registerTranslations` 与驱动/宿主词条无前缀冲突。
- **R-6（Tester 补登，对应 O-2）**：`--drivers=basic`（不含 mongodb）构建下打开任意非 document 模式连接并触发
  `getConnectionView()` 的未知 mode 兜底路径，确认 `DocumentConnectionView` 的 20 处 `t('mongo.*')`
  是否显示为 raw key（改动前后同源，属既有耦合，但需在真实 GUI 取证一次）。前置：webdriver 构建 + 一条 hbase/vector 类连接。
- **R-7（Tester 补登，对应 O-3）**：`basic` 与 `all` 两种选型下走查驱动 UI 对宿主 key 的依赖
  （`common.*` 32 处、`newConn.*` 22 处、`sqlserver.*` 4 处）：新建连接表单（redis / sqlserver）、
  工作台按钮与右键菜单文案不得出现 raw key；重点验证「驱动 UI 组件不再经 `src/hooks/useI18n`
  间接拉起宿主 `src/locales` 副作用」后宿主字典仍在渲染前注册完毕。
- **R-8（Tester 补登，对应 O-1 / BUG-001 修复项）**：若协调者裁定退回 `en`+`zh-CN` 两档，
  需同时复量 main chunk（预期 1,528.55 kB / gzip 460.56 kB）并确认 `i18n-sync-check` 的 structural
  校验仍绿；若裁定保留 10 语言，则补一条「扩展经 `registerLocale()` 引入第 3 语言时驱动词条命中」
  的手工验证（当前宿主语言下拉只有 en / zh-CN，8 个驱动语言运行时不可达）。

## Tester 复测记录（commit 9d4016295）

独立实例全新复测（与 Coder 无共享状态；一切数字为本人实测）。worktree `feature/i18n-drivers`，
起点 `git status` 干净，结束时 `git status` 仅剩本文件与 `bugs.md`（探针/临时文件已全部删除）。
实际基准：`d172476fc` 为分支父提交（任务书写 `fd23a66a8`，两者仅差一个 hub 聚合提交，非祖先关系；
`fd23a66a8..HEAD` 的 `hub.md` 差异来自该聚合提交，**本轨 7 个 commit 均未触碰 `hub.md`**——已逐 commit 核对）。

### 1. 阶段 B 独立复跑（Coder 自报 vs Tester 实测）

| 命令 | Coder 自报 | Tester 实测 | 判定 |
| --- | --- | --- | --- |
| `node scripts/resolve-drivers.mjs --codegen-only --drivers=all` | 成功，仅产 generated.ts/driver_init.rs | exit 0；同样两文件产出，`src/extensions/` 仅 `generated.ts` + `generated-pro.ts`；`generated.ts` 第 10/15 行确认 `redis/ui/shared/meta`、`mongodb/ui/meta` 为静态 import（挂载点选择与真实装载链路一致） | ✅ |
| `npx tsc --noEmit -p tsconfig.json` | 0 error | **0 error**（`--drivers=all` 与 `--drivers=basic` 两档分别实测） | ✅ |
| `npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui` | 26 files / 218 / 0 | **26 files / 218 passed / 0 failed** | ✅ |
| `npx vitest run --config vitest.drivers.config.ts`（驱动全量） | 31 / 233 / 0 | **31 files / 233 passed / 0 failed** | ✅ |
| `npx vitest run src packages/driver-sdk packages/ui` | 412 / 4243 / 0 | **412 files / 4243 passed / 0 failed**（= 基线 4241 + 本轨 2 例，逐文件核对为 ui i18n 快照 1 例 + locales 驱动可见 1 例） | ✅ |
| `npx vitest run scripts` | 21 / 190 | **21 files / 190 passed** | ✅ |
| `npx vite build` | exit 0；main 1,605.12 kB / gzip 467.15 | **exit 0；main-rSNK-HW9.js 1,605.12 kB / gzip 467.15 kB**；`main-*.js` 内 grep 到 `バイト`(redis/ja) 与 `批次 TTL`(redis/zh-TW) → 驱动词条确在 main chunk、启动即注册 | ✅ |
| `npx vitest run packages/drivers`（任务书验收 #6 原文） | 按 include 收集 0 文件、退出 1 | 复现：`No test files found, exiting with code 1`（默认 include 不含 `packages/drivers/**`）→ **非回归**，驱动入口是 `vitest.drivers.config.ts` | ⚠️ 任务书命令本身需更正 |
| `node scripts/i18n-sync-check.mjs` | 2400 missing / 1650 stale / 0 driver issue，退出 1 | **同一数字，exit 1**；算术核对 `1216 + 8×139 + 72 = 2400` ✅ | ✅ |
| CI guard 连带面（Coder 未跑） | — | `node scripts/check-managed-stubs.mjs` ok / `node scripts/check-id-terminology.mjs` ok（1717 files，SKIP_FILES 缩减后无影响） | ✅ |
| Rust（`src-tauri/**`、`Cargo.lock`、`Cargo.toml`） | 未改 | `git diff --name-only` = 0 命中 → **判定不受影响，未跑 `cargo test`** | ✅ |

### 2. 验收标准逐条对照（任务书 10 条）

| # | 验收项 | 实测 | 判定 |
| --- | --- | --- | --- |
| 1 | 驱动 UI `from '…src/` 命中 = 0（除裁定 fixture） | **2 命中**，均在 `redis/ui/__tests__/redisKeyWebContextMenu.test.tsx:5,9`（已裁定夹具） | ✅ |
| 2 | 全仓无 `DRIVER_LOCALES` / `generated-locales` 残留 | 生产代码 / `scripts/` / `src/` / `packages/` / `e2e/` **0 命中**；仅 `AGENTS.md`×2、`CONTRIBUTING.md`×1、`.gitignore`×1、`docs/**`（越界不动，Coder 已登记） | ✅ |
| 3 | codegen 成功且不再产 generated-locales | 见上表；另测干净 checkout 等价：删 `generated.ts` → `node scripts/ensure-generated-drivers.mjs --drivers=basic` 正常补齐（25 import、redis 11 处、mongodb 0 处） | ✅ |
| 4 | `tsc --noEmit` 0 error | 0 error（all + basic 两档） | ✅ |
| 5 | redis UI 218/0 且**不得靠改断言保绿** | 218/0；`git diff` 逐文件核实 8 个测试文件**只有 `vi.mock` 落点那一处 hunk**，断言文本一字未动；改动前它们本就是 `t: (key) => key` 回显，**不存在从译文断言退化为 raw key 的用例** | ✅（但见 §3-B3 与 BUG-001） |
| 6 | `npx vitest run packages/drivers` 全绿 | 命令本身收集 0 文件（配置使然）→ 以 `vitest.drivers.config.ts` 全量代跑：31/233 全绿 | ✅（附命令更正建议） |
| 7 | `src packages/driver-sdk packages/ui` 全绿 | 412 / 4243 全绿 | ✅ |
| 8 | `npx vitest run scripts` 全绿 | 21 / 190 全绿 | ✅ |
| 9 | sync-check 能报驱动缺失 + 当前仓库全绿 | 能报（四分支实测，见 §3-B6）；**「当前全绿」不可达成且属既有债**（基线版脚本在 `fd23a66a8` 提交上实测就是 1216/1650 + exit 1）→ Coder「如实上报不刷绿」判定成立 | ✅（半条按事实修正） |
| 10 | 运行时切换语言 | 以 bundle 取证（main chunk 含 10 语言值）+ 驱动套件探针取证；GUI 实机切换按任务书登记 R-1 | ✅（R-1 待跑） |

### 3. 重点审查项逐条判定（协调者指定 6 项）

**B1 — 全部 10 语言自注册的体积代价**：Coder 数字**成立且归因准确**。三档受控对照（同 config，仅切装配）：
无驱动词条 1,501.93 kB / gzip 452.27 → 仅 en+zh-CN（= 改动前真实基线）1,528.55 / 460.56 → 全 10 语言 1,605.12 / 467.15；
本轨净增 **+76.57 kB min / +6.59 kB gzip**。确认驱动字典被 `ui/**/meta.ts` 静态 import 拉进 **main chunk**（启动即加载，非按需）。
被忽略的低成本替代：宿主自身 eager 集合只有 `BUILTIN_LOCALES = ['en','zh-CN']`（`builtinLocales.ts` 由 codegen 生成），
设置页语言下拉亦只有这两项（`SettingsContent.tsx:91-95`），`registerLocale()` 生产侧零调用 →
**这 8 个语言在现网运行时不可达**；而宿主 `lazyPacks.isBuiltin()` 同样硬编码 en/zh-CN，
故「按键命中时再注册」需要新的共享机制（不属本轨遗漏）。已量化登记为 **O-1**，请协调者在
「认可 76.6 kB 消灭 8 个死文件」/「退回两档并连同宿主可选语言集合一起做」之间明确裁定（登记 R-8）。

**B2 — 测试装配改动**：断言一字未改（§2 第 5 条），8 个 partial-mock 落点迁移正确，驱动侧零 `src/` import。
但**协调者担心的方向恰好相反并已被证实**：`src/test/driverUiSetup.ts` 直接 import 驱动 `locales/index.ts`，
**绕过了 `meta.ts` 挂载点**，即 harness 会掩盖「真实装载链路」是否生效；叠加「218 用例全部与语言无关」
（把 harness 三行 import 清空后 218 仍全绿，实测日志见 BUG-001），得到结论：
**驱动 UI 套件对本轨核心链路的常驻有效断言 = 0 条**。不依赖 harness 的证据我已自行取证（见 B3），
但它只是临时探针，未进入仓库 → 登记 **BUG-001**（中，待修复）。

**B3 — 「真绿非假绿」反证（亲测）**：在清空 harness 的最坏情形下，只 import `../shared/meta` 的探针得到
`t('redis.batchDelete') === 'Delete selected'` 且 `getRegisteredTranslations('en')` > 100 key；
mongodb 同法得 `t('mongo.collections') === 'Collections'`、en/zh-CN/ja/pt-BR 各 15 key（10 语言全注册）
→ **机制为真，不是「查不到就回显 key」的假绿**。反向：再把 `meta.ts` 的 `import '../../locales'` 注释掉，
探针以 `expected 'redis.batchDelete' to be 'Delete selected'` **失败**，而同一次运行 **218 常驻用例仍全绿**
→ 无法满足协调者要求的「≥2 个常驻驱动用例在移除自注册后失败」，因为**这类用例根本不存在**。
所有探针/临时文件已删除，`git status` 干净。

**B4 — codegen 链删除完整性**：(a) 干净 checkout 等价场景（删 `generated.ts` 后 `ensure-generated-drivers.mjs`）
与 `--codegen-only --drivers=all` 均正常产出 `generated.ts` / `driver_init.rs`，且 `--drivers=basic` 档
`npx vite build` 亦 exit 0；(b) 遗留 gitignored `generated-locales.ts` 在开发机上**仍可 0 error 通过 tsc**
（其 import 的 `RedisTranslationKey` / `MongoTranslationKey` 与默认导出均未删）→ 无悬空引用，仅惰性死文件（O-6）；
(c) 7 个 commit 内**无任何 codegen 产物**（`generated.ts` / `driver_init.rs` / `.driver-features.json` /
`capabilities/default.json` 均不在提交面；`Cargo.toml` / `Cargo.lock` / `hub.md` 未动）；
(d) `npx vitest run scripts` 21/190 全绿。附带：`resolve-drivers.mjs` 删码后无失效 import / 死函数
（`mkdirSync` / `dirname` / `workPath` 均有其他用处）；`check-managed-stubs.mjs`、`check-id-terminology.mjs` 实跑 ok。
**未发现问题。**

**B5 — `I18nKey` 收敛的连带影响**：宿主 40+ 处 `I18nKey` 消费点全部为类型位（`t(key as I18nKey)` /
`Record<string, I18nKey>`），`TranslationKey | (string & {})` 使类型退化为 string 超集 → 零 breakage（tsc 0 error 佐证）。
`getAllTranslations()` / `getTranslation()` 改走新快照 API 后生产消费者仍只有测试/工具路径。
宿主侧读取驱动前缀 key 的位置**只有一处**：`src/windows/connection/DocumentConnectionView.tsx`（20 处 `mongo.*`），
其可达性判据是「mongodb 是否被选型」，与改动前（codegen 只合并已选型驱动）**完全同集** →
**判定非本轨回归**（既有宿主硬编码耦合，登记 O-2 + R-6；`getConnectionView()` 的未知 mode 兜底放大该风险）。
两个 HTML 入口（`index.html` / `window.html`）均指向 `src/main.tsx` → 不存在「某窗口未加载 generated.ts 却渲染驱动 key」的时序窗口。

**B6 — sync-check 未全绿**：(a) 以只读方式复核基线：`git show fd23a66a8:scripts/i18n-sync-check.mjs`
另存为 `scripts/.tester-base-sync-check.mjs` 后运行 = `1216 missing / 1650 stale`、exit 1 →
**债务确为改动前既有，非本轨引入**（临时文件已删）。(b) 新增驱动扫描四分支实测全部如实上报：
删 key → `Missing 1 key(s)` 且合计 2400→2401；加 key → `Extra 1 key(s): redis.testerExtraKey`（多余不致命，与宿主一致）；
删 `locales/index.ts` → `locales/index.ts is missing` + `1 driver pack issue(s)` + exit 1；
index 去掉一个 import → `does not import 1 locale file(s): de.ts` + exit 1。
key 抽取正确性独立佐证：我用朴素正则统计 redis `en`=340 / `de`=201 / `zh-CN`=268（→ 139 / 72 缺失），
与脚本正则给出**完全相同**的 key 数（340/201），无「折行值 key 不可见」的假缺失。(c) 退出码语义：
`.github/workflows/ci.yml:66-68` 该步骤为 `continue-on-error: true`（"Guard i18n sync (warning only)"），
且基线本就 exit 1 → **不会打破 CI**；`package.json` 无脚本调用方。**未发现问题**，但脚本新增逻辑
0% 单测覆盖（实测 v8 报告 0/0/0/0，`20-246` 未触达）→ 登记 **BUG-002**。

### 4. 阶段 C 覆盖率实测（v8，改动核心模块）

| 模块 | Stmts / Branch / Funcs / Lines | 判定 |
| --- | --- | --- |
| `packages/ui/src/i18n.ts`（含新增快照 API） | 100 / 100 / 100 / 100 | ✅ 达标（新 API 的只读性、未知 locale、跨包合并三点均有断言） |
| `packages/drivers/redis/locales/index.ts` | 100 / 100 / 100 / 100 | ⚠️ 数字达标**仅因** `src/locales/locales.test.ts` 显式动态 import；常驻驱动套件对该文件零触达 |
| `packages/drivers/mongodb/locales/index.ts` | 100 / 100 / 100 / 100 | 同上 |
| `src/locales/index.ts`（本轨语义改写主体） | 95.23 / 83.33 / 100 / 95（未覆盖 L99：`getHostTranslations` 非内置 locale 回落 `en`） | ✅ 达 80% 线；L99 建议补 1 例（并入 BUG-001 修复回合即可，不单开 Bug） |
| `src/test/driverUiSetup.ts` | 报告不含（`vitest.config.ts` coverage.exclude `src/test/**`） | 纯 3 行 import 装配文件，无分支；**豁免理由**：可执行语句仅模块副作用，且被配置排除。真正缺口是它「测错了对象」→ BUG-001 |
| `ui/**/meta.ts` 副作用行（本轨核心挂载点） | 常驻套件 0 条断言（变异测试：删除该行 CI 全绿） | ❌ **BUG-001** |
| `scripts/i18n-sync-check.mjs`（本轨 +97 行） | 0 / 0 / 0 / 0（L20-246 未触达） | ❌ **BUG-002** |

### 5. 红线复核（设计底线）

- `@datazen/ui` 唯一实现：全仓插值正则 `replace(/\{` 在 `packages/ui/src/i18n.ts` 之外 **0 命中**；无 bridge / 无兼容 re-export。
- `packages/ui/src/i18n.ts` 既有 5 个 API：diff 仅**新增** 13 行函数，其余 5 个 API 一字未动。
- 只有宿主调用 `setLocale`：`packages/drivers/**` 内 `setLocale` **2 命中且全为文档注释**、0 调用；`packages/extension-points` / `wapp-sdk` 亦 0。
- 快照 API 只读性：浅拷贝 + 值类型为 string → 外部写回不影响注册表（实测篡改后重读原值）；未知 locale 返回 `{}`（与 `t()` 回落 en 不同，已登记 O-5 提醒勿作第二查表入口）。
- 未新增/未重命名/未翻译任何 key：`packages/drivers/*/locales/{en,zh-CN,…}.ts` 与 `src/locales/**` 词典文件**零改动**（`git diff --name-status` 全量清单中仅两个新增 `locales/index.ts`）；宿主 2,181 key × 驱动 355 key **交集 0**，驱动 key 100% 带前缀 → O-4。
- 越界检查：`src-tauri/**`、`Cargo.lock`、`Cargo.toml`、`packages/pro-extensions/**`、`src/hooks/useI18n.ts`（未删未改）、`src/lib/localeSync.ts`（未改）、`hub.md`（7 commit 均未触碰）全部 0 命中；32 个驱动 UI 文件的改动逐行核对为「`useI18n` import 单行换源」，无夹带。

### 6. 判定与交棒

`TEST_FAILED`：功能与红线全绿，但 `i18n-drivers-BUG-001`（注册链路零常驻覆盖，变异测试可静默删除挂载点）
与 `i18n-drivers-BUG-002`（守门脚本 0% 覆盖）为本轨「核心链路必须可被自动化守住」的硬标准缺口，
`BUG-003` 为注释语义失真（其中 2 处属任务书禁改文件，需协调者裁定落点）。
缺陷明细、实测日志与建议用例见 `docs/development/coordination/tracks/i18n-drivers/bugs.md`。


## Coder Bug 修复记录（第 1 轮）

Phase = `READY_FOR_TEST`（Bug 修复循环第 1 轮；修复者不自标「已修复」，`bugs.md` 三条状态已推进为
**待复测**，判定权在 Tester）。本轮范围严格限定在 3 条 Bug + 协调者裁定，无范围外改动。

### 0. 协调者裁定登记

- **O-1 裁定：10 语言全量自注册维持不变**，包体代价（相对真实改动前基线 **+76.57 kB min / +6.59 kB gzip**）
  由用户明确认可。本轮**未**改回「仅 en + zh-CN」，**未**引入任何惰性/按需注册机制。
  Tester 的三档实测数字（同 config，仅切 locale 装配）一并归档：

  | 装配 | main chunk (min) | gzip |
  | --- | --- | --- |
  | 无驱动词条装配（meta 副作用注释掉） | 1,501.93 kB | 452.27 kB |
  | 仅 `en` + `zh-CN`（= 本轨前 codegen 实际接线集合） | 1,528.55 kB | 460.56 kB |
  | 全部 10 语言（HEAD） | **1,605.12 kB** | **467.15 kB** |

- **O-2 裁定：本轨不修**。宿主 `src/windows/connection/DocumentConnectionView.tsx` 20 处 `t('mongo.*')`
  与改动前同集、非本轨回归 → 保持登记在「留待 R 回归」（R-6）/ Wave 4 lint 里程碑，本轮零改动。
- **BUG-003 授权范围**：协调者授权对 `src/hooks/useI18n.ts` / `src/lib/localeSync.ts`
  **仅改注释文案**（语句、import 行为、语义零改动）；`src/locales/t.ts` 本就在本轨职权内。
- **验收 #6 口径更正**：已按 Tester 结论把任务书第 6 条改为
  `npx vitest run --config vitest.drivers.config.ts`（= `pnpm test:unit:drivers`），见上文修订行。

### 1. BUG-001（中）：驱动词条自注册挂载点零常驻覆盖

**改法**

1. `src/test/driverUiSetup.ts` 的驱动装配改走**真实装载链路**：
   `import '../../packages/drivers/redis/ui/shared/meta'` /
   `import '../../packages/drivers/mongodb/ui/meta'`（原来是直接 import 驱动 `locales/index.ts`，
   绕过 meta 挂载点，属假绿来源）。宿主侧 `import '../locales'` 保持不变；文件头注释同步改写，
   说明「为什么必须走 meta 而不是 locales」。两个 meta 入口除副作用行外只有
   `import type { DatabaseTypeMeta }`，装配期不引入额外运行时模块，故对既有 218 用例零影响。
2. 每个带词条包的驱动补 1 个常驻注册链路套件（新增 2 文件 / 8 用例）：
   - `packages/drivers/redis/ui/__tests__/localePackRegistration.test.ts`（4 例）
   - `packages/drivers/mongodb/ui/__tests__/localePackRegistration.test.ts`（4 例）
     （两文件首行 `/** @vitest-environment node */`：需要 `node:fs` 读目录做守门断言，不用 DOM）

   用例只 `import '../shared/meta'` / `import '../meta'`，**绝不 import `locales/index.ts`**，断言：
   (a) `t('redis.batchDelete') === 'Delete selected'`、`t('redis.console') === 'Console'` /
   `t('mongo.collections') === 'Collections'`，并附一条「未注册 key 必须原样回显」的反向断言
   （`redis.definitelyNotAKey` → 回显）堵死「查不到就回显 key」式假绿；
   (b) **守门断言**：`readdirSync(locales/)` 得到的 `*.ts`（除 `index.ts`）code 集合，与套件内
   10 个 locale code 字面量集合（`en/zh-CN/zh-TW/de/es/fr/ja/ko/pt-BR/ru`）逐项相等
   —— 新增语言文件但漏 `index.ts` import、或漏更新本清单，均在此转红；
   (c) 每个语言的字典**逐 key** 已在共享注册表命中且值与词条文件一字不差
   （`getRegisteredTranslations(code)`，10 语言 × 全部 key）；
   (d) 连字符 code 只以精确字面量命中（`pt-BR` / `zh-CN` / `zh-TW` 有驱动 key；
   `pt_BR` / `zh_cn` 快照为 `{}`）—— 钉住 Tester 指出的「拼写错误无人发现」缺口。
3. 采纳的落点取舍：BUG-001 建议 3（守门脚本补「meta 是否 import locales」的第 3 条结构断言）
   **本轮不做** —— 该链路现由常驻用例 (a)(c) 直接守住（比脚本的字面量扫描更强：跑真模块、断真值），
   而把它塞进 `i18n-sync-check` 会扩大 BUG-002 的脚本改动面。留待 Wave 4 的 lint 里程碑统一处理。

**反证（Tester 要求的判据：至少 1 个常驻用例在移除 `import '../locales'` 后转红）**

| 探针（全部为临时改动，跑完还原，`git status` 已核对干净） | 结果 |
| --- | --- |
| A：仅注释掉两个 meta 的 `import '…/locales'`（harness 保持新写法） | `Test Files 2 failed \| 31 passed (33)`；`Tests 6 failed \| 235 passed (241)` —— 失败项恰为 2 套注册链路各 3 例（(a)(c)(d)，(b) 是纯文件集合守门、不依赖注册，符合预期继续绿）；子集 `packages/drivers/redis/ui` = `1 failed \| 26 passed (27)`、`3 failed \| 219 passed (222)` |
| B：`src/test/driverUiSetup.ts` 清空为 `export {}`（meta 完好） | `33 files / 241 passed / 0 failed` —— 常驻用例自带 meta import，**不再依赖 harness**，即装配点无法掩盖挂载点 |
| C：B + A 同时（最坏情形，等价 Tester 的「注册链路被完全切断」） | 同 A：`6 failed \| 235 passed (241)` |
| 还原后 | `33 files / 241 passed / 0 failed` |

对照 Tester 首轮记录的缺陷点：同样的 A 变异在修复前是「218 常驻用例零感知」，现在稳定产生 6 条红。
`ui/**/meta.ts` 副作用行与 `src/test/driverUiSetup.ts` 由此进入常驻覆盖闭环。

### 2. BUG-002（低-中）：`scripts/i18n-sync-check.mjs` 驱动扫描逻辑 0% 覆盖 / 无导出面

**改法**：按 `scripts/check-id-terminology.mjs` 的既有惯例把扫描逻辑提为导出纯函数，CLI 只做装配：

- `extractPackKeys(src)` —— 签名由「文件路径」改为「文件源码」（纯函数）；
- `findDriverLocalePacks(driversDir)` —— 目录参数化，「目录不存在」分支可测；
- `inspectDriverLocalePack(pack)` → `{ lines, structural, missing }`（返回行而非直接 `console.log`）；
- `checkDriverLocalePacks(driversDir)` → `{ packCount, lines, structural, missing }`；
- `buildSyncReport({ totalMissing, totalStale, totalStructural, driverPackCount, hostLocaleCount })`
  → `{ lines, exitCode }`；
- 原先在模块顶层裸执行的报告体收进 `runCli()`，末行以
  `if (process.argv[1] && process.argv[1].endsWith('i18n-sync-check.mjs')) runCli();` 守卫
  （导入模块不再触发扫描、不再改 `process.exitCode`，这是能被单测导入的前提）。
- 新增 `scripts/__tests__/i18n-sync-check.test.mjs`（18 例，`@vitest-environment node` + `mkdtempSync`
  fixture，落在 `vitest.config.ts` 已 include 的 `scripts/__tests__/**/*.test.{ts,mjs}` 路径内）：
  折行值 key 可见（含「折行值不误报 missing」）、单/双引号与模板字符串值、非词条行忽略、
  驱动目录不存在、无 `en.ts` 目录与 `node_modules` 跳过、pack 排序、
  **全同步（0 issue 0 行）**、缺 `locales/index.ts`、index 漏 import 某语言、缺 key 计 missing
  （含 >10 截断的 `…`）、多余 key 只列不计、多包聚合、报告的三种退出码组合（含 structural-only 致命）。

**CLI 逐字一致性证据**：改动前后 `node scripts/i18n-sync-check.mjs` 全量输出
`md5 = c2ffde854c8b1dde7bd6fce1bfa28b44`（55 行，含 `Summary: 2400 missing key(s), 1650 stale
translation(s) across 8 host locales; 0 driver pack issue(s) across 2 driver locale pack(s).`），
`diff` 无差异，退出码仍 **1**（`.github/workflows/ci.yml` 的 `continue-on-error: true` 依赖不变）。
负向自证（临时改后还原）：删 `redis/locales/de.ts` 的 `redis.batchDelete` →
`[driver.redis/de] Missing 140 key(s): redis.batchDelete, …`、合计 2400→**2401**；
删 `redis/locales/index.ts` 的 `import de from './de'` →
`[driver.redis] locales/index.ts does not import 1 locale file(s): de.ts`、
`1 driver pack issue(s)`、合计仍 2400；两者退出码均 1。

**覆盖率**（`npx vitest run scripts --coverage --coverage.include='scripts/i18n-sync-check.mjs'`）：
该文件未覆盖行仅剩 `39-59`（宿主 `extractKeys`/`extractLocaleKeys`，本轨未触碰的存量逻辑）与
`210-329`（`getEnKeysAtRef` / `getLatestTag` / `runCli` CLI 装配，其中 main 守卫已 `istanbul ignore next`）
⇒ **本轨新增/重构的驱动扫描段（L60-209，5 个导出函数）行覆盖 100%**，满足 ≥80% 目标（BUG-002 前为 0%）。

### 3. BUG-003（低）：三处宿主注释语义失真

按裁定仅改注释文案，语句与 import 行为一字未动（`git diff` 三文件各 1-2 行、全为 `//`）：
`src/locales/t.ts:1`、`src/hooks/useI18n.ts:8`、`src/lib/localeSync.ts:3-5`
统一改为「注册宿主 eager 字典（驱动/扩展包经各自 locales 入口自注册，宿主不聚合）」。

### 4. 本轮自验（真实数字，全跑）

| # | 命令 | 实测 |
| --- | --- | --- |
| 1 | `node scripts/resolve-drivers.mjs --codegen-only --drivers=all` | exit 0（`generated.ts` / `driver_init.rs` 正常产出，无 `generated-locales.ts`） |
| 2 | `npx tsc --noEmit -p tsconfig.json` | **0 error** |
| 3 | `npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui` | **27 files / 222 passed / 0 failed**（基线 26/218 + 新套件 1/4） |
| 4 | `npx vitest run --config vitest.drivers.config.ts`（驱动全量 = 更正后验收 #6） | **33 files / 241 passed / 0 failed**（首轮 31/233 + 2 files / 8 tests） |
| 5 | `npx vitest run src packages/driver-sdk packages/ui` | **412 files / 4243 passed / 0 failed**（与首轮逐项一致，本轮零宿主断言改动） |
| 6 | `npx vitest run scripts` | **22 files / 208 passed / 0 failed**（首轮 21/190 ⇒ **新增 1 file / 18 用例**） |
| 7 | BUG-001 反证 A / B / C + 还原 | 见 §1 表：A/C = `6 failed \| 235 passed (241)`；B = 241 全绿；还原后 241 全绿 |
| 8 | `node scripts/i18n-sync-check.mjs` | 输出 md5 与首轮基线逐字一致（`c2ffde854c8b1dde7bd6fce1bfa28b44`，`2400 missing`，exit 1）；负向自证 2 次（2401 / 1 driver pack issue）后还原 |
| 9 | 三条红线 Grep | ① `packages/drivers/**` 中 `from ['"].*src/` = **2 命中**（仅已裁定 fixture）；② `DRIVER_LOCALES\|generated-locales\|DriverTranslationKey` 在 `scripts/ src/ packages/` **0 命中**（本轮新写的两处注释措辞已改写，不再复述退役标识符）；③ 驱动/扩展侧 `setLocale` **0 调用**（4 命中全为文档注释：两个 `locales/index.ts` 头 + 本轮两个新用例的说明行） |
| 10 | `git status --short` | 提交后干净；探针文件（`/tmp/*.bak`、变异行、临时 harness）全部还原，无 codegen 产物入库（`src/extensions/generated*.ts`、`driver_init.rs`、`.driver-features.json`、`capabilities/default.json` 均不在提交面） |

附带自查：`node scripts/check-managed-stubs.mjs` / `check-id-terminology.mjs` /
`check-module-layers.mjs` 实跑 exit 0（新增测试文件未触发任何 guard）。

### 5. 本轮改动面

| 文件 | 改动 |
| --- | --- |
| `src/test/driverUiSetup.ts` | 驱动装配改走 meta 入口 + 注释改写（BUG-001 建议 1） |
| `packages/drivers/redis/ui/__tests__/localePackRegistration.test.ts`（新） | 常驻注册链路套件 4 例（BUG-001 建议 2） |
| `packages/drivers/mongodb/ui/__tests__/localePackRegistration.test.ts`（新） | 同上 |
| `scripts/i18n-sync-check.mjs` | 扫描逻辑提为 5 个导出纯函数 + `runCli()` main 守卫（BUG-002） |
| `scripts/__tests__/i18n-sync-check.test.mjs`（新） | 18 例 fixture 分支用例（BUG-002） |
| `src/locales/t.ts` / `src/hooks/useI18n.ts` / `src/lib/localeSync.ts` | 仅注释文案（BUG-003） |
| 本文件 + `bugs.md` | 裁定归档、验收 #6 口径更正、修复记录、状态→待复测 |

未触碰：`packages/ui/src/i18n.ts`（既有 5 API 与新增快照 API 一字未动）、任何词条文件
（`packages/drivers/*/locales/*.ts` 词典、`src/locales/**` 词典）、`src-tauri/**`、
`Cargo.toml` / `Cargo.lock`、`packages/pro-extensions/**`、`hub.md`、他轨 progress.md。

## Tester 第 2 轮复测记录（commit a61e42f14）

Phase = `TEST_DONE (PASSED)`。全新 Tester 实例独立复测（与 Coder / 首轮 Tester 无共享状态；
前轮报告仅作待验证断言，本文所有数字为本人实测）。起点 `git log` 确认 HEAD = `a61e42f14`
（`fc78d3014` / `9c5dae59b` / `2d3253300` / `a61e42f14` 四修复提交齐备）、`git status` 干净；
所有探针/临时文件已还原，结束时 `git status --short` 干净（本文与 bugs.md 除外）。

### 1. BUG-001（中）复验：注册链路挂载点常驻覆盖 → **已修复**

- **harness 审查**（`src/test/driverUiSetup.ts` diff）：驱动装配已从直接 import 驱动
  `locales/index.ts` 改为 import 真实入口 `packages/drivers/redis/ui/shared/meta` /
  `packages/drivers/mongodb/ui/meta`；两 meta 除副作用行外仅 `import type { DatabaseTypeMeta }`
  （42/39 行，无其它运行时依赖），装配期不夹带额外模块。
- **新增套件逐例判定**（redis 4 例 / mongodb 4 例，`localePackRegistration.test.ts`）：全部为真断言，
  无填充式断言。(a) 断具体译文值（`Delete selected` / `Console` / `Collections` / `mongo.noIdHint` 全文）
  **且**附「未注册 key 原样回显」反向对照（`*.definitelyNotAKey` → 回显）；(b) `readdirSync` 的
  `locales/*.ts`（除 index）code 集合与套件 locale 字面量集合 `toEqual` 逐项相等（守门）；
  (c) 10 语言 × 全部 key 逐一 `in` 注册表且值与词条文件一字不差；(d) `pt-BR`/`zh-CN`/`zh-TW` 连字符
  精确命中 + `pt_BR`/`zh_cn` 快照为 `{}` 的反证。唯一弱断言是 redis 的 `en` key 数 `>100`（mongodb 已用
  `toEqual(Object.keys(en))` 精确式），但其同用例内另有两条精确译文断言兜底，不构成假绿风险。
- **反证三组（本人独立重跑，跑完全部还原）**：
  | 探针 | 预期 | 实测 |
  | --- | --- | --- |
  | A：仅注释两 meta 的 `import '…/locales'`（harness 保持新写法） | 常驻套件转红 | 驱动全量 **`2 failed \| 31 passed (33)` files，`6 failed \| 235 passed (241)`**；redis 子集 **`3 failed \| 219 passed (222)`**。被红用例（两驱动各 3 条）：`registers the pack through the meta.ts side effect (t() resolves real strings)`、`registers every key of every shipped dictionary with its shipped value`、`registers each locale under its exact hyphenated code only`；文件集守门例 `keeps the registered locale-code literals in sync with locales/*.ts` 按设计不依赖注册、保持绿。与 Coder 自报数字逐字相符 |
  | B：`driverUiSetup.ts` 清空为 `export {}`（meta 完好） | 仍全绿（不再依赖 harness 旁路） | **`241 passed (241)` 全绿** ✅ |
  | C：新增游离 `packages/drivers/redis/locales/xx.ts`（不进 `locales/index.ts`） | 「locale 集合一致」守门例转红 | **`1 failed \| 221 passed (222)`**，红例即 `keeps the registered locale-code literals in sync with locales/*.ts` ✅（守门有效） |
- **覆盖闭环自证**：在 harness 禁用状态下仅跑两个注册套件（node env），v8 实测
  `redis/ui/shared/meta.ts`、`redis/locales/index.ts`、`mongodb/ui/meta.ts`、`mongodb/locales/index.ts`
  四文件 **100/100/100/100**（stmts/branch/funcs/lines）→ 挂载点进入常驻覆盖，首轮缺口关闭。
- **建议 3（脚本层「meta 是否 import locales」结构断言）故意未做的独立判定：可接受，不判 Bug。**
  常驻用例跑真模块、断真值，A（删挂载行）与 C（漏接线语言文件）两类变异均被稳定拦截，
  拦截能力严格强于对 `meta.ts` 做字面量正则扫描；脚本扫描属重复防线，塞进
  `i18n-sync-check` 反而扩大守门脚本改动面。Wave 4 lint 统一处理合理。

### 2. BUG-002（低-中）复验：守门脚本导出面 + 单测 → **已修复**

- **CLI 行为逐字不变**：`node scripts/i18n-sync-check.mjs`（HEAD）实测 **55 行、
  md5 `c2ffde854c8b1dde7bd6fce1bfa28b44`、exit 1** —— 与 Coder 自报完全一致；
  与首轮交付 `9d4016295` 版脚本（置于 `scripts/` 同层运行）输出 **`diff` 零差异**；
  与基准 `fd23a66a8` 版 diff 仅为本轨预期的驱动扫描段（+18 行 `[driver.redis/*]`）与汇总行
  措辞（`across 8 host locales; 0 driver pack issue(s) across 2 driver locale pack(s)`），
  宿主段逐字节一致（`1216 + 8×139 + 72 = 2400`），两者退出码均 1。
  （注：首轮 Tester 与 Coder 的「逐字不变」对照对象是 `9d4016295`，非 `fd23a66a8`，语义正确。）
- **18 例逐例判定**：真实分支覆盖——折行值 key 可见 + 「折行值不误报 missing」双向、单/双引号与
  模板字符串值、非词条行忽略、drivers 目录不存在、无 en.ts 目录与 node_modules 跳过、pack 排序、
  全包同步（0 行 0 计数）、缺 `index.ts`、index 漏 import 单语言、缺 key 计数 + **>10 截断 `…`**、
  多余 key 只列不计、多包聚合、`buildSyncReport` 三态退出码（全同步 0 / missing 1 / structural-only 1 /
  stale-only 1）与汇总行整串断言。无为凑数而写的同义断言（个别 `toContain` 配合精确计数断言使用，可接受）。
- **覆盖率复核**：`npx vitest run scripts --coverage --coverage.include='scripts/i18n-sync-check.mjs'`
  实测未覆盖行仅 **`39-59`**（宿主既有 `extractKeys`/`extractLocaleKeys`，本轨零触碰其代码，
  行为以三版 CLI 输出对比佐证）与 **`210-329`**（`getEnKeysAtRef`/`getLatestTag`/`runCli` CLI 装配段，
  main 守卫带 `istanbul ignore next`）⇒ 本轨新增/重构的驱动扫描段（L60-209，5 个导出函数）
  **行覆盖 100%**，达标（首轮为 0%）。残余事实登记：`runCli` 装配本体无单测，由逐字 CLI 对比覆盖，可接受。
- **main 守卫不破坏调用方**：全仓 Grep 调用点 = `.github/workflows/ci.yml:68`（`continue-on-error: true`，
  exit 1 语义不变）、`scripts/ci-local.sh:65`、`scripts/run-full-automation-test.sh:79`、
  `.cursor/skills/i18n-sync/SKILL.md`、`AGENTS.md:203` —— 均为 `node scripts/i18n-sync-check.mjs`
  （可带 `--verbose`/`--from`）直接执行，argv[1] 以该文件名结尾 → 守卫必然触发；`package.json` 无调用方。
  实跑验证 CLI 正常出报告（上条），import 路径（新单测）无副作用。✅

### 3. BUG-003（低）复验：三处宿主注释 → **已修复**

`git diff fd23a66a8..HEAD -U0` 逐行核对：`src/locales/t.ts:1`、`src/hooks/useI18n.ts:8`、
`src/lib/localeSync.ts:3-5` 全部改动行均为 `//` 注释文本替换（localeSync 2 行注释改 3 行），
import 语句、任何可执行语句与语义零改动。✅

### 4. 全局不回归 + 红线复检（全部本人实测）

| 命令 | Coder 第 1 轮自报 | Tester 第 2 轮实测 | 判定 |
| --- | --- | --- | --- |
| `node scripts/resolve-drivers.mjs --codegen-only --drivers=all` | exit 0，仅产 generated.ts/driver_init.rs | exit 0；`src/extensions/` 仅 `generated.ts` + `generated-pro.ts` | ✅ |
| `npx tsc --noEmit -p tsconfig.json`（all / basic 两档） | 0 error（未提 basic） | **0 error / 0 error**（basic 档为本人补测） | ✅ |
| `npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui` | 27 files / 222 / 0 | **27 files / 222 passed / 0 failed** | ✅ |
| `npx vitest run --config vitest.drivers.config.ts` | 33 / 241 / 0 | **33 files / 241 passed / 0 failed**（还原后终值复跑一致） | ✅ |
| `npx vitest run src packages/driver-sdk packages/ui` | 412 / 4243 / 0 | **412 files / 4243 passed / 0 failed** | ✅ |
| `npx vitest run scripts` | 22 / 208 / 0 | **22 files / 208 passed / 0 failed**（首轮 21/190 + 1 file / 18 例） | ✅ |
| `npx vite build` | exit 0 | **exit 0；`main-rSNK-HW9.js 1,605.12 kB / gzip 467.15 kB`**（chunk hash 与首轮全同 ⇒ 本轮零包体变化）；main chunk 内 grep 到「バイト」/「批次 TTL」 | ✅ |
| 红线① 驱动 `from '…src/` | 2（裁定 fixture） | **2 命中**，仅 `redisKeyWebContextMenu.test.tsx:5,9` | ✅ |
| 红线② `DRIVER_LOCALES\|generated-locales\|DriverTranslationKey` | 生产码 0 | `src/ scripts/ packages/ e2e/` **0 命中**（仅 AGENTS/CONTRIBUTING/.gitignore/docs，越界不动，维持登记给 decouple-docs） | ✅ |
| 红线③ 驱动/扩展侧 `setLocale` 调用 | 0 调用（4 注释） | `packages/drivers/**` 内 **0 调用**（6 命中全为注释/文档行）；调用仅存在于 `packages/ui` 实现与其自身测试 | ✅ |
| 越界检查 `git diff --name-only fd23a66a8..HEAD` | — | `src-tauri/**` / `Cargo*` / `packages/pro-extensions/**` / codegen 产物 = **0**；`hub.md` 出现于两点差集中但经 `git log fd23a66a8..HEAD -- hub.md` 核实**本轨无任何 commit 触碰**（差异源于分支父 `d172476fc` 的聚合提交，`fd23a66a8` 非 HEAD 祖先，首轮已裁定同结论）；`vitest.drivers.config.ts` 为本轨 setupFiles 测试装配，属任务书范围 | ✅ |
| 既有断言放宽检查 | — | `git diff fd23a66a8..HEAD -- packages/drivers/*/ui/__tests__`：8 个 redis 用例文件各仅 1 个 hunk（`vi.mock` 落点迁移为 partial mock，断言文本零改动）；全 diff 无 `-` 侧 `it(`/`test(` 用例删除（scripts 测试的删除项仅清除已退役 codegen 夹具，首轮已审）、**无 `it.skip`/`it.only`/`describe.skip`**、无译文断言退化为 raw key；新增文件仅 2 个注册套件 + 1 个脚本单测 | ✅ |
| 附带 guard | — | `node scripts/i18n-sync-check.mjs` 汇总行与首轮逐字一致；`check-managed-stubs`/`check-id-terminology` 首轮已实跑 ok，本轮改动面不含其触达路径 | ✅ |

### 5. 判定与交棒

**TEST_DONE (PASSED)**：BUG-001 / BUG-002 / BUG-003 全部闭环（bugs.md 状态已置「已修复」），
本轮无新 Bug。O-1/O-2 按协调者裁定不判；「建议 3 留给 Wave 4 lint」取舍经独立判定为可接受。
观察项 O-3~O-6 维持原登记。「留待 R 回归」R-1~R-8 清单不变（本轮为测试/脚本/注释改动，
无新增 UI 交互路径，E2E 阶段无新增登记项；R-8 若走「保留 10 语言」分支的手工验证仍待 R 环境）。
建议轨道状态推进：`Tester 复测 → TEST_DONE (PASSED)`（见「状态」小节）。
