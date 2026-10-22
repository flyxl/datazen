# 验证方法论

本文是**已实现**的验证规则，供写测试、写门禁、做验收时直接照做。文中每条规则都对应本仓真实发生过的失效——列出的不是理论风险，是已修复或仍需防范的具体缺陷。

---

## 1. 证据分级

任何因果陈述必须**拆开**并标注等级。三级强度不同，**不得并置在同一句**：

| 等级 | 含义 | 可否单独支撑结论 |
| --- | --- | --- |
| **变异证据** | 亲手注入缺陷，**亲眼看到用例转红** | 可 |
| **真实管线实测** | 在真实构建/门禁/运行路径上跑出来的数字 | 可（仅限该路径） |
| **机制论证** | 读源码推断「应该会怎样」 | **不可**，必须降级表述为「原理上」 |

**提交信息、台账、报告里的自述声明，本身都不是证据。** 「我已修复 X」需要一条能失败的用例来支撑，不是靠叙述。

### 1.1 不要把两次不同上下文的测量拼成一条结论

两条各自成立的证据，组合起来可以是一条**完全错误**的结论 —— 因为它们描述的不是同一个世界。

- 实例：主检出的 Pro 检出停在 `main`（ep `1.0.0`），据此推断「在主检出跑全量门禁会拿到 2 个断言失败」。**但那推断用的是集成分支的宿主版本（`1.1.0`）**；主检出的宿主也声明 `1.0.0`，二者**相等，断言通过**。在同一检出内直接比对即证伪。
- ⇒ 任何「A 状态 + B 状态 ⇒ C 后果」的推断都必须**在单一上下文内重新测量**。
- ⇒ **跨检出的 git 对象可以跨**（`git show <branch>:<path>` 读的是版本库里的内容，与你站在哪个检出无关）；**运行时版本必须现场取**（依赖文件、被 gitignore 的产物、symlink 目标、node_modules 布局 —— 这些随检出而异）。
- ⇒ 报告者把这类推断主动降级为「机制论证、不当定案」是正确做法；**协调方收到时应当要求在单一上下文内复测，而不是直接采信或直接否定。**

---

## 2. 断言存在 ≠ 断言能失败

**这是本仓最重要的一条。** 一条全绿的测试可能完全没有约束力。

判定方式只有一个：**反向注入**——把被测行为改坏，看它是否转红。**全绿永远不会告诉你这件事。**

已知会退化成假守卫的四种形态：

1. **`toContain` 命中别处** — 断言的字符串在文件里还有第二个出现位置。
   - 实例：`panelStore` 夹具的 PROVENANCE 守卫断言 banner 含 `227b6af8e`，而 banner 里另有一行也引用同一 commit ⇒ 把 banner 改成 `deadbeef1` 仍全绿。
2. **断言存在而非唯一** — 断言「某键在列表中」而非「恰好出现一次」。
3. **跨仓 guard / guarded 错配** — 守卫检查的对象与被守卫的对象不是同一个。
4. **恒真断言** — 断言两侧由同一表达式导出。

**假守卫比没有测试更糟**：它制造「已覆盖」的错觉，并阻止后续修复。发现假守卫时应**删除并重写**，而不是保留。

### 2.1 修法本身也要反向注入

用错误的方式消错，会制造出新的假绿。

- 实例：`pack-ep.mjs` 的 `catch (err)` 直接取 `err.message`。修法有两种：
  - `err instanceof Error ? err.message : String(err)` —— 真实修复；
  - 加一句 `assert(err instanceof Error)` 让类型收窄 —— **门禁变绿，但 handler 仍可能写不出标记**，而该标记的作用正是让 `resolve-pro` 不把过期的 staged 树当成完整的。
- ⇒ **选完修法，对修法再注入一次**，确认它是在真修而不是在糊类型。

### 2.2 纠正本身也要被检验

当你「纠正」一处不准确的说法时，**先验证纠正后的版本是否也准确**，否则你会引入一个新的错误陈述。

- 实例：`.gitignore` 中「真正起作用的是 `src-tauri/resources/builtin-ep/` 目录规则」——这只对**落在该目录内**的标记成立；`--stage-dir` 可指向任意位置，此时根级 `*.incomplete` 只有 `*.incomplete` 一条规则能匹配。**按原话写进注释就会造出新的说谎注释。**

---

## 3. 跨仓门禁互不可见

**SQL Editor Pro 是独立 git 仓、且被宿主 gitignored。** 由此产生三条必须知道的规则：

### 3.1 宿主门禁对 Pro 零覆盖

`npx tsc --noEmit` 与 `npx vitest run` **不覆盖任何 Pro 文件**。Pro 侧回归在宿主门禁眼里**完全不可见**。

