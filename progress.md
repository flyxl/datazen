# P3 文档漂移与死引用清理 — 进度台账

> **删除条件：本轨验收合并后必须删除本文件，不得存活在 `main`**（AGENTS.md「进度台账：开发期间允许，交付即销毁」）。
> 本文件是台账，不是交付物。下列结论最终必须能从代码与 `docs/` 读出。

- 轨道：`p3-doc-drift-ledger`
- 分支：`feature/p3-doc-drift-ledger`，基于 `main` @ `f1d843271510eb7b64f5a3a435e1cc4ea6603206`
- 工作树：`.worktrees/datazen-p3-doc-drift-ledger`
- 范围：**纯文档 + 死引用清理，零运行时代码改动**

## 一、已完成

### Task A — CM-60 p95 阶段归属（`docs/architecture/platform/connection-management.md`）

`hub.md` 里的「现阶段不关心性能；不纠结 p95」裁定从未进入 `docs/`。已在两处**原地**补上阶段归属句，读者不看台账即可判断本条何时达标：

- `:857`（§15.3 基准口径段尾）追加一句，拆开三个阶段：基准口径与基准 harness 属 **P3**、实测 p95 ≤10 ms 的达标判定属 **P7**、多 worker 半属 **P9**。
- `:1236`（CM-60 性能门槛 bullet）追加一句，回指 §15.3。
- §15.3 方法学（nearest-rank `ceil(0.95*N)`、每轮 10000 个获准且未排队请求、5 轮、并发 8、预热 1000、release 构建、不删失败样本、排队分位数另报）**逐字未动**；≤10 ms 门槛**未删未弱化**；两处均不含 `hub.md` 字样。

### Task B — CM-67 结论被后代码推翻（`docs/development/platform-development-plan.md:344`）

原文称「全仓没有 pool / lease 管理器的实现……`ResourceManager`……连类型都不存在」。逐类型复核后改写为真实状态：

- `ResourceManager`（`packages/runtime/src/resource/manager.rs`）、`SessionRegistry`（`packages/runtime/src/registry/registry.rs`）**已是 `datazen-runtime` 导出的生产类型**；
- `LeaseManager` / `PoolManager` / `ConnectionPool` **全仓确实仍无类型定义**（自行复核，非沿用派单说法）；
- lease / pool 现有的是数据与 port 层类型（`LeaseRequest`/`LeaseState`/`LeaseRecord`/`QueuedLease`；`PoolAcquireRequest`/`PoolLease`/`DriverPoolBudgetPort`/`InMemoryDriverPoolBudget`）+ `coordinator.rs` 的 `BudgetCoordinator` **trait**；
- 四条断言仍未落地。与 §17.1「CM-64 / CM-67 移交 P3」不冲突。

### Task C — `cm04_baseline.rs:13` 判据行号错指

原指向 `:852`，实为 `### 15.3` 前的**空行**（既不是 §17 的 CM-04 行，也不是普通笔误）。改指 CM-04 判据正文所在节（§16.1，判据标题原文）。**该行是注释行，非业务代码。**

### 未点名漂移（自行发现并修复）

1. **与 Task C 同类的错误锚点共 9 处，派单只给了 1 处。** 全部改为「§节号 + 判据标题原文」形式（抗行号漂移），涉及 `cm70_no_disk.rs`、`cm70_idempotency_replay.rs`、`cm73_baseline_tests.rs`（2 处）、`cm73_idle_eviction_tests.rs`（3 处）、`schema_metadata/tests.rs`、`cm04_baseline.rs`。其中 2 处是 `#[ignore]` 理由串，依据 CM-73 的 `- 基线说明` bullet 允许的已知失败表述重写。
2. **死文件引用 10 处、5 个类别**（自行全仓扫描，见下节方法）：`src/plugins/generated.ts`（2 处，真错，正确路径 `src/extensions/generated.ts`，见 `resolve-drivers.mjs:909`）、`commands/ai.rs`→`commands/ai/mod.rs`（3 处）、`commands/driver_command.rs`→`commands/driver_command/mod.rs`（2 处）、`docs/updater.md`→`docs/development/updater.md`（1 处）、`editor-pro-productivity-plan.zh-CN.md`（2 处，已交付即失效的计划文档，**删除交叉引用、把理由本身留在原地**）。
3. **`transport.rs` 自身注释的错误声明**：原称该聚合策略「documented in the module docs」，复核模块文档只有一行、并未记录该策略，**该声明为假，已删除**，只保留真实成立的结论。
4. **Task D 作用域大于派单所述**：除 9 处 `progress.md` 外，另有 `bugs.md`（6 处）、`## 契约冻结`（7 处）、`## 自验记录`（3 处）同类失效引用，**全部按同一判据原地写成事实，无一处只换文件名**。~~现四类全部归零。~~（该结论已被 R1 Tester 判为 FAIL-3 并于第二轮推翻：它只是「对我那份清单」的结论，不是对仓库的结论。见第五节。）

## 二、门禁（逐字结论行）

全部在同一工作树、首尾各记录一次 HEAD 与工作区 sha，证明运行期间无人改动。

```
HEAD_BEFORE = HEAD_AFTER = f1d843271510eb7b64f5a3a435e1cc4ea6603206
WT_SHA_BEFORE = WT_SHA_AFTER = ae85404ce088139f1a43ed7f35878f0408e22714
```

| 命令 | 结论行 | 退出码 |
| --- | --- | --- |
| `pnpm typecheck` | `tsc --noEmit` / `tsc -p tsconfig.scripts.json --noEmit` / `tsc -p tsconfig.pack-ep.json --noEmit` 全部无输出通过；`error TS` 计数 **0** | `EXIT=0` |
| `cargo test -p datazen-runtime --lib` | `test result: ok. 436 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s` | `EXIT=0` |
| `git grep -n 'progress\.md' HEAD -- packages src src-tauri scripts .github` | 该**作用域内** 0 命中（全仓复算为 **44** 处 @HEAD / 46 @`f1d843271`，含本台账自身与 `hub.md`） | — |

**作用域自证**：对全部 `*.rs / *.ts / *.tsx / *.mjs` 改动行做过滤，剔注释前缀后**剩余 0 行**——即本轨每一处代码改动都是注释 / 文档注释 / `#[ignore]` 理由串，无测试逻辑、断言或运行时行为变更。

### 关于文档内既有的测试计数

`platform-development-plan.md:341` 的 `139 passed`、`:343` 的 `19 个单测` 与今日实测 `436` 不符。这两处均自带时点标注「计数与退出码测于 `5b7c5b49c`」，属**自标注的历史实测值而非失效引用**，故未改；今日 436 记录于本台账备查。

## 三、遗留与待裁定项（本轨未擅自改写）

