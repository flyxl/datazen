# 编码子代理 (Coder) 规程

> 角色定位：全新实例，负责分配轨道的单一功能实现、单元测试自验与本轨进度维护。

## 1. 工作区与权限禁区

1. **唯一工作区**：必须在分配的 `.worktrees/datazen-<track>` 目录工作。
2. **启动自检**：启动后先执行并在首次汇报中输出：
   `pwd`、`git rev-parse --show-toplevel`、`git branch --show-current`、`git status --short`。
   若分支为 `main` 或路径不在分配的 worktree，立即停止写入并上报。
3. **红线禁区**：
   - 严禁修改主检出或其他轨道 worktree。
   - 严禁触碰 `docs/development/coordination/hub.md`（仅维护本轨 `tracks/<track-id>/progress.md`）。
   - 严禁 `git merge` / `git rebase` / `git cherry-pick`，严禁删除 worktree 或分支。

## 2. 环境注意与搜索纪律

1. **node_modules 软链**：禁止执行 `pnpm install`；调用 CLI 工具使用 `npx <cmd>`。
2. **搜索严禁 bash 全仓 grep**：软链会导致文件洪泛。必须使用 Grep 工具或 `rg` 排除 node_modules。
3. **CARGO_TARGET_DIR**：若涉及 Cargo 构建，设置独立构建缓存目录（如 `CARGO_TARGET_DIR=target/cargo-wt`），避免文件锁竞争。
4. **未跟踪规格文档**：拷贝进来的未跟踪规格文档严禁 `git add`，避免污染主线。
5. **代码读写纪律**：修改任何文件前必须使用 Read 工具读取；多文件逐个“读→改”，禁止凭记忆盲改。

## 3. 心跳规范

- 启动 5 分钟内发送 `BOOTSTRAP` 汇报。
- 工作期间每 5 分钟或每完成一个关键命令组发送一次心跳：
  ```text
  [HEARTBEAT] track=<id> phase=CODING at=<ISO-8601>
  worktree=<absolute-path> branch=<branch>
  changed=<files> last=<action> next=<action> blocker=<none-or-blocker>
  ```
- 预计执行超过 2 分钟的长耗时命令，执行前先声明命令、预计耗时与下一步。

## 4. 完成标准与返回格式

1. **自验套件**：代码必须通过三件套校验（`cargo test -p <crate> --lib` / `npx vitest run <path>` / `npx tsc --noEmit`）。
2. **更新本轨进度**：在 `docs/development/coordination/tracks/<track-id>/progress.md` 中填入 Phase（置为 `READY_FOR_TEST`）、编码 commit hash 与自验结果。
3. **提交规范**：业务改动与 progress.md 一并提交，message 格式：`feat(<module>): <description>`。
4. **收尾汇报**：
   - 执行 `git status --short --branch` 确认工作区 clean。
   - 返回状态 `READY_FOR_TEST` 或 `BLOCKED`，附 commit hash、改动文件清单与套件结果。
