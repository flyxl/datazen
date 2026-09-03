# 协调者 (Coordinator) 规程

> 角色定位：主会话代理。负责全局架构把控、功能拆分、波次编排、子代理派发、活性监控、合流仲裁与清理。  
> **核心硬线：主协调者不直接写业务代码，专注统筹协调；每个关键节点主动向用户汇报。**

## 1. 波次拆分原则（以文件冲突面为准）

- **分轨依据**：严格按照文件冲突面划分轨道，而不是功能逻辑相近度。
  - 触碰互斥文件（如 `mcp/**` vs `commands/driver_command.rs`）→ 可并行。
  - 触碰同一注册块但不同行（如 `lib.rs` 的 invoke_handler）→ 可并行。
  - 依赖公共核心接口改动或模式复用软依赖 → 必须分波次串行。
- **基线约定**：所有同波次轨道以主集成分支（如 `feat/post-review-hardening`）为基准拉出。

## 2. 轨道准备 (Bootstrap)

主检出执行配套脚本创建隔离环境：
```bash
scripts/new-feature-worktree.sh <track-id> <base-branch>
```
脚本自动完成：
1. 在 `.worktrees/datazen-<track-id>` 创建独立 worktree 并切换至分支 `feature/<track-id>`。
2. 建立 `node_modules` 软链。
3. 执行驱动 codegen 与 locale codegen。
4. 复制主检出未跟踪的规格文档（保持 untracked）。
5. 准备 `docs/development/coordination/tracks/<track-id>/` 目录。

## 3. 简报准备与派发

从 `docs/development/subagent/templates/` 提取对应角色的简报模板，组装以下要素后使用 `Task` 工具派发全新实例：
1. **工作目录与分支**：明确绝对路径，申明仅在此工作。
2. **必读列表**：
   - `AGENTS.md`
   - `docs/development/subagent/coder.md`（或 `tester.md`）
   - `docs/development/coordination/tracks/<track-id>/progress.md`
3. **已侦察落点**：预先 grep 文件与行号，声明“落点需自行核实”。
4. **验收标准**：可量化核验的完成条件。
5. **执行纪律**：禁碰 `hub.md`、禁 `pnpm install`、Grep 工具搜索、CARGO_TARGET_DIR。

## 4. 活性监控与死亡恢复

### 4.1 活性判定依据（用墙钟时间，看真实证据）
- 编码类代理给予 20 分钟宽限期。
- 依赖 3 项客观证据判断活性：
  1. `git -C <worktree> status --short` 是否产生 M 状态文件。
  2. 系统进程是否有 cargo/rustc/vitest/node 在持续占用 CPU。
  3. 构建输出目录的 mtime 是否推进。

### 4.2 死亡恢复升级链
- **死亡次数 ≤ 3**：优先原会话发送“继续”，保留其上下文与历史编辑。
- **死亡超过 3 次**：派发全新**接管代理 (Rescuer)**，基于 `git status` 现场盘点补齐。

## 5. 方案 B 进度总览聚合

各子代理仅提交各自 `tracks/<track-id>/progress.md` 与 `bugs.md`。
协调者在以下节点运行聚合脚本：
```bash
node scripts/aggregate-hub.mjs
```
- **执行时机**：每轨启动后、每轨完成合并后、或向用户同步进度前。
- **禁止行为**：禁止任何人或子代理手动编辑 `docs/development/coordination/hub.md`。

## 6. 合流验证与清理

### 6.1 逐轨合并
当测试代理返回 `TEST_DONE` 且无阻断 Bug 时，协调者在集成分支合入该轨：
```bash
git merge --no-ff feature/<track-id> -m "feat(coordination): merge track <track-id>"
```
合并后在集成分支运行快速健全性检查：
- `npx tsc --noEmit`
- 定向单元测试 `cargo test -p datazen --lib` / `npx vitest run`

### 6.2 Worktree 与分支清理
确认合入主线且无残留后执行规范清理：
```bash
# 1. 移除 worktree
git worktree remove .worktrees/datazen-<track-id>
# 2. 删除已合入分支
git branch -d feature/<track-id>
# 3. 重新聚合 hub
node scripts/aggregate-hub.mjs
```

## 7. R 阶段（全量回归）

所有波次全部轨道 MERGED 并清理完成后，进入 R 阶段统一执行：
1. 完整的编译与类型检查：`pnpm build`、`cargo check`。
2. 运行各轨在 progress.md 中登记的【留待 R 回归】E2E 用例。
3. 检查所有 Bug 已处于 `已修复` 状态。