1. **`PRD §` 共 193 处、跨 125 文件**——**无任何 _tracked_ PRD 文件**；见 6.6，原「untracked / ignored 皆无」属过度断言。计数方法可复现（**入库口径，排除两个台账自身**——否则本台账里写下的字面量会把自己算进去）：`git grep -o 'PRD §' -- . ':!progress.md' ':!hub.md' | wc -l` = **193**；`git grep -l 'PRD §' -- . ':!progress.md' ':!hub.md' | wc -l` = **125**；`git ls-files | grep -ic prd` = 0。**不排除台账的原始数是 197 / 126**，差额 4 处全部来自本台账自身文本。**台账计数规则：凡统计「入库内容」必须同时排除 `progress.md` 与 `hub.md`。**占 tracked 文件 3490 个中的 3.6%。规模远超单点引用，且每处的「§」所指内容已无法复原，**无据可写**；须由各轨文档 owner 决定是就地改写为已实现事实还是删节。（第一轮 192/124，第二轮 195/126——第二轮把台账自己算进去了，R2 纠正；本轮 193/125 为准。）
2. **`connection/adapters.rs`（`shared-boundaries-and-ports.md:589`）**：该行是「过渡实现、逐 consumer 迁移后删除旧路径」的**迁移状态声明**。文件不存在，但本轨无证据判断迁移是已完成还是计划有误——**改写它等于替 owner 下结论**，故保留原状待裁定。
3. **已删除的测试文件引用 3 处**：`e2e/helpers/ops-process-server.ts`（`e2e-coverage.md:189`）、`ConnectionNavigatorTree.test.tsx`（2 处）、`extensionThemes.test.ts`（`packages/wapps/README.md:99`）、`ui/plugin-meta.test.ts`（`docs/architecture/testing.md:178`）。均为删除测试后的悬空引用，替换目标不存在，需 owner 确认删除意图。
4. **`docs/blogs/diagrams/*.html|json` 共 12 处旧路径**：属已生成的推广物料产物，重写收益低且易与生成器脱节，未纳入本轨。

### 已排除的误报（复核后确认无误，不改）

`src/locales/builtinLocales.ts`（gitignored codegen，由 `builtin-locales.json` 生成）、`src/extensions/generated.ts`（同上）、`barrier/tests.rs`（真实路径 `packages/runtime/src/connection/testing/barrier/tests.rs`）、`src/lib/driverSettings.ts` 与 `src/lib/resolveEditorFontFamily.ts`（文档**明写这些宿主路径已被移除**，是有意的历史引用）、`cluster_async/mod.rs`（指 redis 依赖内部路径，非本仓）。

## 四、对派单的纠正

1. **Task B 文件路径写错**：真实路径是 `docs/development/platform-development-plan.md`，非 `docs/architecture/platform/`。（行号 `:344` 本身正确。）
2. **Task C 对 `:852` 的两种猜测均不成立**：它是 `### 15.3` 前的空行，既不是 §17 的 CM-04 行，也不是普通笔误。
3. **Task C 不是孤例**：同类错误锚点共 9 处，派单只给了 1 处。
4. **Task D 作用域偏窄**：除 9 处 `progress.md`，另有 `bugs.md`(6)、`## 契约冻结`(7)、`## 自验记录`(3) 同类失效引用，`PRD §` 另有 192 处。前三类已全部修复。
5. **Task B 的技术论断与 Task A 的两处行号经复核全部正确**，无需纠正。

---

## 五、第二轮（R1 返修）

### 5.1 门禁（逐字结论行）

在同一工作树重跑，首尾各记录一次 HEAD 与工作区 sha，证明运行期间无人改动：

```
HEAD_BEFORE = HEAD_AFTER = 756f2e6f2507f881466987b8bc609eabdae7cfac
WT_SHA_BEFORE = WT_SHA_AFTER = 76ab30513e876e222a569f4bfa4540f193e6ad6f
```

| 命令 | 结论行 | 退出码 |
| --- | --- | --- |
| `pnpm typecheck` | `error TS` 计数 **0** | `EXIT=0` |
| `cargo test -p datazen-runtime --lib` | `test result: ok. 436 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s` | `EXIT=0` |
| `cargo test -p datazen-driver-redis --no-run` | 4 个测试二进制编译通过，0 error | `EXIT=0` |

第三项是第二轮新增的：本轮改到了 redis 驱动 crate 的 Rust 测试文件（`census.rs`/`mod.rs`/`routing.rs`/`fix_round1.rs`/`fix_round1_retest.rs`/`write/tests.rs`/`tree_contract_tester.rs`），第一轮的三项门禁覆盖不到它们。

### 5.2 「零运行时代码改动」这句本身是假的（BLOCKER C）——已按验证方实测整节重写

第一轮提交信息写「纯文档，零运行时代码改动」。**那句话字面为假**，纠正已写入第二轮提交信息本身，而不是靠本台账补记。

R2 验证方查出本节原先的账目**三处全错**，本节整体重写。复算命令（基线 `f1d843271`，可复现）：

```
git diff -U0 f1d843271..HEAD -- '*.rs' '*.ts' '*.tsx' \
  | grep -E '^\+' | grep -vE '^\+\+\+' \
  | grep -vE '^\+[[:space:]]*(//|/\*|\*)' | wc -l          # -> 11
```

**11 行 / 9 处 / 5 文件**——不是本节原先写的 3 处。

| 位置 | 行数 | 性质 |
| --- | --- | --- |
| `packages/drivers/redis/tests/tree_contract_tester.rs` | 5 | `#[ignore = "…"]` **理由串**改写，删掉 `; see ## 留待 R 回归` 尾巴 |
| `e2e/specs/dialog-injection.ts` DI-004 | 3 | **纯格式重排**，表达式逐字不变 |
| `src-tauri/src/commands/cm73_baseline_tests.rs` | 1 | `#[ignore = "…"]` 理由串改写 |
| `src-tauri/src/commands/cm73_idle_eviction_tests.rs` | 1 | 同上 |
| `packages/drivers/redis/src/ops/workbench/tests/fix_round1.rs` | 1 | `assert_eq!` 的**多行自定义消息串**（`\` 续行的第二行） |

原先的三处具体错误，均为**我自己的账目错**，一并记在这里：

1. 原写 `packages/runtime/tests/cm70_no_disk.rs:3` 与 `cm70_idempotency_replay.rs:3` 是
   `#[ignore = "…"]` 理由串——**假的**，这两个文件是纯 `//!` 改写，**根本没有 `#[ignore]`**。
   真正的 `#[ignore]` 改动在 `tree_contract_tester.rs`(5) / `cm73_baseline_tests.rs`(1) /
   `cm73_idle_eviction_tests.rs`(1)。
2. 原写 `fix_round1.rs:24` 是「`///` 块的折行续行」——**假的**，它是活代码里
   `assert_eq!` 多行消息串的续行。
3. 原计数 **3**，实际 **11**（少数 8 行）。

**「行为变化 0」是可复现的实测，不是断言**：

```
for f in packages/drivers/redis/tests/tree_contract_tester.rs \
         src-tauri/src/commands/cm73_baseline_tests.rs \
         src-tauri/src/commands/cm73_idle_eviction_tests.rs; do
  git show f1d843271:$f | grep -cE '^[[:space:]]*#\[ignore'   # base
  grep -cE '^[[:space:]]*#\[ignore' $f                          # HEAD
done
# tree_contract_tester.rs 5 -> 5 ; cm73_baseline_tests.rs 1 -> 1 ; cm73_idle_eviction_tests.rs 1 -> 1
```

