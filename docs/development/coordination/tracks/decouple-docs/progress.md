# Track: decouple-docs — 驱动↔宿主解耦契约文档收口

- 分支: `feature/decouple-docs`（基准 `feat/driver-decoupling` @ d172476fc）
- 角色: Coder → Tester
- 性质:**纯文档轨**，零生产代码改动（与并行轨 `i18n-drivers` 文件面完全互斥）

## 背景

`feat/driver-decoupling` 分支已完成 5 条轨：`cn-to-ui`（驱动统一用 `@datazen/ui` 的 `cn`）、`types-to-sdk`（共享类型下沉 `@datazen/driver-sdk/src/types/*`）、`fix-redis-tests`、`i18n-core`（`packages/ui/src/i18n.ts` 单一 i18n 运行时）、`cap-bridge`（宿主值能力下沉 + `bind*`/`useBound*` 注入桥）。文档仍停留在解耦前的描述，且**并行轨 `i18n-drivers` 正在落地"驱动词条自注册"**。本轨负责把新契约写成一份可直接照做的规范，供 Wave 4 的 import 护栏与后续 git 驱动作者使用。

## 必读（先读后写，禁止凭想象描述 API）

1. `AGENTS.md`、`docs/development/subagent/coder.md`、本文件。
2. 代码现状（以文件实际内容为准）：
   - `packages/ui/src/i18n.ts`（`setLocale`/`getLocale`/`registerTranslations`/`t`/`useI18n`）
   - `packages/driver-sdk/src/index.ts` 与其 `types/`、`ipc/`、`*Bridge.ts` 各模块
   - `packages/driver-sdk/src/{settingsStoreBridge,connectionStoreBridge,confirmDialogBridge}.ts`、`src/lib/nativeContextMenu.ts`、`src/hooks/useConfirmDialog.tsx`、`src/stores/*.ts` 中的 `bind*` 调用点
   - `src/lib/localeSync.ts`、`src/main.tsx`、`src/extensions/generated.ts`
   - `scripts/resolve-drivers.mjs`（驱动 codegen 与 `--drivers` 语义）
3. 既有文档：`docs/development/driver-api-dependency-boundary.md`、`docs/development/independent-driver-development.zh-CN.md` 与 `.en.md`、`docs/architecture/frontend/extensibility.md`、`docs/architecture/frontend/components.md`、`docs/development/coordination/tracks/{cap-bridge,i18n-core,types-to-sdk,cn-to-ui}/progress.md`（各轨 Coder/Tester 记录是权威落点清单）。

## 范围

1. **主交付**：重写/扩写 `docs/development/driver-api-dependency-boundary.md` 为「驱动前端与宿主解耦契约」，至少覆盖：
   - 允许 import 面：`@datazen/ui`（基础组件 + `cn` + i18n）、`@datazen/driver-sdk`（元数据/方言/Command/下沉类型/IPC 封装/注入桥）、`@datazen/extension-points`（仅 EP 契约类型）、npm 依赖；**禁止** `../../../src/**` 形态的宿主 import（含 `src/hooks`、`src/stores`、`src/lib`、`src/types`、`src/components`、`src/locales`）。
   - 宿主能力取用模式：纯函数/IPC → 直接下沉 driver-sdk；需要宿主 store/hook 运行时状态 → `bindX()` + `useBoundX()` 注入桥模式（给出宿主 bind 时机与驱动侧用法各一段最小代码示例，示例必须与实际 API 签名一致）。
   - i18n 契约：单一实现、`setLocale` **仅宿主**调用、词条由各 package 自注册（`packages/drivers/<id>/locales/index.ts` 由驱动 UI 入口 `ui/shared/meta.ts` 挂副作用 import、`<driverId>.` key 前缀、驱动侧 `t()` key 为普通 `string`、parity 由 `scripts/i18n-sync-check.mjs` 扫描 `packages/drivers/*/locales/`）。**并注明该自注册由并行轨 `i18n-drivers` 同期落地**，避免读者误以为已合并。
   - 新增能力时的落点决策表（下沉 driver-sdk / 建注入桥 / 留宿主 / 走 EP 插槽），以及"为什么不用 bridge 式回退查表"的一句话理由（防止后人重新引入）。
   - Wave 4 将上的 import 护栏预告（ lint 规则名/CI 位置尚未定，写成"待 Wave 4 落地"，不要编造脚本文件名）。
2. **同步开发者指南**：`docs/development/independent-driver-development.zh-CN.md` 与 `.en.md`（两份内容必须一一对应，英文版为中文直译，术语一致）中涉及前端 UI 的章节：把"从宿主 import"的旧示例改为新契约写法；补 i18n 与能力桥两小节（可引用第 1 项文档，不重复长篇）。
3. **勘误**：`docs/architecture/frontend/components.md`、`extensibility.md` 中与现状不符的 `@datazen/ui` / driver-sdk / i18n 描述（如仍提到 `HostLocaleBridge`、`src/lib/cn`、宿主类型出处）逐条改正；`AGENTS.md` 仅在确有一处错误时最小修改（默认不动，避免与其他轨竞争）。
4. 每处改动在 progress.md 记录「文件 → 改了哪一节 → 依据哪个源文件行」，便于 Tester 核对代码事实。

## 禁止事项

- **零生产代码/脚本/测试文件改动**（`git diff --name-only` 必须全部落在 `docs/**`；若动了 `AGENTS.md` 需在 progress 说明理由）。
- 不创建新的驱动 README（`i18n-drivers` 轨负责驱动目录内 README 的词条自注册说明，避免重复）。
- 不动 `docs/development/coordination/hub.md`、其他轨 `progress.md`/`bugs.md`。
- 不写"计划/分析"类新文档堆砌：优先在既有文档内改写；新增文件仅限确无合适落点时（须在 progress 说明）。
- 描述任何 API 前先读源码；**禁止**写出与代码不一致的签名、文件名或命令。
- 禁止 `pnpm install`；搜索用 Grep 工具；不提交 codegen 产物、`Cargo.lock`、`src-tauri/Cargo.toml` 注入段。

## 验收标准

1. `git diff --name-only` 仅含 `docs/**`（至多 1 处 `AGENTS.md` 且已说明）。
2. 文档中出现的所有 import 路径、包名、函数名、文件路径、命令，可被 grep/test 验证为真实存在（Tester 需抽验 ≥10 处并列表）。
3. 全文检索新文档：无 `HostLocaleBridge`、`setHostLocaleBridge`、`src/lib/cn`、`../../../src/`（作为推荐写法出现即为不合格；作为"禁止示例"出现需明确标注为反例）。
4. `independent-driver-development.zh-CN.md` 与 `.en.md` 章节结构一一对应（Tester 比对标题列表）。
5. 内部链接全部可达：文档内所有相对链接指向的文件存在（Tester 逐条校验）。
6. `npx tsc --noEmit -p tsconfig.json` 与 `node scripts/aggregate-hub.mjs` 结果不受影响（文档轨不应触发；仍需自证未误改代码）。

## 状态

- [x] Coder 完成 → READY_FOR_TEST（commits `da30426b3` / `9a88c7778` / `8ac2705d2` / 本记录 commit）
- [x] Tester 复测 → **TEST_FAILED**（4 个 Bug 待修复，见 `bugs.md`；结构类验收全部通过，仅事实/引用一致性问题）
- [x] Coder Bug 修复（第 1 轮）→ READY_FOR_TEST（BUG-001..004 全部修正并实测复核，4 条判定均无反驳；状态见 `bugs.md` 已推进为「待复测」）
- [x] Tester 复测（第 2 轮，commit `9bf1c6600`）→ **TEST_FAILED**（BUG-001..004 **全部判定「已修复」**；新发现 1 条低级残留缺陷 `decouple-docs-BUG-005`（指南 §6.2 zh/en 未同步 BUG-003 三分规则），状态「待修复」；全局不回归 7 项全部通过。详见下方「Tester 第 2 轮复测记录」）
- [x] Coder Bug 修复（第 2 轮，仅 BUG-005）→ READY_FOR_TEST（zh/en 两份 §6.2 已同步改写为三分规则并换用实测存在的先例；判定成立、无反驳项；状态由复测 Tester 推进）
- [x] Tester 复测（第 3 轮，commit `595f106dd`）→ **TEST_FAILED**（BUG-005 判定**已修复**，全局不回归 7 项全过，抽验 28 项命中 27；新登记 2 条低级一致性缺陷 `BUG-006`（2.4.3 三层语言真值未对表 O-1 裁定的驱动侧 10 语言运行时集合）与 `BUG-007`（副作用示例 `import '../locales';` 与点名的嵌套入口深度不匹配）。详见文末「Tester 第 3 轮复测记录」）
- [x] Coder Bug 修复（第 3 轮，BUG-006 / BUG-007）→ **READY_FOR_TEST**（第 3 轮由接管 Coder 完成：现场遗留改动经逐行复核后大部分保留、1 处事实不精确已纠正；详见文末「Coder Bug 修复记录（第 3 轮 · 接管）」）
- [x] Tester 复测（第 4 轮 · 收口，commit `91921a87d`）→ **TEST_DONE (PASSED)**（BUG-006 / BUG-007 均判定**已修复**，含接管自我更正的 `:299` 运行时可达性调用链逐条实测命中；全局不回归 7 项全过；本轮新抽验 14 条断言 14 命中；无 Blocker，仅 3 条 Nit 登记于 `bugs.md`「Nit · 留待 Wave 4 文档回扫」。详见文末「Tester 第 4 轮（收口）复测记录」）**本轨收口，可合流**

## Tester 复测记录（commit `6199d9d95`，全新实例独立实测，不采信 Coder 自报）

工作目录 `.worktrees/datazen-decouple-docs` @ `feature/decouple-docs`；基线 `d172476fc`；本 worktree 起始 `git status --short` 干净。

### 阶段 A：实现审查 + 事实一致性抽验

1. **diff 范围（越界检查）**：`git diff --name-only d172476fc..HEAD` → **6 个文件，全部 `docs/**`**；`git diff --stat d172476fc..HEAD -- src packages scripts src-tauri e2e AGENTS.md` → **空输出**（零代码影响，AGENTS.md 确实未改）。逐文件读 diff，未见夹带越界改动。
2. **一致性抽验清单：47 项断言 → 43 项命中，命中率 91.5%；4 项不一致 → `decouple-docs-BUG-001..004`**（覆盖 Coder 自验 1-14 全部条目，另加 33 项扩展核对）。

