# Track: r-phase — Wave 4-B 全量回归与关账（本专项唯一 R 阶段）

- 分支: `feature/r-phase`（基准 = Wave 4-A `import-guard` 合并后的 `feat/driver-decoupling` HEAD）
- 角色: Tester（回归执行 + 关账记录）；发现缺陷只登记 `bugs.md`，不修代码
- 波次: Wave 4-B（本轨完成即整个「驱动↔宿主解耦」专项收口）

## 口径

Wave 1~4 中间各次合入只做「合并健全性校验」，完整回归统一在本轨跑一次。本轨输入 = 下方
【A 门禁】全量复跑 + 【B 各轨留待项】逐项闭环 + 【C 关账】文档与状态收口。

## A. 门禁基线（合并后主检出实测，2026-09-21 @ `8b66586e4`；Wave 4-A 二次合流后代码基准 = `b22b41ac8`，文档基准 = 本文件所在 HEAD）

| 命令 | 基线 |
| --- | --- |
| `node scripts/resolve-drivers.mjs --codegen-only --drivers=all` | exit 0（worktree 默认只 boot basic，任何驱动 UI 校验前必须先跑） |
| `npx tsc --noEmit -p tsconfig.json` | 0 error（`--drivers=all` 与 `--drivers=basic` 两档分别实测） |
| `npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui` | **27 files / 222 pass / 0 fail** |
| `npx vitest run --config vitest.drivers.config.ts` | **33 files / 241 pass / 0 fail** |
| `npx vitest run src packages/driver-sdk packages/ui` | **412 files / 4243 pass / 0 fail** |
| `npx vitest run scripts` | **23 files / 244 pass / 0 fail**（Wave 4-A 已并入：护栏 36 例，其中 BUG-008 跟踪域分类 5 例；原基线 22/208 已过时） |
| `node scripts/check-driver-import-boundaries.mjs` | exit 0（**必须在主检出跑**：worktree 缺外部树会假绿）；2 条夹具豁免命中 + 12 条 advisory（R1×2 superset / R2×6 editor-pro 单测 / R3×4 宿主引驱动内部） |
| `node scripts/check-id-terminology.mjs` / `check-module-layers.mjs` / `check-ci-docs-consistency.mjs` | 全绿 |
| `node scripts/i18n-sync-check.mjs` | 结构与 Wave 3 实测一致（**既有翻译债，非本专项回归**：宿主缺 1216 / 冗余 1650；redis 9 语言各缺 139、1 语言缺 72；CI 该步 `continue-on-error: true`） |
| `cargo test -p datazen --lib`（独立 `CARGO_TARGET_DIR`） | **1453 pass / 0 fail** |
| `npx vite build`（禁止裸 `pnpm build`，会自触发 install） | exit 0；main chunk 参考值 **1,605.12 kB / gzip 467.15 kB**（O-1 裁定「全 10 语言注册」后量级，勿因体积回退） |

任何数字与基线不符都必须在报告里点名解释（新增用例数？回归？），不得默写。

**硬性口径（Wave 4-A 合流门禁刚踩过的坑）**：worktree 里**不存在** gitignored 的外部树
（`packages/drivers/{kiwi,olap,superset}` 这类 `source: git` 驱动、`packages/pro-extensions/*` 独立 git 仓），
CI 又用 `--drivers=basic` 且 Guard 先于 codegen，所以**任何全仓静态检查必须在主检出复跑一次**才算数，
worktree 绿不构成证据。只读脚本用 `--root=` 指主检出即可（`check-driver-import-boundaries.mjs` 已支持）。

## B. 各轨留待 R 回归项（逐项闭环，标注 PASSED / BLOCKED-需人工 / N/A）

| 来源轨 | 项 | 落点与判定 |
| --- | --- | --- |
| cn-to-ui | redis 工作区视觉回归 | `SearchModeTabs` 激活/非激活高亮、`KeyTreeList` 行 hover、`RedisConsole` `CompletionPopup` 弹层样式（`cn()` 换源后类名合并等价）→ GUI 项，见「C. E2E/GUI 处置」 |
| cap-bridge | redis 键树右键真实弹出 | 连接真实 Redis → key-browser 右键 → Web context menu 在光标处弹出（`showNativeContextMenu` → `bindContextMenuBridge` 新链路）；弹出后移动/按下指针可取消（懒挂载语义）；Esc / 点击外部关闭 → GUI 项 |
| cap-bridge | 危险操作确认对话框真实渲染 | Safe Mode 开 → 写命令触发 `useBoundConfirmDialog` → 宿主 `useConfirmDialog` 弹窗可见、确认/取消行为正确、gate 拦截与放行结果正确 → GUI 项 |
| i18n-core | 设置页切语言 → redis 驱动 UI 实时刷新 | 无需重启窗口，Keys 面板 / ConnectionWizard 文案随 zh-CN/en 切换 → GUI 项 |
| i18n-core | 设置页切语言 → SQL Editor Pro 文案实时刷新 | editor-pro 直连 `@datazen/ui` 单例；需 Pro webdriver 构建（stage pro EP）→ GUI 项 + 依赖 editor-pro 子仓（`c60f7fc` 尚未 push，见开放项） |
| i18n-drivers R-1 | zh-CN 下 redis 工作台/键浏览器/控制台为中文，切回 en 立即生效；mongodb 文档视图 `mongo.*` | GUI 项 |
| i18n-drivers R-2 | `--drivers=basic` 与 `--drivers=all` 两档构建无 raw-key 泄漏 | 可脚本化：两档分别 `npx tsc` + `npx vite build` + codegen 产物核对；raw-key 泄漏判定属 GUI |
| i18n-drivers R-3 | 三套 vitest 复跑一致 | 即本表 A 门禁，直接引用 |
| i18n-drivers R-4 | `i18n-sync-check` 输出与 Wave 3 实测逐项一致 | A 门禁覆盖；翻译回合后转 exit 0 属**另立回合**，不在本轨 |
| i18n-drivers R-5 | Pro/EP 与 wapp 自带词条时 `registerTranslations` 无前缀冲突 | 现状无自带词条 → 记 N/A，并在契约文档留观察项 |
| i18n-drivers R-6（O-2） | `basic` 档下 `DocumentConnectionView` 20 处 `t('mongo.*')` 是否 raw key | 既有耦合（非本专项引入），GUI 取证一次即可 |
| i18n-drivers R-7（O-3） | 驱动 UI 依赖宿主 key（`common.*` 32 / `newConn.*` 22 / `sqlserver.*` 4）在换源后仍命中；宿主字典在渲染前注册完毕 | 单测层已证明；GUI 走查新建连接表单（redis / sqlserver）与工作台菜单文案 |
| i18n-drivers R-8（O-1） | 保留 10 语言档：补「扩展经 `registerLocale()` 引入第 3 语言时驱动词条命中」的手工验证 | 可用 `packages/ui` 单测证明（`registerTranslations` + 第三 locale 快照），无需 GUI |
| decouple-docs | ②⑤⑥ 文档回扫：2.6 落实名、2.1.2/2.7 基线数字改「生产码 0 / 夹具 2」、3 条 Nit | **Wave 4-A 已并入其范围第 5 条**；本轨只做抽验（文档内路径/符号逐条 Read 核实，零失配） |
| import-guard | 新护栏纳入全量回归清单 + 注入/还原自证复跑 | A 门禁 + 本轨独立复做一次「注入红 → 还原绿」。〔Wave 4-A 关账时 `scripts/run-regression.sh` 已把护栏加为**步骤 1/7**（秒级失败即停，放在 10 分钟级 cargo 之前），契约 2.6「本地等价」行同步登记第 5 个接入点；R 阶段只需复跑该脚本确认编号与耗时口径，不必再接线〕 |
| fix-redis-tests / types-to-sdk | 无 E2E | N/A（仅门禁数字对齐） |

## C. E2E / GUI 项处置（协调者裁定）

专项期间**不在子代理内跑真实 `pnpm e2e` / `pnpm tauri:build:webdriver`**（构建代价大、且需真实 Redis
与 GUI 会话）。本轨职责：

1. 能单测化的留待项（R-8、护栏有效性、字典注册时序）全部单测化闭环。
2. 纯 GUI/手工项**逐条写成可照做的验收清单**（前置条件 + 点击路径 + 判定），汇总进本文件
   「人工验收清单」小节，交由用户在本地 GUI 逐项打勾。
3. 若用户明确要求补自动化，另立轨道（不在本专项关账范围）。

