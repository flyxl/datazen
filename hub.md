# 共享层下沉 · 协调者台账

集成分支 `feature/shared-layer-extraction`（worktree `.worktrees/datazen-shared-layer-extraction`）。
主 checkout 保持 `main` 不动；**合并到 main 不在本轮范围内**。

缺陷记录按 bug 一文件，根目录 `bug-*.md`，随分支流转；`bugs.md` 是它们的索引（状态、归属、关闭要点）。

---

## 一、计划

### 目标

把宿主里「跨形态共享」的代码下沉到 `packages/*`，使 web / server 形态复用同一份实现，而不是各自再写一遍。

分层契约（`docs/architecture/platform/shared-boundaries-and-ports.md` §2.1）：
`platform-api` → `application` → `runtime` → `backend-client`

### 波次

| 波次 | 范围 | 状态 |
| --- | --- | --- |
| Wave-1 | sql_guard、driver_cache + schema_cache、frontend_bridge | 已合并 |
| Wave-2 A2 | `packages/ai-providers`：OpenAI/Anthropic/DeepSeek/Ollama/Custom provider 适配 | 已合并 |
| Wave-2 B1 | `packages/workspace-apps`：安装 / manifest / storage / zip 路径 | 已合并 |
| Wave-2 C3 | `packages/platform-infra`：日志脱敏基础设施 | 已合并 |
| Wave-3 | 收敛 Wave-2 制造出的重复实现（见 `bug-*.md`） | 未开始 |
| R | 全部波次合并后的全量回归 | 未开始 |

### 并行纪律

- 一波内所有 Coder 在**同一条消息**里并行派发，不串行；每个 Coder 都是全新实例。
- 一律在独立 worktree 开发；**提交方与验证方不得共用一棵工作树**。
- 验证方必须是**全新实例**，不得复用 Coder；每轮修复复开新验证方。
- 合并前先干跑（`git merge` 到一次性工作树），冲突数实测为 0 才动手。
- 合并后跑 `pnpm typecheck` + `npx vitest run`，再拆 worktree、删分支。

---

## 二、进度

### Wave-3 W3-1 脱敏收敛（实测 PASS，独立验证 8 门禁 + R1/R2）

- 合并 `66e97f69f`；验证提交 `ba8c5f4f4`，3 文件 +6/−49。
- 独立验证方实测：宿主 1555/0/6、ai-providers 90、platform-arch 0 violation 0 error、`cargo fmt` 空。
- **变异探针证明覆盖是「搬家」不是「丢失」**（本轨最有价值的一条实测）：把 `redaction.rs:65` 的 ``${1}***@` 改成 `${1}${2}@` 关掉 userinfo 掩蔽后，platform-infra 红 4、宿主红 3，
而 `ai-providers` 仍全绿 90 —— 被删的 2 个测试没有留覆盖缺口，断言已随实现下沉到 platform-infra 且确实能红。
- B9 经独立复核**属实且为既有缺陷**：新旧两版对 `https://user:pa/ss@host/v1` 均整条输出、零遮蔽。
  独立证据两条：`redaction.rs` 在父提交与 HEAD 间 sha256 逐字节相同；旧实现同样泄漏。

| 轨道 | 分支 | 状态 |
| --- | --- | --- |
| A2 ai-providers | 已删 | 合并完成，门禁全绿 |
| B1 workspace-apps | 已删 | 合并完成，门禁全绿 |
| C3 platform-infra | 已删 | 独立验证 PASS 16/16，合并完成，门禁全绿 |

Wave-2 三轨全部合入集成分支。**剩余：Wave-3 收敛轨 → 全量回归（R）。**

### Wave-3 W3-2 zip 路径守卫收敛（实测 PASS，独立验证 PASS）

- 合并 `583666808`；验证提交 `58c5df2944f1c1a0a8016cc202af84d4583d4344`，8 文件 +40/−92，
  `rename packages/{workspace-apps/src/zip_path.rs => platform-infra/src/path_safety.rs} (70%)`。
  验证树隔离指纹 `ddf8aa83c9c103f7042286f7f458f79710e07433`。
