# P5 i18n 轨进度台账（`feature/p5-i18n`）

分支根目录台账，交付合并时删除（见根 AGENTS.md「进度台账：开发期间允许，交付即销毁」）。

基线：`codex/p5-integration` @ `16faad738`，起点工作区 clean。

---

## 1. 已完成

### 1.1 补齐 `migrationJob.*` 两个 key（任务 1）

`migrationJob.reprepareOnStalePlan`、`migrationJob.pendingVerificationHint` 已补入 9 个 locale 的
`src/locales/<L>/sync.ts`，插入位置与 en 一致（`const pack = {` 之后、`migrationHistory.*` /
`// --- Data Sync ---` 之前），保持 key 集合与 en 完全相同：

| locale | 状态 | 备注（术语与该 locale 周边条目对齐） |
| --- | --- | --- |
| de | ✅ | job=Auftrag（沿用 `transfer.limitations.noResume`）、reconcile=abgleichen（沿用 resume hint）、Sie 称呼 |
| es | ✅ | job=trabajo、reconcile=conciliar、tú 式（与周边 noResume/invalidan 一致） |
| fr | ✅ | job=tâche（沿用 noResume）、reconcile=rapprochement（沿用 resume hint）、jeton/valider 一致 |
| ja | ✅ | job=ジョブ（沿用 noResume）、plan=プラン（沿用 `schemaDiff.step.plan`）、検証/整合 一致 |
| ko | ✅ | job=작업（沿用 noResume）、계획（沿用 `schemaDiff.step.plan`） |
| pt-BR | ✅ | job=trabalho（沿用 noResume）、reconciliar（沿用 resume hint）、你式祈使（与周边一致） |
| ru | ✅ | job=задание（沿用 noResume）、согласовать（沿用 resume hint）、Вы 称呼 |
| zh-TW | ✅ | job=作業／作業中心、核對狀態、計畫（沿用 `schemaDiff.step.plan`） |
| zh-CN | ✅ | job=任务／任务中心（沿用 `transfer.limitations.noResume` 的「当前任务」）、计划（沿用 `schemaDiff.step.plan`） |

**与任务书的一处事实差异（重要）**：任务书称「zh-CN 疑似已齐」，实测 **zh-CN 同样缺这 2 个 key**
（`grep migrationJob src/locales/zh-CN/sync.ts` 无命中）。已一并补齐——zh-CN 是与 en 并列的
唯一两个内建（built-in）locale，缺 key 会让中文 UI 直接回退英文原文。

`scripts/i18n-sync-check.mjs` 的 `LOCALE_FILES` 硬编码为 8 个 host locale（第 36 行），
**不含 zh-CN**，因此该门禁原本看不到 zh-CN 的任何缺失；这也是「疑似已齐」结论的来源。

### 1.2 `schemaDiff.limitations.*` 运行时警告根因（任务 2）

**根因：不是 key 缺失、不是命名空间前缀、不是 `lazyPacks` 注册 bug，而是那 3 个测试文件自身的
i18n fixture 缺失。** 结论行：`[i18n] Missing translation ... not registered for "en"` 里的
"en" 指的是**运行时注册表为空**，而不是 en 字典缺该 key。

链条（逐环实测）：

1. `schemaDiff.limitations.*` 7 个 key 在**每个** locale 的 `sync.ts` 都存在且与 en 同条数，
   `sync.ts` 属 **lazy domain**（`domains.ts`：`LAZY_DOMAINS = ['sync','workflows','dashboard','mcp']`）。
2. lazy pack 只有两条注册入口：`lazyPacks.ensureLocaleDomains()`（由 `useLocaleDomains` 触发）
   或 `fullLocales.ts`。两者都不在 `src/locales/index.ts` 的 eager 注册循环里。
3. `SchemaDiffWindow.test.tsx` / `SchemaDiffWizard.test.tsx` / `SchemaDiffProfileLoad.test.tsx`
   三个套件 `vi.mock('../../../hooks/useLocaleDomains', () => ({ useLocaleDomains: () => true }))`，
   于是**永远不会触发** `ensureLocaleDomains('en', ['sync'])`。
