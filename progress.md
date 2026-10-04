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
4. **Task D 作用域大于派单所述**：除 9 处 `progress.md` 外，另有 `bugs.md`（6 处）、`## 契约冻结`（7 处）、`## 自验记录`（3 处）同类失效引用，**全部按同一判据原地写成事实，无一处只换文件名**。现四类全部归零。

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

1. **`PRD §` 共 192 处、跨 124 文件，全仓无任何 PRD 文件**（tracked / untracked / ignored 皆无）。规模远超单点引用，且每处的「§」所指内容已无法复原，**无据可写**；须由各轨文档 owner 决定是就地改写为已实现事实还是删节。
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