⇒ **「宿主全绿」不构成关于 Pro 的任何证据。** 两套门禁必须分开跑，且只有 Pro 那一套是关于 Pro 改动的证据。

⇒ 已发生的真实后果：CodeMirror 折叠曾**整个是死的且完全静默**（`codeFolding(config)` 只把配置写进内部 `foldConfig` facet，而 `FoldConfig` **没有** `foldService` 字段），而宿主门与 Pro 门**同时全绿**。

### 3.2 跨仓接缝是无人看守的

接缝（「宿主设置 → EP → Pro 实现」）**两端都没有真测试**时会同时静默失效。

- 宿主侧的 `createFoldExtensions` 是宿主本地包装，**与 Pro 同名但不是同一个东西**；Pro 侧的设置门用例直接调底层函数、**绕过 EP**；宿主侧用例则 **mock 掉整个 EP**。
- ⇒ 测接缝必须**两端都不 mock**，真实穿过整条路径；接缝无法在宿主测试内表达时，**报告「做不到」并改在门禁层面收口**，不要用一个绕过接缝的测试充数。

### 3.3 Pro 分支错配有两种截然不同的失败签名

`packages/pro-extensions/sql-editor-pro` 在 worktree 中按设计可能不存在。同一件事的失败形态**随 Pro 分支而变**：

| Pro 状态 | 失败签名 | 极易误判为 |
| --- | --- | --- |
| 无检出 | `ENOENT .../manifest.json`（`security.test.ts` 4 例） | 自己的改动破坏了构建 |
| 落在 `main`（ep `1.0.0`） | **断言失败** `expected '1.0.0' to be '1.1.0'` | 自己的改动破坏了契约 |
| 落在 `productivity/*`（ep `1.1.0`） | 全绿 | —— |

⚠️ 第二行是最危险的一种：**它长得完全像「我的改动弄坏了它」。**

⇒ **开工第一件事是报告 Pro 分支与它的 `manifest.json` 版本。** 映射链：`宿主 feature/<slug> → Pro productivity/<slug> → Pro feature/<slug> → Pro main`。

⚠️ 同理，**不要把缺失的 codegen 产物当成失败原因**：`src/extensions/generated-locales.ts` 等 gitignored 产物在 worktree 中缺失，对全量门禁**零影响**（实测：删除后 `tsc` 与全量 vitest 逐项一致）。全仓唯一引用是 `scripts/check-driver-import-boundaries.mjs` 的一个常量。

---

## 4. 门禁本身的门禁

### 4.1 `checkJs: false` 会让门禁保护测试却不保护实现

`scripts/*.mjs` 配 `checkJs: false` ⇒ **函数体永远不被类型检查**，而**保护它的测试却在门禁里**。

⇒ 被保护者进了门禁，保护者的实现没有。`pack-ep.mjs` 在此状态下积压 **78 个错误**且无人察觉。

**修法：给闭合的 `files` 打开 `checkJs`，而不是放宽选项。** 三个互不遮蔽的程序并存：

| 程序 | 范围 | `checkJs` | 进 CI |
| --- | --- | --- | --- |
| `tsconfig.scripts.json` | `scripts/__tests__` | 关 | 是 |
| `tsconfig.pack-ep.json` | `pack-ep.mjs` + `sign-ep.mjs`（闭合 `files`） | **开** | 是 |
| `tsconfig.scripts-checkjs.json` | 整个 `scripts` | 开 | **否**（棘轮基线） |

⚠️ **闭合 `files` 程序优于 `// @ts-check` 注解**：注解在有人改动上一行时会**静默停止检查**，而 `files` 里的文件被改名或删除会变成 TS6053，**直接把门禁打红**。

### 4.2 棘轮：基线只许下降

新增更严的门禁时，第一步**永远是打一份只许下降的基线快照**，不是直接打开开关。门禁的放宽只允许发生在「基线已归零」之后。

### 4.3 门禁要证明自己不冗余

新增门禁后，用一个真实类型错误验证它**会红且指名文件**，同时确认既有门禁**仍全绿**——两者同时成立才说明不冗余。

---

## 5. 体积闸门

宿主打包 Pro 时有产物体积上限闸门。**校准必须由产物字节实测得出，不能估算。**

⚠️ **已知局限**（实测）：内联 `@codemirror/state`（418 kB）**不会**触发该闸门，而该模块正是 `Facet` / `StateField` / `RangeSet` 的定义处。因此**体积闸门不能替代功能断言**——折叠曾整个失效而体积完全正常。保留体积闸门是为了挡住**大规模**内联，不是为了守住行为。

⚠️ 同理，闸门注释里写的余量必须与实测一致：真实余量约 **9%**，不是注释曾声称的约 50%。

---

## 6. CodeMirror 事实（易踩）