## 禁止事项

- 只测不修：缺陷一律登记 `tracks/r-phase/bugs.md`（状态 `待修复`），并停止关账等我裁定。
- 不触碰其它轨的 `progress.md` / `bugs.md`；不修改 `hub.md`（协调者专用）。
- 不提交 codegen 产物（`src/extensions/generated*.ts`、`src-tauri/src/driver_init.rs`、
  `src-tauri/capabilities/default.json`）、`Cargo.lock`；不动 `src-tauri/**`、`packages/pro-extensions/**`。
- 禁止 `pnpm install`、裸 `pnpm build`、真实 `pnpm e2e`；搜索用 Grep 工具；cargo 用独立 `CARGO_TARGET_DIR`。

## 验收标准

1. 【A 门禁】全部复跑，逐项给出真实命令与真实输出，与基线差异全部解释清楚。
2. 【B 表】每一行有明确终态（PASSED / N/A + 理由 / 转人工清单）。
3. 独立复做 Wave 4-A 的「注入 R1（`from` + `vi.mock`）与 R2 → 红并点名文件:行 → 还原 → 绿」，
   结束时 `git status` 干净。
4. 抽查解耦契约达成（BUG-006 校正，与契约 §2.4.2 逐字一致）：
   ① `packages/drivers/*/ui/**` 内说明符字面量指向宿主 `src/` 的命中数 = 豁免 **2 条**
      （`packages/drivers/redis/ui/__tests__/redisKeyWebContextMenu.test.tsx:5,9`，且仅这 2 条）；
   ② `packages/**` 内 `setLocale(` 仅命中 `R2_FILE_CARVEOUTS` 的**两个文件**（`packages/ui/src/i18n.ts:34`
      定义处 + `packages/ui/src/__tests__/i18n.test.tsx` 调用处），其余为 0（旧稿「除 `i18n.ts` 外 = 0」漏了第 2 个豁免文件）；
   ③ 宿主生产码调用点 = `src/lib/localeSync.ts`（唯一接线：`:20` 播种 / `:24` 切换）+ `src/locales/index.ts`
      的**文档化临时适配器**（`getTranslation` 在同一次同步调用内 `:78` 换 locale、`:82` 复位，非渲染路径，契约 §2.4.2 明文允许）。
5. 文档抽验零失配（路径/符号/行号/数字，含 Wave 4-A 回扫后的 2.1.2 / 2.6 / 2.7）。
6. 输出「人工验收清单」小节（≥ 上表全部 GUI 项，含前置条件与判定），供用户本地打勾。
7. 全轨 Bug 闭环或明确移交；返回 `TEST_DONE(PASSED)` 或 `ESCALATED` + 未闭环清单。

## 状态

- [x] 派发 Tester → 回归执行（协调者已按任务书派出独立 Tester 实例，worktree `.worktrees/datazen-r-phase` @ `feature/r-phase`）
- [x] TEST_DONE / 关账汇报 → **TEST_DONE(PASSED)**（Wave 4-B 收尾实例：A-1~A-15 逐项复核——
  本实例复跑与引用前任日志严格分列见 §1；B 表 **16 行**（末行合并 fix-redis-tests / types-to-sdk
  两轨，共 17 个轨道项）终态见 §2；GUI-1~GUI-9 人工验收清单见 §3；缺陷全部明确移交——
  BUG-001/002/006 属文档/口径、BUG-003/005 需 Coder、BUG-004 属外部仓，见 §4；待裁定 5 条见 §5
  ——注：置位 commit 中此处曾写「A-1~A-15 独立复跑」「B 表 17 行」，与 §1/§2 实际口径不符，
  已由紧随的修正 commit 据实更正）
- [ ] 第 2 轮修复回合（Coder，2026-09-21）：BUG-005（测试落点重定向）→ BUG-003（构建前置无条件补齐）+
  BUG-001/002/006 文档与口径 + BUG-004 契约侧观察项 → **提交后待复测**（修复 commit `29a0698d1`；
  复测须由独立 Tester 实例执行，记录见「第 2 轮修复回合」节）
- [x] 第 2 轮复测（独立 Tester 实例，2026-09-21）：BUG-001/002/003/005/006 复测**通过**（状态 → `已修复`，各条证据见
  `bugs.md`）；`bash scripts/run-regression.sh` **7/7 全绿**（实跑，耗时见「第 2 轮复测记录」）；BUG-004 维持
  契约侧观察项完成 + 外部仓移交（26 key 命名空间经主检出只读清点与契约现文一致）；**新登记 BUG-007**（契约 §2.4.2
  `:283` 行内数字漂移，低危非阻断）→ 待协调者裁定派修；复测判定 = `TEST_DONE(PASSED)` + 未闭环清单
- [ ] 第 3 轮修复回合（Coder，2026-09-21；协调者裁定只修 BUG-007）：契约 §2.4.2 末句「单测调用 7 行」→ **15 行**
  （独立复测同口径实测值）并就地补写防腐坏口径（行数权威落点 = `packages/ui/src/__tests__/i18n.test.tsx` 本身 +
  复核命令）；同源数字全仓排查无其它现役残留 → **已提交、待复测**（修复 commit `a5eed7b57`；
  复测须由独立 Tester 实例执行，记录见 `bugs.md` BUG-007「修复回合」小节）
- [x] 第 3 轮复测（**BUG-007 收尾复测**，独立 Tester 实例，2026-09-21）：契约 `:283`「单测调用 **15** 行」经
  独立逐行清点一致（Grep 命中 16 行 − `:100` JSDoc 注释 1 = 15 调用行；`packages/**` 内 `setLocale(` 仅命中
  豁免两文件 ⇒「其余为 0」成立；定义处 `i18n.ts:34` 恰 1 行）；防腐坏口径（权威落点 = 单测文件自身 + 复核
  命令）准确、未新增 EP/wapp 强制条款、契约无顺带改动（diff = 单行替换）；`node scripts/check-ci-docs-consistency.mjs`
  exit 0、`npx vitest run scripts` = **23 files / 246 pass**（exit 0，日志 `/tmp/rphase3-vitest-scripts.log`）、
  `git status` 干净 ⇒ BUG-007 → `已修复`（BUG-001~003、005~007 全部闭环；BUG-004 维持契约侧观察项 +
  外部仓移交）。本轮不重跑全量回归：上游实例 @ `283d2ad05` 已 7/7 全绿，本回合仅文档 diff，除秒级脚本外
  无回归面。复测记录详见 `bugs.md` BUG-007「第 3 轮复测」小节

## 执行记录

### 0. 实例交接与证据口径（Wave 4-B 由两个 Tester 实例接力完成）

- **前任实例**（2026-09-21 10:22~10:55）：跑完 A 门禁主体，产物未提交；在收尾阶段网络中断静默死亡。
  遗留两份未提交成果——`packages/ui/src/__tests__/i18n.test.tsx`（R-8 单测，+128 行）与
  `tracks/r-phase/bugs.md`（BUG-001~004）。其真实运行日志保留在 `/tmp/rphase-logs/`。
- **收尾实例（本实例）**（2026-09-21 11:11 接手）：先 `pwd` 自检（worktree `.worktrees/datazen-r-phase`
  @ `feature/r-phase` HEAD `20393084e`）；独立复跑全部秒级/分钟级门禁项并逐条留原文；前任的 R-8 单测
  逐行复核后保留；长跑项（`run-regression.sh` 全量）引用前任日志，其余（A3/A4/A5/cargo/vite 两档）
  **本实例全量重跑**，不转抄。本实例日志：`/tmp/rphase-logs-mine/`。
- 本轨全程**只测不修**：未改任何业务代码与契约文档；仅新增（接续前任）R-8 单测、`bugs.md` 与本记录。
- 复跑结束时 `git status` 只剩两份待提交产物，按纪律分三步提交（`test/` 单测 + `bugs.md` → 本「执行记录」
  → 「状态」置位，见 `git log feature/r-phase` 顶部三条）；`Cargo.lock` 的注入残留已还原
  （`git diff --quiet Cargo.lock` 通过）；无探针残留（注入/还原自证见 A-14）。

### 1. A 门禁逐项实测（「自己跑」与「引用前任日志」严格分列）