即**没有任何 `#[ignore]` 与「运行」的开关翻转**，也没有断言表达式被改。被改的只有
`#[ignore = "…"]` / `assert_eq!` 的**字符串字面量内容**，外加 1 处纯缩进重排。

> 计数陷阱：`grep -c '#\[ignore'` 会把 `//!` **散文行**也算进去
> （本文件 `tree_contract_tester.rs` HEAD 新增了一行提到 `#[ignore]` 的模块文档），
> 于是 base 6 / HEAD 7，凭空多出 **delta=+1** 的假开关。必须锚行首。

**对称性记录**：验证方在我这三条错误上先自行复算并全部更正后才出报告，并指出
**我犯的是同一类错误**（拿工作清单的数当仓库的数）。这三条错误由我自己在本轮查实并写入。

### 5.3 锚点必须成对验证（BLOCKER A 的教训）

我把 `cm70_no_disk.rs:3` / `cm70_idempotency_replay.rs:3` 的 `§16.6` 改成 `§16.7` 反而改错了：**CM-70 实际在 `connection-management.md:1294`，落在 `### 16.7`（1238–1325）内，`### 16.6` 只占 1193–1237**。两文件 numstat 行数中性，说明**原来的行号锚点本来就是对的**。

由此确立本轨的锚点判据（已适用于全部锚点）：**节号存在，且被引标题自身的行号落在该节区间内，两个条件同时成立才算解析成功；不得由邻近路标反推节号。** 推论：行数中性的改写，通常意味着旧行号原本正确。

据此全仓复扫 `connection-management\.md:[0-9]`，剩余 3 处全部是**登记不改动**的正确锚点（`tunnel/journey_sharing.rs:2`→CM-32、`tunnel/mod.rs:23`→CM-27、`tunnel/transport.rs:90`→`networkRouteRevision` @676），`:1284`/`:1285`/`:1307`/§16.6 全仓归零。

### 5.4 BLOCKER B 实为 5 处，不是 2 处

`src-tauri/src/commands/cm73_baseline_tests.rs` 里派单点了 `:19`、`:23`，我自己 grep 出还有 `:590`、`:661`、`:664`（另 `:661-662` 内还嵌着一个 `:1307`）。5 处全部由**行号锚点**改为 **CM-73 自身的 bullet 名**（`- 基线说明` / `- 保留声明`）。round-1 的 `:669` `#[ignore]` 串本已是正确写法。

**这条本身就是「必须按类穷举而非按清单逐条修」的证据**：派单给 2，我的清单给 2，实际 5。

### 5.5 引用保真：造引文与「证明不存在」的坑（FAIL-2）

- `:857` 原句里「性能门禁在单实例形态下测量」**在开发计划里不存在**——`git grep -F` 对 `docs/development/platform-development-plan.md` 两个串均为 0 命中。该半句已删（FAIL-2 裁定）。
- **须同时更正我自己的措辞、以及 FAIL-2 前提的边界**：这两句并非「全仓不存在」。该句**逐字存在于 `docs/architecture/platform/team-server-and-auth.md:780`**——CM-60「资源压力与 drain（单实例部分）」阶段归属行，验收范围列写着「总量/控制保留/逻辑会话/队列上限生效；**性能门禁在单实例形态下测量**」；`单实例形态` 另见同文件 `:392`、`:611` 与 `persistence-model.md:158`。即：**被删的是一条真事实，错的是出处**——我把它挂在了开发计划的 CM-60 性能门槛 bullet 上，而真实出处是团队服务架构文档。它同时与已改写的 `:857` 阶段归属一致（同行的「owner 路由、worker 分区、跨节点预算归 P9」正对应 WN 部分属 P9），因此删除未造成阶段归属丢失。按 FAIL-2 的字面裁定**维持删除、不擅自加回**；是否按 `team-server-and-auth.md:780` 重新锚定，登记待属主裁定。
- **同类的 `:1236` 从未被点名，却带同一处造引文**，一并修掉。这印证「未被质疑 ≠ 正确」。
- **我第一轮说 `基准口径` 不存在，这个说法是错的**：它确实存在，恰好 1 处，在 `platform-development-plan.md:128`（不在 `:124` 退出门槛那一行，且与 CM-60 无关联）。第一轮 `:857` 那句把 `:124` 与 `:128` 焊成了一句假合并。教训：**证明「不存在」必须对正确的文件做精确串 grep，而「我 grep 出来的结论」不是「仓库的结论」。**
- 改写后只用三段**逐字可核**的退出门槛原文：P3「60 单机部分」+「H 断言」(`:124`)、P7「60 单实例部分」+「W1 断言」(`:211`)、P9「CM-57～60 的 WN 部分」(`:256`)，三句在计划中各恰好 1 命中。

### 5.6 `bugs.md` 连带损伤：三类指针全仓归零（FAIL-3）

第一轮删掉 `bugs.md` 指针后，**遗留的编号引用变成了新错误引用**——这是删除指针本身制造的连带损伤，也是「四类全部归零」被推翻的真正原因。按类穷举（`git grep -cF`，路径含 `packages src src-tauri scripts .github e2e docs locales test` 及仓根）后统一处理：

| 类 | 第一轮计数 | 实际 | 处置 |
| --- | --- | --- | --- |
| `## 留待 R 回归` | 6 | **8** | 删指针，`#[ignore]` 理由串保留其自身语义；`e2e/` 实为 **1** 处（R2 更正） |
| `契约 C-4`（裸编号） | 1 | 1 | 锚到真实符号 `ops/write.rs` 的 `is_keepttl_keyword_rejection`；同处裸 `C-3` 一并标明属另一套编号 |
| `R 项 Nx` | 未列 | **5** | 5 处全部改述实质，不换编号 |
| `BUG-007 修法 3` / `BUG-007 排除项` | 未列 | 2 | 见下 |

- 计数差异（6 vs 8）本身就是「派单视野窄于仓库」的证据，**不是清单抄错**。
- **`BUG-00x` 编号空间冲突**：`BUG-0` 全仓计数是一个**会漂移**的数，跨提交出现过 472 / 476 / 486 / 492 等互斥历史值。在两个互斥数字之间挑一个继续引用，等于把一次快照写成承诺——本轮已把该数字**整段删除**，不再作为论据；需要计数时现场复算（见 §7.4 的复算命令），且**必须写明测于哪个提交**。

### 5.7 穷举中排除的合法项（复核后确认无误，登记不改）

