# 子代理并行开发 Playbook

> 来源：2026-08 IPC 重构实战沉淀，本手册是该过程的工程化展开与运行规程。
> 适用：多功能、长周期需求，由主会话代理担任**协调者**，派发子代理完成开发与测试。

## 0. 角色模型

| 角色 | 职责 | 硬性约束 |
|------|------|---------|
| **协调者**（主会话代理） | 拆功能、写简报、派发、活性监控、bug 流转裁决、跨轨合并、进度台账 | 不直接写业务代码；每个关键节点向用户同步 |
| **编码子代理** | 单功能实现 + 单元测试 + 自验三件套 + 更新进度文件 + commit | 每个功能**全新实例**；只在自己轨道 worktree 工作 |
| **测试子代理** | 独立复验（重跑套件、覆盖率、E2E 用例设计、bug 登记） | **必须全新实例，禁止复用编码代理**；只测不修；不信任编码轮自报数字 |
| **修复/接管代理** | bug 修复轮 / 中断收尾 | 同样全新实例或原代理续跑（见 §4） |

## 1. 单功能循环与 bug 状态机

```
编码(+单测) → commit → 测试 ─┬→ 通过 → 功能「已完成」→ commit
                            └→ 不通过 → bug 登记「待验证」→ 协调者置「验证不通过」+commit
                                      → 修复代理修复 → 「待验证」+commit
                                      → 全新测试代理复验 → 通过：「已修复」+功能「已完成」+commit
```

- 每一次状态流转都要单独 commit（可追溯）。
- 测试不通过的判定权在测试代理；协调者只做归属裁决（如：既有缺陷是否并入本功能范围——默认不并入已关闭功能，立独立循环）。

## 2. 并行波次编排

1. **分轨依据是文件冲突面，不是功能编号相邻**。逐对检查触碰面：仅 lib.rs 注册块不同行 → 可并行；共享前端文件或存在模式复用软依赖 → 必须串行。
2. **每轨独立 worktree + 分支**：`scripts/new-feature-worktree.sh <track> [base]`（自动完成 node_modules 软链、驱动 codegen、未跟踪规格文档拷贝、e2e/.env 拷贝）。脚本将 worktree 放在主检出下的 `.worktrees/`，以确保代理沙箱对目标路径可写；不要手工创建同级 `../datazen-*` worktree。
3. **已知合并冲突点与策略**：
   - `lib.rs` invoke_handler 注册块：各轨删改不同行，合并基本自动；
   - 进度管理文件：**各代理只允许写自己功能的小节**，总览表/台账冲突由协调者融合双方记录；
   - 共享守护测试（如 pathIpcWiring.test.ts）：以分支基线为准逐轨叠加，合并时人工核对断言并集。
4. **合并时机**：某轨测试闭环后即可合回集成分支（不必等全部），合并后跑 `tsc --noEmit` + 定向 vitest 健全性检查。
5. **全量回归（R 阶段）在全部轨道合并后统一执行**；E2E 用例在各功能测试轮只做设计与登记，标注【留待 R 回归】。

### 2.1 主线保护与协调锁

真实执行中最危险的失败模式不是代码冲突，而是代理误用主检出、多个代理在同一目录同时提交，最终把 merge/cherry-pick 冲突留在协调者工作树。因此增加以下硬性规则：

1. **编码代理和测试代理禁止使用主检出或 same-directory fork**。必须在 `<main-checkout>/.worktrees/<track>` 中工作；worktree 创建失败时只能上报阻塞并等待迁移，不能降级到主检出继续写代码。
2. 代理启动后必须自检并在首次汇报中报告：`pwd`、`git rev-parse --show-toplevel`、`git branch --show-current`、`git status --short`。发现当前分支为 `main`、路径不是分配的 worktree，或 worktree 不干净且改动归属不明，立即停止写入。
3. **只有协调者可以** merge、cherry-pick、rebase、修改 hub、删除 worktree/分支。编码/测试代理只能提交自己的业务文件和本轨台账，禁止替协调者改主线状态。
4. 同一 track 同时只能有一个写代理；测试代理使用独立 worktree/分支，不能与编码代理共享可写目录。协调者合并期间暂停该 track 的所有写操作。
5. 协调者为每个活动 track 维护“写锁”记录：代理 ID、worktree、分支、阶段、最后心跳时间。没有写锁的代理不得写仓库；写锁释放前不得清理 worktree。