| # | 门禁项 | 本实例独立复跑（真实输出） | 前任日志（引用出处） | 判定 |
| --- | --- | --- | --- | --- |
| A-1 | `node scripts/resolve-drivers.mjs --codegen-only --drivers=all` | 收尾态 exit 0：`.driver-features.json` = **15 驱动**，产出 `generated.ts` / `driver_init.rs` | `R2-codegen-basic.log` / `R2-codegen-all.log` 均 exit 0 | PASSED |
| A-2 | `npx tsc --noEmit -p tsconfig.json` | **all 档 exit 0**（11:15:27，无输出）；basic 档见前任（本实例未重跑 basic tsc） | `R2a-tsc-basic.log` / `R2b-tsc-all.log` 均 0 字节 = 0 error | PASSED |
| A-3 | `npx vitest run --config vitest.drivers.config.ts packages/drivers/redis/ui` | **27 files / 222 pass / 0 fail**（11:17:01，exit 0） | `A3-vitest-redis-ui.log` 27/222 | PASSED |
| A-4 | `npx vitest run --config vitest.drivers.config.ts` | **33 files / 241 pass / 0 fail**（11:15:38，exit 0） | `A4-final.log` 33/241 | PASSED |
| A-5 | `npx vitest run src packages/driver-sdk packages/ui` | **412 files / 4247 pass / 0 fail**（11:13:10~11:14:2x，exit 0；全量重跑） | `A5-final.log` 412/4247 | PASSED |
| A-6 | `npx vitest run scripts` | **23 files / 244 pass / 0 fail**（11:15:34，exit 0） | `A6-vitest-scripts.log` 23/244 | PASSED |
| A-7 | `node scripts/check-driver-import-boundaries.mjs`（worktree） | exit 0 · **1403 files / 0 blocking / 4 advisory**（本轮共跑 5 次：3 次常态绿逐次一致 + 2 次注入红，见 A-14） | `A7-guard-worktree.log` 同 | PASSED |
| A-8 | `node scripts/check-driver-import-boundaries.mjs --root=<主检出>` | exit 0 · **1489 files / 0 blocking / 12 advisory**（12 条逐条点名见 §1.2） | `A7b-guard-maincheckout.log` 同 | PASSED |
| A-9 | `check-id-terminology` / `check-module-layers` / `check-ci-docs-consistency` | exit 0 / exit 0 / exit 0（11:14:11，1719 files / 3 rules / 11 ids） | `A8*.log` 同 | PASSED |
| A-10 | `cargo test -p datazen --lib`（`with-driver-inject --drivers=basic` + HOME 沙箱 + `CARGO_TARGET_DIR=/tmp/datazen-rphase-target`） | **1453 passed / 0 failed / 3 ignored**，exit 0，dur=**15s**（target 预热；无复跑触发） | `cargo-lib.log` / `cargo-runner.out`：exit 0 / 2m19s / 1453/0/3（前任首次冷编译） | PASSED |
| A-11 | `node scripts/i18n-sync-check.mjs` | **exit 1**；输出与前任日志**逐字节一致**（`diff` 从第 2 行起 IDENTICAL）：`Summary: 2400 missing key(s), 1650 stale translation(s) across 8 host locales; 0 driver pack issue(s) across 2 driver locale pack(s).` | `A9-i18n-sync-check.log` 同 | PASSED（既有翻译债，非本专项回归；CI 该步 `continue-on-error: true`） |
| A-12 | `npx vite build`（两档） | **basic：1,574.66 kB / gzip 460.61 kB**（`main-iWjiAdIb.js`）；**all：1,605.12 kB / gzip 467.15 kB**（`main-rSNK-HW9.js`）；均 exit 0，`built in 4.8s / 4.6s` | `R2a-vite-basic.log` / `R2b-vite-all.log`：同文件名 hash、同数字 | PASSED |
| A-13 | `bash scripts/run-regression.sh`（合并前全量，10 分钟级） | 本实例未重跑（口径：步骤 1/7 护栏已在最前，其余步骤与 A-2~A-12 重叠） | `regression2.log`：**7/7 全绿**（1 护栏 0m01s / 2 cargo 0m17s / 3 vitest 1m12s / 4 驱动 vitest 0m06s / 5 ID 0m01s / 6 tsc 0m07s / 7 vite 0m05s，`全量回归门禁通过 ✔`）；`regression.log` 首轮 exit=101 = BUG-003 | PASSED（编号与耗时口径按前任日志确认） |
| A-14 | 护栏有效性自证（注入红 → 还原绿） | **本实例独立复做**（`/tmp/rphase-logs-mine/R5-injection-redgreen.log`）：注入 R1 `from`（`settingsHelpers.ts:19`）+ R1 `vi.mock`（`settings.test.ts:20`）+ R2 `setLocale(`（`connectionWizardValidate.ts:78`）→ **exit 1，3 条违规逐条点名文件:行**；`cp` 还原后 `git diff --stat` 对 3 文件为空 → **exit 0 / 1403 files / 0 blocking / 4 advisory** | 前任 A7 系列只含常态绿测，无注入自证日志 | PASSED |
| A-15 | 解耦契约抽查（验收标准 4） | `packages/drivers/*/ui/**` 指向宿主 `src/` 的说明符 = **恰好 2 条**（`redisKeyWebContextMenu.test.tsx:5,9`，与 `ALLOWLIST` 两条三元组逐字一致）；`packages/**` 内 `setLocale(` 仅命中 `R2_FILE_CARVEOUTS` 两文件；宿主生产码调用点 = `src/lib/localeSync.ts:20,24` + `src/locales/index.ts:78,82`（§2.4.2 明文适配器）→ 口径差异已登记 BUG-006 | —（本实例首次执行） | PASSED（口径校正见 BUG-006） |

#### 1.1 与基线数字的差异解释（逐条）

1. **A-5：4243 → 4247（+4）** —— 前任新增的 R-8 单测块恰好 4 个 `it`（本实例跑该文件 = 10 tests，其中既有 6 + 新增 4），
   412 文件数不变。**非回归**。
2. **A-12：1,605.12 vs 1,574.66** —— 两者是**同一 worktree 的两个 codegen 档位**，与外部树无关：
   `--drivers=basic`（postgres/mysql/sqlite/redis 4 驱动）= 1,574.66/460.61（`main-iWjiAdIb.js`）；
   `--drivers=all`（15 驱动）= 1,605.12/467.15（`main-rSNK-HW9.js`）。本实例两档均重跑，产物**文件名 hash 与前任日志逐字相同**
   ⇒ 结果确定可复现；任务书基线 1,605.12 是 all 档（O-1 十语言装配的对照点），
   前任 `regression2.log` 步骤 7 的 1,574.66 是承接 cargo 注入后的 basic 档。**非回归**。
3. **A-6：脚本套件 23 files / 244**（任务书旧基线 22/208 已由 Wave 4-A 更新）—— 无差异。
4. **A-10：15s vs 2m19s** —— 本实例复用前任预热好的 `/tmp/datazen-rphase-target`（4.1G），非冷编译；用例数完全一致。
5. **A-11：exit 1** —— 既有翻译债（宿主 8 语言缺 2400 / 冗余 1650；redis 缺 139×9 语言 + 72×1），
   与 Wave 3 实测逐项一致（本实例 `diff` 逐字节比对通过）。**非回归**。

#### 1.2 主检出 12 条 advisory 逐条点名（本实例实测，exit 0）

```
R1  packages/drivers/superset/ui/SupersetConnectionFields.tsx:3   → host src/hooks/useI18n   （外部树漂移）
R1  packages/drivers/superset/ui/SupersetSchemaTree.tsx:19        → host src/hooks/useI18n   （外部树漂移）
R2  packages/pro-extensions/sql-editor-pro/src/intentions/__tests__/intentionCodeActions.test.ts:119
R2  packages/pro-extensions/sql-editor-pro/src/intentions/__tests__/intentionCodeActions.test.ts:141
R2  packages/pro-extensions/sql-editor-pro/src/locales/__tests__/locales.test.ts:28
R2  packages/pro-extensions/sql-editor-pro/src/locales/__tests__/locales.test.ts:32
R2  packages/pro-extensions/sql-editor-pro/src/locales/__tests__/locales.test.ts:36
R2  packages/pro-extensions/sql-editor-pro/src/locales/__tests__/locales.test.ts:39
R3  src/locales/locales.test.ts:107
R3  src/test/driverUiSetup.ts:25
R3  src/test/driverUiSetup.ts:26
R3  src/windows/connection/DocumentConnectionView.tsx:25
```