4. `src/windows/schema-diff/SchemaDiffWindow.tsx:801` 把 `<LimitationsDialog>` 无条件渲染（`open` 默认
   false），它是 `@datazen/ui` 的**真实组件**，内部用 `packages/ui/src/i18n.ts` 的真 `t()`——
   **不受** 这三个套件对 host `useI18n` 的 identity mock 影响。`LimitationsDialog` 无条件求值
   `t(titleKey)` / `t('common.close')` / `t(dontShowAgainKey)` / `t(limitationKey)`，`Dialog` 的
   `if (!open) return null` 在其之后，救不了。
5. 探针实测（临时文件，已删除）：`getRegisteredTranslations('en')` 在这 3 个套件的 mock 形态下
   只有 eager 的 2103 个 key，`schemaDiff.limitations.title === undefined`。被告警的是 **9 个 distinct
   key**：`schemaDiff.limitations.*` 8 个（`title` + `dontShowAgain` + `schemaDiffLimitationKeys.ts`
   里的 6 条 bullet）+ `common.close`。任务书写的「7 个 key」漏计了 `title`（它同样被
   `LimitationsDialog` 无条件求值）。

**修复**：在这 3 个套件补测试侧字典 fixture——`import '../../../locales'`（eager 注册）+
`beforeAll(() => ensureAllLazyDomains('en'))`（沿用 `locales.test.ts` / `MigrationEndpointsBar.test.tsx`
已有的 test-only 预热路线）。生产代码、`lazyPacks.ts`、`domains.ts`、`useLocaleDomains.ts` **零改动**。
host `useI18n` 的 identity mock 保留原样，因此 `getByText('schemaDiff.objectKind.trigger')`
一类以 key 为定位符的断言不受影响（已逐条核对：`common.cancel` / `common.selectAll` /
`common.deselectAll` / `schemaDiff.copySummary` / `schemaDiff.regeneratePlan` /
`schemaDiff.objectSelectAllSource` / `schemaDiff.back` 全部由 host 组件经被 mock 的 `t()` 渲染）。

改法选它的理由：影响面最小，且与既有懒加载机制完全一致（只是把「测试自己 mock 掉了触发器」这件事
在测试里补回来），不改任何运行时行为。

**效果**：门禁 3 的 `Missing translation` 警告 **27 行 → 0 行**（回退我的测试改动后重跑实测：
9 个 distinct key × 3 个套件 = 27 行，其中 24 行属 `schemaDiff.limitations.*`、3 行属 `common.close`；
`packages/ui/src/i18n.ts` 的 `reportedMissingKeys` 每个 module graph 只报一次，故行数是 key 数 × 文件数）。

---

## 2. 门禁（逐字结论行）

统一重定向到系统 temp；工作区内容哈希首尾各取一次，证明跑门禁期间无人改动。

```
WS_HASH_START=d3281b245361ff1c
HEAD=16faad738317f43ef044ecd043464b61a128b553  TREE=30bdbecb4fe1df0761012b8659cb176111ae5eca
```

### 门禁 1 — `node scripts/i18n-sync-check.mjs`

```
GATE1_EXIT=0
All locale files are in sync with en.ts.
```

（0 missing / 0 stale；`Comparing the English locale against v0.2.3: 2 key(s) changed/added`，
两个 changed key 均已翻译，故 stale=0。补充旁证：`node scripts/i18n-key-collision-check.mjs`
EXIT=0，`2412 host key(s) from 10 locale(s) ... 0 key(s) owned by both, on both axes`。）

### 门禁 2 — `npx vitest run src/locales`

```
GATE2_EXIT=0
 Test Files  1 passed (1)
      Tests  21 passed (21)
```

该套件里保留 1 条 `Missing translation` 警告，是 `locales.test.ts > falls back through dict chain
for unknown keys` **故意查询** `'this.key.does.not.exist'` 造成的，属被测行为，不是缺陷。

### 门禁 3 — `npx vitest run src/windows/schema-diff src/commands/__tests__/schemaDiff.test.ts`

```
GATE3_EXIT=0
 Test Files  10 passed (10)
      Tests  87 passed (87)
GATE3_limitations_warn=0
GATE3_any_missing_warn=0
```

`schemaDiff.limitations.*` 警告与全部 `Missing translation` 警告均为 0（不依赖「遗留」豁免）。

### 门禁 4 — `pnpm --config.verify-deps-before-run=false typecheck`

```
GATE4_EXIT=0
G4_errorTS=0
```

（`check:restore-guards` ok + `tsc --noEmit` + `typecheck:scripts` + `typecheck:pack-ep` 全绿。）