- `packages/runtime/src/tunnel/transport.rs:86` 的「由**台账**在引用归零时且仅此时调用」——我一度把它当成悬空 actor，**复核后确认是活的**：`tunnel::ledger::Ledger` 是真实类型，`ledger.rs:322` 确有 `self.transport.close(&spec)`，且 `ledger.rs:319` 注释「计数归零 ⇒ 关闭，且**恰好一次**」与该行文档逐字对应。**不改动。**
- `.agents/skills/subagent-coordinator/SKILL.md:99` 的【留待 R 回归】是该词条的**定义处**，不是指向已删 `bugs.md` 的指针。**不改动。**
- `e2e/specs/dialog-injection.ts:19` 引用的 `docs/development/ipc-refactor-progress.md` **确已被删**（`git log` 命中 `927c1ac51 docs: remove completed ipc-refactor plan and progress ledgers`，即工作完成即销毁，符合文档纪律），同句的「R regression agent」为幻影，一并删去、只留本 spec 自身可证的模板关系。

### 5.8 新登记项：CM-60 测量窗口的 gateway 段终点，两处文档不一致

逐参数比对 `fake-runtime-fixtures.md` §11「基准 harness（CM-60）」与 `connection-management.md` §15.3 / CM-60 `:1236`：**release 构建、4 vCPU/8 GiB 单进程无数据库网络、固定 fake 命令 10 ms 虚拟时间、并发 8、预热 1000、每轮 10000 已获准且未排队、5 轮、每轮 p95 ≤10 ms nearest-rank、排队请求单独报告、不删失败样本、压力变体额度 20/控制预留 2/100 逻辑 session/队列上限 32/1000 次操作加取消/2 worker——全部一致，无数值漂移。**

唯一实质分歧是 gateway 段的**终点定义**：§15.3 写「到**派发 driver**」，`fake-runtime-fixtures.md` §11.2:560 写「**driver 收到调用并开始执行**的时刻」。

**登记不改**：这是测量窗口的规格问题，归 CM-60 轨所有；且 `p3-cm60-pressure-drain` 正在并发编辑该文件 `:849`/`:859`，距我 `:857` 仅 2 行，此时改动会抬高冲突风险（该轨的合并安全性：`MERGE_EXIT=0`、0 冲突标记、文件仍 1348 行）。

顺带核实：`platform-development-plan.md:128` 的「详见共享边界 §4.6、夹具 §6.3/§11」**交叉引用是活的、非失效**——共享边界 §4.6 在 `:438`，夹具 §11 就是「基准 harness（CM-60）」在 `:541`。不属本轨待修项。

### 5.9 穷举发现的第三套 `BUG-00x` 编号空间：已整体删除的 `test/bugs/`

`docs/architecture/testing.md` §6 以**现在时**描述 `test/` 目录，并画出一棵含 `bugs/BUG-001.md ~ BUG-008.md` 的树。复核结论：

- `test/` **确实曾经入库**（`git log --all -- 'test/*'` 有历史），删除发生在 2026-08 的三个提交：`33860f548 test: remove obsolete bug report docs and outdated screenshots`（删 `bugs/BUG-001..008.md` 与 `screenshots/`，共 16 文件 1042 行）、`a18d45c6c remove outdate files`、`7e3d8cbe8`。现 `git ls-files test/` = **0**，目录已整体清空。
- 这正是 `BUG-004/005/007/008` 的**来源**，也是与 `F1-BUG-005` 无关的**第三套**编号空间。

处置（**原地写事实，不换路径、不整节删除**）：

- `:16` 测试落点表该行标注「当前仓库无此目录」；
- §6 开头加状态块，写明删除事实与提交号，并把下方目录树、6.1 用例数、6.2 报告格式**一并降级为删除前的历史计划**；
- 就地声明 `test/bugs/BUG-001..008` 与现存 `F1-BUG-00x` 是两套无关编号，防止被读成同一编号；
- `docs/blogs/15-testing-strategy.zh-CN.md:14` 同类一行改为「已删除」并指回 §6。

**保留历史记录而非抹除**，是为了不替 owner 决定「手工黑盒层是否恢复」；但要让读者一眼看出这些数字描述的是已删除的存量、而非当前存量。

**登记不改**：`AGENTS.md:13` 与 `:60` 同样声称 `test/` 存在，但 AGENTS.md 是跨轨工作约定、其目录树属全局事实，属主裁断范围，本轨不擅改。

---

## 六、第三轮（R2 返修）

### 6.0 本轮的根因：我是按文件清理的，不是按模式清理的

R2 FAIL-3 的指控不是「漏改」，是**方法错**：我拿到报告里的 5 处，就按文件逐个撞。

按模式枚举全族（**⚠ 本法已被本轨自己判定为不成立，勿再沿用，替代方案见 §7.1**）：

```
git grep -n 'ops_tree_scan/\|ops_workbench/\|ops_exec\.rs\|redis_driver_on\.rs' -- packages/drivers/redis/
```

> **为什么这法是错的（BLOCKER-1 的真正根因）**：它做的是**前缀枚举**——把 `ops_workbench/` 这种「目录尾部带斜杠的写法」当成模式。但引用是按**形态**出现的，不是按「是不是某个前缀」出现的。后果有二：
>
> 1. **只会命中我脑子里已有的写法**，写形态不同就漏；
> 2. **更致命：它把 `ops/workbench/tests.rs` 这样的引用也一并扫进去，连带扫进不存在的 `workbench/tests.rs`**——同一张表里 `workbench/transport.rs`、`ops/write.rs` 确实存在，被混为一谈。**前缀匹配从不问「这条路径存不存在」**，所以它**天然漏掉「往后删除」的那一半**：条目被我自己删掉后，前缀仍在，或路径根本不再匹配任何真实文件，前缀枚举都不会报警。
>
> 枚举必须按形态穷举、并对**每一条**断言「它能解析到一个真实存在的已跟踪文件」。注入只能证明可达性，永远不能证明完备性。

**实枚举 8 处，不是报告的 5 处。** 多出的 3 处只有枚举才会暴露：

| # | 位置 | 原引用 | 现引用 |
| --- | --- | --- | --- |
| 1 | `tests/tree_scan_budget.rs:20` | `src/ops_tree_scan/tests.rs` | `src/ops/tree/scan/tests.rs` |
| 2 | `tests/workbench_commands.rs:16` | `src/ops_workbench/tests.rs` | `src/ops/workbench/tests.rs` |
| 3 | `ui/console/consoleCommandBatch.ts:6` | `src/ops_exec.rs::split_redis_commands` | `src/ops/exec.rs::split_redis_commands` |
| 4 | `ui/value-editors/redisInsertStatement.ts:6` | `redis_driver_on.rs` | `driver/session.rs` |
| 5 | `ui/__tests__/redisInsertStatement.test.ts:5` | `redis_driver_on.rs` | `driver/session.rs` |
| 6 | `src/ops/workbench/tests.rs:1` | 「declared by `ops_workbench.rs`」 | ~~「declared by `ops/workbench/mod.rs` (`mod tests;`) under `#[cfg(test)]`」~~ **← 本行右列在 `58d654f0e` 写下时是假的**：`git log f1d843271..58d654f0e -- .../ops/workbench/tests.rs` 返回 **0 条提交**，该文件从未进过我的改动集，首行在 `f1d843271` / `1117fd868` / `516ead304` / HEAD 上逐字节相同。真实落点是 **`1fc0399c0`**，见 §7.0 |
| 7 | `ui/__tests__/kvBarSlotTesterGaps.test.tsx:402` | `` `ops_workbench.rs:425` `` / `` `:439` `` | `` `ops/workbench/shapes.rs` `` 的 `parse_key_info` / `unreadable_key_state`（函数名锚，去掉行号） |
| 8 | `src/ops/tree/scan/tests.rs:5` | `` `ops_workbench/tests.rs` `` | `` `ops/workbench/tests.rs` `` |