| # | 文档断言（位置） | 实测出处与结果 |
| --- | --- | --- |
| 1 | `@datazen/ui` 导出 11 组件 + `cn` + 5 个 i18n API + `I18nParams`（2.1.1） | `packages/ui/src/index.ts:1-30` 逐项命中 ✅ |
| 2 | driver-sdk 允许面全清单：`DatabaseTypeMeta`/`ConnectionMode`/`BaseTableSqlGenerator`/下沉类型/`driverCommands`/`fileCommands`/6 纯函数+`HOST_DEFAULT_EDITOR_FONT`/5 菜单 API/`bind*`+`useBound*`/6 Schema API（2.1.1） | `packages/driver-sdk/src/index.ts:7-134` 逐项命中 ✅ |
| 3 | 「`@datazen/extension-points` 不导出任何 i18n 能力」（2.1.1 / 2.4.1） | Grep `i18n\|registerTranslations\|setLocale\|useI18n` on `packages/extension-points/src/` → **0 命中** ✅ |
| 4 | EP 先例 `sqlEditorProEP`（2.2 行 5） | `packages/extension-points/src/index.ts:27` ✅ |
| 5 | 三处 alias 一致（2.1.1） | `tsconfig.json:18-24`、`vite.config.ts:27-33`、`vitest.drivers.config.ts:9-15` ✅ |
| 6 | `pnpm test:unit:drivers`（2.1.1） | `package.json:86` `vitest run --config vitest.drivers.config.ts` ✅ |
| 7 | 5 个 bridge + 未绑定抛错文案（2.2 / 2.3.1） | `grep -rn "has not been bound to driver-sdk yet"` → SettingsStore/ConnectionStore/ConfirmDialog/SchemaStore/ContextMenu 各 1 处，文案与文档模板完全一致 ✅ |
| 8-12 | 宿主 bind 时机 5 处（2.3.1 表） | `settingsStore.ts:197`、`connectionStore.ts:248`、`schemaStore.ts:694`、`useConfirmDialog.tsx:69`、`contextMenuStore.ts:40-43`（含 `show: showWebContextMenu`）✅ 行号逐条命中 |
| 13 | `hide` 未绑定为安全 no-op（2.3.1 约束 2） | `packages/driver-sdk/src/nativeContextMenu.ts:104-111`（`if (!boundBridge) return;` + 同因注释）✅ |
| 14 | 桥类型只暴露子集，`SettingsBridgeState` 只有 `settings.safeMode/editorFontFamily/driverSettings`（2.3.1 约束 3） | `settingsStoreBridge.ts:11-17` ✅ |
| 15 | `useBoundConnectionStore` 仅 selector/`getState`（2.3.1 表，无 setState） | `connectionStoreBridge.ts:38-41` ✅ |
| 16 | `useBoundConfirmDialog(): [ConfirmDialogFn, ReactNode]`（2.3.1 表） | `confirmDialogBridge.ts:28/36` ✅ |
| 17 | 用法示例（2.3.2） | `SafeModeBadge.tsx:10` selector 形态；`useRedisGate.ts:28` 二元组、`:32` `getState()`、`:36-41` `confirm({... kind})` ✅ |
| 18 | 「单测例外：测试内显式 bind」（2.3.1 约束 1） | `packages/drivers/redis/ui/__tests__/useRedisGate.test.tsx:33` `bindSettingsStore(harness…)`、`:78/96/115/138` `bindConfirmDialog(…)` ✅ |
| 19 | 唯一运行时 5 签名 + 查找链 + `{param}` 插值 + `useSyncExternalStore`（2.4.1） | `packages/ui/src/i18n.ts:34/44/53/73/89`，查找链 `:74`，插值 `:63`，`:90` ✅（`registry[locale] ?? registry['en'] ?? key` 逐字符相符） |
| 20 | 宿主 `src/hooks/useI18n.ts` 为别名再导出（2.4.1） | 该文件 9 行，`:8` 副作用 import locales、`:9` `export { useI18n, type I18nParams } from '@datazen/ui'` ✅ |
| 21-22 | `startLocaleSync` 唯一接线 + `main.tsx` 调用一次（2.4.2） | `src/lib/localeSync.ts:18-26`、`src/main.tsx:66` ✅；Grep 全仓 `setLocale(` 生产调用方 = localeSync + `src/locales/index.ts:73/77`（getTranslation 适配器，见下方观察项）✅ |
| 23 | EP 经 `__DATAZEN_HOST__['@datazen/ui']` 共享单例（2.4.3） | `src/main.tsx:46-56`，`'@datazen/ui': ui` 在 `:48` ✅ |
| 24 | 宿主 eager 字典 `registerTranslations` 灌入（2.4.3 表） | `src/locales/index.ts:29-32` ✅ |
| 25 | lazy 域包经 `useLocaleDomains` / `ensureLocaleDomains`（2.4.3 表） | `src/locales/lazyPacks.ts:50`、`src/hooks/useLocaleDomains.ts:14` ✅ |
| 26 | 驱动前缀 `redis.*` / `mongo.*`（2.4.4） | `packages/drivers/redis/locales/en.ts:2-4`、`packages/drivers/mongodb/locales/en.ts:2-6` ✅ |
| 27 | `i18n-sync-check.mjs` 当前仅扫宿主 `src/locales`，驱动扫描随 i18n-drivers 加入（2.4.4） | `scripts/i18n-sync-check.mjs:21` `resolve(root,'src/locales')` ✅ 标注到位 |
| 28 | `BUILTIN_LOCALES` 佐证 `zh-CN`/`pt-BR` 连字符（2.4.3） | `builtinLocales.ts:9` 仅 `['en','zh-CN']`，**全文件无 `pt-BR`** ❌ **BUG-002** |
| 29 | `DRIVER_LOCALES` 聚合链路现存、终态由 i18n-drivers 删除（2.4.3） | `src/extensions/generated-locales.ts:14` + `scripts/resolve-drivers.mjs:432/437` ✅ 未来态框架与标注正确 |
| 30 | 驱动 `locales/index.ts` 自注册尚未存在（2.4.3 表） | `ls packages/drivers/{redis,mongodb}/locales/index.ts` → 均缺失；入口 `meta.ts` 无 `import '../locales'` ✅ 标注为同期落地，未冒充已交付 |
| 31 | redis 入口 `ui/shared/meta.ts`、mongodb 入口 `ui/meta.ts`（2.4.3 / extensibility 1.4） | `resolve-drivers.mjs:239`、`:270` ✅ |
| 32 | `generated.ts` codegen 符号（2.4.3 / extensibility 1.1、1.4） | `src/extensions/generated.ts:10`（redis 入口）、`:41`（`DatabaseType`）、`:44`（`DRIVER_DB_ENTRIES`）✅ |
| 33 | `src/types/index.ts` 的 `DatabaseType` 来自 generated（extensibility 1.4） | `src/types/index.ts:2` ✅ |
| 34 | `DB_REGISTRY` 在此合并驱动条目（extensibility 1.1） | `src/lib/databaseTypes.ts:10`（import）+ `:20-21`（`...DRIVER_DB_ENTRIES`）✅ |
| 35 | 唯一实现原则的薄再导出路径（2.1.2） | `src/lib/cn.ts:1`、`src/lib/nativeContextMenu.ts:7-15`、`src/commands/driver.ts:1-10`、`src/commands/file.ts:9/17` ✅ |
| 36 | 决策表行 1 先例 `src/lib/driverSettings.ts` → SDK（2.2） | SDK 侧存在，但宿主 `src/lib/driverSettings.ts` **已被整体移走、文件不存在** ❌ **BUG-003** |
| 37 | 行 4 先例 `src/lib/connectionViews/types.ts`（含 `ConnectionViewActions`） | 该文件 re-export 4 个类型含 `ConnectionViewActions` ✅ |
| 38 | 「过渡期例外…除上述两点外不存在任何豁免」（2.1.2） | `packages/drivers/sqlserver/ui/ConnectionFields.tsx:2` 同类宿主 import，未被登记 ❌ **BUG-001** |
| 39 | 反例污染扫描（验收 3） | 5 份文档 `HostLocaleBridge`/`setHostLocaleBridge`/`getExtensionTranslation` **0 命中**；`src/lib/cn`(3 处) 与 `../../../src/`(9 处) **逐处判定**：均在带 `❌ 反例` 标注的代码块或「禁止/零新增」句内 ✅ |
| 40 | Wave 4 护栏不得杜撰（任务书 6） | 2.6 明确「具体脚本文件名与 CI 位置尚未确定（待 Wave 4 落地）」；`scripts/` 下仅无关的 `check-structure-editor-guardrails.mjs`，无 import 护栏 ✅ |
| 41 | Redis Key 菜单 builder 路径勘误（components.md 9.1.1） | `packages/drivers/redis/ui/key-browser/redisKeyContextMenu.ts` 存在、旧路径 `ui/redisKeyContextMenu.ts` 不存在、`RedisWorkbench.tsx:28` 相对引用 ✅ 勘误正确 |
| 42 | components.md 其余入口未被误伤 | `WebContextMenu.tsx`+`App.tsx:100` 挂载 `WebContextMenuHost`、`contextMenuPosition.ts`、4 个宿主 builder 文件均存在 ✅ |
| 43 | extensibility 1.4 新增的 `.drivers-dev.json` / registry 说法 | `drivers-registry.json` 存在、`resolve-drivers.mjs:54` 读取 `.drivers-dev.json`（注释 `:53` 标 gitignored）✅ |
| 44 | Part 1「逐字未动」 | `git diff 824c7830b..HEAD` 对 Part 1 只有顶部 H1 一行被替换 ✅；抽查 Part 1 事实：`PROTOCOL_VERSION` → `packages/driver-api/src/lib.rs:73`，workspace path 依赖 → `Cargo.toml:16` ✅ |
| 45 | 相对链接可达（验收 5） | 一次性 node 脚本（写在 `/tmp`，跑完已 `rm`，仓库内零残留）解析 5 份文档全部相对链接：**14 条，broken 0** ✅ |
| 46 | zh/en 章节一一对应（验收 4） | `#`×1 + `##`×13 + `###`×7 = **21 : 21**，序号/层级/顺序逐行对应（1-13 + 可选小节 + 6.1/6.2/6.3）✅；但 progress.md 自报「各 24 个」失真 ❌ **BUG-004** |
| 47 | `sideEffects: false`（2.3.1 约束 4） | `packages/driver-sdk/package.json` 实测 `"sideEffects": false` ✅ |

### 阶段 B：独立复跑（实测数字）

| 命令 | 结果 |
| --- | --- |
| `npx tsc --noEmit -p tsconfig.json` | **exit 0**，0 error ✅ |
| `node scripts/aggregate-hub.mjs` | exit 0，输出「聚合 11 个 tracks」；随后 `git status --short` **仅 ` M docs/development/coordination/hub.md`**（证明本轨未污染聚合总览，重跑结果与提交态一致）；已 `git checkout -- hub.md` 恢复，**hub.md 未提交** ✅ |
| `node scripts/check-id-terminology.mjs`（`pnpm test:ids`） | exit 0，「5 allow-listed occurrence(s) skipped / ok（1714 files scanned）」✅ |
| `node scripts/check-ci-docs-consistency.mjs`（`pnpm test:ci-docs`，文档一致性守卫） | exit 0，drivers 11 ids / window boundaries / toolchain 三项全 ok ✅ |
| `node scripts/check-module-layers.mjs`（`pnpm test:layers`） | exit 0，「ok（3 rules）」✅ |
| `git diff --stat d172476fc..HEAD -- src packages scripts src-tauri e2e AGENTS.md` | **空**（零代码影响）✅ |

说明：文档轨不跑 `vitest`/`cargo test`（本轨未触碰任何可执行文件，上表 5 条守卫 + tsc 已覆盖「零回归」自证需要）。

### 阶段 C：一致性覆盖率（替代行覆盖率）

- 可验证断言总数 **47**，实测核对 **47**（100% 覆盖，未留未核项），命中 **43**，命中率 **91.5%**。
- 4 处不一致全部登记为 Bug（BUG-001 中级 / BUG-002、003、004 低级），均为「文档写 X / 代码实为 Y」型引用与清单完整性问题，**不涉及契约结论错误**：允许面/禁止面、5 桥清单、i18n 五 API、决策表主落点、勘误方向经核对全部为真。
- 残余风险（未列入 Bug 的观察项，见 `bugs.md` 末节）：2.4.2「唯一接线点」措辞与 `getTranslation()` 的临时 `setLocale` 存在可读性歧义；Part 1 遗留 `"0.1"` 版本示例与 crate 实测 `0.0.8` 不吻合（历史文本，不在本轨范围）。

### 阶段 D：留待 R 回归

- 本轨无需 E2E。文档指导出的后续验证点（Wave 4 护栏落地时必须拦住、且当前**无法**由本轨验证的行为）：
  1. `packages/drivers/*/ui/**` 内任意 `.../src/` 形态 import —— 现网基线应为 **32 处**（redis 31 + sqlserver 1，见 BUG-001）+ `redisKeyWebContextMenu.test.tsx` 夹具 2 处（已裁决豁免）；护栏白名单必须与该口径一致，否则落地即红。
  2. 驱动/EP 生产代码的 `setLocale` 调用（宿主 `localeSync.ts` 接线与 `getTranslation` 适配器需白名单区分）。
  3. 驱动 `t()` key 断言宿主 `I18nKey`（2.4.4）与新增宿主聚合 codegen（2.4.3 禁止项）。
- i18n 自注册终态（`locales/index.ts`、入口副作用行、`DRIVER_LOCALES` 删除、`i18n-sync-check` 驱动扫描）在 `i18n-drivers` 轨合并后需**回扫本文件 2.4.3**，把「同期落地」措辞改为已交付并核对实际模块名一致。


## Tester 第 2 轮复测记录（commit `9bf1c6600`，全新实例独立实测；第 1 轮报告仅作断言清单，不采信）

工作目录 `.worktrees/datazen-decouple-docs` @ `feature/decouple-docs`；基准 `fd23a66a8`；起始 `git status --short` 干净。HEAD 实测 `9bf1c6600`（其父 `8e68a79dc`、`006906c6a` 均在链上）。

### 阶段 A：BUG-001..004 定点复验（逐条独立重现）

| Bug | 复验动作（全部为本轮自己执行的命令/Read） | 实测结果 | 判定 |
| --- | --- | --- | --- |
| **BUG-001（中）** | `grep -rn "from '\.\./.*src/" packages/drivers/*/ui/` 全量清点 + 目录分布 + `vi.mock` 独立扫描 + 宽松正则 `grep -rEn "['\"]\.\./.*src/" packages/drivers/*/ui/` 交叉验证 | total **34** =（redis **31** + sqlserver **1**）`src/hooks/useI18n` **32**（且恰为 32 个不同文件，`grep -rl` 复核）+ 夹具 `redisKeyWebContextMenu.test.tsx:5,9` **2**；redis 目录分布 `connection/3, console/1, console/consoleCompletion/1, key-browser/7, observe/4, shared/2, value-editors/11, value-editors/valueView/1, value-search/1 = 31` 与 Coder 主张逐目录一致；`sqlserver/ui/ConnectionFields.tsx:2` 属实；**8 处 `vi.mock` 宿主 `src/hooks/useI18n` 属实**（`__tests__/` 下 ValueViewer:11 / connectionWizard:6 / useRedisGate:15 / PubSubPanel:8 / redisConsoleCompletionJourney:22 / jsonModeBar:11 / ttlControlsJourney:13 / consoleResultRenderer:7 各 1 处，非 `from` 形态确不被基线命令命中）；宽松正则总命中恰 **42 = 34 + 8**，`ui/` 之外驱动目录 **0** 命中 ⇒ **基线 42 成立**，Wave 4 白名单口径与 2.1.2/2.7 文档声明逐项吻合 | **已修复** |
| **BUG-002（低）** | `sed`/`grep`/`ls` 核对 2.4.3 三层表述三出处 | 接线层：`builtinLocales.ts:9` = `BUILTIN_LOCALES = ['en', 'zh-CN']`（全文件 `pt-BR` 0 命中；真值源 `builtin-locales.json` 仅 en/zh-CN；`BUILTIN_LOCALE_LABELS:26-29` 两项；`fullLocales.ts` 仅 en/zh-CN 且注释自陈 "Import only from tests or tooling"）；校验层：`i18n-sync-check.mjs:23` = `LOCALE_FILES = ['de','es','fr','ja','ko','pt-BR','ru','zh-TW']`（8 语言，行号命中）；命名层：`src/locales/pt-BR.ts` + `src/locales/pt-BR/` 存在、redis/mongodb `locales/` 各 **10** 语言文件含 `pt-BR.ts`；「生产路径无运行时 import」复核：`src/` 内 `pt-BR` 仅 `resolveUiLanguage.ts` 语言码字面量（非词条模块 import），表述成立；`9bf1c6600` 修订后正文为「真值分**三层**」，与三个项目符号实数一致 | **已修复** |
| **BUG-003（低）** | 2.2 行 1 先例列 4+1 个引用逐个 Read/ls | `src/lib/cn.ts` 整文件 1 行薄壳 ✅；`src/lib/nativeContextMenu.ts:7-15` 值+类型再导出块 ✅；`src/commands/driver.ts:6-11` `driverCommands`+类型薄壳 ✅;`src/commands/file.ts:2/9`（`:2` import SDK `fileCommands`、`:17` 起 spread + host-only 命令）「合并再导出」形态标注准确 ✅；反例：宿主 `src/lib/driverSettings.ts` 确不存在（`ls` No such file；`find src -iname "*driverSettings*"` 仅 `DriverSettingsSection.tsx`；`git log -1 --` = `92a039383` ✅）；消费点直连 SDK 属实（`DriverSettingsSection.tsx:3` `mergeDriverSettings` from SDK；`JsonSchemaSettingsForm.tsx:6` from SDK）；2.1.2「唯一实现原则」与 2.5 第 2 步的「无消费方连文件删除」规则句均在位 | **已修复**（但修复未同步两份指南 §6.2 ⇒ 新登记 BUG-005） |
| **BUG-004（低）** | `grep -c "^#"` + 层级分布 + 序列 diff | zh **21** / en **21**（`grep -o "^#+ "` 分布两份完全相同：`# `×1 + `## `×13 + `### `×7）；标题层级序列 `diff` 逐行一致；progress.md B 表末行与自验第 6 条均已写 21 + 口径（含 ATX/H1/不计表格与代码块的排除说明），与实测吻合 | **已修复** |