```
WS_HASH_END=d3281b245361ff1c   ← 与 START 相同
```

### 额外自证（非任务书门禁，均 EXIT=0）

- `npx vitest run scripts/__tests__/i18n-sync-check.test.mjs` → `Tests 19 passed (19)`
- 大范围 i18n 相邻套件 `src/locales src/lib/__tests__ src/components/migration src/windows/data-sync
  src/windows/data-transfer src/test scripts/__tests__` → `Test Files 191 passed (191) / Tests 2160 passed (2160)`
- `npx prettier --check "src/locales/*/sync.ts"` → All matched files use Prettier code style.
- 新 key 运行时回读：en / zh-CN 均解析出译文（非 raw key）；8 个 host locale 静态解析值逐条核对通过。

---

## 3. 遗留 / 待裁定

### 3.1 zh-CN 另有 117 个 key 缺失（超出本轨任务 1 范围）

- 实测 `en - zh-CN = 117`（`sync.ts` 114 个 + `settings.ts` 3 个，清单见附录 A），`extra=0`。
- 两个门禁都看不见它：门禁 1 的 `LOCALE_FILES` 不含 zh-CN（第 36 行硬编码），门禁 2 的
  `every built-in locale resolves every UI key` 用 `getTranslation`，缺 key 会**静默回退 en**，
  断言 `not.toBe(key)` 照样成立。所以 zh-CN 的 key 集合与 en **不一致**，但没有任何自动门禁会失败。
- 我没有顺手补：任务 1 明确限定「8 个 host locale × 2 个 key」，且我无从确认这 117 个是不是别的轨
  （data-sync / data-transfer）尚未补的自有新增——在并行分支上替别的轨写 117 条中文文案，属于越界，
  且冲突面大。**建议**：由协调者在集成分支上裁定（补 or 放宽 `LOCALE_FILES` 覆盖 zh-CN 后统一补）。

### 3.2 同类警告在别的窗口套件仍有残留（与本轨任务 2 无关，未动）

回退我的改动后实测，这些数字与我的改动无关（改动前后一致）：

| 套件 | `Missing translation` 警告 |
| --- | --- |
| `src/windows/data-transfer` | 9 条（`transfer.limitations.*` 7 + `common.close` + `common.copy`） |
| `src/components/migration/MigrationRunHistoryDialog.test.tsx` | 1 条（`common.close`） |
| `src/windows/data-sync` | 0 条 |

同一根因（套件 mock 掉 `useLocaleDomains` + `@datazen/ui` 真 `t()`），修复模板就是本轨 1.2 的两行
fixture。未顺手改：任务 3 的门禁 3 只圈定 `src/windows/schema-diff` 与 `schemaDiff.test.ts`，
data-transfer 属别的轨的领地，改了会在集成分支上多制造冲突点。**建议**：协调者按上面的模板各补一次，
或把该 fixture 收敛进一个共享 test setup，避免每个套件重复。

### 3.3 结构性观察：9 个 locale 有字典却没有任何注册入口（需产品裁定，非本轨可修）

`src/locales/<L>/` 下 de/es/fr/ja/ko/pt-BR/ru/zh-TW（+ zh-CN）各有一份 14 域的完整字典，但：

- `builtin-locales.json` 只列 `en, zh-CN`，生成的 `builtinLocales.ts`（gitignored）只 import 这两个的
  `eager.ts`，`src/locales/index.ts` 的注册循环因此只注册 en/zh-CN；
- `lazyPacks.ts` 的 `loaders` 同样只有 `en` / `zh-CN` 两个键，`isBuiltin()` 对其他 code 直接 return false；
- `src/extensions/generated-locales.ts` 为空，全仓无任何一处 import `locales/de` 等（除
  `src/test/__tests__/dialogCloseLabelLocaleSubscription.test.tsx` 自用）。
- 探针实测：`setLocale('de')` 后 `t('probe.key')` 返回的是 en 文案（`registry['de']` 不存在，走
  `registry[DEFAULT_LOCALE]`）。

所以这 8 套翻译目前在**运行时不可达**，`i18n-sync-check` 只是在维护「静态 key 集合一致」这一件事。
注意 UI 侧不至于显示 raw key（会回退 en），但语言仍表现为英文。本轨只按要求补齐 key 集合，
**不改 `builtin-locales.json` / `lazyPacks.ts`**：加注册入口会改变 shipped chunk 体积与语言下拉
（`SettingsContent.tsx:100` 直接 map `BUILTIN_LOCALES`），属产品决策。留给协调者裁定。