- R1×2 + R2×6 全部落在 gitignored 外部树（superset git 驱动、editor-pro 独立仓）⇒ 降级 advisory，由各自仓库整改；
- R3×4 为本仓跟踪代码，规则 `blocking: false`（协调者裁定前只报告）；与契约 §2.6 / §2.7 登记逐条一致。
- worktree 只有 4 条（R3×4），因为外部树不在此检出 —— 与「必须回主检出复跑才算数」的口径一致。

### 2. B 表逐行终态

| 来源轨 | 项 | 终态 |
| --- | --- | --- |
| cn-to-ui | redis 工作区视觉回归 | **转人工（GUI-1）**。等价性已由「单一实现」背书：`src/lib/cn.ts` 全文 = `export { cn } from '@datazen/ui'`，`packages/ui/src/cn.ts` = `twMerge(clsx(inputs))` ⇒ 宿主与驱动调的是同一函数，不需要新单测；剩余为纯视觉确认。 |
| cap-bridge | redis 键树右键真实弹出 | **转人工（GUI-2）** |
| cap-bridge | 危险操作确认对话框真实渲染 | **转人工（GUI-3）** |
| i18n-core | 设置页切语言 → redis 驱动 UI 实时刷新 | **转人工（GUI-4）** |
| i18n-core | 设置页切语言 → SQL Editor Pro 文案实时刷新 | **转人工（GUI-5）**（需 Pro 构建 + editor-pro 子仓 `c60f7fc` 本地存在，尚未 push） |
| i18n-drivers R-1 | zh-CN redis 中文 / mongodb `mongo.*` | **转人工（GUI-6）** |
| i18n-drivers R-2 | 两档构建无 raw-key 泄漏 | **脚本部分 PASSED**（tsc 0 error ×2 档 / vite exit 0 ×2 档 / codegen 15 与 4 驱动产物核对），**运行时 raw-key 判定转人工（GUI-7）** |
| i18n-drivers R-3 | 三套 vitest 复跑一致 | **PASSED**（27/222、33/241、412/4247，本实例独立复跑逐项一致） |
| i18n-drivers R-4 | `i18n-sync-check` 输出与 Wave 3 一致 | **PASSED**（逐字节 `diff` 一致；exit 1 属既有翻译债，不属本轨） |
| i18n-drivers R-5 | Pro/EP 与 wapp 自带词条前缀冲突 | **改判**：wapp 侧 N/A（`registerTranslations` 全仓 0 命中）；**Pro EP 侧实测已自带 26 key 且 5 个 `query.*` 与宿主同名（3 异值 / 2 同值 en，zh-CN 5 全异值）→ 观察项 + 待裁定（BUG-004，移交 editor-pro 仓）** |
| i18n-drivers R-6（O-2） | `DocumentConnectionView` 的 `mongo.*` | **转人工（GUI-6）** + 静态口径已固化：`t('mongo.*')` = **21 次出现 / 15 个不同 key**；同文件另有 `common.*`×5（loading×3/save/delete）、`query.*`×3、`connWin.*`×1（本实例逐行复现协调者口径） |
| i18n-drivers R-7（O-3） | 驱动 UI 依赖宿主 key 换源后命中 | **PASSED（静态）** + **转人工（GUI-8）**。口径 = `packages/drivers/*/ui/**`（path 驱动）：`common.*` **32/7**、`newConn.*` **22/10**、`sqlserver.*` **4/4**；若口径放宽到含 `e2e/**`，redis e2e helper 再加 `common.*`×1、`newConn.*`×4；主检出多出的 `common.*`+2 / `newConn.*`+5 全部来自 superset git 驱动 clone（`SupersetSchemaTree.tsx:141`×2、`SupersetConnectionFields.tsx:13,17,18,41,53`）⇒ **非回归** |
| i18n-drivers R-8（O-1） | 扩展引入第 3 语言时驱动词条命中 | **PASSED**：4 例新单测（独立复跑 `packages/ui/src/__tests__/i18n.test.tsx` = **10/10**，全量口径 A-5 随之 4247）。落点说明：必须并入 R2 豁免文件 `i18n.test.tsx`——护栏 R2 的豁免是**文件级精确清单**，新建 `packages/**` 测试文件调用 `setLocale()` 会直接红。 |
| decouple-docs | ②⑤⑥ 文档回扫抽验 | **PASSED（抽验执行）**：逐条复核 §2.1.1 / §2.1.2 / §2.4.2 / §2.4.3 / §2.4.4 / §2.6 / §2.7 的路径·符号·行号·数字（含 `:260`/`:141`/`:279`/`:295`、`index.ts:23-31`、`i18n.ts:69`、`localeSync.ts:20,24`、`locales/index.ts:78,82`、`builtinLocales.ts:9,26-29`、`i18n-sync-check.mjs:36`、`ALLOWLIST` 2 条三元组、`run-regression.sh` 步骤 1/7、`ci-local.sh:64` 3.3/11、`ci.yml:67`）——**发现并登记 2 处失配（BUG-001/002）**，故验收标准 5 的「零失配」字面**未达成**，需协调者派修文档（见 §4）。 |
| import-guard | 新护栏纳入全量回归 + 注入/还原自证 | **PASSED**：① 编号/耗时口径 = `regression2.log` 7/7（护栏为步骤 **1/7**，秒级失败即停，实测 0m01s）；② 本实例独立复做 3 探针红→还原绿（A-14）；③ 主检出 12 条 advisory 逐条点名（§1.2）。 |
| fix-redis-tests / types-to-sdk | 无 E2E | **N/A**（仅门禁数字对齐：A-3 27/222 与 A-4 33/241 独立复跑一致） |

### 3. 人工验收清单（GUI，交用户在本地逐项打勾）

> 通用前置：源码构建一律走 `pnpm tauri:dev`（默认 basic = postgres/mysql/sqlite/redis）。
> 需要 mongodb / sqlserver / superset 时用 `pnpm tauri:dev --drivers=all`（仍为 Community 版）。
> 语言下拉只含宿主已接线的 `en` / `zh-CN`（`BUILTIN_LOCALES`）；不重启窗口、不手动刷新。
> 每项判定中若出现 `xxx.yyy` 形态的 raw key、或切语言后文案不跟随，即为**不通过**。