**残余 1 处，不是 0——我在 `516ead304` 的提交信息里写的「8 -> 0」是错的，本轮查实更正。**

基线 8 处的真实构成是 **7 处漂移 + 1 处合法溯源**：

```
git grep -n 'ops_tree_scan/\|ops_workbench/\|ops_exec\.rs\|redis_driver_on\.rs' -- packages/drivers/redis/
# f1d843271 -> 8 ; 1117fd868 -> 7 ; 516ead304 -> 1 ; HEAD -> 1
```

唯一幸存者 `packages/drivers/redis/src/ops/tree/scan/budget.rs:108`
是 `/// (\`8981d3078\` 的 \`redis_driver_on.rs:138\` 把 DBSIZE 读作 \`unwrap_or(0)\`)` 的**历史溯源引文**，
已核对该 commit 的 `:138` 逐字存在（`let db_size: i64 = redis::cmd("DBSIZE").query_async(conn).await.unwrap_or(0);`），
**属可解析的自锚点，不改**。所以本轮实际是 **8 -> 1**，那 1 处本就不该改。

**这正是 FAIL-3 指控的同一形态在我自己身上的复发**：我在修复 FAIL-3 的那个提交里
写了一个没有复算的 0。同一条命令、同一个形状，只要我先跑一遍就会看到 1。


**为什么这 8 处是漂移、另有 11 处裸模块名不是**——两类的区别是**形状**不是**前缀**：

- Rust 里裸写的 `ops_workbench` / `ops_tree_scan` / `ops_exec` 是**模块标识符**，
  经模块系统解析即成立，**合法**，不算死引用；
- 只有**长得像文件**的记法（`ops_workbench.rs`、`src/ops_exec.rs`、`redis_driver_on.rs`）
  才是漂移候选。上面的命令按**形状**匹配（带 `/`、带 `.rs` 后缀或带 `src/` 前缀），
  所以只命中这 8 处。

`redis_driver_on.rs` 的替换目标 `driver/session.rs` **不是猜的**：提交 `80fadb848`
把它做了 `packages/drivers/redis/src/{redis_driver_on.rs => driver/session.rs}` 的改名，
`key_value_json` 在改名前后**都在 :228**，属同一文件改名而非另找。

**注入证伪**：往 tracked 文件 `ops/workbench/tests.rs` 注入一行同形状引用
（``// mutation probe: see `src/ops_exec.rs` ``），命中数 **1 -> 2**（落在 `:302`），
`git checkout` 复原后回到 1。命令确实在工作——这正是它没能被信任的原因：
同样的命令、同样的形状，一次报 0、一次报 1，差别只在**跑之前有没有复算**。

### 6.1 FAIL-4：`BUG-00x` 不是一套全局编号（14 套并存）

我上一轮写的 `docs/architecture/testing.md` blockquote 说了假话：它只承认两套编号空间
（已删的 `bugs/BUG-001..008` + `F1-BUG-00x`），等于暗示裸写 `BUG-00x` 就是那套已删的。

```
git grep -ohE '[A-Za-z0-9_.-]+-BUG-[0-9]+' -- . ':!progress.md' ':!hub.md' \
  | sed -E 's/-[0-9]+$//' | sort -u | wc -l          # -> 14 套活跃系列
git grep -ohE '(^|[^A-Za-z0-9_.-])BUG-[0-9]+' -- . ':!progress.md' ':!hub.md' | wc -l   # -> 367 处裸写
```

14 套：`F1-` `F3-` `R2-` `cap-bridge-` `e2e-ops-menu-` `ep-hooks-settings-` `pre-`
`redis-codec-write-` `redis-detail-ui-` `redis-kvbar-ui-` `redis-tree-backend-`
（**复算 15 套 / 141 处带前缀**，测于 `95a60bea5`；第 15 套是本轮新增的 `redis-workbench-`，另见 §7.3）
其中 `pre-BUG` 的 2 处是子串产物，不是独立系列，登记不改。

新表述：**裸写 `BUG-00x` 不可被假定为已删除那套**，引用必须连前缀写全。
`BUG-007` 单是横跨 **9 个文件**、涉及至少 3 套系列，裸编号根本不足以定位。

**零值证伪**：注入 `redis-mutation-series-BUG-900` 后系列数 **14 -> 15**，复原回 14。

同时按族修掉 `ops/write/tests.rs` 全部 4 处 `BUG-004`（**不止报告的 1 处**）：
`:350` 裸写、`:360` 已有前缀、`:381` 裸写、`:400` 裸写——3 处是裸写。
系列名取自 `:360` 既有的 `#[ignore]` 理由串本身，不另起命名。现在该文件裸写残留 0。

### 6.2 本轮新增的两条纪律

**(1) 坏命令会伪造出一个 0。** 第一次跑 14 的复算命令时我把它塞进 shell 变量，
嵌套引号写坏后输出 `0`。那个 0 来自**坏管道**，不是仓库事实。差点把「命令坏了」
当成「仓库没有」记进台账——这正是「我 grep 出来的结论 ≠ 仓库的结论」的又一种形态。
**规则：任何计数为 0 时，先跑裸命令确认命令本身能出非零，再采信这个 0。**

**(2) 我自己造了一个假缺陷。** 复核 `tunnel/mod.rs:21` 的 `:589` 锚点时，我在
**错误的工作目录**下用**漏掉 `platform/` 的路径**测 `[ -e ... ]`，得到 `ABSENT`，
差点把 `shared-boundaries-and-ports.md` 报成死引用。在正确路径下实测：
`:585` 是 `### 6.3 保持不动的现有服务与窄适配`、`:589` 确含「过渡实现，逐 consumer 迁移后删除旧路径」、
`:590` 确含「隧道生命周期通过 `NetworkProvider::ensure_tunnel` 的桌面实现承接」——**三处锚点全对，是活的**。
**规则：报告缺陷前必须同时确认「工作目录」和「路径」两项。**

### 6.3 登记不改（本轮新发现，均不在派单的 6 项内）

- `src-tauri/src/commands/cm73_baseline_tests.rs` 现在**并存两套互斥锚点体系**
（`CM-73` 条目锚，15 处 / `§16.7` 节号锚，1 处；`:NNN` 形式的**行号锚为 0 处**——`grep -cE ':[0-9]{3,4}'` = 0，且 19/23/590/661/665/670 六数只出现在注释里，是条目锚不是行号锚）
  本轮只迁移了 FAIL-1/FAIL-2 点名的那一批，**故意不做统一**：统一属结构性重构，
  会一次性改动大量断言注释，超出「纯文档」范围。