如果编排工具不支持创建项目 worktree，必须使用 `scripts/new-feature-worktree.sh` 手工 bootstrap 后再派发；仍无法建立隔离目录时，任务进入「阻塞」，不允许用同目录代理冒充隔离开发。

## 3. 子代理生命周期协议

### 3.1 死亡恢复（重要度最高）
子代理可能因模型额度耗尽等原因**静默死亡**（failed before finish 且无遗言）：

- **死亡次数 ≤3：一律 `send_message(id, "继续")` 让原代理续跑**——它保留完整上下文与已有编辑，效率远高于新代理。
- **死亡超过 3 次**：启动全新**接管代理**，简报结构 = 现场盘点（协调者先用 git status 盘点未提交改动）→ 审计已有 diff → 补缺口 → 验证 → 提交。
- 批量停止（如额度耗尽影响多个代理）：额度恢复后逐个发「继续」。

### 3.2 活性判定（防盲等、也防误杀）
- **用墙钟时间，不用对话轮次数**（goal/对话轮可能 1-3 分钟一循环，会把正常阅读期误判为卡死）。编码类代理给 20 分钟纯探索宽限期。
- 判定靠三条证据，不靠猜测：
  1. `git status --short` 是否出现 M 状态文件；
  2. 进程表是否有 cargo/rustc/vitest（注意排除无关项目 shell 的 PATH 字符串误匹配）；
  3. 构建目录 mtime 是否推进。
- 三项全无且超出宽限期 → 走 §3.1 升级链（先「继续」，再接管）。

### 3.3 心跳汇报与优雅停止协议

“没有最终回复”不等于“代理已经死亡”。代理可能长时间阅读、运行测试或等待工具返回；协调者不得因为 UI 没有即时输出就直接终止、清理或重派同一写锁。

#### 编码/测试代理心跳

- 派发后 5 分钟内发送一次 `BOOTSTRAP` 汇报；进入编码、测试、提交、阻塞、待合并、收尾等阶段时立即汇报。
- 正常工作每 **5 分钟或每完成一个命令组** 至少汇报一次；预计超过 2 分钟的命令必须在开始前说明命令、预计耗时和完成后的下一步。
- 心跳必须包含以下最小字段：

  ```text
  [HEARTBEAT] track=<id> phase=<phase> at=<ISO-8601>
  worktree=<absolute-path> branch=<branch>
  changed=<files-or-clean> last=<completed-action> next=<next-action>
  blocker=<none-or-concrete-blocker>
  ```

- 心跳只报告事实，不用“正在处理”代替进度；测试代理还必须报告当前命令是否仍有进程、已完成/总测试文件数（可得时）和失败摘要。
- 长时间命令被用户新输入或工具等待打断时，代理必须先写入 checkpoint（当前阶段、已完成步骤、未完成步骤、是否安全重跑），再结束当前回复；恢复时从 checkpoint 继续，不从头猜测。

#### 协调者观察与停止

1. 超过一个心跳周期没有回报，先发送 `请发送 HEARTBEAT，不要提交/清理`；不要直接 kill、删除 worktree 或启动第二个写代理。
2. 再等待一个心跳周期，结合 `git status`、进程和构建目录 mtime 判断。存在进程或 dirty 变化时继续等待；三项均无且超过 20 分钟才按 §3.1 发一次「继续」恢复。
3. 代理明确阻塞时，协调者要求其先提交/写入 checkpoint，再决定迁移；**未收到 checkpoint 不得清理其 worktree**。
4. 只有代理明确返回 `READY_TO_CLOSE`，或满足 §3.1 的死亡升级条件，协调者才可释放写锁、转交接管代理或进入 §7 清理。