| ID | 项（来源轨） | 前置条件 | 点击路径 | 判定标准 |
| --- | --- | --- | --- | --- |
| GUI-1 | redis 工作区视觉回归（cn-to-ui） | `pnpm tauri:dev` + 真实 Redis 连接（看 hover/激活态需有键） | 连接树 → redis 连接 → 工作台 → **Items** 标签（`data-testid="redis-tab-items"`）→ ① 顶部搜索模式 tab（`redis-search-mode-tabs` 的 key/value/all）切换；② 键树列表行 hover；③ 切 **Console** 标签（`redis-tab-console`）→ 输入 `GE` 触发补全弹层 | ① 激活 tab 高亮/字重与非激活可辨、非激活 hover 变亮；② 键树行 hover 背景正常（无塌陷/无重复类异常）；③ 补全弹层边框/阴影/选中项高亮正常；④ DevTools 无 React className 相关告警。等价性已由单一实现背书，本项只看视觉。 |
| GUI-2 | redis 键树右键真实弹出（cap-bridge） | `pnpm tauri:dev` + **真实 Redis**（键树需有键） | 工作台 → 键树 → 在某个键上**点右键** | ① Web 右键菜单在**光标处**弹出且不溢出窗口（`showNativeContextMenu` → `bindContextMenuBridge` 新链路）；② 弹出后**移动鼠标或按下指针**→ 菜单取消（懒挂载语义）；③ `Esc` 或点击菜单外部 → 关闭；④ 全程无 Tauri 原生系统菜单样式出现。 |
| GUI-3 | 危险操作确认对话框（cap-bridge） | `pnpm tauri:dev` + **真实 Redis** | ① Settings → **Behavior** → 打开 **Safe Mode**（`SettingsContent.tsx:621`）→ 回工作台 → 删除一个键；② 关闭 Safe Mode → 再删除一个键；③ 切 Console 执行 `SET k v` | ① Safe Mode 打开时写操作先被 gate 拦截，弹提示（`redis.safeMode.blocked`）；② 关闭后删除弹**宿主 ConfirmDialog**（`redis.danger.confirmTitle`/`…Message`）：**取消** → 不执行、键仍在；**确认** → 键被删除；③ Console 写命令同样弹出危险确认（Console-only 语义，见 `useRedisGate.ts`）。 |
| GUI-4 | 切语言 → redis 驱动 UI 实时刷新（i18n-core） | `pnpm tauri:dev` + 一个 redis 连接（文案可见即可） | Settings → **General** → **Language** 改 `简体中文` → 回 redis 工作台 / 连接向导 / 键浏览器；再切回 `English` | 不重启、不刷新即整体变中文/英文；四个 tab 标签、键浏览器、批量删除按钮均为译文而非 raw key；切回 en 立即复原。 |
| GUI-5 | 切语言 → SQL Editor Pro 文案实时刷新（i18n-core） | **Pro 构建**：`pnpm tauri:dev:pro` 或 `pnpm tauri:build:pro`；**且**本地存在 `packages/pro-extensions/sql-editor-pro` 子仓（commit `c60f7fc`，尚未 push，`--codegen-only` 时不会自动克隆） | SQL 编辑器 → Settings → General → Language 切 `中文`/`English` → 看 EP 文案：绑定参数面板（`query.params`）、参数历史项（`query.editor.param.*`）、补全/意图「快速操作」（`query.intention.*`） | EP 文案随语言即时刷新。**顺带观察 BUG-004**：`query.params` 等 5 个 key 宿主/EP 同名，Pro 版取 EP 文案（如 `参数`）而 Community 版取宿主文案（`绑定参数`）⇒ 两版本不一致即为覆盖生效证据（当前宿主 0 消费方，无用户可见后果）。 |
| GUI-6 | zh-CN 下 redis / mongodb 驱动 UI 中文（i18n-drivers R-1 / R-6） | `pnpm tauri:dev --drivers=all` + **真实 Redis + 真实 MongoDB** | Settings → Language = `简体中文` → ① redis 工作台四 tab（Items/Console/Monitor/Pub-Sub）+ 键浏览器 + 控制台；② mongodb 连接 → 库/集合树 → **Documents** 文档视图（`DocumentConnectionView.tsx`，其 `t('mongo.*')` 共 21 处/15 key） | ① redis 侧全中文无 raw key；② mongodb 文档视图 21 处 mongo.* 全为中文（`mongo.noIdHint`/`documents`/`collections`/`insert`/`queryHint` 等）；③ 切回 `English` 立即生效。 |
| GUI-7 | 两档构建 raw-key 泄漏走查（i18n-drivers R-2 运行时侧） | 分别以 `pnpm tauri:dev`（basic）与 `pnpm tauri:dev --drivers=all` 起 | 每档随机走查 ≥5 个界面：连接页 / 新建连接向导 / 工作台 / 控制台 / 设置，并在 zh-CN 与 en 间切换 | 界面不出现 `xxx.yyy` 形态 raw key；切语言后所有已渲染文案跟随。（脚本侧已验：tsc 0 error ×2 档、vite exit 0 ×2 档、codegen 产物 15/4 驱动一致。） |
| GUI-8 | 驱动 UI 复用宿主 key 的显示正确性（i18n-drivers R-7） | `pnpm tauri:dev --drivers=all` | ① 新建连接 → 选 **redis**：Host / Port / Username / Password / 数据库索引 标签（`newConn.*`）；② 新建连接 → 选 **SQL Server**：Host / Port / Database / 用户名 / 密码 / 加密下拉（`sqlserver.encryption`、`sslNone/sslPrefer/sslRequire`）；③ redis 键树右键菜单与批量删除确认里的 Cancel/Delete/Confirm（`common.*`） | 以上全部显示正常英/中文（不是 raw key）⇒ 宿主字典在驱动 UI 渲染前已完成注册；切 zh-CN 后同样为正常译文。 |
| GUI-9 | 新增连接表单整体语言一致性（GUI-8 的扩展观察） | 同 GUI-8 | 在 zh-CN 下依次切换数据库类型（redis → sqlserver → mongodb）看表单标签 | 切换类型后标签无残留英文/无 raw key（验证驱动词条按装载注册、宿主词条已接线）。 |

### 4. 缺陷处置（本轨只登记，不修）

| Bug | 严重度 | 是否需 Coder | 处置建议 |
| --- | --- | --- | --- |
| BUG-001 契约 §2.4.1/§2.1.1 少登记第 6 个 i18n API | 低 | 否（文档，1 处两句） | 派 Coder/docs 改契约「仅五个 → 仅六个」并补签名；否则评审按字面会把在用 API 判成越界 |
| BUG-002 契约 §2.4.3「0 命中」残留枚举不全 | 低 | 择一 | 措辞收敛为「生产码 0 命中」+ 登记护栏 `:94`；或删掉已不存在的 skip-list 条目（后者才是代码改动） |
| BUG-003 干净检出 cargo 编译失败（builtin-ep 资源目录缺失） | 中 | **是**（构建脚本/回归脚本） | 三选一（建议 1：`resolve-drivers` 无条件 `mkdir -p`）；注意 BUG-005 的单测顺带创建会掩蔽首跑失败 |
| BUG-004 Pro EP 自带词条与宿主共用 `query.*`（R-5 前提失实） | 低 | 否（观察项 + 外部仓） | 任务书 R-5 改判（本记录已改）；契约 §2.4.4 补 EP/wapp 前缀要求或明确豁免；移交 editor-pro 仓裁定前缀 |
| BUG-005 `resolve-pro.test.ts` 读写真实仓库路径（删 Pro staging、改写 `generated-pro.ts`） | 中 | **是**（测试隔离） | 把落点改到 `mkdtempSync`（`stageDir`/`proPath` 已具备覆写能力），否则本地 Pro staging 会被静默删除 |
| BUG-006 验收标准 4 静态口径与 §2.4.2 不一致 | 低 | 否（任务书口径） | 按本记录 §1 A-15 的精确口径改写（契约无错） |
| BUG-007 契约 §2.4.2 行内数字漂移（「单测调用 7 行」实测 15 行） | 低 | 否（文档，1 处数字） | 第 2 轮复测新登记；派 docs 改契约 `:283` 数字或改为不带行数的写法（避免 R-8 类单测扩写再次漂移） |

### 5. 待协调者裁定

1. **BUG-001/002/006 均为「文档/口径」类**：是否派一个 docs 小回合统一修正（含任务书 R-5 改判与验收标准 4 改写）？
   - **裁定（协调者，2026-09-21）**：派本修复回合统一修正（含 R-5 改判与验收标准 4 改写）。→ 已在第 2 轮修复回合执行。
2. **BUG-003 与 BUG-005 是同一片区域的两面**（`builtin-ep` 资源目录）：建议一并派 Coder，先做「测试重定向到 tmp」再做「构建前置无条件补齐」，避免前者继续掩蔽后者。
   - **裁定（协调者，2026-09-21）**：一并派 Coder，**顺序先 005 后 003**（本回合执行）。→ 已按序执行。
3. **R-5 / BUG-004**：Pro EP 的 5 个 `query.*` 同名 key 是否要求 editor-pro 仓改 `pro.*` 前缀（属外部仓，需其自身 commit）。
   - **裁定（协调者，2026-09-21）**：移交 editor-pro 仓；本仓只补观察项、**不强制**。→ 契约 §2.4.4 观察项已补（含 §2.4.3 交叉引用），命名空间裁定留在其自身仓库。
4. **GUI-5 可复现性**：editor-pro 子仓 `c60f7fc` 未 push，Pro 侧 GUI 项只能在有该子仓的机器上验收。
   - **裁定（协调者，2026-09-21）**：保持开放（等用户 push `c60f7fc`）。→ 本回合不动。
5. 既有翻译债（A-11）是否另立翻译回合（转 exit 0）—— 不在本轨范围。
   - **裁定（协调者，2026-09-21）**：另立翻译回合，**不在本轨**。→ 本回合不动。

## 第 2 轮修复回合（Coder，2026-09-21）

> 输入：本文件 §4 缺陷处置表 + §5 待裁定（协调者裁定 2026-09-21 逐条批注见 §5）。
> 范围：BUG-005 → BUG-003（按序，避免前者继续掩蔽后者）+ BUG-001/002/006 文档口径 + BUG-004 契约侧观察项；
> BUG-004 的命名空间裁定移交外部仓，本仓不改行为。纪律遵守：未动 `src-tauri/**` 被跟踪文件、未碰
> `packages/pro-extensions/**`、未改护栏脚本、未跑 `pnpm install` / 裸 `pnpm build` / 真实 e2e。