- 合并后全量门禁（HEAD `d98956480` 实测）：`pnpm typecheck` EXIT=0（`error TS` **0 行**）；`npx vitest run` EXIT=0，
  **602 文件 / 6335 测试全过**，与基线逐字相同；`test:platform-arch` EXIT=0，`0 violation(s), 0 error(s), 3 advisory(ies)`；
  `test:platform-crates` EXIT=0，`10 core crate(s) tested`；`cargo test -p datazen --lib` EXIT=0 → **1552 passed / 0 failed / 6 ignored**；
  platform-infra 10/0、workspace-apps 62/0、ai-providers 90/0。
- **宿主 1555 → 1552 的 −3 正是本轨删掉的那 3 个宿主纯路径测试**（副本换成 `pub use` 后已无意义），算术自洽；
  新覆盖落在 platform-infra 的 5 个 `path_safety::tests::*`。这是计数约定问题，不是覆盖缺口。
- 验证方遗留的 **L5 不是轨道缺陷，是它自己的变异探针残留**：`install/tests.rs:101` 断言 `TempDir` 的父目录不产生
  落盘文件，而 `TempDir` 直接挂在共享 `$TMPDIR` 下 ⇒ 探针短路守卫后真的往 `$TMPDIR/outside.html` 写了 17 字节，
  重跑因此多出 1 个 failed。验证方删掉残留后复跑 workspace-apps 62/0、宿主 1552/0、树 sha 回到起点。
  断言写法本身没错，错在把共享临时目录当自有沙箱 —— 已记 `bug-*.md`。
- L3/L4 文档失准由 `b58f8583b` 修掉：`shared-boundaries-and-ports.md` 待办段改写为已实现事实、
  `wapps.md:30` 模块表指向 `path_safety`、`path_safety.rs` 模块头的历史叙述更正。
- `Cargo.lock` 补提交 `9e634012d`：实测差 **3 行**（`ai-providers` 与 `workspace-apps` 各增 `datazen-platform-infra`，
  `platform-infra` 去 `thiserror 1.0.69`），前两条边正是 W3-1 / W3-2 合并时漏补的。`cargo metadata --locked` EXIT=0。
- 台账收尾 `d98956480`：B1 / B2 / B4 各自走「独立验证 → 删缺陷文件 → 改 `bugs.md` 状态」，关闭后再复跑门禁复核；
  `progress.md` 随合并销毁。

### R 阶段全量回归（实测 PASS，HEAD `576d37a47`）

- 脚本 `/tmp/dz-r-phase-gates.sh`，跑在集成分支 worktree，`CARGO_TARGET_DIR=/tmp/dz-target-int`，每道门禁各自新开日志。
- **首尾快照一致**：HEAD `576d37a47` → 同值；工作区 sha `da39a3ee…9909` → 同值（空 diff）；
  脏文件 0 → 0；驱动指纹 `9c7c24ef92dd380f` / 1625 B → 同值。**运行期间无人动过这棵树**（AGENTS.md 硬要求）。
- 本轮门禁**没有**弄脏 `Cargo.lock`：重写后内容与 `9e634012d` 已提交的增量一致，无差异。

| 门禁 | 命令 | 退出码 | 结论行（实测） |
| --- | --- | --- | --- |
| G1 类型 | `pnpm --config.verify-deps-before-run=false typecheck` | 0 | `error TS` 0 行 |
| G2 前端 | `npx vitest run` | 0 | 602 文件 / 6335 用例全过，与基线逐字一致 |
| G3 边界 | `node scripts/check-platform-crate-boundaries.mjs` | 0 | 29 member / 10 rule / 31 crate，`0 violation(s), 0 error(s), 3 advisory(ies)` |
| G4 crate | `node scripts/run-platform-crate-tests.mjs` | 0 | `10 core crate(s) tested`，10 个 crate 全过 |
| G5 宿主 | `cargo test -p datazen` | 0 | **1552 passed / 0 failed / 6 ignored** |
| G6 新 crate | `platform-infra` / `workspace-apps` / `ai-providers` | 0 / 0 / 0 | 10/0 · 62/0 · 90/0 |

- G5 的 1552 是 W3-2 之后的**合法基线**（见「坑」中作废 1710/1705 那条），与 W3-2 合并后那次逐字相同。
- 3 条 advisory **不作门禁**（跨树在 2/3 间浮动）；只有 violation / error 的 0/0 是稳定判据。