- **`FoldConfig` 没有 `foldService` 字段。** 真正的注册是 `foldService.of(fn)`，facet 类型为 `(state, lineStart, lineEnd) => {from, to} | null`（**不是** `RangeSetBuilder`）。只把配置写进 `codeFolding(config)` 不会产生任何折叠能力。
- **`typeof foldService` 命名的是 Facet，不是 facet 里那个函数。** 这是类型检查能通过的原因。
- **`foldGutter()` 自带 `codeFolding()`**（实测于 `@codemirror/language` 6.12.3 的 `dist/index.js`）。`foldExtension.ts` 里显式那行目前冗余；若将来 gutter 换���自研实现，它会重新变成承重行。
- **facet 为空会静默废掉整条链**：`foldCode` / `foldAll` / `foldInside` / `toggleFold` 与 gutter 共用 `foldable()` 这一入口。
- 宿主编辑器**没有 SQL parser**，语法树回退路径无法工作。

---

## 7. 已知环境 flake —— 不要修，不要用 `testTimeout` 掩盖

| 用例 | 现象 |
| --- | --- |
| `sqlSnippetsLifecycleJourney.test.tsx` | v8 覆盖率下 5s 超时 |
| `DataTransferWindow.test.tsx` | ×3 |
| `schemaStore.test.ts` | ×1（隔离重跑通过） |
| `statementRanges.test.ts:184` | 偶发 |
| `paneFocusDifferentialRepro.tester.test.ts` | PROVENANCE 1 用例在全量负载下 5s 超时，**隔离连跑 3 次全过** |

⇒ 报告全量结果时**必须逐项列出这些失败并说明其归属**，不得计入自己的改动。

### 7.1 worktree 里的 pnpm 会假失败

**已修（2026-10-22，缺陷甲-7）**。下面是历史形态与现行契约。

历史形态：worktree 中 `pnpm <script>` 会在**任何脚本运行之前**先试装一次依赖。`node_modules` 是软链，
pnpm 认不出链接目标而判定依赖过期，但那次安装永远装不完：pnpm 11 直接中止
（`ERR_PNPM_ABORTED_REMOVE_MODULES_DIR_NO_TTY`），pnpm 12 打印
`× installing dependencies … workspace hoist directory is not a real directory` 后仍继续执行并以 0 退出。
两种形态下**未改动的脚本同样如此**——不是接线缺陷，而是 `verifyDepsBeforeRun` 默认值 `install` 在软链
拓扑下的必然结果。

现行契约：仓内 `pnpm-workspace.yaml` 把该键默认设为 `false`（值取自
`${DATAZEN_VERIFY_DEPS_BEFORE_RUN:-false}`），CI（`ci.yml` / `release.yml` 的 workflow 级 `env`）用同一
环境变量设回 `install`。⇒ 裸 `pnpm <script>` 在 worktree 里直接可用，不必再加
`pnpm --config.verify-deps-before-run=false`，也不必退回 `npx`。**禁止** `CI=true` /
`confirmModulesPurge=false`：它们**不能**重新打开这个键（实测 `CI=true` 对
`verifyDepsBeforeRun: false` 无影响），只会把别的 pnpm 行为一起改掉。

---

## 8. 探针自身的坑

**探针出错时，先怀疑探针。** 本仓反复出现的形态：

- 用 python 先按 `'codemirror' in k` 过滤 `dependencies` 再读结果 `{}` ⇒ 把过滤后的空集当结论。
- `git ls-files 'scripts/**/*.mjs'` 返回 5（实际 46）—— git pathspec 的 `**` 后需要再有一层目录。
- `grep -c "is okay"` 对 `git bundle verify` 恒为 0——git 输出跟随 locale。
- 用**非法的 TS 写法**（如 `payload[X]!`）做变异，得到 `0/0` 的假结果。
- **断言「某集合为空 / 完整 / 恒等」时，看到 `{}` 或 `0 条` 先怀疑查询本身。** 反过来也成立：断言「已覆盖」时，看到非零命中先确认**每一条都指向正确位置**。

⇒ **反向验证要有正向对照**：证明「查询确实在工作」的独立探针（如同目录其他关键符号的命中数），否则 0 命中可能只是查询写错了。

⇒ **不要用与被修数据同源的探针键**——那会测出「改动没生效」而非「守卫有效」。

⚠️ **跨上下文拼接**（把两次不同上下文的测量当成一条结论）见 §1.1。

---

## 9. 量具与被测对象必须对得上

§8 讲的是「探针写错了」。这一节讲更隐蔽的一种：**量具运行成功了、报数也准，但它量的不是你要的那个东西。** 三种子形态，都已真实发生。

### 9.1 用错量具：仪器对，结论错