**BUG-005（测试隔离，代码）** —— 改 `scripts/resolve-pro.mjs`（只新增可选 path 覆写口，缺省值与改动前逐字一致）与
`scripts/__tests__/resolve-pro.test.ts`（全部写/删落点重定向到 `mkdtempSync` 沙箱）：

- 新增覆写口：`resolvePro({ codegenPath, stageDir, outDir })`、`clearBuiltinEpStaging(ext, { stageDir })`、
  `stageProExtension({ stageDir, outDir })`、`downloadPrebuiltEp({ stageDir })`、`ensureProCheckout({ proDest, tmpFallbackDir })`
  （后两者是同批审计发现的同类真实路径落点：`packages/pro-extensions/sql-editor-pro` 复制目标、
  `/tmp/datazen-extension-sql-editor-pro` 回落源）。无全局可变状态、未放宽任何生产校验。
- 修复前实测（红）：`npx vitest run scripts/__tests__/resolve-pro.test.ts` 的 stdout 打印
  `[resolve-pro] removed staged builtin-ep at .../src-tauri/resources/builtin-ep/sql-editor-pro`；真实
  `src/extensions/generated-pro.ts` mtime 11:16:53 → 11:27:30（被就地改写，因两态同为 pro 故 md5 未变）。
- 修复后实测（绿）：同套件跑完后对 `src/extensions/generated-pro.ts` 与 `src-tauri/resources/builtin-ep/`
  做 `stat`/`md5` 快照，before/after `diff` = **`IDENTICAL_ZERO_CHANGE`**。
- 套件口径：`npx vitest run scripts` = **23 files / 246 pass / 0 fail**（244 → 246：新增 2 例——1 例只断值地
  断言两个默认常量仍是仓库相对路径，1 例补 `ensureProCheckout` 的「处处无 checkout + codegen-only → null」分支；
  原环境相关的旧用例改为确定性夹具用例，故净增 2，非回归）。

**BUG-003（干净检出编译失败，代码）** —— 改 `scripts/resolve-drivers.mjs`：`main()` 首行**无条件、幂等**
`mkdirSync(resolve(ROOT,'src-tauri','resources','builtin-ep'), { recursive: true })`（3 行 WHY 注释：
`tauri.conf.json` 无条件声明该 bundle 资源、目录被 gitignore、tauri-build 在干净检出上直接失败；所有 cargo
前路径都先跑本脚本）。未改 `src-tauri/` 被跟踪文件，未改 `clearBuiltinEpStaging` 删除语义（仍只删子目录）。

- 先红：`rm -rf src-tauri/resources/builtin-ep` → `CARGO_TARGET_DIR=/tmp/datazen-rphase-target node
  scripts/with-driver-inject.mjs --drivers=basic -- env HOME="$PWD/.regression-home" CARGO_HOME="$HOME/.cargo"
  RUSTUP_HOME="$HOME/.rustup" cargo test -p datazen --lib` → **exit 101**，末尾
  `resource path \`resources/builtin-ep\` doesn't exist`（热 target 下仍复现：注入改写 Cargo.toml 特性触发
  build.rs 重跑；`HOME` 沙箱需显式带 `CARGO_HOME`/`RUSTUP_HOME`，与 `run-regression.sh` 同法）。
- 后绿：同一 `rm -rf` → `node scripts/resolve-drivers.mjs --codegen-only --drivers=all`（exit 0）→
  `test -d src-tauri/resources/builtin-ep` = **DIR_OK**；再跑同一 cargo 命令 → **exit 0 /
  `test result: ok. 1453 passed; 0 failed; 3 ignored`**。
- 追加自证：再次 `rm -rf` 后直接跑「with-driver-inject + cargo」（= `run-regression.sh` 步骤 2 的真实形态）
  → exit 0 / 1453 passed，且目录被自动补齐（`DIR_OK_AFTER_INJECT_PATH`）。
- 单测化说明：**未加新单测**——任何「断言该目录存在」的用例本身会在 `npx vitest run scripts` 中把该目录造出来，
  与 BUG-005 要求的「跑完套件该目录零变化」直接冲突；该缺陷由复测 Tester 以「删目录 → codegen → cargo」
  真实场景独立验收。
- 副作用还原：注入跑产生的 `Cargo.lock` 单行残留（`datazen-driver-redis`）已 `git restore`，`git status` 无残留。

**BUG-001（文档）** —— 契约 `:260`「仅五个 → 仅六个」+ 补第 6 行签名
`getRegisteredTranslations(locale: string): Record<string, string>`（说明：浅拷贝只读快照、未知 locale → `{}`、
不订阅变化、非渲染路径；定义 `packages/ui/src/i18n.ts:69`、导出 `packages/ui/src/index.ts:27`、消费方
`src/locales/index.ts:11,108` 与 `packages/drivers/{redis,mongodb}/ui/__tests__/localePackRegistration.test.ts`）；
§2.1.1 的 `@datazen/ui` 行 i18n 列举补该 API（标注「只读快照，供工具/测试用」）。

**BUG-002（文档）** —— 契约 `:295` 措辞收敛为「**生产码** 0 命中」（对齐
`docs/development/independent-driver-development.zh-CN.md:214`），并把
`scripts/check-driver-import-boundaries.mjs:94`（`SKIPPED_CODEGEN_FILES` 第 2 条）登记为第 4 处已知残留
（3 处散文 + 1 处代码内常量）；**未改护栏脚本**（避免牵动其 36 例夹具复验）。

**BUG-004（契约侧观察项）** —— §2.4.4 新增观察项（**不构成**对 EP/wapp 的强制条款）：EP 自带 26 key
（24 个 `query.*` + 2 个 `settings.editor.intention*`），其中 5 个与宿主同名且同为 `query.*`（en 3 异值/2 同值、
zh-CN 5 全异值；宿主当前 **0 消费方**），按 `registerTranslations` 后写覆盖语义会出现 Pro/Community 文案分歧；
命名空间归属移交 editor-pro 自身仓库裁定（本仓不代其豁免、不即刻要求整改）。§2.4.3 表格「Pro 扩展词条」行加交叉引用。
（本回合独立复核：26 / 5 / 322 / 0 消费方与 BUG-004 登记一致；另实测 `packages/wapps/**` 内
`registerTranslations`、`@datazen/ui` 均 0 命中，R-5 后半句对 wapp 成立。）

**BUG-006（口径）** —— 本文件「验收标准 4」已按契约 §2.4.2 精确口径改写为三条：① 豁免 2 条
（`redisKeyWebContextMenu.test.tsx:5,9`）；② `packages/**` 内 `setLocale(` 仅命中 `R2_FILE_CARVEOUTS` 两个文件；
③ 宿主生产码调用点 = `src/lib/localeSync.ts`（唯一接线）+ `src/locales/index.ts` 的文档化临时适配器。

## 第 2 轮复测记录（独立 Tester 实例，2026-09-21 11:36~11:43）

> 输入：修复回合 commit `29a0698d1` + hash 回填 `1b22d428a`（复测起点 HEAD `1b22d428a`，diff 基准 `da5ffc2aa`）；
> 复测实例 = **全新 Tester**（未复用编码代理），`pwd` 自检 worktree 后执行。只测不修；唯一例外 = checklist
> 第 3 步的可选 mutation test（`cp` 备份 → 摘除修复行 → 红 → `cp` 还原 → `git diff` 空证，见 §3）。
> 本实例日志：`/tmp/rphase2-*.log`（`vitest-scripts` / `cargo-bug003` / `mutation-cargo` / `mutation-codegen` /
> `regression` / `guard` / `codegen-a|b|c` / `a3` / `a5` / `tsc-all` / `vite-all` / `cidocs` / `sdk-suites`）。

### 1. 变更面与边界审查（checklist 1）

- 实测 `git diff --numstat da5ffc2aa..HEAD`：`scripts/resolve-pro.mjs` +68/-27、`scripts/resolve-drivers.mjs` +5/-0、
  `scripts/__tests__/resolve-pro.test.ts` +189/-69、契约 `driver-api-dependency-boundary.md` +7/-4、
  `bugs.md` +139/-7、`progress.md` +80/-3（后两者 = 修复回合记录，非代码）。复测起点 `git status --porcelain` = 空。
- **`src-tauri/**` 被跟踪文件零改动**：`git diff --name-only da5ffc2aa..HEAD -- src-tauri` = 空；
  `clearBuiltinEpStaging` 删除语义未变（仍只删 `<root>/<extension>` 子目录：`:273` / `:289` 的 `target` 计算）。
