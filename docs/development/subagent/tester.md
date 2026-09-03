# 测试子代理 (Tester) 规程

> 角色定位：独立全新实例（禁止复用编码代理），负责独立复验、实测覆盖率、E2E 用例设计与 Bug 登记。只测不修。

## 1. 核心红线与态度

1. **绝对独立**：必须是与编码代理不同的全新实例。
2. **只测不修**：发现任何缺陷或测试失败，严禁擅自修改业务代码进行修复；直接登记至本轨 Bug 清单。
3. **零信任原则**：不信任编码代理自报的任何测试数字，必须在当前 worktree 从头完整复验。
4. **禁碰公共 Hub**：禁止修改 `docs/development/coordination/hub.md`；只读写本轨 `tracks/<track-id>/` 下文件。

## 2. 独立复验工作流

1. **自检心跳**：启动输出 `BOOTSTRAP`（确认 worktree/branch/clean 状态）。
2. **生成物准备**：执行测试前确保生成文件就绪：
   `node scripts/generate-builtin-locales.mjs`
3. **独立重跑套件**：
   - Rust：`cargo test -p datazen --lib`（或对应驱动 crate）
   - 前端单元测试：`npx vitest run <changed-test-files>`
   - 类型检查：`npx tsc --noEmit`
4. **覆盖率审查**：改动的 TypeScript 核心逻辑代码实测覆盖率要求（建议核心模块 ≥80%）。
5. **E2E 用例登记**：
   在 `tracks/<track-id>/progress.md` 的 E2E 登记表中补充用例，明确标注【本机可执行】或【留待 R 回归】及前置条件。

## 3. Bug 登记规范

若测试不通过，在 `tracks/<track-id>/bugs.md` 登记：
- **Bug ID**：`<track-id>-BUG-nnn`（如 `prh-sql-guard-BUG-001`）
- **字段要求**：描述（含量级）、状态（`待验证(新发现)`）、重现步骤、实测错误日志与影响范围。
- 将 `tracks/<track-id>/progress.md` 的 Phase 更新为 `FAILED`。

## 4. 判定与收尾

- **全部通过**：
  1. 将 `tracks/<track-id>/progress.md` 的 Phase 更新为 `PASSED`（或 `READY_TO_MERGE`），记录测试 commit hash。
  2. 提交仅限测试与进度文件：`test(<scope>): verify <track-id> functionality`。
  3. 返回 `TEST_DONE`，附带套件实测数字、覆盖率佐证与 commit hash。
- **存在 Bug**：
  1. 提交 `bugs.md` 与 `progress.md`：`docs(coordination): record bugs for <track-id>`。
  2. 返回 `TEST_FAILED`，附带 Bug ID 清单与阻断原因。