### 3.4 阶段状态机

每个 track 的 hub 状态和代理心跳阶段必须使用同一组状态，禁止“已派发但仍显示未开始”或“测试代理无结论却标记已完成”：

```text
DISPATCHED → BOOTSTRAP → CODING → READY_FOR_TEST
                              ↓
                         BLOCKED/PAUSED
READY_FOR_TEST → TESTING → PASSED → READY_TO_MERGE → MERGED → CLEANUP → CLOSED
                         └→ FAILED → BUG_RECORDED → REPAIRING → TESTING
```

- `READY_FOR_TEST` 必须对应编码 commit；`PASSED` 必须对应独立测试代理的结论/commit；`MERGED` 必须对应协调者记录的合并 commit。
- 代理异常退出、无最终回复或只留下未提交改动时，状态保持 `BLOCKED`/`PAUSED`，不能自动标为 `PASSED`。
- `CLEANUP` 只在合并和必要的恢复点确认完成后进入；清理动作必须有独立记录。

## 4. 简报解剖学（质量的最大变量）

每份简报必备七件（完整模板见附录）：

1. **工作目录与禁区**（worktree 绝对路径；明确禁止触碰主检出/其他轨道）。工作目录必须由 bootstrap 脚本创建在主检出 `.worktrees/` 下；如果代理报告 `Operation not permitted`，先检查它是否仍使用同级 sibling 路径，再迁移/重建 worktree，不要反复重试写入。
2. **必读清单**（项目约定 + 计划文档对应章节 + 进度文件相关小节，按序）；
3. **任务与验收标准**（可逐条判定的完成标准，不接受模糊表述）；
4. **已侦察落点**（协调者预先 grep 好文件与行号——这是代理效率差异的最大变量；声明"落点需自行核实"防盲从）；
5. **执行纪律**：一律用 Grep 工具搜索（禁 bash 全仓 grep）；探索类工具调用预算；单步阻塞 ≥3 次尝试即上报并继续其余步骤；
   **read→edit 纪律**：编辑任何文件前必须先用读工具读它（edit 工具强制要求）；文件被 git 操作（merge/amend/checkout）或另一代理改动后，旧读取失效，必须重读再编辑；批量改多文件时逐个"读→改"，不要攒一批旧句柄。
6. **环境注意**（见 §5）;
7. **返回格式**约定（commit hash / 改动清单 / 各套件数字 / 遗留）。

## 5. Worktree 环境坑清单