- **resolve-pro.mjs 五个覆写口逐口对表（缺省行为与修复前逐字一致）**：

| 覆写口 | 缺省回退（实测行） | 等价性判定 |
| --- | --- | --- |
| `clearBuiltinEpStaging(ext,{stageDir})` | `stageDir ? resolve(stageDir) : resolve(DEFAULT_BUILTIN_EP_ROOT, extension)`（`:273`） | 不传 ⇒ 原表达式逐字 |
| `downloadPrebuiltEp({…,stageDir})` | 同上（`:289`） | 不传 ⇒ 原表达式逐字 |
| `stageProExtension({…,stageDir,outDir})` | 透传 `packEp`；`pack-ep.mjs:445` `resolve(opts.stageDir ?? join(DEFAULT_BUILTIN_EP_ROOT, extension))` | `null ?? default` ⇒ `stageDir:null` 与不传等价 |
| `ensureProCheckout({proDest,tmpFallbackDir})` | `proDest = DEFAULT_PRO_DEST`（`:391`）、`tmpFallbackDir = '/tmp/datazen-extension-sql-editor-pro'`（`:392`） | 原常量逐字迁为默认参数 |
| `resolvePro({codegenPath,stageDir,outDir})` | `:473-475` `opts.codegenPath ? resolve(…) : GENERATED_PRO_TS` / `… : null` / `… : undefined` | 不传 ⇒ 原值；`writeCommunity/ProCodegen(dest=GENERATED_PRO_TS)` 本就参数化 |

- 模块级 `let/var` = **0**（无全局可变状态）；生产调用方无一处传新参数；未放宽任何生产校验。
- **resolve-drivers.mjs（BUG-003 修复）**：`mkdirSync(resolve(ROOT,'src-tauri','resources','builtin-ep'),{recursive:true})`
  为 `main()` 首行（`:1108-1112`，先于 `wantsRestoreOnly()` 早退），无条件幂等；diff 全量 = 3 行 WHY 注释 + 1 空行
  + 1 行 `mkdirSync`（该函数原文件已 import，无新 import 面）。文件尾自执行守卫（`:1225`）⇒ **import 无副作用**，
  独立自证 `IMPORT_DID_NOT_CREATE_DIR`（见 §3-e）。覆盖全部 pre-cargo 路径：CI（`.github/workflows/ci.yml:119`）、
  `with-driver-inject.mjs:123`（回归步骤 2 真实形态）、`--codegen-only`（本轮 3 次）、`--restore-only` 早退前。

### 2. BUG-005 复测（测试落点重定向）

- 跑前/跑后快照 `diff` = **`IDENTICAL_ZERO_CHANGE`**：`src/extensions/generated-pro.ts` size/mtime/md5 逐字段一致；
  `src-tauri/resources/builtin-ep/` 全树一致（跑前存在且为空，filecount=0）。
- `npx vitest run scripts` = **23 files / 246 pass / 0 fail**（基线 244 + 本回合新增 2 例，与 Coder 自报一致）。
- stdout 真实路径泄漏检查：只剩沙箱 `…/T/resolve-pro-sandbox-*/builtin-ep/sql-editor-pro` 两处删除行；
  `grep /Users/…` 仅命中 vitest 自带 RUN 头 ⇒ **无真实仓库路径写入**。
- 测试源码落点审计：`GENERATED_PRO_TS` / `DEFAULT_BUILTIN_EP_ROOT` 仅纯值断言（`:215-219`）；`process.cwd()`
  仅 `:384` 用例的只读 `existsSync` 探测（worktree 无外部树 ⇒ 早退）；其余 flow 用例写删全部落 `mkdtempSync` 沙箱。
- 长程零变化：该快照在整轮 `run-regression.sh`（含步骤 3 全量 442 files）之后仍逐字段一致。

### 3. BUG-003 复测（构建前置无条件补齐）

| 场景（本实例实跑） | 命令 | 关键输出 |
| --- | --- | --- |
| a. codegen 路径 | `rm -rf src-tauri/resources/builtin-ep` → `node scripts/resolve-drivers.mjs --codegen-only --drivers=all` → `test -d …` | exit 0 / `DIR_OK` |
| b. 注入路径 | 再 `rm -rf` → `node scripts/with-driver-inject.mjs --drivers=basic -- echo INJECT_PATH_OK` | `DIR_OK_AFTER_INJECT_PATH` |
| c. cargo 全链（回归步骤 2 同形） | 再 `rm -rf` → `… with-driver-inject --drivers=basic -- env HOME=… CARGO_HOME=… RUSTUP_HOME=… cargo test -p datazen --lib` | exit 0 / `test result: ok. 1453 passed; 0 failed; 3 ignored` |
| d. mutation（授权例外） | `cp` 备份 → 摘除 mkdir 行 → `rm -rf` → codegen → 目录仍缺失 → 同形 cargo | 红（exit 101，运行记录）：日志 `:22` `process didn't exit successfully … (exit status: 1)` + 末段原文 ``resource path `resources/builtin-ep` doesn't exist`` |
| e. import 边界 | `import scripts/resolve-drivers.mjs` 后断言目录 | `IMPORT_DID_NOT_CREATE_DIR` |

- d 还原证据：`cp` 还原后 `git diff --stat -- scripts/resolve-drivers.mjs` = 空；md5 = `845a999b60918a8724f7b606d5d45d79`
  （与备份一致，亦与当前 HEAD 一致）⇒ 文件逐字节还原。
- 结论：三场景全绿 + mutation 红证修复行**必要**（且红为原始报错 `resource path … doesn't exist`，非新错误）⇒ BUG-003 通过。

### 4. BUG-004 独立清点（主检出只读：`packages/pro-extensions/sql-editor-pro/src/locales/en.ts`）

| 项 | 实测 | 与契约/登记比对 |
| --- | --- | --- |
| EP 自带 key 总数 | **26** | = 契约 §2.4.4 现文 |
| 命名空间分布 | `query.*` **24**（`:2-25`）+ `settings.editor.intention*` **2**（`:26-27`） | 契约现文「24 + 2」✔（原报告「全部落在 `query.*`」措辞已由修复回合明示修正） |
| 与宿主同名 key | 5（全在 `query.*`）；en 3 异值/2 同值、zh-CN 5 全异值 | 与登记一致 |
| 宿主 `query.*` 总量 / 宿主消费方 | 322 / **0** | 与登记一致 |
| wapp 侧 `registerTranslations` / `@datazen/ui` | **0 命中** | R-5 后半句对 wapp 成立 |

⇒ 契约 §2.4.4（`:310`）与实测一致、**无新缺陷**；命名空间裁定维持外部仓移交。

### 5. 文档核对（checklist 4）与新增发现

- §2.4.1（`:260`）「仅六个」+ 第 6 签名 `getRegisteredTranslations(locale: string): Record<string, string>`
  vs `packages/ui/src/i18n.ts:69` / `index.ts:27` ✔；§2.1.1（`:141`）`@datazen/ui` 行已列该 API ✔。
- §2.4.3（`:297`）「**生产码** 0 命中」+ 4 处残留（含护栏 `check-driver-import-boundaries.mjs:94`）✔；
  §2.4.4 有 EP 观察项（`:310`，明示「不构成强制条款」）且 §2.4.3 表格 `:293` 有交叉引用 ✔；**无 EP/wapp 新增强制条款** ✔。
- 任务书侧：验收标准 4 已改写为三条（`:80-86`，与 §2.4.2 对齐）✔；§5 五条裁定在文 ✔；「第 2 轮修复回合」节存在 ✔。
- **新发现（已登记 BUG-007）**：契约 `:283` 括号内「单测调用 7 行」实测 **15 行**（`packages/ui/src/__tests__/i18n.test.tsx`；
  数字在 `d250e52de` 写入时准确（7 行全为调用），R-8 commit `5328cd5e0` 扩写后未同步）——低危非阻断，详见 `bugs.md`。
- 其余行号锚点（`:279`/`:281`/`:283`、`localeSync.ts:20,24`、`locales/index.ts:78,82`、`R2_FILE_CARVEOUTS`
  两文件、`ALLOWLIST` 两条三元组）逐条 Read/Grep 复核一致。

### 6. 全量回归 7/7（本实例实跑一次）与数字对表