- `.superpowers/sdd/task-7-report.md:53` 的路径清单已过期，登记待 owner 处理。
- `docs/architecture/platform/shared-boundaries-and-ports.md:589` 的 `connection/adapters.rs`
  一行（原「三、2」）：复核确认为**迁移状态声明**且锚点正确，文件不存在是事实陈述，
  迁移是否完成属 owner 判断，本轨不改写。

### 6.4 R3 门禁（逐字结论行）

```
HEAD_BEFORE=516ead304e9251dadc04ee9a97c039b3b0382551   # R3 当时的工作点；该提交**不是**本节最终状态，后续还有 3 个提交（至 `9e161254e`）才落盘，见 §7.5
REDIS_TEST_EXIT=0 ; test result: ok. 396 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out
本次 Rust 改动非注释 diff 行 = 0
本次 diff 新增 .md 文件 = 0
COMMIT_EXIT=0 ; leak check grep -cE '^(APPEND_EOF|EOF|MSG)$' = 0
porcelain=0
```

`tsc` 走 `./node_modules/.bin/tsc --noEmit -p tsconfig.json`（EXIT=0，error TS 计数 0）。
**不用 `pnpm typecheck`**：本 worktree 的 pnpm wrapper 会因 hoist 目录是符号链接而无法落盘、恒退出 1，
属**工作树构造产物而非代码缺陷**（属主已裁定）；直调 `tsc` 覆盖 `src` + `packages`，
**含 `__tests__/` 与 `*.test.tsx`**，与 `pnpm typecheck` 的 tsconfig 一致。

### 6.5 「零」这份清单的适用边界（必须随台账一起被读）

本轮每一个 0 都配了「可复算命令 + 注入后计数会动」的证伪。但**证伪只在同一
枚举命令的同一形状内成立**：验证方把同形状引用注入**我清单之外的**文件，我照样归零。
所以准确表述是：**「四类别归零」对我自选清单成立，对本仓库不成立。**
仓库级的 115 处死路径 / 12 个类别属**另轨登记**，本轮不掩盖、不代改。

### 6.6 本轮每个「零」的注入证伪结果（含一个被证伪的**过度断言**）

| 断言 | 命令 | 注入 | 计数变化 |
| --- | --- | --- | --- |
| FAIL-3 漂移族已清 | `git grep -n 'ops_tree_scan/\|ops_workbench/\|ops_exec\.rs\|redis_driver_on\.rs' -- packages/drivers/redis/` | 往 tracked `ops/workbench/tests.rs` 注入一行同形状引用 | `1 -> 2`，复原回 1 |
| `BUG-00x` 活跃系列 14 套 | `git grep -ohE '[A-Za-z0-9_.-]+-BUG-[0-9]+' -- . ':!progress.md' ':!hub.md' \| sed -E 's/-[0-9]+$//' \| sort -u \| wc -l` | 注入 `redis-mutation-series-BUG-900` | `14 -> 15`，复原回 14 |
| `write/tests.rs` 裸写 `BUG-004` 已清 | 同上裸写族（限该文件） | 该文件内加一处裸写 | 会重新出现 |
| `test/` 树已整体删除 | `git ls-files test/ \| wc -l` | `git add -N test/BUG-999-mutation-probe.ts` | `0 -> 1`，复原回 0 |
| 无 PRD 文件 | `git ls-files \| grep -ic prd` | `git add -N PRD-mutation-probe.md` | `0 -> 1`，复原回 0 |

**最后一行同时证伪了我自己那句过度断言。** 台账原文写「全仓无任何 PRD 文件
（tracked / untracked / ignored 皆无）」。实测：注入一个**未 staged** 的
`PRD-mutation-probe.md`，`git ls-files \| grep -ic prd` **仍然报 0**——
`git ls-files` 只看索引，**看不见 untracked / ignored 文件**，所以这条命令
在结构上就无法支撑「皆无」。它只能支撑「无 **tracked** PRD 文件」。

这是**第七次**同一根因：把「我的命令没查到」写成「仓库没有」。

**规则固化**：
- 负向断言（没有 X）必须先证明**命令有能力发现 X**，再引用它的 0；
  能力证明 = 注入一个同类 X，看计数是否动。
- 「untracked / ignored 皆无」这类断言，`git ls-files` 系列命令**永远无法给出**；
  要覆盖只能换成 `find`（但本轨无权遍历忽略目录）——**故此类断言一律降级为
  「无 tracked X」，不写更强的说法。**
本轮逐个 0 已按此重验（**逐条**结论见 §6.6；不再用一句总括覆盖全部 0）

## 七、第四轮（R3 返修）

本节每条结论都注明**测于哪个提交**。凡是会漂移的计数，一律给复算命令而不给承诺值。

### 7.0 BLOCKER-1：`tests.rs:1` 的悬空模块名（落点 `1fc0399c0`）

`packages/drivers/redis/src/ops/workbench/tests.rs:1` 原写「declared by `ops_workbench.rs`」，
而该文件不存在。实测（`git ls-files` 口径）：

| 候选 | 存在 |
| --- | --- |
| `src/ops_workbench.rs` | ✗ |
| `src/ops/workbench.rs` | ✗ |
| `src/ops/workbench/mod.rs` | ✓，`:199-200` 持 `#[cfg(test)]` + `mod tests;` |

`ops_workbench` 是**重组前的旧模块名**；`ops/mod.rs:28` 现声明 `pub mod workbench;`。
首行已改为描述真实的声明方式，**只动注释，6 行改 6 行，行为变化 0**。

#### 7.0.1 顺带查实的**第 10 类缺陷：台账声称已改、代码未改

§6.0 那张表的第 6 行在 `58d654f0e` 里写下「已改为引用 `ops/workbench/mod.rs`」，但

```
git log f1d843271..HEAD -- packages/drivers/redis/src/ops/workbench/tests.rs
```

返回 **0 条提交**——该文件从未进过我的改动集，首行自 `f1d843271` 起逐字节未变。

这与此前九次「我 grep 出来的结论 ≠ 仓库的结论」**不同类**：那九次错在**结论**，
这一次错在**记录了一条从未发生的改动**。台账是缺陷结论的下游输入，
一旦它声称的修复不存在，下游就会跳过这一项——缺陷就此从账上消失而代码仍然坏着。
**故此类错误必须由真实提交纠正，不能靠把台账那行删掉了事。**

### 7.1 BLOCKER-1b：新枚举形态——**按形态穷举 + 逐条存在性断言**

父任务书问：「新枚举形态到底是什么，怎么保证删条目也能被抓」。回答如下。

**旧法（§6.0）**：按「目录尾部带斜杠的写法」grep 模式。

**新法**，两步：

1. **按形态穷举**：从改动集里用正则抽出**每一个**形如
   `path/like/this.{rs,tsx,ts,mjs,json,md}` 的 token（`TOKEN` 正则）。
   枚举的是**形态**，不是我记忆里的写法——这一步保证「没有漏形态」。
2. **逐条存在性断言**：每个 token 按**路径后缀**去 `git ls-files` 的已跟踪集合里解析，
   断言它能落到一个**真实存在的已跟踪文件**上。