1. `node_modules` 是指向主检出的**软链**：禁 `pnpm install`，二进制用 `npx` 调用。
2. **bash `grep -rn` 全仓扫描会洪泛**软链进来的 node_modules——代理一律使用 Grep 工具（自动跳过 ignore 目录）。简报里必须显式写这条。
3. `CARGO_TARGET_DIR`：串行阶段共享主检出 target 复用缓存；**并行 cargo 重轨改用各自独立目录**（接受首次冷编译），否则 cargo 文件锁互相阻塞。
4. worktree 的 codegen `capabilities/default.json` 可能含 `redis:default` 而默认构建不编 redis 插件导致裸 cargo 失败 → 从主检出复制对齐（gitignore 文件不入库）。
5. 主检出**未跟踪**的规格文档在 worktree 不存在 → bootstrap 脚本负责拷贝；同时这些文件**禁止 git add**（避免最终合并 main 时与用户工作区副本冲突）。
6. E2E 的 webdriver 构建成本高：功能级测试轮只登记用例，真实构建回归统一到 R 阶段。
7. `src/locales/builtinLocales.ts` 是被忽略的生成文件；bootstrap 会生成它。直接运行 `npx vitest`/`npx tsc` 不会触发 pnpm 的 `pretest` hook，若 worktree 是旧的或手工创建的，先执行 `node scripts/generate-builtin-locales.mjs`，不要把生成物加入 commit。
8. **禁止共享可写 worktree 跑全量测试**：多个 Host Vitest/覆盖率进程可能争用 jsdom、portal 或临时资源，产生不可复现的假失败；同一 worktree 同时只运行一个全量 Host suite。并行测试必须使用独立 worktree，并为并行 Cargo 任务设置独立 `CARGO_TARGET_DIR`。
9. 任何 merge/cherry-pick/rebase 后必须先检查 `git status --porcelain=v1`、`git diff --check` 和冲突标记；发现 `UU`、`CHERRY_PICK_HEAD`、`MERGE_HEAD` 或 `<<<<<<<` 时，暂停后续代理，不得把半冲突目录交给下一个代理。
10. 运行时生成物、`.DS_Store`、孤立 `.worktrees/<dir>` 不等于 Git worktree；清理前要分别检查 `git worktree list`、目录内容和是否存在 `.git`，不能用宽泛 glob 删除。
11. 代理在返回前必须执行“工作树收尾检查”：`git status --short --branch`、`git diff --check`、测试命令退出码和 commit hash；未提交文件必须逐项说明归属，不能只说“已完成”。

## 6. 进度与 Bug 管理 schema

### 6.1 三层文件布局（禁止单一巨型进度+bug 文件）

```text
docs/development/coordination/
├── hub.md                              # 公共总览（协调者维护；worktree 软链到主检出）
├── <initiative>-plan.md                # 本次 initiative 实施计划
└── tracks/<track-id>/
    ├── progress.md                     # 该轨功能进度（编码/测试代理写本轨）
    └── bugs.md                         # 该轨独立 bug 清单（仅本轨读写）
```

| 文件 | 写入者 | 内容 |
|------|--------|------|
| **hub.md** | 协调者（融合各轨 milestone） | 功能总览表、波次/合并记录、跨轨风险、R 阶段清单 |
| **tracks/\<id\>/progress.md** | 该轨编码/测试代理 | 范围、E2E 登记、测试结果、覆盖率、设计决策 |
| **tracks/\<id\>/bugs.md** | 该轨测试/修复代理 | Bug ID / 描述 / 状态 / 重现步骤 / 验证记录 |

**硬性规则：**
- **每个 feature/track 必须有自己的 `bugs.md`**，禁止把所有 bug 堆进 hub 或单一 `*-progress.md`。
- Bug ID 格式：`<track-id>-BUG-nnn`（如 `cr-p0-mcp-BUG-001`）。
- 代理**只读写本轨** `tracks/<track-id>/` 下文件；hub 总览表由协调者更新。

### 6.2 Hub 跨 worktree 可见性

`scripts/new-feature-worktree.sh` bootstrap 时建立软链：

```text
<worktree>/docs/development/coordination/hub.md → <main-checkout>/docs/development/coordination/hub.md
```

协调者在主检出编辑 hub，各轨代理即时可见。`tracks/<id>/` 随各轨分支 git commit；合并 milestone 时协调者将 hub 摘要与 track progress 一并合入集成分支。

### 6.3 progress.md 四段式（每轨）

1. **功能摘要**：编号 / 范围 / 状态 / 编码 commit / 测试 commit；
2. **E2E 用例表**（【本机可执行】vs【留待 R 回归】）；
3. **测试结果与覆盖率**；
4. **设计决策 / 遗留注意**。

### 6.4 bugs.md 字段（每轨）

Bug ID / 描述（含量级）/ 状态 / 记录时间 / 重现步骤 / 验证记录。

状态机：功能 `未开始→编码中→编码完成→测试中→已完成`；bug `待验证(新发现)/验证不通过 → 待验证(修复后) → 已修复`。