### 阶段 B：全局不回归（7 项全过）

| # | 验收项 | 实测结果 |
| --- | --- | --- |
| 1 | diff 范围 | `git diff --name-only fd23a66a8..HEAD` = 8 文件全在 `docs/**`（含 `hub.md` 系本分支自 `d172476fc` 分叉、未含 `fd23a66a8` 的 hub 再聚合，**非本轨改动**：`git log fd23a66a8..HEAD -- hub.md` 空）；`git diff --stat fd23a66a8..HEAD -- src packages scripts src-tauri e2e AGENTS.md` = **空**；两个修复 commit 触及文件仅 `boundary.md` + 本 track `progress.md`/`bugs.md`，未触碰指南（与 BUG-005 判定相互印证） |
| 2 | `npx tsc --noEmit -p tsconfig.json` | **exit 0**，0 error |
| 3 | 三守卫（脚本名取自 `package.json:91-93`） | `check-id-terminology` exit 0（5 allow-listed skipped / ok 1714 files）；`check-ci-docs-consistency` exit 0（11 ids / window boundaries / toolchain 全 ok）；`check-module-layers` exit 0（3 rules） |
| 4 | 相对链接 | 临时 node 脚本（`/tmp`，跑完即删，`git status` 复核仓库零残留）解析 5 份文档：14 条相对链接，**broken 0** |
| 5 | zh/en 标题结构 | 21 : 21，层级分布与序列 1:1（同 BUG-004 复验，两份均未在本轮修复中被改动） |
| 6 | 新一轮 API/路径一致性抽验 | **20 项，20 命中（100%）**，优先覆盖被改动的 2.1.2/2.2/2.4.3/2.5/2.7 与 §6.1~6.3，明细见下表 |
| 7 | `node scripts/aggregate-hub.mjs` | exit 0（聚合 11 tracks）；`git status --short` 仅 ` M hub.md` → `git checkout --` 恢复，**未提交** |

阶段 B-6 抽验明细（文档断言 → 实测出处）：

| # | 断言（位置） | 实测 |
| --- | --- | --- |
| 1 | 2.1.2 基线命令与 34/32/2/8/42（同 BUG-001，此处不重复计入命中的细分项） | ✅ |
| 2 | 2.2 行 5 EP 先例 `sqlEditorProEP` | `packages/extension-points/src/index.ts:27` ✅ |
| 3 | 2.2 行 4 `src/lib/connectionViews/types.ts` re-export `ConnectionViewActions` | 该文件 `:9/:11` ✅ |
| 4 | 2.3.1 宿主 bind 时机 5 处 | `settingsStore.ts:197`、`connectionStore.ts:248`、`schemaStore.ts:694`、`useConfirmDialog.tsx:69`、`contextMenuStore.ts:40` ✅ |
| 5 | 2.2/2.3 未绑定抛错文案全 SDK 恰 5 处 | `grep -rc "has not been bound to driver-sdk yet"` 合计 **5** ✅ |
| 6 | 2.3.1 约束 3 `SettingsBridgeState` 三字段 | `settingsStoreBridge.ts:11-17` ✅ |
| 7 | 2.3.1 约束 2 `hide` 未绑定 no-op | `nativeContextMenu.ts:104-111`（`if (!boundBridge) return;`）✅ |
| 8 | 2.3.1 约束 4 / 2.1.1 `sideEffects: false` | `packages/driver-sdk/package.json:13` ✅ |
| 9 | 2.4.1 五 API 签名与行号（`setLocale:34`/`getLocale:44`/`registerTranslations:53`/`t:73`/`useI18n:89`） | `packages/ui/src/i18n.ts` 逐行命中 ✅ |
| 10 | 2.4.2 `startLocaleSync` + `main.tsx` 恰一次调用 | `localeSync.ts:18`、`main.tsx:26` import + `:66` `startLocaleSync();` ✅ |
| 11 | 2.4.3 表：宿主 eager `registerTranslations` + `__DATAZEN_HOST__['@datazen/ui']` | `src/locales/index.ts:29-32`、`src/main.tsx:46-48` ✅ |
| 12 | 2.4.3 驱动自注册「同期落地未合并」标注仍为真 | `packages/drivers/{redis,mongodb}/locales/index.ts` 均不存在；两入口 `meta.ts` 无 `import '../locales'` ✅ |
| 13 | 2.4.4 前缀 `redis.*`/`mongo.*` | 两包 `locales/en.ts` 首 key 前缀命中 ✅ |
| 14 | 2.4.5 / 6.3 `DriverFormValidator` 第二参 `t: (key: string) => string` | `packages/driver-sdk/src/index.ts:49-60` ✅ |
| 15 | 2.5 步 3 桥单测先例目录 | `packages/driver-sdk/__tests__/`（confirmDialog/connectionStore/driverSettingsForm 等存在）✅ |
| 16 | 6.1 `@datazen/ui` 导出组件清单（含 `Slider`/`TemporalValueInput`/`PathInput`、`cn`、i18n） | `packages/ui/src/index.ts:1/15/17-21/24-26` ✅ |
| 17 | 6.1 SDK 消费面 `useBound*`/`bindContextMenuBridge`/`showNativeContextMenu`/`resolveEditorFontFamily` | `packages/driver-sdk/src/index.ts:85/89-90/99/107/116/131` ✅ |
| 18 | 2.4.3 generated.ts redis 入口 `ui/shared/meta` | `src/extensions/generated.ts:10` ✅（mongodb 入口 `ui/meta` 见 resolve-drivers `:270`，第 1 轮已核） |
| 19 | 2.1.2 夹具引用的宿主文件存在 | `src/components/ui/WebContextMenu.tsx`、`src/stores/contextMenuStore.ts` ✅ |
| 20 | 「消费点直连 SDK」旁证（BUG-005 证据链） | `src/components/sql-editor/editorExtensions.ts:36-38` from `@datazen/driver-sdk` ✅ |

**未回归项复核（验收 3）**：5 份文档 `HostLocaleBridge|setHostLocaleBridge|getExtensionTranslation` 0 命中复确认由三守卫 + 本轮 `薄再导出` 全量 grep 顺带覆盖；字面 `../../../src/` 仍仅存在于 ❌ 反例块（2.1.2 正文与 2.5 步 5），未扩散。

### 阶段 C：一致性覆盖率（替代行覆盖率）

- 本轮独立核对断言 **44 条**（阶段 A 逐 Bug 细分 24 条 + 阶段 B-6 抽验 20 条）：命中 **43**，命中率 **97.7%**；唯一失配 = 指南 §6.2（zh:196/en:211）无条件「宿主原路径仅剩薄再导出」且先例含 2 个无壳模块 → `decouple-docs-BUG-005`（低，待修复）。
- 风险清单（未核对断言）：Part 1 Rust 段内部 API 细节（第 1 轮抽查 2 项通过，全量不在本轨范围）；观察项 1（2.4.2 `getTranslation` 括注）与 2（Part 1 `"0.1"` 版本示例）维持第 1 轮记录，不升级。

### 阶段 D：留待 R 回归（本轮复核 + 补登）

- 已覆盖确认：① Wave 4 白名单口径 **34（32+2）+ 8 = 42**（本轨 `## 留待 R 回归` 段已按修复后事实登记，且 2.1.2/2.7 双处一致）；② `i18n-drivers` 合并后回扫 **2.4.3** 措辞与模块名。
- **补登缺项**（已并入文末「留待 R 回归」）：③ `i18n-drivers` 合并后 **2.1.2/2.7 的 32/2/8/42 基线必然失效**（换源归零），须回扫删除或改「历史基线」注记；④ **Wave 4 护栏落地后** 2.6/2.1.2/2.7「待 Wave 4 落地」措辞须替换为实际脚本名与 CI 位置，并把白名单实现与本档口径对表；⑤ BUG-005 修复须 **zh/en 两份同步**（保持 21:21，不新增标题）。


## Coder 实施记录

### Commit 索引

| commit | 主题 | 文件 |
| --- | --- | --- |
| `da30426b3` | 主交付：解耦契约重写 | `docs/development/driver-api-dependency-boundary.md` |
| `9a88c7778` | 开发者指南同步（zh/en 一一对应） | `docs/development/independent-driver-development.zh-CN.md`、`.en.md` |
| `8ac2705d2` | 架构文档勘误 | `docs/architecture/frontend/components.md`、`extensibility.md` |
| （本 commit） | 实施记录 + 状态推进 | 本文件 |
| （Bug 修复第 1 轮 commit） | 修复 `BUG-001..004` | `docs/development/driver-api-dependency-boundary.md`、本文件、`bugs.md` |
| （Bug 修复第 2 轮 commit） | 修复 `BUG-005`（仅指南 §6.2 zh/en 两处） | `docs/development/independent-driver-development.zh-CN.md`、`.en.md`、本文件、`bugs.md` |

### A. `docs/development/driver-api-dependency-boundary.md`（重写）

保留原 Rust Part 1 全文（降级为「Part 1」章节，内容逐字未动）；新增「Part 2 · 驱动前端与宿主解耦契约」：

| 小节 | 内容 | 依据源文件（核对行） |
| --- | --- | --- |
| 2.1.1 允许 import 面表 | `@datazen/ui` / `@datazen/driver-sdk` / `@datazen/extension-points` / npm | `packages/ui/src/index.ts:1-30`（组件+cn+i18n 导出清单）；`packages/driver-sdk/src/index.ts:7-134`（方言/类型/IPC/driverSettings/右键菜单/bridge 导出清单）；`packages/extension-points/src/index.ts:5-55`（EP 契约导出、无 i18n）；`tsconfig.json:19-23`、`vite.config.ts:28-32`、`vitest.drivers.config.ts:9-15`（三处 alias 一致） |
| 2.1.2 禁止面 + 反例块 | `../../../src/**` 违规；过渡期例外两条 | 例外1 实测 `packages/drivers/redis/ui/**`（如 `connection/ClusterNodePicker.tsx:5`）尚存宿主 useI18n 相对 import（i18n-drivers 轨范围）；例外2 `packages/drivers/redis/ui/__tests__/redisKeyWebContextMenu.test.tsx` 存在性经 find 核实（cap-bridge progress.md 裁决段） |
| 唯一实现原则 | 宿主薄再导出 | `src/lib/cn.ts`（整文件 `export { cn } from '@datazen/ui'`）、`src/lib/nativeContextMenu.ts:7-15`、`src/commands/driver.ts:1-10`、`src/commands/file.ts:9/17`（合并再导出） |
| 2.2 决策表 | 5 分支落点 + 「为何不做回退查表」 | 先例列全部指向 A 表已核实文件；EP 先例 `sqlEditorProEP`（`packages/extension-points/src/index.ts:27`） |
| 2.3.1 bridge 清单表 | 5 桥 + 宿主 bind 时机与未绑定抛错 | `settingsStoreBridge.ts:27-36`/`settingsStore.ts:197`；`connectionStoreBridge.ts:27-36`/`connectionStore.ts:248`；`confirmDialogBridge.ts:32-41`/`useConfirmDialog.tsx:69`；`schemaStoreBridge.ts:24-33`/`schemaStore.ts:694`；`nativeContextMenu.ts:18-26`/`contextMenuStore.ts:40-43`（`show: showWebContextMenu`，`showWebContextMenu` 定义于 `contextMenuStore.ts:30`）；`sideEffects:false` 见 `packages/driver-sdk/package.json` |
| 2.3.2 用法示例 | selector / getState / confirm 二元组 | 逐行对照 `packages/drivers/redis/ui/shared/SafeModeBadge.tsx:10`、`useRedisGate.ts:28/32/36-41`、`src/stores/settingsStore.ts:193-197` |
| 2.4.1 唯一运行时五 API + 查找链 | `registry[locale] ?? registry['en'] ?? key`、`{param}` 插值 | `packages/ui/src/i18n.ts:34-92`；EP i18n 删除现状见 `packages/extension-points/src/`（无 i18n.ts，全仓 grep `HostLocaleBridge` 生产代码 0 命中）；宿主别名定位 `src/hooks/useI18n.ts:1-9` |
| 2.4.2 setLocale 仅宿主 | `startLocaleSync` 唯一接线 | `src/lib/localeSync.ts:18-26`、`src/main.tsx:66`；全仓 `setLocale(` 生产调用方 grep 仅 `src/lib/localeSync.ts` + `src/locales/index.ts`（getTranslation 工具适配器） |
| 2.4.3 词条归属表 + 自注册终态 | 宿主 eager/lazy、驱动 locales/index.ts 自注册、EP 直连；**显式标注「由 i18n-drivers 轨同期落地」** | 现状基线：`src/locales/index.ts:29-32`（registerTranslations 灌入）、`src/locales/lazyPacks.ts:50`、`src/main.tsx:46-56`（`__DATAZEN_HOST__['@datazen/ui']`）；终态描述逐条对齐 `.worktrees/datazen-i18n-drivers/docs/development/coordination/tracks/i18n-drivers/progress.md`（只读参考）A1-A3/C 节；驱动入口实例 `scripts/resolve-drivers.mjs:239`（redis `ui/shared/meta`）与 `:270`（mongodb `ui/meta`）、`src/extensions/generated.ts:10-11` |
| 2.4.4 key 前缀/普通 string/parity | `redis.*`、`mongo.*` 前缀实测 | `packages/drivers/redis/locales/en.ts:1-3`、`packages/drivers/mongodb/locales/en.ts:1-5`；`scripts/i18n-sync-check.mjs:21`（当前仅扫 `src/locales`，故标注驱动扫描随 i18n-drivers 加入）；locale code 对照 `src/locales/builtinLocales.ts:9` |
| 2.4.5 t() 注入先例 | `DriverFormValidator` 第二参 `t` | `packages/driver-sdk/src/index.ts:49-60` |
| 2.5 流程 / 2.6 Wave 4 预告 / 2.7 清单 | 护栏仅写「待 Wave 4 落地」，未杜撰脚本名 | 任务书设计红线 |

