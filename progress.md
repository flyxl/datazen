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
| `git grep -n 'progress\.md' HEAD -- packages src src-tauri scripts .github` | 0 命中 | — |

**作用域自证**：对全部 `*.rs / *.ts / *.tsx / *.mjs` 改动行做过滤，剔注释前缀后**剩余 0 行**——即本轨每一处代码改动都是注释 / 文档注释 / `#[ignore]` 理由串，无测试逻辑、断言或运行时行为变更。

### 关于文档内既有的测试计数

`platform-development-plan.md:341` 的 `139 passed`、`:343` 的 `19 个单测` 与今日实测 `436` 不符。这两处均自带时点标注「计数与退出码测于 `5b7c5b49c`」，属**自标注的历史实测值而非失效引用**，故未改；今日 436 记录于本台账备查。

## 三、遗留与待裁定项（本轨未擅自改写）

1. **`PRD §` 共 195 处、跨 126 文件，全仓无任何 PRD 文件**（tracked / untracked / ignored 皆无）。计数方法可复现：`git grep -o 'PRD §' | wc -l` = 195；`git grep -l 'PRD §' | wc -l` = 126；`git ls-files | grep -ic prd` = 0。占 tracked 文件 3490 个中的 3.6%。规模远超单点引用，且每处的「§」所指内容已无法复原，**无据可写**；须由各轨文档 owner 决定是就地改写为已实现事实还是删节。（第一轮此处记为 192/124，第二轮按上述方法重新计数修正。）
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

### 5.2 「零运行时代码改动」这句本身是假的（BLOCKER C）

第一轮提交信息写「纯文档，零运行时代码改动」。逐行复核：**实际有 2 处非注释改动**，各是一个 `#[ignore = "…"]` 理由串（`packages/runtime/tests/cm70_no_disk.rs:3`、`cm70_idempotency_replay.rs:3`）。经裁定**保留这两处字符串**（它们只改测试元数据，不改变任何测试是否执行），但那句话字面为假——纠正已写入第二轮提交信息本身，而不是靠本台账补记。

### 5.3 锚点必须成对验证（BLOCKER A 的教训）

我把 `cm70_no_disk.rs:3` / `cm70_idempotency_replay.rs:3` 的 `§16.6` 改成 `§16.7` 反而改错了：**CM-70 实际在 `connection-management.md:1294`，落在 `### 16.7`（1238–1325）内，`### 16.6` 只占 1193–1237**。两文件 numstat 行数中性，说明**原来的行号锚点本来就是对的**。

由此确立本轨的锚点判据（已适用于全部锚点）：**节号存在，且被引标题自身的行号落在该节区间内，两个条件同时成立才算解析成功；不得由邻近路标反推节号。** 推论：行数中性的改写，通常意味着旧行号原本正确。

据此全仓复扫 `connection-management\.md:[0-9]`，剩余 3 处全部是**登记不改动**的正确锚点（`tunnel/journey_sharing.rs:2`→CM-32、`tunnel/mod.rs:23`→CM-27、`tunnel/transport.rs:90`→`networkRouteRevision` @676），`:1284`/`:1285`/`:1307`/§16.6 全仓归零。

### 5.4 BLOCKER B 实为 5 处，不是 2 处

`src-tauri/src/commands/cm73_baseline_tests.rs` 里派单点了 `:19`、`:23`，我自己 grep 出还有 `:590`、`:661`、`:664`（另 `:661-662` 内还嵌着一个 `:1307`）。5 处全部由**行号锚点**改为 **CM-73 自身的 bullet 名**（`- 基线说明` / `- 保留声明`）。round-1 的 `:669` `#[ignore]` 串本已是正确写法。

**这条本身就是「必须按类穷举而非按清单逐条修」的证据**：派单给 2，我的清单给 2，实际 5。

### 5.5 引用保真：造引文与「证明不存在」的坑（FAIL-2）

- `:857` 原句里「性能门禁在单实例形态下测量」**在开发计划里根本不存在**（`性能门禁`、`单实例形态` 全仓 0 命中）——已删该半句。
- **同类的 `:1236` 从未被点名，却带同一处造引文**，一并修掉。这印证「未被质疑 ≠ 正确」。
- **我第一轮说 `基准口径` 不存在，这个说法是错的**：它确实存在，恰好 1 处，在 `platform-development-plan.md:128`（不在 `:124` 退出门槛那一行，且与 CM-60 无关联）。第一轮 `:857` 那句把 `:124` 与 `:128` 焊成了一句假合并。教训：**证明「不存在」必须对正确的文件做精确串 grep，而「我 grep 出来的结论」不是「仓库的结论」。**
- 改写后只用三段**逐字可核**的退出门槛原文：P3「60 单机部分」+「H 断言」(`:124`)、P7「60 单实例部分」+「W1 断言」(`:211`)、P9「CM-57～60 的 WN 部分」(`:256`)，三句在计划中各恰好 1 命中。