```text
$ bash scripts/run-regression.sh     # EXIT=0（日志 /tmp/rphase2-regression.log）
1  node scripts/check-driver-import-boundaries.mjs  PASS  0m00s
2  cargo test -p datazen --lib [注入+HOME包装+复跑] PASS  0m58s（1453 passed; 0 failed; 3 ignored）
3  npx vitest run                                   PASS  1m17s（442 files / 4572 tests）
4  npx vitest run --config vitest.drivers.config.ts PASS  0m06s（33 / 241）
5  node scripts/check-id-terminology.mjs            PASS  0m00s
6  npx tsc --noEmit                                 PASS  0m08s
7  npx vite build                                   PASS  0m06s（main-iWjiAdIb.js 1,574.66 kB）
全量回归门禁通过 ✔
```

- 步骤顺序口径：1 秒级护栏 → 2 分钟级 cargo，**不再出现 BUG-003 首跑红**；步骤 2 目录被自动补齐（§3-c）。
- 数字对表（对任务书/前任基线，差异全部解释）：
  - 步骤 3 = 442/4572 = 本实例 `npx vitest run src packages/driver-sdk packages/ui`（**412/4247**，本轮实跑）
    + `npx vitest run scripts`（**23/246**）+ wapp-sdk（5 文件/69）+ extension-points 根（2 文件/10）
    ⇒ 442 文件 / 4572 例；extension-points/src（2 文件/34 例）已被 `src` 过滤含在 412 内，不重复计。
  - 步骤 4 = **33/241**，与基线逐字一致；其中 redis UI 单跑 = **27/222**（本轮复跑，同基线）。
  - tsc：all 档本轮实跑 0 error（日志 0 字节）；basic 档由回归步骤 6 覆盖（exit 0）。
  - vite：步骤 7（basic）main chunk = **1,574.66 kB**（与基线同 hash `main-iWjiAdIb.js`）；all 档本轮另跑 =
    **1,605.12 kB / gzip 467.15 kB**（`main-rSNK-HW9.js`，与任务书参考值一致，无体积回退）。
- 收尾核对：`git diff --quiet Cargo.lock` = 通过（注入残留已还原）；`.driver-file-stash/` 无残留。
- 秒级补跑：worktree 护栏 = exit 0 · **1403 files / 0 blocking / 4 advisory**（与 A-7 逐项一致）；
  `node scripts/check-ci-docs-consistency.mjs` = exit 0（契约文档本轮改动后仍绿）。

### 7. 本实例实测 vs 未做

- **实测**：§1 变更面/边界逐项（含 5 覆写口对表、`src-tauri` 零改动、import 无副作用）；§2 BUG-005 快照+套件+日志审计；
  §3 BUG-003 三场景 + mutation + 还原；§4 BUG-004 主检出只读清点；§5 文档逐行取数（含新发现 BUG-007）；
  §6 全量回归 7/7 实跑 + 412/4247、27/222、两档 tsc/vite 抽验 + 护栏 + ci-docs。日志均在 `/tmp/rphase2-*.log`。
- **未做（范围外/不适用，非遗留缺陷）**：GUI-1~GUI-9 真实 e2e（`pnpm e2e` / `pnpm tauri:build:webdriver`
  本专项明令禁止，仍留给用户本地打勾）；`sql-editor-pro` 6 条 R2 advisory 的收敛（外部仓裁定）；
  既有翻译债（已另立翻译回合，不在本轨）。
- **判定**：BUG-001/002/003/005/006 复测**通过**（→ `已修复`）；BUG-004 维持「契约侧观察项已完成 + 外部仓移交」；
  **新登记 BUG-007**（低危非阻断，待协调者裁定派修）；全量回归 7/7 全绿 ⇒ **`TEST_DONE(PASSED)`**，
  未闭环清单 = BUG-007（文档数字）+ BUG-004 外部仓项 + GUI 人工项。

## 开放项（等用户，不阻塞本轨）

- **外部仓漂移移交项**（BUG-008 裁定为 advisory，不在本专项修复）：`packages/drivers/superset`（git driver，
  独立仓库）仍有 2 处宿主 `src/hooks/useI18n` 引用（`ui/SupersetConnectionFields.tsx:3`、
  `ui/SupersetSchemaTree.tsx:19`），需在其**自身仓库**换源 `@datazen/ui`；`sql-editor-pro` 的 6 处
  `setLocale` 是其单测合法用法，暂不动。R 阶段只核对这两项仍如实出现在 advisory 输出与契约文档里。
- editor-pro 子仓 commit `c60f7fc` 已本地提交但**未 push**。
- **Pro EP 词条命名空间移交项（BUG-004，Wave 4-B 新发现）**：`packages/pro-extensions/sql-editor-pro` 自带 26 个
  自有 key（实测 **24 个 `query.*` + 2 个 `settings.editor.intention*`**），其中 5 个 `query.*` 与宿主同名
  （en 3 异值 / zh-CN 5 全异值；宿主当前 0 消费方）；按
  `registerTranslations` 后写覆盖语义，装 Pro 的构建取 EP 文案、Community 取宿主文案 ⇒ 需在 editor-pro
  自身仓库裁定是否改 `pro.*` 前缀（契约 §2.4.4 的「前缀互斥」目前只对驱动强制）。
- 既有翻译债（宿主 8 语言合计缺 2400 / 冗余 1650；redis 9 语言各缺 139、zh-CN 缺 72）是否立独立翻译回合。
- 延后里程碑：① `WebContextMenuHost` 下沉 `@datazen/ui`；② `ConfirmDialogOptions`(driver-sdk)
  与 `ConfirmOptions`(宿主) 去重；③ 移除 `getTranslation` 的跨语言临时 `setLocale` 交换适配器；
  ④ `DocumentConnectionView` 的 `mongo.*` 归属宿主还是 mongodb 驱动（实测口径 = **21 处出现 / 15 个不同 key**，
  非任务书初稿的「20 处」）。

## 关账补记（协调者 · 2026-09-21）

- **合流**：`feature/r-phase` 以 `--no-ff` 合入 `feat/driver-decoupling` = **`1c293cc00`**
  （7 文件 / +1597 −109，零冲突；分支尖端 `1be4de89c` 已全量并入）。
- **主检出终局门禁**（合并后实跑；含外部树的口径只有主检出算数）：
  - `bash scripts/run-regression.sh` = **7/7 全绿**（护栏 0m01s → cargo 1m01s → host vitest 1m27s →
    驱动 vitest 0m06s → ID 0m01s → tsc 0m08s → vite 0m05s）；
  - `node scripts/check-driver-import-boundaries.mjs` = `ok (1489 file(s) scanned · 0 blocking violation(s) ·
    12 advisory finding(s))`，与 A-8 基线逐条一致（R1×2 superset / R2×6 editor-pro 单测 / R3×4 宿主引驱动内部）；
  - `git status` 仅剩会话外既有改动 `src-tauri/src/commands/ai/integration_tests.rs`；`git diff --quiet Cargo.lock` 通过。
- **all 档 `npx vite build` 干净值 = 1,603.30 kB / gzip 466.45 kB**（basic 档 1,572.84 / 459.96）：与任务书参考值
  1,605.12 的 **−1.82 kB** 属 BUG-005 修复的确定性结果——旧基线是在 `src/extensions/generated-pro.ts` 被脚本单测
  改写为 `Edition: pro`（含 builtin-ep loader 片段）的**污染态**下测得；修复后该产物恢复 clean community，
  且 basic / all 两档差值一致（各 −1.82 kB）⇒ 差异只来自共享的 generated-pro 片段。**非体积回退**，
  O-1 十语言量级不受影响，后续以干净值为准（与 BUG-005「后果 3」呼应）。
- **缺陷终局**：BUG-001/002/003/005/006/007 = `已修复`（2 个修复回合 + 3 个复测回合，含 BUG-003 的
  mutation 红证与「删目录 → codegen → cargo」三场景）；BUG-004 = 契约侧观察项完成 + **移交 editor-pro
  外部仓**（是否改 `pro.*` 前缀由其自行裁定）。
- **清理**：worktree `.worktrees/datazen-r-phase` 已移除、分支 `feature/r-phase` 已删除。
- **交用户**：§3 人工验收清单 GUI-1~GUI-9（GUI-5 依赖未 push 的 editor-pro `c60f7fc`）；开放项见文末。
- **口径校正（关账例行）**：文末开放项中原写「自带 26 个 `query.*` key」，据复测实测（24 个 `query.*` +
  2 个 `settings.editor.intention*`）校正为现文，与该契约 §2.4.4 现文一致；不涉及任何已复测结论的改动。
