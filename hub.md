# 共享层下沉 · 协调者台账

集成分支 `feature/shared-layer-extraction`（worktree `.worktrees/datazen-shared-layer-extraction`）。
主 checkout 保持 `main` 不动；**合并到 main 不在本轮范围内**。

缺陷记录按 bug 一文件，根目录 `bug-*.md`，随分支流转。

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

| 轨道 | 分支 | 状态 |
| --- | --- | --- |
| A2 ai-providers | 已删 | 合并完成，门禁全绿 |
| B1 workspace-apps | 已删 | 合并完成，门禁全绿 |
| C3 platform-infra | 已删 | 独立验证 PASS 16/16，合并完成，门禁全绿 |

Wave-2 三轨全部合入集成分支。**剩余：Wave-3 收敛轨 → 全量回归（R）。**

### Wave-2 收尾（实测）

- 三轨合并：A2 `77be2b19b`、B1 `88a7fdaac`、C3 `d2bb5f4ae`，均零冲突（合并前已 5 次 dry-run 预演）。
- 合并后门禁：`pnpm typecheck` EXIT=0（`error TS` **0 行**）；`npx vitest run` EXIT=0，
  **602 文件 / 6335 测试全过**；`test:platform-arch` EXIT=0，`0 violation(s), 0 error(s), 3 advisory(ies)`；
  `test:platform-crates` EXIT=0，`10 core crate(s) tested`，含 `datazen-platform-infra`。
- C3 独立验证 V1–V15 全绿 16/16：crate `cargo test` 5 passed；宿主 `cargo test -p datazen --lib`
  1705 passed / 0 failed / 6 ignored（基线 1710，下沉 5 个单测）；F-11 变异探针 EXIT=1 并报 `VIOLATION F-11`。
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