### B. `docs/development/independent-driver-development.zh-CN.md` / `.en.md`（同步）

| 改动 | 依据 |
| --- | --- |
| §2 布局树新增 `locales/` 行（两份对应） | 驱动词条目录现实（`packages/drivers/redis/locales/`、`mongodb/locales/`） |
| §6 重写：末段接契约 Part 2 链接；新增 6.1（允许/禁止 import 面 + ❌/✅ 示例）、6.2（下沉 + 注入桥 + tsx 示例）、6.3（i18n 单一运行时 + 自注册终态，标注 i18n-drivers 同期落地） | 全部复用 A 表已核实出处；旧文无字面「从宿主 import」代码示例，按任务书以新契约示例替换泛化描述 |
| §13 总结各加一条前端边界 bullet（两份对应） | 与 6.1-6.3 一致 |
| 标题结构：zh/en 各 **21** 个 `#` 级标题（`#`×1 + `##`×13 + `###`×7），顺序一一对应（见自验 5；原自报 24 有误，已由 BUG-004 修正） | — |

### C. `docs/architecture/frontend/components.md` / `extensibility.md`（勘误）

| 文件 → 节 | 改动 | 依据 |
| --- | --- | --- |
| components.md §9.1.1 入口列表 | `src/lib/nativeContextMenu.ts` 条目改为 `@datazen/driver-sdk` 唯一实现 + 宿主薄再导出 + `bindContextMenuBridge` 注入点 | `packages/driver-sdk/src/nativeContextMenu.ts:18`、`src/stores/contextMenuStore.ts:40-43`、`src/lib/nativeContextMenu.ts:7-15` |
| components.md §9.1.1 builder 表 | Redis Key 行路径修正为 `packages/drivers/redis/ui/key-browser/redisKeyContextMenu.ts` | find 实测（原路径文件不存在） |
| extensibility.md §1.1 条目1 | 补注驱动条目经 codegen `DRIVER_DB_ENTRIES` 合并 | `src/lib/databaseTypes.ts:10/20-21`、`src/extensions/generated.ts:44` |
| extensibility.md §1.4 清单 | 由「手改 types/index.ts + databaseTypes.ts」改为驱动包 + registry + codegen 流程，并链接契约/指南 | `src/types/index.ts:2`（DatabaseType re-export 自 generated）、`src/extensions/generated.ts:41`、`scripts/resolve-drivers.mjs:220/239/270`、`drivers-registry.json` 机制（指南 §3/§4） |
| AGENTS.md | **未改动**（未发现确凿事实错误；@datazen/ui 组件列举不完整但不算错误，遵守默认不动原则） | — |

### 自验结果（真实输出，worktree=`.worktrees/datazen-decouple-docs`）

1. `git diff --name-only d172476fc..HEAD` → 6 个文件全部 `docs/**`：
   `docs/architecture/frontend/components.md`、`docs/architecture/frontend/extensibility.md`、`docs/development/coordination/tracks/decouple-docs/progress.md`（任务书 commit `824c7830b` 自带）、`docs/development/driver-api-dependency-boundary.md`、`docs/development/independent-driver-development.en.md`、`.zh-CN.md`。零生产代码改动。
2. API/路径/命令抽验 ≥10 处（文档片段 → 代码出处）：

   | # | 文档片段 | 验证出处 |
   | --- | --- | --- |
   | 1 | `@datazen/ui` 导出 `setLocale/getLocale/registerTranslations/t/useI18n` | `packages/ui/src/index.ts:23-30`、`packages/ui/src/i18n.ts:34-92` |
   | 2 | 未绑定抛错文案 `'<X> has not been bound to driver-sdk yet.'` | driver-sdk 5 个 bridge 模块各 1 处（grep -c 全中） |
   | 3 | `bindSettingsStore(useSettingsStore)` 于宿主 store 文件末尾 | `src/stores/settingsStore.ts:197` |
   | 4 | `bindContextMenuBridge({ show: showWebContextMenu, hide })` | `src/stores/contextMenuStore.ts:40-43` |
   | 5 | `useBoundSettingsStore((s) => s.settings.safeMode)` 示例 | `packages/drivers/redis/ui/shared/SafeModeBadge.tsx:10` |
   | 6 | `const [confirm, dialog] = useBoundConfirmDialog()` 示例 | `packages/drivers/redis/ui/shared/useRedisGate.ts:28` |
   | 7 | `setLocale` 宿主唯一接线 | `src/lib/localeSync.ts:18-26` + `src/main.tsx:66`（全仓 grep 生产调用方仅此+getTranslation 适配器） |
   | 8 | `DriverFormValidator` 第二参数 `t: (key: string) => string` | `packages/driver-sdk/src/index.ts:59` |
   | 9 | 下沉类型清单 `ConnectionFormState`/`KeyEntry`/`KeyScanResult`/`NativeMenuItemDef`/`ConnectionViewProps` | `packages/driver-sdk/src/{types/connection-form.ts,types/kv.ts,types/menu.ts,types/connection-view.ts}` + index.ts:29-43 |
   | 10 | 命令 `node scripts/i18n-sync-check.mjs`、`pnpm test:unit:drivers` | `scripts/i18n-sync-check.mjs` 存在；`package.json:86` |
   | 11 | alias 三处一致（tsconfig/vite/vitest.drivers） | `tsconfig.json:19-23`、`vite.config.ts:28-32`、`vitest.drivers.config.ts:9-15` |
   | 12 | redis 入口 `ui/shared/meta.ts`、mongodb 入口 `ui/meta.ts`（前缀 `redis.*`/`mongo.*`） | `scripts/resolve-drivers.mjs:239/270`、`src/extensions/generated.ts:10-11`、两包 `locales/en.ts` 首行 key 前缀 |
   | 13 | ~~BUILTIN_LOCALES 字面量（`zh-CN`/`pt-BR` 连字符规则所指）~~ **本条断言有误，见 BUG-002**：`BUILTIN_LOCALES` 只含 `en`/`zh-CN`，`pt-BR` 连字符的真实出处是 `scripts/i18n-sync-check.mjs:23` 的 `LOCALE_FILES` 与各包语言文件名 | `src/locales/builtinLocales.ts:9` |
   | 14 | `sideEffects:false`（bridge 禁顶层副作用依据） | `packages/driver-sdk/package.json` |
3. 违禁词扫描：新写/改动的 5 份文档 grep `HostLocaleBridge|setHostLocaleBridge|getExtensionTranslation` = 0；`../../../src/` 与 `src/lib/cn` 仅出现在 ❌ 反例块与禁止性表述中（逐条核对于本记录）。
4. `npx tsc --noEmit -p tsconfig.json` → **exit 0**。`node scripts/aggregate-hub.mjs` 未运行（避免改写禁止触碰的 hub.md）；diff 不含 `scripts/**` 与任何代码，结论等价。
5. 相对链接校验：5 份文档共 14 条相对 markdown 链接，Node 脚本逐条 exists 检查 → **broken: 0**。
6. zh/en 标题对照：各 **21** 个 `#` 级标题（`#`×1 + `##`×13 + `###`×7），序号与顺序一一对应（1-13 + 6.1/6.2/6.3 + 无编号小节）。**计数口径**：以行首 `#` 的 ATX 标题行计（`grep -c '^#'`），含 H1 文档主标题，不含表格内 `#` 列与代码块内注释；原自报「各 24 个」失真，已由 `decouple-docs-BUG-004` 修正（详见下方 Bug 修复记录）。

### 偏离与说明

- 无范围缩窄。两处主动决策：① 主文档采用「Part 1 原文保留（英文不动）+ Part 2 中文新契约」双部结构，因该文件被 `docs/README.md`、`external-contract-policy.md` 以路径引用且历史引用方均为英文语境；② extensibility.md §1.4 原「手改宿主注册表」清单被判定为任务书「宿主类型出处」类过期描述，一并勘误（改动限于该节与 §1.1 一条注记）。
- i18n 自注册、`i18n-sync-check` 驱动扫描、`DRIVER_LOCALES` codegen 删除均按任务书终态描述并显式标注「由 i18n-drivers 轨同期落地」；redis UI 现存宿主 useI18n 相对 import 作为过渡期例外登记（含唯一豁免测试文件），与并行轨文件面零冲突（本轨仅 `docs/**`）。**注：本条「redis 范围」表述不完整，例外实际跨两驱动，已由 BUG-001 修正（见下方 Bug 修复记录）。**

## Coder Bug 修复记录（第 1 轮）

被修对象：Tester 登记的 `decouple-docs-BUG-001..004`（Tester 基准 commit `6199d9d95`，其 docs commit `006906c6a`）。本轮**只改这 4 处**，未新增章节、未调整文档结构、零生产代码改动。每条修正前均自行 Read/Grep 源码复核 Tester 给的正确事实，**4 条判定全部实测成立，无反驳项**。

| Bug | 改法（文件 → 节 → 具体改动） | 本轮实测到的正确事实（文件:行） |
| --- | --- | --- |
| **BUG-001（中）** | `driver-api-dependency-boundary.md` 2.1.2 过渡期例外：删掉「`packages/drivers/redis/ui/**` 仍有部分文件…」的模糊表述与「除上述两点外不存在任何豁免」，改为**带可复现命令 + 精确计数的三段式基线**（32 处宿主 `useI18n` import / 2 处测试夹具 import / 8 处 `vi.mock`），并显式并列 redis 与 sqlserver 两个驱动、指明由 `i18n-drivers` 轨收口；同步 2.7 自查清单加入「命中数超过该基线即为新增违规」的判据 | `grep -rn "from '\.\./.*src/" packages/drivers/*/ui/` → **34 行**；其中 `from '<宿主相对路径>/src/hooks/useI18n'` **32 处**（redis **31** 个文件：`connection/`3 + `console/`1 + `console/consoleCompletion/`1 + `key-browser/`7 + `observe/`4 + `shared/`2 + `value-editors/`11 + `value-editors/valueView/`1 + `value-search/`1 = 31；sqlserver **1** 处 = `packages/drivers/sqlserver/ui/ConnectionFields.tsx:2`）；余 2 处为 `packages/drivers/redis/ui/__tests__/redisKeyWebContextMenu.test.tsx:5,9`（`WebContextMenuHost` + `contextMenuStore`）。**新发现（Tester 未列，一并登记）**：另有 **8 处** `vi.mock` 指向宿主 `src/hooks/useI18n`（`packages/drivers/redis/ui/__tests__/{ValueViewer,connectionWizard,useRedisGate,PubSubPanel,redisConsoleCompletionJourney,jsonModeBar,ttlControlsJourney,consoleResultRenderer}.test.tsx` 各 1 处，如 `ValueViewer.test.tsx:11`），形态非 `from` 故不被该命令命中，Wave 4 护栏按 mock 路径扫描时基线为 42 处 |
| **BUG-002（低）** | 同文件 2.4.3 配套终态第 2 条：把「`zh-CN`、``pt-BR` 对照 `BUILTIN_LOCALES`」的错误佐证拆成**分层三小条**——宿主接线层（只有 `en`/`zh-CN`，出处 `builtinLocales.ts:9` + 真值源 json）、parity 校验层（其余 8 语言，出处 `i18n-sync-check.mjs:23` `LOCALE_FILES` 与文件名）、命名约定层（`pt-BR` 连字符以 `LOCALE_FILES`/文件名为出处），明确「有语言文件 ≠ 宿主已接线」 | `src/locales/builtinLocales.ts:9` = `export const BUILTIN_LOCALES = ['en', 'zh-CN'] as const;`（全文件 `grep pt-BR` **0 命中**）；真值源 `src/locales/builtin-locales.json` 只列 en / zh-CN 两项；`BUILTIN_LOCALE_LABELS:26-29` 同仅两项；`src/locales/fullLocales.ts` 亦只含这两个（注释自陈「Import only from tests or tooling」）；`scripts/i18n-sync-check.mjs:23` = `const LOCALE_FILES = ['de', 'es', 'fr', 'ja', 'ko', 'pt-BR', 'ru', 'zh-TW'];`；宿主 `src/locales/` 下 `pt-BR.ts` + `pt-BR/` 目录存在，且 `pt-BR` 在 `src/` 内除 `src/locales/` 自身再导出外**无生产 import**；驱动侧 `packages/drivers/{redis,mongodb}/locales/` 各 **10** 个语言文件（含 `pt-BR.ts`） |
| **BUG-003（低）** | 同文件 2.2 决策表行 1：先例列**换成实测存在的薄再导出壳** `src/lib/cn.ts`、`src/lib/nativeContextMenu.ts:7-15`、`src/commands/driver.ts:6-11`、`src/commands/file.ts:2/9`（后者标注为「合并再导出」），并把已不存在的宿主 `src/lib/driverSettings.ts` 从「壳先例」改列为「**整体移走不留壳**」反例；同时在 2.1.2「唯一实现原则」与 2.5 流程第 2 步补写真实规则「薄再导出只为存量宿主消费方而留，无消费方则连文件删除」 | `ls src/lib/driverSettings.ts` → No such file；`find src -iname "*driverSettings*"` → 仅 `src/windows/settings/DriverSettingsSection.tsx`；全仓 `lib/driverSettings` 只剩文档命中（本文件与 `tracks/cap-bridge/progress.md:111`，后者本身即记载「**移动**，宿主消费点改为直接 import sdk」，与修正后表述一致）；`git log --oneline -1 -- src/lib/driverSettings.ts` → `92a039383`；SDK 侧 `packages/driver-sdk/src/driverSettings.ts` 存在；`src/lib/cn.ts` **整文件 1 行** `export { cn } from '@datazen/ui';`；`src/commands/file.ts:2` import SDK `fileCommands` 后 `:17` 起 spread 并追加 host-only 命令（故属合并而非纯薄壳） |
| **BUG-004（低）** | 本文件 B 表末行 + 自验第 6 条：24 → **21**，并写死计数口径（行首 `#` ATX 标题行、含 H1、不含表格 `#` 列与代码块注释）；顺带把自验表第 13 条对 `BUILTIN_LOCALES` 的错误断言就地标注作废（指向 BUG-002） | `grep -c "^#"` → zh **21** / en **21**；层级分布两份完全相同：`# `×1 + `## `×13 + `### `×7（`#### `×0）；`paste` 逐行比对标题序列 → 1:1 对应（1-13 + 6.1/6.2/6.3 + 三个无编号 `###`），验收标准 4 结论不变 |