### 两条遗留裁定的结论

- **宿主少 3 个纯路径测试，不记缺陷。** 删的是 `app_data_archive.rs` 里与被下沉实现逐字重复的副本，
  换成 `pub use` 之后它们测的已经是别处的代码。覆盖不止是搬家：宿主仍留两个端到端恶意 zip 测试
  （`app_data_archive.rs:690 rejects_malicious_zip_on_import`、
  `wapps/integration_tests.rs:267 malicious_zips_rejected_without_side_effects`），platform-infra 另有 5 个 `path_safety::tests::*`。
  1555 → 1552 是计数约定变化，不是覆盖缺口。
- **保留宿主 `pub use` 薄壳。** `app_data_archive.rs:13` 再导出 `validate_zip_entry_path` 与 `MAX_COMPRESSION_RATIO`，
  而 `commands/wapps/storage.rs:77` 正经由 `crate::app_data_archive::` 调用它 —— 薄壳让调用点零改动，
  与 `log_redact.rs`（8 行、18 处调用点全未改）同一形态，也保住宿主对外路径稳定。
  代价是 `MAX_COMPRESSION_RATIO` 从「宿主自有的解压炸弹阈值」变成「宿主转出的常量」；
  日后 Web 形态若要自带策略，应改 `platform-infra`，而不是在宿主另立一份。

**Wave-1～Wave-3 与 R 阶段全部完成。剩余：待裁定项与合并回 main（未获授权，不执行）。**

### Wave-2 收尾（实测）

- 三轨合并：A2 `77be2b19b`、B1 `88a7fdaac`、C3 `d2bb5f4ae`，均零冲突（合并前已 5 次 dry-run 预演）。
- 合并后门禁：`pnpm typecheck` EXIT=0（`error TS` **0 行**）；`npx vitest run` EXIT=0，
  **602 文件 / 6335 测试全过**；`test:platform-arch` EXIT=0，`0 violation(s), 0 error(s), 3 advisory(ies)`；
  `test:platform-crates` EXIT=0，`10 core crate(s) tested`，含 `datazen-platform-infra`。
- C3 独立验证 V1–V15 全绿 16/16：crate `cargo test` 5 passed；宿主 `cargo test -p datazen --lib`
  1705 passed / 0 failed / 6 ignored（基线 1710）—— **此二数已作废**，见「坑」末两条；同一棵树重测实为 1555；F-11 变异探针 EXIT=1 并报 `VIOLATION F-11`。
  验证方工作树指纹首尾逐字节一致（`cd1ffcdee…`）。
- **D2 已闭合**：`cargo metadata --locked` EXIT=0；三个新成员在入库 lock 中各命中 1。
  合并后 lock 增量**恰好 +9 行**，唯一新增 `datazen-platform-infra`，删除 0，`source = "git"` 噪音 0 ——
  与合并前一次性工作树预演逐字一致。驱动噪音**绝不提交**。
- 验证方提的 D2–D6：D2 已闭合；D3/D4 → `bug-*.md`；**D5 作废**（`stash@{0}` 已不是 infra 基线）；
  D6 → 合并回 main 时删 `progress.md`。
- infra 的编码树与验证树、分支 `/tmp/dz-target-infra` 均已清理。

### 关键 commit

| commit | 内容 |
| --- | --- |
| `179c6c8b6` | infra 轨预置：crate 注册 + F-11 边界规则 |
| `d24779e2f` | C3 交付：日志脱敏下沉 |
| `77be2b19b` / `88a7fdaac` / `d2bb5f4ae` | A2 / B1 / C3 合入集成分支 |
| `514528e30` / `38a654673` | Wave-2 lock 增量（A2+B1 / platform-infra） |
| `f418e2f93` | 先例：迁移原语下沉时同提交了 `Cargo.lock` |

### Wave-3 起点

- 两条必修：`bug-ai-providers-redact-endpoint-duplicate.md`、`bug-workspace-apps-zip-path-duplicate.md`；
  落点都在 `packages/platform-infra`（`redaction` / 待建的 `path_safety`）。