### 3.4 本 worktree 的 `node_modules` 原先不是那个「指名 symlink」

任务书说明 `node_modules` 是指名 symlink，但实测本 worktree 里它是一个**空的真实目录**（仅含
`.vite` / `.vite-temp` / `.cache` 缓存），而同级 `datazen-p5-data-sync` / `datazen-p5-data-transfer`
都是 `node_modules -> <主仓>/node_modules` 的 symlink。第一次门禁 4 因此失败：

```
GATE4_EXIT=127
sh: tsc: command not found
```

处理：把那 3 个缓存子目录 `mv` 到 `/tmp/dz-i18n-node_modules-cache-backup`（未 `rm`），按同级 worktree
的既有形态建立 symlink，然后重跑，EXIT=0。**未执行 `pnpm install`**。
这属环境修复、非代码改动；`node_modules` 为 gitignored，不进提交。

### 3.5 其他事实（供集成参考）

**改动清单（12 个文件，+83 / −3）**

- `src/locales/{de,es,fr,ja,ko,pt-BR,ru,zh-CN,zh-TW}/sync.ts` — 各 +4 行（2 个 key）。
- `src/windows/schema-diff/__tests__/SchemaDiffWindow.test.tsx` — +17 / −1（净 +16，86 → 102 行）。
- `src/windows/schema-diff/__tests__/SchemaDiffProfileLoad.test.tsx` — +15 / −1（净 +14，560 → 574 行）。
- `src/windows/schema-diff/__tests__/SchemaDiffWizard.test.tsx` — +15 / −1（净 +14，852 → 866 行）。
- `progress.md` — 本台账。
- `src-tauri/**`、`packages/**` **零改动**（`git status --porcelain -- src-tauri packages` 为空）。

- 门禁前本 worktree 缺 gitignored codegen（`src/locales/builtinLocales.ts`、`src/extensions/generated.ts`），
  会使 3 个套件直接 `Failed to resolve import` 而整组失败。已用仓库自带脚本补齐：
  `node scripts/generate-builtin-locales.mjs` + `node scripts/resolve-drivers.mjs --codegen-only --drivers=basic`
  （即 `pretest` 的既有路线）。产物均 gitignored，`git status` 保持干净。
- `SchemaDiffWizard.test.tsx` 现为 866 行（HEAD 时已 852 行，本就超 AGENTS.md 推荐的 800 行上限；
  我的净增 14 行，其中含被压缩过的注释）。未为凑行数拆分该测试文件——不在本轨范围内，且拆分一个
  852 行的 journey 套件风险远大于收益。
- `SchemaDiffWizard.test.tsx` / `SchemaDiffProfileLoad.test.tsx` 在 HEAD 就已被 prettier 标记
  （缩进/引号风格漂移，位置在我未触碰的对象字面量区段）。我新增的行本身符合 prettier。
  未顺手全文件 `--write`，以免在集成分支上制造大面积无关 diff。

---

## 附录 A：zh-CN 缺失 key 清单（117 个，门禁均不可见）

见 `/tmp/dz-zhcn-missing.txt`（本机临时文件，不入库；按字典序）。按 domain 文件分布（逐字统计）：

- `sync.ts` 114 个、`settings.ts` 3 个（`settings.updater.manualDescription` /
  `settings.updater.manualVariant` / `settings.updater.openReleases`）。

`sync.ts` 内按前缀计数（starts-with，故前缀之间有包含关系，不求和为 114）：

| 前缀 | 条数 |
| --- | --- |
| `transfer.destination*` | 16 |
| `sync.profile*` | 11 |
| `sync.recordset*` | 11 |
| `schemaDiff.object*` | 18 |
| `schemaDiff.profile*` | 9 |
| `migrationHistory.sync*` | 9 |
| `schemaDiff.objectKind*` | 7 |
| `transfer.tableOutcome*` | 5 |
| `transfer.historyOutcome*` | 5 |

其余为 `sync.executionNotStarted`、`sync.clearAll*`、`sync.pageScopeAll`、
`schemaDiff.crossDialect*`、`schemaDiff.savedObjectsMissing`、`transfer.resume*`、
`transfer.runUnknownOutcome` 等零散条目。