### 第 1 轮自验（真实输出）

1. **逐条重跑 bugs.md 重现命令**：BUG-001 `grep -rn "from '\.\./.*src/" packages/drivers/*/ui/ | grep -v "^packages/drivers/redis/"` → 仅 `sqlserver/ui/ConnectionFields.tsx:2` 一行，且**该文件现已登记在 2.1.2 基线内**（34 = 32 + 2 与命令命中数逐条一致）；BUG-002 `sed -n '9p'` → `['en','zh-CN']`、`grep -n pt-BR builtinLocales.ts` → 无命中，文档不再以其为 `pt-BR` 佐证；BUG-003 `ls` → No such file，文档已不再引用该宿主路径作先例；BUG-004 → 21 / 21，与 progress.md 新数字一致。
2. `npx tsc --noEmit -p tsconfig.json` → **exit 0**（0 error）。
3. 文档守卫三连（脚本名取自 `package.json:91-93` 的 `test:ids` / `test:ci-docs` / `test:layers`）：
   - `node scripts/check-id-terminology.mjs` → exit 0，「5 allow-listed occurrence(s) skipped / ok（**1714 files scanned**）」
   - `node scripts/check-ci-docs-consistency.mjs` → exit 0，「drivers ok (**11 ids** in ci-test-matrix.md) / window boundaries ok / toolchain ok (Node 24, pnpm 11, Rust stable)」
   - `node scripts/check-module-layers.mjs` → exit 0，「ok（**3 rules**）」
4. 违禁词/反例复扫（验收 3）：5 份文档 `HostLocaleBridge|setHostLocaleBridge|getExtensionTranslation` = **0 命中**；字面 `../../../src/` 形态仍为 **9 处**，与修复前逐处同一（均在 ❌ 反例块或禁止性句内），本轮新增文字未引入该字面量（新写内容用命令正则 `\.\./.*src/` 表述）。
5. `git diff --name-only 006906c6a..HEAD` → 见下方交付段，全部落在允许清单（本 track `progress.md`/`bugs.md` + `driver-api-dependency-boundary.md`）。
6. `node scripts/aggregate-hub.mjs` 未运行（避免改写禁止触碰的 `hub.md`）；本轮 diff 不含任何 `src/`、`packages/`、`scripts/` 路径，结论等价。

## Coder Bug 修复记录（第 2 轮）

被修对象：Tester 第 2 轮新登记的 `decouple-docs-BUG-005`（Tester 被复测 commit `9bf1c6600`，本轮基准 HEAD `88715c03b`）。本轮**只改指南 §6.2 的两处无条件句式**（`docs/development/independent-driver-development.zh-CN.md:196` / `.en.md:211`）＋本 track 两份记录；未新增/删除任何标题、未改契约主文档（复核确无同类残留）、零生产代码改动。落笔前逐条独立复核 Tester 事实，**判定成立，无反驳项**。

### 事实复核（本轮亲自执行 → 真实输出）

| # | Tester 主张 | 本轮命令与输出 | 结论 |
| --- | --- | --- | --- |
| 1 | 两个宿主壳不存在 | `test -f` 逐项：`src/lib/cn.ts` **EXISTS**（1 行）/ `src/lib/nativeContextMenu.ts` **EXISTS**（15 行）/ `src/commands/driver.ts` **EXISTS**（11 行）/ `src/commands/file.ts` **EXISTS**（92 行）/ `src/lib/driverSettings.ts` **ABSENT** / `src/lib/resolveEditorFontFamily.ts` **ABSENT** | 成立 |
| 2 | 宿主仅剩同名组件文件 | `find src -iname "*driverSettings*"` → 仅 `src/windows/settings/DriverSettingsSection.tsx`；`find src -name "resolveEditorFontFamily*"` → **0 命中** | 成立 |
| 3 | 两模块随同一 commit 整体移走 | `git log --oneline -1 -- src/lib/driverSettings.ts` → `92a039383 refactor(driver-sdk): sink pure/IPC modules and add capability bridges`；`git log --oneline -1 -- src/lib/resolveEditorFontFamily.ts` → **同一** `92a039383` | 成立 |
| 4 | SDK 侧为唯一实现 | `ls packages/driver-sdk/src/{driverSettings.ts,resolveEditorFontFamily.ts,nativeContextMenu.ts}`、`ls packages/driver-sdk/src/ipc/` → `driverCommands.ts`、`fileCommands.ts` 全部存在 | 成立 |
| 5 | 宿主消费点直连 SDK（无壳） | `src/windows/settings/DriverSettingsSection.tsx:3` = `import { mergeDriverSettings } from '@datazen/driver-sdk';`；`src/components/sql-editor/editorExtensions.ts:36-38` = `import { resolveEditorFontFamily, HOST_DEFAULT_EDITOR_FONT } from '@datazen/driver-sdk';`（`:215` 消费该函数） | 成立 |
| 6 | 新引用的薄壳行号 | `grep -n` 带行号复核：`src/lib/nativeContextMenu.ts:7-15` 为 `export { showNativeContextMenu, … } from '@datazen/driver-sdk';` 值+类型再导出块；`src/commands/driver.ts:6-11` 为 `driverCommands` + 类型再导出；`src/commands/file.ts:2` import SDK `fileCommands as dialogFileCommands`、`:9` 类型再导出、`:17-18` `export const fileCommands = { ...dialogFileCommands,` ＋ host-only 命令（故本轮沿用契约口径标注为**合并再导出**，不称纯薄壳）；`src/lib/cn.ts` 整文件 1 行 | 成立 |
| 7 | 同类残留是否只此两处 | `grep -n "re-export\|再导出\|仅剩\|keeps only"` 三份文档 → 指南命中即 zh:196 / en:211 两行；契约 `:176`（唯一实现原则）、`:197`（2.2 行 1）、`:326`（2.5 第 2 步）、`:339` 均已带限定语 ⇒ 契约主文档**不需改动**，未越界扩写 | 成立 |

### 改法

| 文件 → 位置 | 改动 | 依据（实测出处） |
| --- | --- | --- |
| `independent-driver-development.zh-CN.md` §6.2 第 1 个 bullet（原 :196） | 删除「宿主原路径仅剩薄再导出」的无条件括注，改为与契约同一套**三分规则**：「实现**移动**下沉（禁止复制）→ 宿主原路径**有存量消费方时**只保留薄再导出（不允许第二份实现）/ **无消费方时**整体移走、不留空壳，消费点一并改为直接 import SDK」，并句内指向「契约文档 2.2 / 2.5」；先例拆两组——**留壳**：`src/lib/cn.ts`、`src/lib/nativeContextMenu.ts:7-15` → SDK `nativeContextMenu`、`src/commands/driver.ts:6-11` → SDK `ipc/driverCommands`、`src/commands/file.ts:2/9` → 与 SDK `ipc/fileCommands` 合并再导出（宿主另留 host-only 命令）；**不留壳**：`driverSettings`、`resolveEditorFontFamily`，并写明 SDK 唯一实现路径、`92a039383` 移走与宿主直连 SDK 的两个消费点 | 上表 #1-#6；规则文本对齐 `driver-api-dependency-boundary.md:176/:197/:326` |
| `independent-driver-development.en.md` §6.2 同位 bullet（原 :211） | 与中文**逐句直译同步**，术语一致（thin re-export / moved away entirely, leaving no empty shell / merged re-export / three-way rule） | 本轮以 `paste` 比对两份该行全部 code span：**20 : 20 逐项完全相同**，反引号计数各 40 |
| 本文件 + `bugs.md` | BUG-005 状态推进为「待复测」，补写 Coder 处理段与本轮记录 | — |

### 第 2 轮自验（真实输出）

1. `grep -rn "宿主原路径仅剩\|thin re-export\|薄再导出" docs/development/independent-driver-development.*.md` → 命中 **2 行**（zh:196、en:211），**逐处判定**：两处均已带「有存量消费方时…薄再导出 / 无消费方时整体移走、不留空壳」限定语并指向契约 2.2/2.5，与三分规则一致；旧无条件句式「宿主原路径仅剩薄再导出」在两份指南中已 **0 命中**。
2. 文档点名的宿主路径存在性逐个验证（命令与输出见上表 #1）：4 个留壳路径 EXISTS、2 个移走路径 ABSENT，与文档表述逐字对应。
3. `grep -c "^#"` → `independent-driver-development.zh-CN.md` **21** / `independent-driver-development.en.md` **21**；层级分布两份完全相同：`# `×1 + `## `×13 + `### `×7；`diff <(grep -o "^#\+" zh) <(grep -o "^#\+" en)` → **空输出**（标题层级序列 1:1，未新增/删除标题，验收 4 保持）。
4. `npx tsc --noEmit -p tsconfig.json` → **exit 0**（0 error）。
   - `node scripts/check-id-terminology.mjs` → exit 0，「5 allow-listed occurrence(s) skipped / ok（1714 files scanned）」
   - `node scripts/check-ci-docs-consistency.mjs` → exit 0，「drivers ok (11 ids in ci-test-matrix.md) / window boundaries ok / toolchain ok (Node 24, pnpm 11, Rust stable)」
   - `node scripts/check-module-layers.mjs` → exit 0，「ok（3 rules）」
5. `git diff --name-only 88715c03b..HEAD` → 4 个文件（`docs/development/independent-driver-development.zh-CN.md`、`.en.md`、本 track `progress.md`、`bugs.md`），全在允许清单内；`git diff --stat 88715c03b..HEAD -- src packages scripts src-tauri e2e AGENTS.md` → **空**；提交后 `git status --porcelain` 空（工作区 clean）。
6. 未运行 `node scripts/aggregate-hub.mjs`（避免改写禁止触碰的 `hub.md`）；本轮 diff 不含任何代码/脚本路径，结论与第 1 轮等价。

## Tester 第 3 轮复测记录（commit `595f106dd`，全新实例独立实测；前两轮记录仅作断言清单，不采信）

工作目录 `.worktrees/datazen-decouple-docs` @ `feature/decouple-docs`；基准 `fd23a66a8`；起始 `git status --porcelain` 空。HEAD 实测 `595f106dd`（父 `88715c03b` → `9bf1c6600` → `8e68a79dc` 链齐）。本轮修复触及文件仅 4 个：两份指南 + 本 track `progress.md`/`bugs.md`（`git show --stat 595f106dd`）。

### 阶段 A：BUG-005 定点复验 → **已修复**

| # | 复验动作（本轮亲自执行） | 实测结果 |
| --- | --- | --- |
| 1 | Read 两份指南 §6.2 首条 bullet（zh:196 / en:211）逐句比对契约 `:176`（2.1.2 唯一实现原则）/ `:197`（2.2 行 1）/ `:326`（2.5 第 2 步） | 三分规则**四处文本同一套**：① 移动下沉、禁止复制；② **有存量消费方时**薄再导出（不允许第二份实现）；③ **无消费方时**整体移走、不留空壳、消费点改直连 SDK；④ **合并再导出**形态单列（`src/commands/file.ts:2/9`）。指南句内指向「契约文档 2.2 / 2.5」✅ |
| 2 | 无条件句式复扫：`grep -rn "宿主原路径仅剩\|仅剩薄再导出\|keeps only a thin re-export\|只保留薄再导出"` 覆盖 5 份文档 | 命中 **0**（en:211 现为同句内加粗条件 `when legacy host consumers still import it`，属条件句而非无条件断言）✅ |
| 3 | 该段点名路径逐个 `test -f` / `wc -l` / `cat -n` | **留壳 4 个 EXISTS**：`src/lib/cn.ts`（**整文件 1 行**，内容正是文中引句 `export { cn } from '@datazen/ui';`）、`src/lib/nativeContextMenu.ts`（15 行，`:7-15` = 5 值 + 4 类型再导出块）、`src/commands/driver.ts`（11 行，`:6-11` = `driverCommands` + 3 类型）、`src/commands/file.ts`（92 行，`:2` import SDK `fileCommands as dialogFileCommands`、`:9` 类型再导出、`:17-18` spread 后接 host-only 命令 ⇒「合并再导出」标注准确）；**移走 2 个 ABSENT**：`src/lib/driverSettings.ts`、`src/lib/resolveEditorFontFamily.ts`（`ls` 均报 No such file），且文中正确归入「整体移走不留壳」组 ✅ |
| 4 | 移走组支撑事实 | SDK 侧 5 个实现文件均在（`packages/driver-sdk/src/{driverSettings,resolveEditorFontFamily,nativeContextMenu}.ts`、`src/ipc/{driverCommands,fileCommands}.ts`）；`git log --oneline -1 --` 两宿主路径**同为 `92a039383`**；宿主消费点 `src/windows/settings/DriverSettingsSection.tsx:3`（`mergeDriverSettings`）与 `src/components/sql-editor/editorExtensions.ts:36-38`（`resolveEditorFontFamily` + `HOST_DEFAULT_EDITOR_FONT`）直连 `@datazen/driver-sdk`；全仓 `src/`+`packages/` 代码对 `lib/driverSettings` / `lib/resolveEditorFontFamily` 的 import **0 命中**；契约 2.2 另举的 `JsonSchemaSettingsForm` 消费符号（`applySchemaDefaults`/`listSchemaPropertyEntries`/`readBooleanField`）实测住在 `packages/driver-sdk/src/driverSettings.ts:41/63/19` ✅ |
| 5 | zh / en 该段 code span 逐项比对 + 全 §6 扩展比对 | §6.2 该 bullet **20 : 20 逐项完全相同**、反引号各 40；按小节比对（6.1 / 6.2 / 6.3）→ 差集只有 en 侧多出 `` `dialog` `` / `` `confirm` ``（zh:205 同行注释词未加反引号，事实一致），6.1 / 6.3 两侧集合互相等 ⇒ **无单边新增事实** ✅ |