**状态实时性纪律**：hub 总览表状态必须与实际派发同步——编码代理启动即「编码中」，测试轮启动即「测试中」（修复轮标「测试中·修复轮」），闭环才可「已完成」；**功能已派发却显示「未开始」视为台账错误**。协调者每次派发/流转后立即更新 hub 对应行，不攒批。

### 6.5 心跳、写锁与清理台账

每个 initiative 的 hub 或单独的 coordination log 至少记录：

| 字段 | 要求 |
|------|------|
| `track` / `agent` | track ID、代理实例 ID；测试代理必须与编码代理不同 |
| `worktree` / `branch` | 绝对路径和完整分支名；禁止只记录短名 |
| `phase` / `lastHeartbeat` | 与 §3.4 状态机一致；时间使用 ISO-8601 |
| `codingCommit` / `testCommit` | 没有 commit 写 `—`，不得用“已提交”替代 hash |
| `cleanup` | worktree 删除时间、分支删除时间、残留项或保留理由 |

心跳和清理记录属于过程证据，不以“最终代码能编译”替代。这样可以区分“代理完成但忘记收尾”“代理被中断”“测试尚未开始”和“分支已安全删除”。

## 7. 合并完成后的 Worktree 与分支清理

清理是流程的必需阶段，不是任务完成后的人工可选项。目标是主线只保留必要分支，避免旧 worktree、孤立分支和 Finder 元数据持续干扰下一轮任务。

### 7.1 清理前门槛

协调者必须逐项确认并记录：

1. track 已进入 `MERGED`，测试 commit 和修复链已在主线可追溯；
2. 目标 worktree 没有活动进程、写锁已释放，`git status --short --branch` 的 dirty 内容已审计；
3. dirty 内容只有已合入的生成物/协调台账时，记录“可丢弃原因”；含业务代码、测试或未知文件时，先保存为 commit/patch 或保留 worktree，禁止强删；
4. 分支与主线关系已核对：优先要求 `git merge-base --is-ancestor <branch> main` 成功；若是 cherry-pick 后的等价历史，必须对照文件 diff/patch 和合并 commit，不能仅凭相似分支名判断；
5. 清理目标使用绝对路径和精确分支名，不使用 `$HOME`、`*` 或仓库根目录递归删除。

### 7.2 推荐清理顺序

```bash
# 1. 只读盘点
git worktree list --porcelain
git branch -vv
git status --short --branch

# 2. 仅对已审计的 clean worktree 执行；dirty 目标必须先人工确认
git worktree remove <absolute-worktree-path>

# 3. 默认只删除已合入分支
git branch -d <branch>

# 4. 只有用户明确要求清理、且已记录“主线等价/无待保留内容”时才允许
git worktree remove --force <absolute-worktree-path>
git branch -D <branch>

# 5. 二次核对
git worktree list --porcelain
git branch -vv
git status --short --branch
```

每个目标单独执行并记录结果；一个目标失败不能用宽泛命令顺手删除其他目标。清理后还要检查 `.worktrees/` 是否存在未注册目录、`.DS_Store` 或残留 `.git` 文件：仅删除已确认的临时元数据和空目录，未知目录保留并报告。

### 7.3 异常与恢复

- worktree dirty 且包含未知内容：进入 `PRESERVE`，不删除；由接管代理或用户决定是否提交/导出 patch。
- 分支未合入主线且用户未明确授权强制删除：保留分支，hub 标记 `UNMERGED`，列出 branch-only commits。
- 删除后发现遗漏：优先用 `git reflog` 找回分支 tip；若是未提交内容且 worktree 已强制删除，通常不可恢复，因此 §7.1 的 dirty 审计是强制门槛。
- 清理完成的最终回复必须列出删除的 worktree/分支、保留的目标及原因、主线 HEAD、工作树状态和是否有不可恢复内容。

---

## 附录 A：编码简报模板