- **Wave-3 的工作树必须从本分支 HEAD 起**（现为 `38a654673`）。infra 轨的基点 `78038d02b`
  不含 A2/B1 代码，在它上面做 Wave-3 会读到 0 命中 —— 那是分支基线假象，不是缺陷。

---

## 三、注意事项

### 红线

- **不得写主 checkout 或其他 worktree**；不得 `git push` / `rebase` / squash / 跨轨合并。
- 合并到 main 必须等全部波次 + R 阶段完成，且需用户显式确认。
- 禁用清单：`packages/runtime/src/{application,registry,resource}/**`、
  `src-tauri/src/platform/{session_backend,session_commands}.rs`、`src-tauri/src/bootstrap/run.rs`、
  `docs/architecture/platform/{team-server-and-auth,consumer-adapters}.md`。

### 环境

- 每棵新 worktree 必须跑 `node scripts/generate-builtin-locales.mjs` **和** `node scripts/resolve-drivers.mjs`，
  否则缺 `driver_init.rs`，任何 workspace 级 `cargo` / `cargo fmt` 都以误导性的 `failed to resolve mod 'driver_init'` 失败。
- worktree 的 `node_modules` 是符号链接 ⇒ 裸 `pnpm run` 退出 1。用 `pnpm --config.verify-deps-before-run=false <script>`。**禁止 `pnpm install`**。
- `resolve-drivers.mjs` 会按设计弄脏 `src-tauri/Cargo.toml` 和 `Cargo.lock`；merge 前先 `git checkout --` 复位。
- 各轨独立 `CARGO_TARGET_DIR`（`/tmp/dz-target-*`）；同一目录同一时刻只跑一个 cargo。
- 建 worktree 用 `NEW_WT_DRIVERS=all`（`new-feature-worktree.sh` 不带该参数时会引导失败）。
- `git branch -d` 必须**从集成分支 worktree** 执行；从主 checkout 执行会因 HEAD=main 拒绝。
- 临时文件只放 `/tmp`；长命令输出落 `LOG="$(mktemp -t x).log"`，`echo "EXIT=$?"` 单独一行。

### 新建工作树（含目录初始化）

完整配方已提升为正式文档 `docs/development/worktree-isolation.md` §7 —— 台账合并进 main 后即销毁，配方必须仍能从文档读出来。
台账只留本次的实测补充：

- 用它建过 W3-1 / W3-2 两棵验证树；全驱动指纹 `9c7c24ef92dd380f` / 15 驱动 / 1625 B，订正过程见「坑」末两条。
- 验证方另有 `/tmp/dz-bootstrap-verify-tree.sh <路径> <commit> <tag>`（detached 专用），实测可跑；它在系统临时目录，不入库。
### Rust 约定

- 新 crate 依赖**必须直接声明**，不得用 `{ workspace = true }`（根 `[workspace.dependencies]` 只有 7 条 path 项）。
- 新增依赖行写在 `# <<driver-dependencies>>` / `# <</driver-dependencies>>` 标记**之外**。
- 新 crate 注册要改 5 处：根 `Cargo.toml` members、`scripts/lib/cargoWorkspace.mjs` LAYERS、
  `scripts/run-platform-crate-tests.mjs` TESTED_LAYERS、`scripts/check-platform-crate-boundaries.mjs` 的 RULES + SPEC_PATH 行、
  以及 `TESTED_LAYERS ⊂ LAYERS` 的单测。
- `Cargo.lock`：纯新 workspace 成员条目**可以提交**；`resolve-drivers` 的驱动噪音**绝不可提交**。
- 生产路径禁止裸 `unwrap()` / `expect()`。

### 命名陷阱

- `packages/wapps/` 已存在（Wapp JS 包）；Rust crate 是 `packages/workspace-apps/`。

### 坑（都已实测踩过）

- **BSD `head` 不支持 `head -n -1`**：该命令报错并输出空文件，曾把本台账 826 行截没。删末行用 `sed '$d'`。
- **拿行数当验收信号时必须与历史值比对**：上面那次的退出码仍是 0，`wc -l` 打出 `1` 当时没被警觉。
- **子代理报 `finished … It left no closing message.` 不等于死亡**。先发一条只回 `PONG` 的极小探针：
  秒回 ⇒ 实例健康、只是简报太重，**改成分段派发**；连探针都不回才是真死。