### 阶段 B：全局不回归（7 项全过）

| # | 验收项 | 实测结果 |
| --- | --- | --- |
| 1 | diff 范围 | `git diff --name-only fd23a66a8..HEAD` = 8 文件，全部 `docs/**`；`git diff --stat fd23a66a8..HEAD -- src packages scripts src-tauri e2e AGENTS.md` = **空**。其中 `hub.md` 系本分支自 `d172476fc`（merge-base）分叉、`fd23a66a8` 为其后再聚合所致：`git log --oneline fd23a66a8..HEAD -- hub.md` **空输出**（本轨无任何 commit 触碰 hub.md），非本轨改动 ✅ |
| 2 | 标题结构 | `grep -c "^#"` → zh **21** / en **21**；层级分布两份完全相同 `# `×1 + `## `×13 + `### `×7（`grep -c '^#### '` = 0 : 0）；层级序列 `diff` 空、编号前缀序列 `diff` 空（1-13 + 无编号小节 + 6.1/6.2/6.3 逐行 1:1）⇒ 验收 4 保持 ✅ |
| 3 | `npx tsc --noEmit -p tsconfig.json` | **exit 0**，0 error ✅ |
| 4 | 三守卫（脚本名取自 `package.json:91-93`） | `node scripts/check-id-terminology.mjs` exit 0（5 allow-listed skipped / ok **1714 files scanned**）；`node scripts/check-ci-docs-consistency.mjs` exit 0（drivers ok **11 ids** / window boundaries ok / toolchain ok Node 24, pnpm 11, Rust stable）；`node scripts/check-module-layers.mjs` exit 0（ok **3 rules**）✅ |
| 5 | 相对链接 | 临时 node 脚本（`/tmp/dz-links-r3.mjs`，跑完即 `rm`，随后 `git status --porcelain` 空 ⇒ 仓库零残留）解析 5 份文档的相对 markdown 链接：**14 条，broken 0** ✅ |
| 6 | 反例污染复检（验收 3） | `HostLocaleBridge\|setHostLocaleBridge\|getExtensionTranslation` 5 份文档逐文件 `grep -c` = **0 : 0 : 0 : 0 : 0**；字面 `../../../src/` 实测 **8 处**（zh 2 / en 2 / boundary 4 / components 0 / extensibility 0），逐处判定均在带 `❌ 反例` 标注的代码块或「禁止 / 零新增」句内（zh:181+185、en:196+200、boundary:160/162/163 反例块 + :329 禁止句）；`src/lib/cn` 字面量 6 处命中（zh:185/196、en:200/211、boundary:162/176）逐处判定：3 处在 ❌ 反例代码块（zh:185、en:200、boundary:162），3 处（契约 :176、指南 zh:196 / en:211）是「宿主侧路径如何收尾」的先例举证而非 import 推荐写法，§6.1 与契约 2.1.2 均已明确驱动只 import 包名 ⇒ 不构成推荐写法 ✅ |
| 7 | `node scripts/aggregate-hub.mjs` | **本轮未运行**（diff 不含任何代码/脚本，且第 2 轮已实证重跑结果与提交态一致、`hub.md` 未被本轨 commit 触碰）；`git status --porcelain` 起始与结束均空 ✅ |

过渡期基线随本轮**重新独立清点**（文档 2.1.2 / 2.7 的判据仍然为真）：`grep -rn "from '\.\./.*src/" packages/drivers/*/ui/` = **34 行**；其中宿主 `useI18n` **32 处 / 32 文件**（redis **31** 文件 + `packages/drivers/sqlserver/ui/ConnectionFields.tsx:2` **1**）+ 夹具 `redisKeyWebContextMenu.test.tsx:5,9` **2**；`vi.mock` 宿主 `useI18n` **8** 处；宽松正则总命中 **42 = 34 + 8** ⇒ 与 2.1.2 / 2.7 登记的 34 / 32 / 2 / 8 / 42 **逐项吻合** ✅

### 阶段 B-6：新一轮一致性抽验（优先 §6.2 与本轮新写路径断言）

| # | 断言（位置） | 实测出处与结果 |
| --- | --- | --- |
| 1 | 指南 §6.2 三分规则四要素与契约一致 | `boundary.md:176/197/326` ↔ `zh:196` / `en:211` ✅ |
| 2 | §6.2「留壳 4 / 移走 2」路径存在性与形态 | 见阶段 A-3（4 EXISTS / 2 ABSENT）✅ |
| 3 | §6.2 `92a039383` 移走归属 | 两宿主路径 `git log -1 --` 同 commit ✅ |
| 4 | §6.2 两个宿主消费点直连 SDK | `DriverSettingsSection.tsx:3`、`editorExtensions.ts:36-38` ✅ |
| 5 | §6.2 桥清单（`useBoundSettingsStore`/`useBoundConnectionStore`/`useBoundConfirmDialog`/`useBoundSchemaStore`/`showNativeContextMenu`+`bindContextMenuBridge`） | `packages/driver-sdk/src/index.ts:89-90/99/107/116/131` ✅ |
| 6 | §6.2 未绑定抛错文案 `'…has not been bound to driver-sdk yet.'` | SDK `src/` 内 `grep -rn` = **5 处** ✅ |
| 7 | §6.2 tsx 示例形态「与 redis 现网代码一致」 | `SafeModeBadge.tsx:10` selector、`useRedisGate.ts:28` 二元组、`:32` `getState()`、`:36-41` `confirm({ … kind })` ✅ |
| 8 | §6.2 / 契约 2.3.1 宿主 bind 时机 5 处 | `settingsStore.ts:197`、`connectionStore.ts:248`、`schemaStore.ts:694`、`useConfirmDialog.tsx:69`、`contextMenuStore.ts:40` ✅ |
| 9 | §6.1 `@datazen/ui` 导出面（10 组件 + `cn` + 5 i18n API） | `packages/ui/src/index.ts:1-21`、`:22-29`（含 `t` / `I18nParams`）✅ |
| 10 | §6.1 SDK 消费面（`DatabaseTypeMeta`/`ConnectionFormState`/`KeyEntry`/`NativeMenuItemDef`/`ConnectionViewProps`/`driverCommands`/`fileCommands`/`resolveEditorFontFamily`） | `packages/driver-sdk/src/index.ts:18/29/32/35/42/63/71/85` ✅ |
| 11 | §6.1 裸包名 alias 三处 | `tsconfig.json:19/23`、`vite.config.ts:28/32`、`vitest.drivers.config.ts:10/13` ✅ |
| 12 | §6.3 / 2.4.3「自注册链路由 i18n-drivers 同期落地」仍为未合并真实现状 | 本 worktree `packages/drivers/{redis,mongodb}/locales/index.ts` **均不存在**；两入口 `meta.ts` 无 `locales` 副作用 import；宿主仍走 `src/locales/index.ts:30 registerTranslations(DRIVER_LOCALES)` 聚合 ✅ |
| 13 | §6.3 / 2.4.4「`i18n-sync-check` 当前仅扫宿主」 | `scripts/i18n-sync-check.mjs:21` `localesDir = resolve(root, 'src/locales')` ✅ |
| 14 | 2.4.3 接线层 | `builtinLocales.ts:9` = `['en','zh-CN']`；`lazyPacks.ts:21-34` loaders 仅 en / zh-CN 两组、`:36-38` `isBuiltin()` 对其余语言早退 ✅ |
| 15 | 2.4.3 校验层 | `scripts/i18n-sync-check.mjs:23` `LOCALE_FILES`（8 语言，含 `pt-BR`）✅ |
| 16 | 2.4.3 命名层「驱动包 10 语言文件」+ 前缀 | `ls packages/drivers/redis/locales` = **10**、mongodb = **10**；首 key `redis.add…` / `mongo.applyFilter…` ✅ |
| 17 | **2.4.3 三层 vs 并行轨 O-1 裁定（D-2 专项）** | 三层文本仅限定**宿主**集合且逐句为真，但未记「驱动装载即把 10 语言灌入共享注册表」的不对称终态与已认可量级（`i18n-drivers/bugs.md:170-182`：+76.57 kB min / +6.59 kB gzip；实现证据 `packages/drivers/redis/locales/index.ts:18-41` 静态 import 10 字典 + `ui/shared/meta.ts:4 import '../../locales';`）⇒ 无硬冲突但**存在读者误判面**，登记 **BUG-006（低）** ❌ |
| 18 | 2.4.3 / §6.3 副作用示例字面路径 | 同句点名 redis 入口 `ui/shared/meta.ts`（嵌套两层）却给 `import '../locales';`，按本 worktree 目录布局该说明符不可能解析成功（并行轨实现为 `'../../locales'`）⇒ 登记 **BUG-007（低）** ❌ |
| 19 | 2.1.2 基线命令与 34 / 32(31+1) / 2 / 8 / 42 | 全部独立复现 ✅ |
| 20 | 2.6「Wave 4 护栏待落地」未杜撰脚本名 | `scripts/` 下无 import 护栏脚本（仅无关 `check-structure-editor-guardrails.mjs`），措辞仍为「待 Wave 4 落地」✅ |
| 21 | components.md:575 薄再导出表述带限定语 | 「宿主 `src/lib/nativeContextMenu.ts` 仅为薄再导出（存量 import 兼容）」✅ |
| 22 | components.md:593 Redis Key builder 路径 | `packages/drivers/redis/ui/key-browser/redisKeyContextMenu.ts` EXISTS ✅ |
| 23 | extensibility.md:7/56/58 codegen 说法 | `databaseTypes.ts` 合并 `DRIVER_DB_ENTRIES`、`drivers-registry.json` + gitignored `.drivers-dev.json`、`generated.ts:10`（redis 入口 `ui/shared/meta`）/`:41`/`:44` ✅ |
| 24 | 2.4.2 宿主唯一接线现状 | `grep -rn "setLocale("` 生产调用方 = `localeSync.ts:19/23` + `locales/index.ts:73/77`（`getTranslation` 适配器 ⇒ 观察项 1 仍成立）✅ |
| 25 | 2.4.6 语言切换链路 | `localeSync.ts:19` 以 settingsStore `language` 播种 + `:20-25` subscribe 差异后 `setLocale` ✅ |
| 26 | §6.2 `src/commands/file.ts` 的 host-only 命令 | `:17-22` 起 `openTextWithDialog` 等宿主专用 invoke ✅ |
| 27 | 「不留壳」组确无存量消费者（反向确认规则适用） | `grep -rn "lib/driverSettings\|lib/resolveEditorFontFamily" src packages --include='*.ts*'` = **0** ✅ |
| 28 | 本轮修复未误改契约主文档 | `git diff 88715c03b..HEAD --name-only` 不含 `driver-api-dependency-boundary.md`；契约 :176/:197/:326 三分规则原样在位 ✅ |

**抽验合计：28 条 → 命中 26（失配 2 = BUG-006 / BUG-007），命中率 92.9%。**

### 阶段 C：一致性度量与风险清单（替代覆盖率）

- 本轮独立核对断言 **67 条**（阶段 A 细分 32 条 + 阶段 B 全局不回归 7 项 + 阶段 B-6 抽验 28 条；A 与 B-6 存在 6 条交叠，去重后净 **61 条**），命中 **65**（去重后 **59**），命中率 **97.0%**（去重口径 **96.7%**）；2 处失配即新登记 Bug（BUG-006 / BUG-007）。另有 1 处**历史记录**数字偏差（前轮记「`../../../src/` 9 处」，本轮 `grep -o` 实测 **8 处**；❌/✅ 判定结论不变，已记 `bugs.md` 观察项 3，不计入断言失配、不升级）。
- **未核对断言风险清单**：
  1. Part 1（Rust 段）内部 API 细节仅第 1 轮抽查 2 项（`PROTOCOL_VERSION`、workspace path 依赖），全量未核——本轨未改动该段，风险不变。
  2. 契约 2.3.1 各桥**类型收窄字段集合**只抽查 `SettingsBridgeState` 与 `confirmDialogBridge` 两处签名，其余 3 桥字段未逐项展开。
  3. 「Pro 扩展词条经 `__DATAZEN_HOST__['@datazen/ui']` 共享单例」只在宿主侧验证（`main.tsx:46-48`）；`packages/pro-extensions/` 本机为 gitignored 空目录，实际装载形态未验。
  4. BUG-006 / BUG-007 涉及的**驱动终态**证据取自并行轨检出（只读，未写入对方文件）；本轨 HEAD 尚无这些文件，两轨合并后须回扫确认描述与实现一致（已并入「留待 R 回归」④）。
- 观察项（不阻断）：`bugs.md` 观察项 1（2.4.2 `getTranslation` 括注）、2（Part 1 `"0.1"` 历史文本）、3（9 → 8 计数）、4（§6.2 先例列表可选补一句「驱动不得 import 宿主壳路径」）。