**为什么这一形态能抓到「删除」**：断言的内容是「抽出的每个 token 都能解析到一个已跟踪路径」。
一旦某个 token 指向的文件被改名或删除，解析立刻失败，计数就动。
**前缀匹配从不问「这条路径存不存在」**，所以它对删除方向天然是瞎的——
**一个只测「往后新增」而不测「往后删除」的守卫，会漏掉自己删掉的条目。**

三处实现选择，都是被自己的错误逼出来的：

| 选择 | 若不这么做会怎样 |
| --- | --- |
| 存在性口径 = **`git ls-files`（索引）**，不用 `os.path.exists` | `os.path.exists` 连 gitignored 产物（`src/extensions/generated.ts`）一起看见。热工作树里有、干净工作树里没有 ⇒ **计数依赖工作树状态**：同一工具两次跑出 **10 vs 11** |
| 解析 = **后缀匹配**，`ROOTS = []`，不写根目录清单 | 我第一版手写根目录，**凭空造出 4 个假缺陷**（`protocol/*.rs` 实为 `src-tauri/src/ai/protocol/*.rs`；`barrier/tests.rs` 实为 `packages/runtime/src/connection/testing/barrier/tests.rs`）。**制造假缺陷，就是重复我自己历史上的错误** |
| 过滤 URL 片段只认 `://`，不认 `//` | 我原按 `//` 过滤，误杀了前面是行注释的真引用，并且**静默丢了 `cluster_async/request.rs`、行号整体位移** |

**基线（测于 `1fc0399c0`，可复现）**：

```
changed_files=55   unresolved_tokens=11   unresolved_distinct=7
```

同一命令在**两棵不同工作树**上跑出**逐字节相同**的输出——因为口径是索引，不是文件系统。

11 个 token / 7 个去重值的归属（**必须逐条分类，否则一个 7 就是无意义的数**）：

| 类别 | 处 | 说明 |
| --- | --- | --- |
| **真死引用** | 3（`cluster_topology/mod.rs:9/:10/:27` 的 `cluster_async/*`） | `f1d843271` 即已存在，在我碰过的文件里。登记见 §7.7 |
| 已声明例外 | 1（`testing.md:221` 的 `bugs/BUG-001.md`） | 我**故意引用**那套已删除的编号来论证「裸编号不可假定」，带理由登记，保留这个 0 才有意义 |
| 既有问题 | 1（`e2e-ipc-migration-guide.md:37` 的 `src-tauri/tests/my_ipc_migration.rs`） | 非本轨引入 |
| 产物/非路径 | 2（`ulid.rs:231` 的 `ulid.io/data.json` 是注释里引 ULID 规范 URL；`generated.ts` 是 gitignored codegen） | 不是仓库路径 |

#### 7.1.1 删除方向的注入探针（**在独立 detached 工作树里做**）

`AGENTS.md`：「同一棵工作树不得同时被提交方和验证方使用」，故探针用
`git worktree add --detach /tmp/dz_probe2 1fc0399c0`：

| 步骤 | unresolved_distinct |
| --- | --- |
| 干净基线 | **7** |
| `git mv .../ops/workbench/shapes.rs .../ops/workbench/shapes_renamed.rs` | **8** |

新增的那一条正是 `packages/drivers/redis/ui/__tests__/kvBarSlotTesterGaps.test.tsx:402  ops/workbench/shapes.rs`。

**探针证明的是「这条命令确实能因删除而报警」，不能证明「基线那 7 真的是 7 个真缺陷」。**
两者是两件事，别混。探针跑完即 `git reset --hard` + `git worktree remove --force`。

#### 7.1.2 本检查器**自身的盲区**（必须随结论一起被读）

TOKEN 正则要求 token **带文件后缀**，因此**裸模块名对它是不可见的**。
`ops_workbench` 正是这类：它没有后缀，是重组前的**模块名**，
旧名字在散文里照样是死引用——而这条正则永远看不见它。

实测 `ops_workbench` 在本仓 **7 个文件**中作为陈旧模块名出现，其中 **2 处在我的改动集内**
（`ops/tree/scan/budget.rs:101`、`cluster_topology/mod.rs:1`），另 5 处不在。
**因此本节不是一次干净的全仓清扫，不许这样引用。**

### 7.2 BLOCKER-2：裸 `BUG-00x` 必须连前缀写全

裁定口径（由协调方裁定）：**裸编号没有可分辨性 ⇒ 零指代力 ⇒ 必须带系统前缀。**
证据不是推演：`BUG-004` 全仓 **19 处 / 10 个文件**，分属 **5 套**活跃系列；
另有 `test/bugs/BUG-004.md`（已于 `6c0cd1ed0` 删除）是**第三个**互不相干的缺陷
（AI NL2SQL「应用到编辑器」写入完整推理文本）。**裸编号连「指向哪一套」都答不出。**

本轮**已在改动集内修掉 9 处**（5 处 kvBar + 4 处 workbench）：

- `ui/__tests__/kvBarRound1Fixes.test.tsx` / `kvBarSlotTesterGaps.test.tsx` ⇒ **`redis-kvbar-ui-BUG-004`**。
  该系列已被同目录另外 5 个 `kvBar*.test.ts*` 的前缀锁定；`redis-detail-ui-BUG-004` 可证是 TTL pill 行内编辑（另一个缺陷）。
- `ops/workbench/tests/fix_round1.rs`（3 处）、`ops/workbench/tests.rs:277` ⇒ **`redis-workbench-BUG-004`**。
  **⚠ 这套前缀是按仓库命名约定 `coordination/tracks/<track>/bugs/<series>-BUG-NNN.md` 补的，
  不是从登记表查出来的——本系列目前没有已入库的 `bugs/` 登记表，`testing.md:224-226` 列出的 14 套也不覆盖 `ops/workbench/`。
  这是本轮唯一的判断题，请复核时重点看它。**

**清理范围与口径必须分开写死**：

- **口径不变**：引用缺陷编号**必须连系统前缀写全**，这条对全仓成立，本轨不改。
- **清理范围有限**：本轮只清理**本轨改动集内**被派单点名的 9 处。
  范围写窄**不等于**口径放宽——范围是「这一轮动了哪些」，口径是「该怎么写」，
  两者混成一句，就会让「没改到」被读成「不用改」。§7.7 逐项登记了范围外的余量。

**披露：本轮我自己引入了一个新系列。** 前缀化使全仓带前缀处数 **141**、系列 **15**，
`redis-workbench-` 是第 15 套；`testing.md` 里「14 套 / 367 处」随之失效，已按 §7.4 复算更新。

**⚠ 其中 2 处是 `describe()` 标题，故 vitest 的用例 ID 变了**（用例名，非行为）。
早先「行为变化 0」的表述对这两处不成立，特此更正。

### 7.3 §6.1 的两处失效值：14 套 / 124 处

已改为**复算值 15 套 / 141 处**，测于 `1fc0399c0`，并给出复算命令（§7.4）。
计数漂移本身就是一个缺陷类别：**标时点，不承诺永久成立**。