- **长简报分段派发**：一次 15 道门禁的简报会让实例静默不启动；5–7 道一轮为宜。
- **绝不用 `git stash` 取门禁基线**：曾因此丢掉一整轮工作。基线从独立 worktree 或已提交 ref 取。
- **不要执行 `git stash drop stash@{N}`**：序号会漂移。C3 的 `progress.md:60` 让协调者 drop `stash@{0}`，
  实测该 stash 早已不存在（我此前已 drop 掉冗余基线），`stash@{0}` 现指向另一轨道的 scratch —— 照做会误删。
  现有 3 条 stash 一律不动。
- **提交信息不得断言未实际观测过的门禁结果**（踩过一次，只能 amend）。
- 文档里区分「实测」与「推断」；跨轨分支上的「事实性错误」要先问「合并后是否仍然如此」再判缺陷。
- **台账内容必须当次提交**：「建立新工作树」那一节曾只停留在工作区未提交，事后全树 `grep` 不到，已丢失并重写。
  AGENTS.md 说台账可入库随分支流转，前提是真的入库 —— 未提交的台账与对话上下文一样会蒸发。
- **别把自己的基线命令 `cd` 进别人的工作树**：曾把「集成树基线」误 `cd` 进 W3-2 验证树，与验证方抢同一棵树
  （两方同时改写 `Cargo.lock`）。跨树操作前逐条核对路径变量。
- **宿主基线数字不可跨树沿用**（实测教训，2026-Wave-3)：台账曾记 P4「真基线 1710」与 Wave-2 后「1705」，
但两棵树在相同驱动指纹（`9c7c24ef92dd380f` / 15 驱动）下实测均为 **1555**，历史日志里也已找不到 1710 的出处。
**1552 与 1555 之差 3，正是 W3-2 删除的 3 个宿主纯路径测试**，算术自洽。
结论：1710/1705 是 Wave-1 把 sql_guard 迁出宿主前的旧数，**已作废，不得作为门禁**。
唯一合法基线 = **在本树 checkout 基线 commit 实测**，或用同一驱动指纹的对照树实测；
跨分支、跨树凭记忆比数字，见下一条。
- **驱动指纹先查 mtime 再下结论**（实测，2026-Wave-3）：`driver_init.rs` mtime `2026-10-16 10:34:26`
  **早于全部 Wave-2/Wave-3 提交**（14:54 起），即整个 Wave-2+Wave-3 期间该文件一个字节都没变 —— 1555 与 1552
  出自同一份驱动配置，基线可比性成立。指纹值本身另有一处订正失误，见下一条。
- **`.driver-file-stash` 陈旧会让 `resolve-drivers.mjs` 退出码 1 硬失败**（实测，2026-10-16）：本树的 stash 建于
  HEAD `7f32ba2a6057`，此后 `Cargo.lock` / `Cargo.toml` / `src-tauri/Cargo.toml` 都变过，脚本判 `managed-files-changed`
  拒绝复用。**这是设计行为不是故障** —— 它挡的正是「旧副本覆盖合并结果」这种静默回滚。此时 `driver_init.rs`
  保持上一版（仍 15 驱动全集），已跑过的门禁结论继续有效；确需重新生成时按脚本提示先 `git diff` 存盘、
  再 `rm -rf .driver-file-stash`、再原样重跑。**不要**跑 `--restore`。
- **订正台账里已记的实测值，必须重新实测一次，不能凭上次的印象改写**（实测，2026-Wave-3 R 阶段）：
  我曾把 `9c7c24ef92dd380f` 判成「不可复现的抄错」，另写 `abbe22d7f80ea761` 当实测值并提交了两次。
  R 阶段脚本开头重新打指纹，打出的恰是被判死的那个值；`shasum` 与 `openssl dgst` 两条独立路径一致，
  而 `abbe22d7f80ea761` 在主树（`7d2218d8` / 807 B / bare）与集成树都复现不出。
  mtime 那条证据一直是对的，错的只是哈希值本身。
- **每次门禁脚本开头重打一次指纹，而不是照抄台账**：正是这一步抓出了上面的错值。
  指纹、HEAD、工作区 sha 应当由脚本现算并首尾各打一次，台账里的对应值只作对照，不作凭证。