### 阶段 D：本轨关账核对

- **「留待 R 回归」三项登记齐备（复核通过）**：① `i18n-drivers` 合并后回扫 **2.1.2 / 2.7 基线 32 / 2 / 8 / 42**（:353 ①；本轮实测该四数仍为真 ⇒ 措辞未过期）；② **Wave 4 护栏落地后**替换 2.6 / 2.1.2 / 2.7 的「待 Wave 4 落地」措辞并对表白名单（:353 ②；实测 `scripts/` 仍无护栏脚本 ⇒ 措辞仍为真）；③ **BUG-005 zh/en 同步**（:353 ③；本轮阶段 A-5 实测 20:20 code span 相同 + 21:21 标题结构 ⇒ 该风险已闭环）。
- **O-1 裁定对表结论**：2.4.3「三层：接线集 / 校验集 / 命名约定」与「驱动 10 语言全量自注册维持」**不构成事实冲突**（三层句句限定宿主 `src/locales/`，实测为真），但**缺一句驱动侧不对称说明**（宿主运行时只接 en/zh-CN，而驱动装载即把全部 10 语言灌入共享注册表），读者可能误判「全仓运行时只有 2 语言」或「驱动应只注册 2 语言」⇒ 已登记 `BUG-006`（低），修复方向内写明宜同时记入 O-1 量级；`BUG-007`（低）为同段落示例字面路径缺陷，建议与 BUG-006 一次改清。
- 本轮判定：**TEST_FAILED**（BUG-001..005 全部闭环为「已修复」，但新增 BUG-006 / BUG-007 两条低级未闭环缺陷）。

## 留待 R 回归

- 本轨无 E2E（纯文档）。后续验证点已由 Tester 登记在上方「阶段 D：留待 R 回归」：Wave 4 import 护栏落地时的现网基线白名单口径（BUG-001 修正后为 **34 处宿主相对 import（32 `useI18n` + 2 测试夹具）+ 8 处 `vi.mock` 宿主 `useI18n` 路径 = 42 处**，以 2.1.2 登记为准）、驱动侧 `setLocale` 拦截、以及 `i18n-drivers` 合并后回扫 2.4.3 的措辞与模块名。
- **Tester 第 2 轮补登**（详见「阶段 D」小节）：① `i18n-drivers` 合并后 **2.1.2 / 2.7 的过渡期基线数字（32 / 2 / 8 / 42）必然失效**——换源归零时须同步删除该清单或改注为「历史基线」，否则「少于基线应更新 2.1.2」的判据会反向悬空；② **Wave 4 护栏脚本落地后**须回扫 2.6 / 2.1.2 / 2.7，把「待 Wave 4 落地」措辞替换为实际脚本文件名与 CI 位置，并核对护栏白名单与本档口径一致；③ BUG-005（指南 §6.2 两层文档矛盾）修复时 **zh/en 两份必须同步改写**，保持标题结构 21:21 不变（**第 3 轮复测已闭环**：20 : 20 code span 相同、21 : 21 标题）。
- **Tester 第 3 轮补登**：④ **BUG-006 / BUG-007 均落在 2.4.3（及其指南 §6.3 引用面）**，修复属纯文本改动、不得新增标题；两条与 ① 的 `i18n-drivers` 合并回扫同属一次改动，建议协调者让 Coder 一并改清，避免第三次往返；⑤ `i18n-drivers` 合并后须**用本仓库实际实现复核** 2.4.3 的驱动词条行与「三层真值」表述（本轮证据取自并行轨检出只读版：`packages/drivers/redis/locales/index.ts:18-41` 静态 import 10 语言 + `ui/shared/meta.ts:4 import '../../locales';`；O-1 裁定与包体量级见 `.worktrees/datazen-i18n-drivers/docs/development/coordination/tracks/i18n-drivers/bugs.md:170-182`），若合并时语言集合或入口挂载方式变更，2.4.3 需同步改写。
- **Tester 第 4 轮（收口）补登**：⑥ 本表 ①-⑤ 经本轮核对**无缺项**（①=第 2 轮 ①；②=第 2 轮 ②；③=第 3 轮 ④⑤）；将本轮 3 条 **Nit**（`bugs.md`「Nit · 留待 Wave 4 文档回扫」：299 bullet 拆分、§6.3 分例补 mongodb 标注、观察项 1/4 归并）并入同一份回扫清单——建议 Wave 4 护栏落地 + `i18n-drivers` 合并后的那次文档回扫**一次性处理** ①②⑤⑥，避免碎片化改写。

## Coder Bug 修复记录（第 3 轮 · 接管）

被修对象：`decouple-docs-BUG-006` / `decouple-docs-BUG-007`（Tester 第 3 轮登记，被复测 commit `595f106dd`，本轮基准 HEAD `8b7338190`）。

**接管背景**：第 3 轮原 Coder 在修复过程中异常中断，留下 4 个文件的未提交工作区改动。本节先记录**现场取舍**，再记录本轮**新增的纠正**与自验输出。改动仅落在 `driver-api-dependency-boundary.md` 2.4.3、两份指南 §6.3、本 track `progress.md`/`bugs.md`；零生产代码改动、未新增/删除任何标题。

### 一、接管现场盘点（`git diff` 逐行复核 → 保留 / 纠正判定）

| # | 现场遗留改动 | 复核动作（本轮亲自执行） | 判定 |
| --- | --- | --- | --- |
| 1 | `driver-api-dependency-boundary.md:289`（2.4.3「驱动词条」行）：删去单一通用串 `import '../locales';`，改为「相对层级随入口目录深度而定」+ 按 mongodb / redis 两类入口分别举例 | Read 并行轨两个入口实测说明符（见下方对照表） | **保留**（事实正确、BUG-007 契约侧已修清） |
| 2 | 同文件 `:297`（第 ② 层末句）：加入「**就宿主 `src/locales/` 的这 8 个语言文件而言**」限定语 + 「此限定只描述宿主词条，不适用于驱动包，见下条不对称说明」 | 与 `bugs.md` BUG-006 建议修复方向逐字对表（该方向即要求「显式限定为宿主 8 个语言文件」） | **保留** |
| 3 | 同文件新增 `:299` 不对称说明 bullet 的**主体**：三层均为宿主口径、驱动侧注册集合有意不对称、驱动一经装载即全量进 main chunk、O-1 三档实测数字与净增、以及「不存在驱动跟着宿主只注册 2 语言这一形态」 | O-1 数字逐个 grep 复核（见「二、事实复核」）| **保留主体**；**其末句存在事实不精确，已纠正 → 见下方「三」** |
| 4 | 两份指南 §6.3（zh:214 / en:229）：补「全部语言字典静态 import + 注册集合不随宿主收缩（不对称是有意终态，指向契约 2.4.3）」＋入口深度分例 | 与 `bugs.md` BUG-006 建议方向中「§6.3 若同步补须 zh/en 两份同改」一致；本轮以 node 脚本比对两份该行 code span → **13 : 13、集合完全相同**、反引号各 26 | **保留** |
| 5 | `bugs.md`：BUG-006/007 状态 `待修复` → **`待复测（第 3 轮修复完成）`**，并补「Coder 第 3 轮修复说明」段 | 核对纪律要求：**未自标「已修复」**，状态推进权限留给复测 Tester | **保留**（合规，无需回退） |

**结论**：现场无越界改动、无半成品假结构、无自我判定违规；除下述 1 处事实不精确外全部保留，未盲目重做，也未无脑保留。

### 二、事实复核（本轮亲自执行，全部只读并行轨检出，未向对方写入任何内容）

| # | 主张 | 实测命令/动作 → 输出 | 结论 |
| --- | --- | --- | --- |
| 1 | redis 入口挂的是两级串 | `Read .worktrees/datazen-i18n-drivers/packages/drivers/redis/ui/shared/meta.ts` → `:4 import '../../locales';` | 成立 |
| 2 | mongodb 入口挂的是一级串 | `Read .worktrees/datazen-i18n-drivers/packages/drivers/mongodb/ui/meta.ts` → `:4 import '../locales';` | 成立 |
| 3 | 驱动侧确为「全量 10 语言静态 import + 一次性注册」 | `Read .worktrees/datazen-i18n-drivers/packages/drivers/redis/locales/index.ts` → `:18` import `registerTranslations` from `@datazen/ui`；`:19-28` 逐个 import `de/en/es/fr/ja/ko/pt-BR/ru/zh-CN/zh-TW`（**10 个**）；`:30-41` 单次 `registerTranslations({ … })`；文件头注释自陈「Side-effect module… imported from `ui/shared/meta.ts`」「registered as soon as the driver UI is loaded」 | 成立 |
| 4 | O-1 裁定与三档量级 | `.worktrees/datazen-i18n-drivers/docs/development/coordination/tracks/i18n-drivers/bugs.md:170-182` → 「维持 10 语言全量自注册…+76.57 kB min / +6.59 kB gzip 的代价由用户明确认可；本轨禁止改回 en+zh-CN、**禁止引入惰性/按需注册机制**」；三档表 = 1,501.93 / 1,528.55 / 1,605.12 kB（gzip 452.27 / 460.56 / 467.15）；`progress.md:226-227` 同记净增与「确在 **main chunk**（启动即加载，非按需）」 | 成立（文档内四个数字与裁定文本逐项吻合） |
| 5 | 宿主接线集合 | `sed -n '9p' src/locales/builtinLocales.ts` → `export const BUILTIN_LOCALES = ['en', 'zh-CN'] as const;`；`grep -c "pt-BR" src/locales/builtinLocales.ts` → **0** | 成立（第 ① 层为真） |

### 三、本轮纠正：`:299` 末句「运行时可达性判据」写错了机制

现场把「某语言在运行时是否可达」归给「宿主 `BUILTIN_LOCALES` **与 `src/locales/lazyPacks.ts` 的接线集合**」。本 worktree 实测两处不精确：

1. **`lazyPacks.ts` 不是驱动词条的可达性闸门**：该模块 `:16-17` 的 `registry` / `inflight` 与 `:21` 的 `loaders` 类型为 `Record<BuiltinLocale, Record<LazyDomain, …>>`，键域只有 `en` / `zh-CN`（`:22-33`），值是宿主 4 个惰性域包（`src/locales/domains.ts:21` `LAZY_DOMAINS = ['sync','workflows','dashboard','mcp']`），`:36-38 isBuiltin()` 对其余语言直接早退。驱动词条是 eager 进共享注册表的，**根本不经过该机制**——把它列为判据还与同句「不做惰性 / 按需注册」自相矛盾。
2. **遗漏真实的第三语言接入路径**：`src/locales/index.ts:34` 维护 `extensionLocales`，`:42-45 registerLocale()` 注册新语言并灌入字典，`:51-56 getExtensionLocales()` 供设置页拼接语言下拉（实测消费点 `src/windows/settings/SettingsContent.tsx:91-100` = `BUILTIN_LOCALES.map(…)` + `…getExtensionLocales()`），`:36-38 getAvailableLocales()` 同样返回两者并集。O-1 收益侧的原始表述正是「为未来某扩展经 `registerLocale()` 引入第 3 语言预付」。

改法：可达性判据改写为「`BUILTIN_LOCALES` 并上宿主扩展经 `registerLocale()` 注册的语言」（带 `index.ts:42-45` 与 `SettingsContent.tsx:91-100` 实测出处），并单列一句说明 `lazyPacks.ts` 只服务宿主词条、驱动词条不经它装载；同时补写「驱动多出的 8 个语言在当前宿主集合下运行时不可达、其字典常驻注册表」这一 O-1 已承认的事实，避免读者反向推成「注册即可用」。

同段另两处小改：① 「也不做惰性 / 按需注册」→「本契约同样**不要求、不建议**驱动改走惰性 / 按需注册——O-1 裁定明令禁止引入该机制，全量 eager 注册即终态」（消除被读成建议的空间，与裁定原文同向）；② 「同轨 `bugs.md` 观察项 O-1」→ 写明完整路径 `docs/development/coordination/tracks/i18n-drivers/bugs.md`，便于后续复测定位。

### 四、文档 import 串 ↔ 实际入口 ↔ 正确相对串 对照表（自验第 1 项）