### 5.6 `bugs.md` 连带损伤：三类指针全仓归零（FAIL-3）

第一轮删掉 `bugs.md` 指针后，**遗留的编号引用变成了新错误引用**——这是删除指针本身制造的连带损伤，也是「四类全部归零」被推翻的真正原因。按类穷举（`git grep -cF`，路径含 `packages src src-tauri scripts .github e2e docs locales test` 及仓根）后统一处理：

| 类 | 第一轮计数 | 实际 | 处置 |
| --- | --- | --- | --- |
| `## 留待 R 回归` | 6 | **8** | 删指针，`#[ignore]` 理由串保留其自身语义；另 2 处在 `e2e/`（派单视野外） |
| `契约 C-4`（裸编号） | 1 | 1 | 锚到真实符号 `ops/write.rs` 的 `is_keepttl_keyword_rejection`；同处裸 `C-3` 一并标明属另一套编号 |
| `R 项 Nx` | 未列 | **5** | 5 处全部改述实质，不换编号 |
| `BUG-007 修法 3` / `BUG-007 排除项` | 未列 | 2 | 见下 |

- 计数差异（6 vs 8）本身就是「派单视野窄于仓库」的证据，**不是清单抄错**。
- **`BUG-00x` 编号空间冲突**：全仓 `BUG-0` 共 472 处，分属 i18n-drivers / mysql / postgres / driver-api / driver-api-dependency-boundary / e2e 等**其他轨道的活跃编号**，且都早于 `f1d843271`；改写它们等于擅自改写他人轨道的编号体系。故义务边界限定为**本轨改动集内**的裸编号：`cluster_topology/mod.rs:74`、`:94` 两处已改为「…spelled out / recorded at the top of this file」，指向同文件 `:12-20` 的本轨 `BUG-001/002/007` 自有系列——**就地声明了指的是哪套编号**，而不是留一个裸 `BUG-007` 与 `F1-BUG-005` 系列撞车。

### 5.7 穷举中排除的合法项（复核后确认无误，登记不改）

- `packages/runtime/src/tunnel/transport.rs:86` 的「由**台账**在引用归零时且仅此时调用」——我一度把它当成悬空 actor，**复核后确认是活的**：`tunnel::ledger::Ledger` 是真实类型，`ledger.rs:322` 确有 `self.transport.close(&spec)`，且 `ledger.rs:319` 注释「计数归零 ⇒ 关闭，且**恰好一次**」与该行文档逐字对应。**不改动。**
- `.agents/skills/subagent-coordinator/SKILL.md:99` 的【留待 R 回归】是该词条的**定义处**，不是指向已删 `bugs.md` 的指针。**不改动。**
- `e2e/specs/dialog-injection.ts:19` 引用的 `docs/development/ipc-refactor-progress.md` **确已被删**（`git log` 命中 `927c1ac51 docs: remove completed ipc-refactor plan and progress ledgers`，即工作完成即销毁，符合文档纪律），同句的「R regression agent」为幻影，一并删去、只留本 spec 自身可证的模板关系。

### 5.8 新登记项：CM-60 测量窗口的 gateway 段终点，两处文档不一致

逐参数比对 `fake-runtime-fixtures.md` §11「基准 harness（CM-60）」与 `connection-management.md` §15.3 / CM-60 `:1236`：**release 构建、4 vCPU/8 GiB 单进程无数据库网络、固定 fake 命令 10 ms 虚拟时间、并发 8、预热 1000、每轮 10000 已获准且未排队、5 轮、每轮 p95 ≤10 ms nearest-rank、排队请求单独报告、不删失败样本、压力变体额度 20/控制预留 2/100 逻辑 session/队列上限 32/1000 次操作加取消/2 worker——全部一致，无数值漂移。**

唯一实质分歧是 gateway 段的**终点定义**：§15.3 写「到**派发 driver**」，`fake-runtime-fixtures.md` §11.2:560 写「**driver 收到调用并开始执行**的时刻」。

**登记不改**：这是测量窗口的规格问题，归 CM-60 轨所有；且 `p3-cm60-pressure-drain` 正在并发编辑该文件 `:849`/`:859`，距我 `:857` 仅 2 行，此时改动会抬高冲突风险（该轨的合并安全性此前已实测：`MERGE_EXIT=0`、0 冲突标记、文件仍 1348 行）。

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