### 7.4 复算命令（复算值与文中不符时以复算为准）

```bash
# 系列数（期望 15）
git grep -ohE '[A-Za-z0-9_.-]+-BUG-[0-9]+' -- ':!progress.md' ':!hub.md' \
  | sed -E 's/-[0-9]+$//' | sort -u | wc -l
# 带前缀处数（期望 141）
git grep -ohE '[A-Za-z0-9_.-]+-BUG-[0-9]+' -- ':!progress.md' ':!hub.md' | wc -l
# 裸编号处数（期望 354）
git grep -ohE '(^|[^A-Za-z0-9_.-])BUG-[0-9]+' -- ':!progress.md' ':!hub.md' | wc -l
```

台账自身引用的正是这些命令会匹配到的字面量，故**这三条命令永远带 `:!progress.md` ':!hub.md'`**。

### 7.5 WARN-6「性能门禁在单实例形态下测量」被误判为误删 —— **撤回该指控**

评审要求把该句恢复到 `docs/architecture/platform/team-server-and-auth.md:780`。实测：

```
git log --oneline f1d843271..HEAD -- docs/architecture/platform/team-server-and-auth.md   # 0 条
git diff f1d843271 HEAD --numstat -- .../team-server-and-auth.md                        # 空
git show f1d843271:.../team-server-and-auth.md | grep -c '性能门禁在单实例形态下测量'    # 1
git show HEAD:.../team-server-and-auth.md      | grep -c '性能门禁在单实例形态下测量'    # 1
```

**该文件根本不在我的改动集里**，该句在基线与 HEAD 上逐字相同、各 1 处——**从未被删，无需恢复**。
若照单执行，反而会在一个我从未碰过的文件里制造一次无来由的改动。

方向与前九次相反：那九次是**我的结论 ≠ 仓库**；这一次是**评审的结论 ≠ 仓库**。
两者的共同教训不变：**结论必须先落成一条可复现的命令，再决定动不动手。**

### 7.6 R4 门禁（逐字结论行）

四条门禁**各只跑一次**，运行期间不编辑；首尾各记一次 HEAD 与工作区状态，运行期间两者均未变。

```
HEAD_BEFORE=6a0a32d7083d9caf9957a1bf908c13e41d5f08b6
PORCELAIN_BEFORE= M progress.md          # 仅台账，无源码

FMT_EXIT=0 ; FMT_BYTES=0
TSC_EXIT=0 ; TSC_ERRORS=0

CARGO_EXIT=0 ; 5 个 test result 行
  test result: ok. 396 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out
  test result: ok. 4 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out
  test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
  test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
  test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
  => 合计 408 passed / 0 failed / 8 ignored

VITEST_EXIT=1
  Test Files  1 failed | 564 passed (565)
  Tests  2 failed | 5918 passed (5920)

HEAD_AFTER=6a0a32d7083d9caf9957a1bf908c13e41d5f08b6
PORCELAIN_AFTER= M progress.md
```

**更正 R3 记录里的一处口径错误**：R3 只报了第一个二进制的 `396 passed; 0 failed; 3 ignored`，
把它当成整个 crate 的结论。实际是 **5 个二进制合计 408 passed / 0 failed / 8 ignored**。
二进制数不等于 `Running` 行数（前者 5，后者 4），所以别用 `Running` 计数代替结论。

#### 7.6.1 vitest 是**真红**，不解释为通过

失败文件：`scripts/__tests__/check-module-layers.test.ts`，2 个用例
（`reports every file that imports Tauri, and none that does not` /
`says out loud that its findings are non-blocking`），断言 `expect(run().code).toBe(0)` 实得 **1**。

**该红与本轨无关**，判据如下（全部可复算，不依赖本轨的叙述）：

| 证据 | 结果 |
| --- | --- |
| 本轨改动集里有没有 `scripts/` | **没有**（`git diff f1d843271 HEAD --name-only \| grep '^scripts/'` 空） |
| 被点名的文件在本轨改动集里吗 | **没有** |
| 那条 `@tauri-apps/api/core` import 何时落地 | `92a039383`，且 `git merge-base --is-ancestor 92a039383 f1d843271` 成立 ⇒ **早于本轨基线** |
| 本轨新增文件是否进入该守卫的文件集 | 本轨只新增分支根台账 `progress.md`，守卫自报「84 files examined」，`progress.md` **不在其中** |

守卫自报：`packages/driver-sdk/src/ipc/driverCommands.ts:1` 命中
`driver-sdk-no-direct-tauri`（advisory）。

**结论：这条红不是本轨造成的，但它是真实的红，本轨不改它、也不假装它是绿的。**
不在派单范围内，登记待 `driver-sdk` owner 处理。

#### 7.6.2 `pnpm` 在本工作树里是假红，不作判据

`pnpm vitest run` 跑 0 个用例却退出 1，`pnpm typecheck` 恒退 1。二者既非通过也非失败，
本轮的门禁一律用 `node <主仓>/node_modules/vitest/vitest.mjs run` 与
`./node_modules/.bin/tsc --noEmit -p tsconfig.json`。

#### 7.6.3 门禁的适用边界

这四条只覆盖**本轨的改动面**。全仓的门禁结论不由本轨推出——包括上面那条 vitest 红：
它既证明不了「其余 564 个文件没问题」，也证明不了「本轨的文档改动让它们过了」。
**结论的范围由命令的读表面决定，不是由「我跑过了」决定。**

### 7.7 登记不改（本轮新查出，均不在 R4 派单范围内）

派单明确要求**不把 115 处死路径 / 12 个类别、或 `BUG-00x` 全量普查折进本轨**，故以下只登记：

| 项 | 实测 | 处置 |
| --- | --- | --- |
| `cluster_topology/mod.rs:9/:10/:27` 引用 `cluster_async/{mod,routing,request}.rs` | 全仓无 `cluster_async/` 目录；但 `ClusterConnection`(23 命中) / `RebuildSlots`(6) / `for_routable`(14) / `keyed_probe_slot`(5) / `table_route`(11) 等标识符**均仍在** | **散文的行为描述很可能仍为真，死的只是路径**。修法不得拿另一个死文件名顶上；判据是「删掉这行之后这段注释还说得通吗」。`f1d843271` 即已存在，登记待 owner |
| 陈旧模块名 `ops_workbench` 散落 7 个文件 | 2 处在改动集内（`ops/tree/scan/budget.rs:101`、`cluster_topology/mod.rs:1`）、5 处不在 | 本轮只改了与 BLOCKER-1 同族的那处（`cluster_topology/mod.rs:1` 的头部亦在 §7.7 登记范围内）。余下登记 |
| 改动集内仍存 **50 处**裸 `BUG-0xx` | 分布在 `ops/workbench/**`、`ops/tree/scan/**`、4 个 kvBar 测试等 | 属「全量普查」范畴，**本轨不折入**；派单点名的 9 处已修（§7.2） |
| `docs/architecture/platform/team-server-and-auth.md` | **不在改动集内** | 该文件的路径、句柄数等**一律不由本轨的结论推出**；引用它之前请先确认它是否在改动集里 |