| 文档写的串 | 文档点名的入口 | 实际入口文件（绝对路径） | 该入口内实测串 | 判定 |
| --- | --- | --- | --- | --- |
| `入口在 ui/meta.ts（如 mongodb）写 import '../locales';` | mongodb | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-i18n-drivers/packages/drivers/mongodb/ui/meta.ts` | `:4` `import '../locales';` | ✅ 一致 |
| `入口在 ui/shared/meta.ts（如 redis，嵌套两层）写 import '../../locales';` | redis | `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-i18n-drivers/packages/drivers/redis/ui/shared/meta.ts` | `:4` `import '../../locales';` | ✅ 一致 |

两串均已在 `:289`（契约）与 `zh:214` / `en:229`（指南）三处按同一口径写明；`grep -rn "\.\./locales" docs/` 全量清点确认：三份正文文档中该串**只出现在上述按深度分例的句内**，无残留「单一通用串」写法（其余命中均为 `bugs.md`/`progress.md` 的缺陷举证与历史记录，属引用旧文，不改）。

### 五、第 3 轮自验（真实输出）

1. **对照表**：见上节「四」，redis / mongodb 各一行，均由 Read 实际文件证实。
2. `grep -n "locales" docs/development/driver-api-dependency-boundary.md docs/development/independent-driver-development.*.md` → 命中 `boundary:154/288/289/290/294/296/297/298/299/305/342`、`zh:41/181/214/215`、`en:41/196/229/230/389`；逐处判定：`:289` 与 `zh:214`/`en:229` 已按两类入口分别给串且深度正确；`:288/290` 为宿主 eager-lazy 与 EP 行，不含驱动入口示例；`:342`（2.7 自查项）只写「经本包 `locales/index.ts` 自注册」，无相对说明符 ⇒ **无残留错误深度**。
3. **2.4.3 新表述自检**：`grep -n "只注册" boundary.md` → 仅 `:299` 1 处，且位于「不存在『驱动跟着宿主只注册 2 语言』这一形态」的**否定引用**内，无「宿主只接线 2 语言 ⇒ 驱动也应如此」暗示；`grep -n "惰性\|按需注册" boundary.md` → `:288`（宿主 lazy 域包既有描述）与 `:299`（「不要求、不建议驱动改走惰性 / 按需注册——O-1 裁定明令禁止引入该机制」＋「宿主惰性域包…驱动词条不经它装载」）⇒ **均为禁止性/澄清性表述，无建议语气**；`grep -n "76.57\|6.59\|1,501.93\|1,528.55\|1,605.12"` → `:299` 命中，O-1 三档与净增数字**全部在文**。
4. **标题结构**：`grep -c "^#"` → zh **21** / en **21**；层级分布两份完全相同 `#`×1 + `##`×13 + `###`×7（`####` 0）；`diff <(grep -o '^#\+' zh) <(grep -o '^#\+' en)` → **空输出**（本轮纯文本改写，未新增/删除标题，验收 4 保持）。
5. **编译与守卫**：`npx tsc --noEmit -p tsconfig.json` → **exit 0**；`node scripts/check-id-terminology.mjs` → exit 0（5 allow-listed occurrence(s) skipped / ok, 1714 files scanned）；`node scripts/check-ci-docs-consistency.mjs` → exit 0（drivers ok 11 ids / window boundaries ok / toolchain ok Node 24, pnpm 11, Rust stable）；`node scripts/check-module-layers.mjs` → exit 0（ok, 3 rules）。
6. **范围纪律**：`git status --short` 提交后为空；`git diff --name-only 8b7338190..HEAD` → 4 个文件，全部在允许清单内（`driver-api-dependency-boundary.md`、`independent-driver-development.zh-CN.md`、`.en.md`、本 track `progress.md`/`bugs.md`）；`git diff --stat 8b7338190..HEAD -- src packages scripts src-tauri e2e AGENTS.md` → **空**，零生产代码改动，未触碰 `hub.md`、`AGENTS.md`、契约 Part 1。
7. **未污染并行轨**：`.worktrees/datazen-i18n-drivers` 仅执行 Read / grep / sed 只读动作，未运行任何写入命令，对方工作区文件清单未因本轮改变。
8. `node scripts/aggregate-hub.mjs` 未运行（避免改写禁止触碰的 `hub.md`），与前两轮口径一致；本轮 diff 不含任何代码/脚本路径，结论等价。

### 六、每条 Bug 的一句话修法

- **BUG-006**：在 2.4.3「三层真值」后补一条独立 bullet，写明三层全为宿主口径、驱动侧注册集合**有意不对称**（全量 eager、启动即进 main chunk、O-1 三档实测 +76.57 kB min / +6.59 kB gzip 已获认可），并把「运行时可达语言集合」的判据钉回宿主 `BUILTIN_LOCALES` ∪ `registerLocale()` 扩展语言；第 ② 层「无运行时 import」显式限定为宿主 `src/locales/` 的 8 个语言文件，两份指南 §6.3 同步该不对称结论并指向契约。
- **BUG-007**：把「挂一行 `import '../locales';`」的单一通用串改为「相对层级随入口目录深度而定」，并按实测分别举例——入口 `ui/meta.ts`（mongodb）→ `'../locales'`、入口 `ui/shared/meta.ts`（redis）→ `'../../locales'`；契约 `:289` 与指南 `zh:214` / `en:229` 三处同改。

## Tester 第 4 轮（收口）复测记录（commit `91921a87d`，全新实例独立实测；前三轮记录仅作断言清单，不采信）

工作目录 `.worktrees/datazen-decouple-docs` @ `feature/decouple-docs`；基准 `fd23a66a8`；HEAD 实测 `91921a87d`（父 `8b7338190` → `595f106dd` 链齐）；起始 `git status --short` 空。本轮修复触及 5 文件（`git show --stat 91921a87d`）：契约、两份指南、本 track `progress.md`/`bugs.md`。全程只读访问 `.worktrees/datazen-i18n-drivers`（仅 Read/sed/ls/grep，零写入命令），未触碰主检出。

### 阶段 A：BUG-006 / BUG-007 定点复验 → **两条均「已修复」**

1. **BUG-006（2.4.3 `:288-299` 读全段判定）**：
   - (a) 三层仍是宿主口径且不可再被误读——`:297` 第 ② 层已带「就宿主 `src/locales/` 的这 8 个语言文件而言」限定语并指向下条；`:299` 新 bullet 首句「上述三层全部只是宿主侧口径；驱动侧的注册集合有意与之不对称，不是越界」+ 显式否定「不存在『驱动跟着宿主只注册 2 语言』这一形态」✅；
   - (b) O-1 数字逐个比对（对照 `.worktrees/datazen-i18n-drivers/.../i18n-drivers/bugs.md` O-1 表与 `progress.md` B1 段）：三档 min **1,501.93 / 1,528.55 / 1,605.12** kB、净增 **+76.57 kB min / +6.59 kB gzip**、「均在 main chunk / 启动即加载」、「为未来经 `registerLocale()` 接入第 3 语言预付」收益侧表述——**逐项一致，无杜撰、无漂移** ✅；
   - (c) 「惰性 / 按需」全扫 3 份文档：仅 boundary `:288`（宿主既有机制的事实描述）与 `:299`（「不要求、不建议…O-1 裁定明令禁止引入该机制，全量 eager 注册即终态」），**均为禁止/澄清性表述，无建议语气** ✅。
2. **接管自我更正的新判据链（`:299`）逐条 Read/Grep 证实**：

   | 文档断言 | 本轮实测 | 判定 |
   | --- | --- | --- |
   | `src/locales/index.ts:42-45 registerLocale()` 写入 `extensionLocales` 并灌字典 | 该函数恰在 `:42-45`，`:43` `extensionLocales.set(locale, label)`、`:44` `registerTranslations({[locale]: translations})` | ✅ 行号精确 |
   | `:51-56 getExtensionLocales()` | 恰在 `:51-56` | ✅ |
   | 汇入设置页语言下拉，见 `src/windows/settings/SettingsContent.tsx:91-100` | `:91-100` 恰为 `languageOptions = useMemo([...BUILTIN_LOCALES.map(...), ...getExtensionLocales()], [])` | ✅ |
   | `registerLocale` 生产代码零调用（机制为未来预留） | Grep 全仓（含 `packages/wapps`、`packages/extension-points`、`packages/wapp-sdk`、`src/`）：除定义与文档外仅 `src/locales/locales.test.ts:205/213/218/224` 测试调用 ⇒ **生产零调用** | ✅ |
   | `lazyPacks.ts:21-34` 只服务宿主词条、键域仅 en/zh-CN | `loaders: Record<BuiltinLocale, ...>` 恰 `:21-34`，仅 `en`（`:22-27`）与 `'zh-CN'`（`:28-33`）两组，值全部为宿主 `./en/*`、`./zh-CN/*` 域包；`:36-38 isBuiltin()` 对其余语言早退（`:54`）| ✅ 行号精确 |
   | 域清单 `src/locales/domains.ts:21` = `sync` / `workflows` / `dashboard` / `mcp` | `:21` `export const LAZY_DOMAINS = ['sync','workflows','dashboard','mcp'] as const;` | ✅ |
   | 驱动 eager 词条不经 lazyPacks 装载 | `locales/index.ts`（并行轨）单次 `registerTranslations({...10 语言})` 直灌共享注册表，全链无 lazyPacks 引用 | ✅ |

3. **BUG-007**：契约 `:289` 与指南 `zh:214` / `en:229` 三处均已按入口深度分例；以 Read 逐字核对并行轨真实入口：`packages/drivers/redis/ui/shared/meta.ts:4` = `import '../../locales';`、`packages/drivers/mongodb/ui/meta.ts:4` = `import '../locales';`——**与文档串逐字一致**；入口归属旁证：`scripts/resolve-drivers.mjs:239/:270` 与 `src/extensions/generated.ts:10`（redis 首个 UI import 即 `ui/shared/meta`）。

### 阶段 B：全局不回归（7 项全过）

| # | 验收项 | 实测结果 |
| --- | --- | --- |
| 1 | diff 范围 | `git diff --name-only fd23a66a8..HEAD` = 8 文件全在 `docs/**`；`git diff --stat fd23a66a8..HEAD -- src packages scripts src-tauri e2e AGENTS.md` = **空**；`git log fd23a66a8..HEAD -- hub.md` 空输出（hub.md 差异系分叉基线再聚合，非本轨改动）⇒ 零越界 ✅ |
| 2 | 标题结构 | zh **21** / en **21**；层级分布两份相同 `#`×1 + `##`×13 + `###`×7；`diff <(grep -o '^#\+' zh) <(grep -o '^#\+' en)` = **空** ✅ |
| 3 | 相对链接 | 临时脚本 `/tmp/dz-links-r4.mjs`（跑完即 `rm`，事后 `git status --porcelain` 空）解析 5 份文档：**14 条，broken 0** ✅ |
| 4 | 反例污染复检 | `HostLocaleBridge\|setHostLocaleBridge\|getExtensionTranslation` 5 份文档 = **0:0:0:0:0**；字面 `../../../src/` 共 **8 处**（boundary :160/162/163 反例块 + :330 禁止句、zh :181 禁止句 + :185 ❌ 块、en :196 + :200 同形），逐处判定均在 ❌/禁止语境 ✅；`src/lib/cn` 6 处（:162/:176/:197、zh:185/196、en:200/211——其中 boundary :176/:197 与指南 §6.2 为先例举证且同句写明「驱动永远 import 包名」）✅ |
| 5 | `npx tsc --noEmit -p tsconfig.json` | **exit 0** ✅ |
| 6 | 三守卫 | `check-id-terminology` exit 0（5 allow-listed / 1714 files）；`check-ci-docs-consistency` exit 0（11 ids / window boundaries / toolchain）；`check-module-layers` exit 0（3 rules）✅ |
| 7 | `node scripts/aggregate-hub.mjs` | exit 0（聚合 11 tracks）；`git status --short` 仅 ` M hub.md` → `git checkout --` 恢复，**未提交**；本轮结束时 `git status --porcelain` 空（除本记录 commit 前瞬时状态）✅ |

### 阶段 B-6：本轮新写/改写断言抽验（含阶段 A 调用链）

| # | 断言（位置） | 实测 |
| --- | --- | --- |
| 1-6 | `:299` 可达性判据链 6 项 | 见阶段 A-2 表，6/6 ✅ |
| 7 | `:299` O-1 三档 + 净增数字 | 与并行轨 bugs.md/progress.md 逐项一致 ✅ |
| 8 | `:289` redis 入口副作用串行 | 并行轨 `redis/ui/shared/meta.ts:4` 逐字 ✅ |
| 9 | `:289` mongodb 入口副作用串行 | 并行轨 `mongodb/ui/meta.ts:4` 逐字 ✅ |
| 10 | `:289` 「静态 import 本目录全部语言字典后一次性 registerTranslations」 | 并行轨 `redis/locales/index.ts`：`:18` import `registerTranslations`、10 个语言静态 import、单次 `registerTranslations({...})`、`export {}` 纯副作用 ✅ |
| 11 | `:296` 第 ① 层 `builtinLocales.ts:9 = ['en','zh-CN']` | `sed -n '9p'` 逐字命中 ✅ |
| 12 | `:297` 第 ② 层 8 语言 = `i18n-sync-check.mjs:23 LOCALE_FILES` | `:23` `['de','es','fr','ja','ko','pt-BR','ru','zh-TW']`，`src/locales/pt-BR.ts` + `pt-BR/` 存在 ✅ |
| 13 | `:298` 「驱动包 locales/ 现覆盖 10 个语言文件（redis、mongodb 各 10）」 | 本 worktree 对 `packages/drivers/redis/locales` 与 `packages/drivers/mongodb/locales` 计数 .ts 文件 = **10 / 10** ✅ |
| 14 | `:288` 「lazy 域包经 `useLocaleDomains` / `ensureLocaleDomains` 按需注册」 | `src/hooks/useLocaleDomains.ts:14`、`lazyPacks.ts:50` ✅ |

**抽验合计 14 条 → 命中 14，命中率 100%**（另叠加阶段 A 定点链 6 项细分，共 20 条断言全命中）。

### 阶段 C：一致性度量（文档轨以命中率替代覆盖率）

- 本轮独立核对断言 **≥20 条**（阶段 A 12 细分 + 阶段 B-6 14，交叠去重后净 20），命中 **20**，命中率 **100%**。前轮 47/44/61 条基线断言中未被本轮改动触及的条目经 diff 范围核对（本轮仅 2.4.3 三行 + 指南 §6.3 两行 + track 文件）确认**无一被回退改写**。
- 未核对项风险与第 3 轮相同（Part 1 Rust 段全量、3 桥字段集合、Pro 装载形态），均不在本轨改动面内，不升级。

### 阶段 D：判定与关账

- 按协调者第 4 轮判级纪律：**Blocker 0、Nit 3** ⇒ 本轨判定 **TEST_DONE (PASSED)**，收口合流。BUG-001..007 全部「已修复」。
- 3 条 Nit（2.4.3 `:299` bullet 拆分、§6.3 分例补 mongodb 标注、观察项 1/4 归并）已登记 `bugs.md`「Nit · 留待 Wave 4 文档回扫」并挂入上方「留待 R 回归」⑥。
- 「留待 R 回归」清单完备性核对：① 合并 i18n-drivers 后回扫 2.1.2/2.7 基线数字（在位）、② Wave 4 护栏落地后替换 2.6 措辞（在位）、③ BUG-006/007 相关段落复核（第 3 轮 ④⑤ 在位）⇒ 无缺项，本轮补 ⑥ 归并指引。
- 交付 commit：本记录 + `bugs.md` 状态推进（Tester 第 4 轮），不含任何生产代码与 hub.md。