删除 `.github/workflows/ci.yml` 中一段时误吞了 `Frontend unit tests` 的 `run: pnpm test:unit`，只剩 `- name:` 加注释。**YAML 解析通过**，判定「合法」。

⇒ **YAML 合法 ≠ Actions 合法。** step 有 `name:` 而无 `run`/`uses`，在 YAML 语法里完全合法，Actions 却直接判 "workflow file issue"：**0 秒结束、一个 job 都不启动、没有日志**。

⇒ 改任何 workflow 后**逐 step 查 `run`/`uses` 是否存在**，并查 `needs` 指向的 job 名真实存在。YAML 解析器对这两个问题**没有发言权**。

⇒ 推广：**验证手段必须与待采信的结论同构。**「格式合法」「解析通过」「能加载」都只证明解析器关心的那部分。

### 9.2 管道吞掉失败信号

同一形态的五次实例（主代理与子代理合计）：

| 写法 | 实际发生了什么 |
| --- | --- |
| `cmd \| tail -20; echo $?` | `$?` 是 **tail** 的退出码，掩盖了 `cmd` 的失败 |
| `echo "EXIT=$?" $(cmd)` | 命令替换先执行，**`$?` 变成 echo 自己的** |
| `n \| wc -l` 而 `n` 本身是 `fatal:` | `2>/dev/null` 丢弃了 stderr ⇒ 数出 0 行当「无变更」 |
| 短 SHA 喂给要求 40 位的函数 | 得到校验失败，被误读成「真守卫失败」 |
| `grep -c` 统计断言数 | 漏掉 `expect.soft` 与 `it.each` 展开 ⇒ 报 118，实测 120 |

⇒ **任何「数字」或「通过」的结论，先捕获退出码，再捕获输出。** 把二者拆成两行、让输出先落文件再回读，最后单独读 `$?`。
⇒ 报数前**核对期望值的来源**：断言数应以测试框架的汇总行为准，不以 grep 为准。

### 9.3 fixture 错配：仪器完全正确，被测对象不是它

反向失效。与 10.1/10.2 相反——**仪器从头到尾报得都对，错的是被指向的对象。**

- 实例：Pro 的全部本地检查都对着宿主 `a31d00359`（因为那正好是 worktree 的 HEAD）跑，全绿。而 CI 会按 `host-pin.json` 检出 `446b78ba0`——**比它老一个提交，不含精确钉版本**，于是声明 caret range，守卫正确地拒绝。守卫自始至终如实报告（10 个问题 → 1 个 → 0 个）。
- ⇒ **在采信一个「你没组装的系统」的结论之前，先确认你的 fixture 就是它会搭出来的那个。** 廉价做法：**对跟踪文件做一次干净的 `git archive`**，挂到管线创建的目录布局上跑一遍。本例成本是一次构建，换来「本地绿 → CI 红」当场暴露。
- ⇒ 这类错配**本仓脚本抓不到**，因为它是「pin 指向的树」与「我实际在跑的树」之间的差，`verify-host-pin` 只把 HEAD 与 pin 的差异当 notice 报出。
- ⚠️ 看到 `verify-host-pin` 之类的 **notice 型告警**（而非 failure），**不要因为「门禁还是绿的」就跳过**——它报的正是这类错配。

### 9.4 全绿可能只是顺序依赖

- 实例：`DataSyncWindow.test.tsx` 的 `requires scope reselection when the history profile revision changed`，全文件跑稳定通过，**只有同文件更早的用例先跑过才通过**；单独跑必红（实测 3/3 红）。根因在 `canNext` 的 `endpoints` 分支要求 `activePairing?.supported`，而它由 `useSyncPairingState` 的异步 effect 写入、冷启动时为 `null`——**与被测的 reconcile 无关**。CI 慢，effect 未落定即断言，于是只有 CI 红。
- ⇒ **判定一条用例是否真被覆盖，要看它在冷 fixture 下单跑是否通过**，而不是看它在全文件里的结果。
- ⇒ 修法**双向验证**：加修复后单跑 3 次全绿，**再把修复回退、单跑 3 次全红**。只报绿的一侧，无法区分「修好了」与「本来就不是顺序依赖」。

---

## 10. 注释必须描述实测事实

说谎的注释比没有注释更危险：它把错误事实固化下来并阻止后来者修正。

- 实例：`sign-ep.mjs` 的 JSDoc 声明 `sigDoc.version: string`，实际值是**数字** `1`（`EP_SIGNATURE_VERSION`），而验签处按精确等值比较——**该注解描述的是宿主会拒绝的一份文档**。它被 `checkJs` 抓住。
- ⇒ 写「X 由 Y 负责」之前先跑 `git check-ignore -v` / 实际调用一次，**确认是哪条规则在起作用**。