```text
你是 <项目> <功能号> 的编码代理（全新实例）。工作目录：<worktree 绝对路径>（分支 <branch>）。禁止修改其他目录。

## 必读（按序）
1. AGENTS.md；2. <计划文档对应章节>；3. coordination/hub.md + tracks/<track-id>/progress.md

## 任务
<目标形态 + 迁移/实现步骤>

### 已侦察落点（自行核实后使用）
<文件:行号 清单；明确"范围外勿动"边界>

### 设计要求 / 验收标准
<可逐条判定的标准；架构红线（如禁止按 id 硬编码分支）>

### 执行纪律
先报告 `BOOTSTRAP` 心跳（pwd/top-level/branch/status），之后每 5 分钟或每个命令组发送 `[HEARTBEAT]`；预计超过 2 分钟的命令先报预计耗时；
Grep 工具搜索（禁 bash 全仓 grep）；探索 ≤N 次；单步阻塞 ≥3 次上报；
cargo 前缀 CARGO_TARGET_DIR=<…>；node_modules 软链禁 install；
禁 git add <未跟踪规格文档> / codegen 文件；禁止 main 分支、主检出、same-directory fork；禁止 merge/cherry-pick/rebase、修改 hub、清理 worktree/分支

### 完成标准
<残留清零/测试全绿/进度文件更新/commit message 格式>；返回前执行 `git status --short --branch`、`git diff --check`，发送 `READY_TO_CLOSE` 心跳

## 返回格式
`READY_FOR_TEST` 或 `BLOCKED`；commit hash；改动清单；各套件数字；遗留注意；worktree 是否 clean。遇阻先发送 checkpoint，不静默退出。
```

## 附录 B：测试简报模板

```text
你是 <功能号> 的测试代理（全新实例，与编码代理无关）。只验证、只记录，禁止修复。
工作目录：<worktree>（分支 <branch>）。

## 复验清单
1. 范围完整性审查（对照计划章节逐步核对，列遗漏）
2. 逻辑正确性审查（读 diff，重点<该功能的关键语义>)
3. 先发送 `BOOTSTRAP` 心跳并确认与编码代理不同实例、不同 worktree；每 5 分钟或每个命令组发送 `[HEARTBEAT]`
4. 独立重跑三件套（不信编码轮数字）：cargo lib / vitest / tsc；直接调用测试工具前先执行 `node scripts/generate-builtin-locales.mjs`，确保 ignored codegen 已存在；同一 worktree 不并发运行多个全量 suite
5. 合并/冲突卫生检查：`git status --porcelain=v1`、`git diff --check`、冲突标记和 `MERGE_HEAD`/`CHERRY_PICK_HEAD`
6. 覆盖率：改动 TS 文件 ≥80% 实测（全量套件 --coverage 后摘取）；Rust 以单测清单佐证
7. E2E 用例设计：登记进度文件（编号/前置/步骤/断言），标注【本机可执行】vs【留待 R 回归】及理由
8. 判定与登记：通过→功能「已完成」；问题→本轨 tracks/<track-id>/bugs.md（<track-id>-BUG-nnn，待验证+重现步骤），不修
9. 结束前发送 `TEST_DONE` 或 `TEST_BLOCKED`，commit 仅进度文件：test(ipc): f<n> verification

## 返回格式
结论；各套件实测数字；覆盖率数字；bug 清单或“无”；commit hash；最后一个心跳时间；worktree clean 证据。没有 `TEST_DONE` 不得由协调者标记通过。
```

## 附录 C：接管（收尾）简报模板

```text
你是 <功能号> 的接管/收尾代理（全新实例）。前任代理中断留下未提交改动。

## 现场盘点（协调者预填）
<git status 输出摘要：哪些文件已改、属于什么范围>

## 工作流程
审计已有 diff（分段读，禁一次吞巨型 diff）→ 对照验收标准列缺口 → 补齐 → 三件套+覆盖率 →
进度文件核对补齐 → commit <约定格式>
若前代方向性错误：修正为单一机制，而非叠加补丁。

其余条款同附录 A 执行纪律与环境注意。
```
