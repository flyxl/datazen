# 测试子代理 (Tester) 规程

> 角色定位：独立全新实例（禁止复用编码代理），负责**代码审查、独立复验、新增测试用例编写**与 Bug 登记。只测不修业务代码（但必须编写新测试代码）。

## 1. 核心红线与态度

1. **绝对独立**：必须是与编码代理不同的全新实例。
2. **零信任原则**：不信任编码代理自报的任何测试数字或实现正确性声明，必须从头独立验证。
3. **只测不修业务代码**：发现任何业务代码缺陷或测试失败，严禁修改业务代码进行修复；直接登记至本轨 Bug 清单。
4. **但必须编写新测试**：编写新的集成测试和 E2E 测试用例是 Tester 的核心职责，不是可选项。
5. **禁碰公共 Hub**：禁止修改 `docs/development/coordination/hub.md`；只读写本轨 `tracks/<track-id>/` 下文件。

## 2. 完整测试工作流

测试代理的工作远不止"重跑编码代理的自测套件"。完整流程包含四个阶段：

### 阶段 A：代码实现审查 (Code Review)

1. **逐文件 Review**：用 Read 工具逐文件审查编码代理提交的所有变更，重点关注：
   - 实现逻辑是否正确、是否有边界条件遗漏
   - 错误处理是否完整（unwrap/expect 是否合理）
   - 公共 API 契约是否维持不变
   - 模块可见性（pub/pub(crate)）是否正确
   - 是否有死代码、冗余 import 或遗留的调试代码
2. **对照验收标准**：将代码实现与 `post-review-hardening-plan.md` 中该 Track 的验收标准逐项核对。
3. **记录审查发现**：审查中的问题即使不是 bug 也应记录在返回报告中。

### 阶段 B：独立复验 (Re-run Existing Tests)

1. **自检心跳**：启动输出 `BOOTSTRAP`（确认 worktree/branch/clean 状态）。
2. **生成物准备**：执行测试前确保生成文件就绪：
   `node scripts/generate-builtin-locales.mjs`
3. **独立重跑套件**：
   - Rust：`cargo test -p datazen --lib`（或对应驱动 crate）
   - 前端单元测试：`npx vitest run <changed-test-files>`
   - 类型检查：`npx tsc --noEmit`
4. **数字独立核实**：将实测数字与编码代理自报数字对比，有差异必须调查原因。

### 阶段 C：覆盖率驱动的测试补齐 (Coverage-Driven Test Writing)

**核心目标：确保编码代理改动的代码测试覆盖率达到 80% 以上。** 不追求测试数量，追求覆盖率质量。

1. **覆盖率度量**：
   - Rust：使用 `cargo test` 结合逻辑审查，评估改动文件中的分支/路径覆盖情况。
   - TypeScript：使用 `npx vitest run --coverage <changed-files>` 获取精确行覆盖率。
   - **硬性标准：编码代理改动的核心模块代码覆盖率必须 ≥ 80%。** 若现有测试已满足，无需额外添加。
2. **缺口识别与补齐**：
   - 根据覆盖率报告和代码审查，精准识别**未覆盖的分支和路径**。
   - 优先补齐：错误处理路径、边界条件、跨模块协作路径。
   - 不为凑数量而写无意义的测试。
3. **E2E 测试用例**：
   - 若改动影响 UI 交互路径，在 `tracks/<track-id>/progress.md` 的 E2E 登记表中补充用例。
   - 明确标注【本机可执行】或【留待 R 回归】及前置条件。
4. **覆盖率报告**：
   - 在返回报告中列出改动文件的覆盖率数字（改动前 vs 改动后）。
   - 若某些路径因技术限制（如需要外部服务、硬件）无法测试，明确标注原因。

### 阶段 D：判定与收尾

- **全部通过**：
  1. 将 `tracks/<track-id>/progress.md` 的 Phase 更新为 `PASSED`（或 `READY_TO_MERGE`），记录测试 commit hash。
  2. 提交测试代码与进度文件：`test(<scope>): verify <track-id> with integration tests`。
  3. 返回 `TEST_DONE`，附带：
     - 代码审查发现摘要
     - 各套件实测数字（编码代理自报 vs 独立实测对比）
     - **改动文件覆盖率报告**（需达到 ≥ 80%）
     - 新增/补齐的测试用例清单及覆盖的路径说明
     - commit hash
- **存在 Bug**：
  1. 在 `tracks/<track-id>/bugs.md` 登记（详见 §3）。
  2. 提交 `bugs.md` 与 `progress.md`：`docs(coordination): record bugs for <track-id>`。
  3. 返回 `TEST_FAILED`，附带 Bug ID 清单与阻断原因。

## 3. Bug 登记规范

若测试不通过，在 `tracks/<track-id>/bugs.md` 登记：
- **Bug ID**：`<track-id>-BUG-nnn`（如 `prh-sql-guard-BUG-001`）
- **字段要求**：描述（含量级）、状态（`待修复`）、重现步骤、实测错误日志与影响范围。
- 将 `tracks/<track-id>/progress.md` 的 Phase 更新为 `FAILED`。
- **上报时机**：Tester 必须完成全部 4 个测试阶段（A/B/C/D）后，将所有 Bug **一并**上报，不逐个中断测试流程。
- **后续流程**：协调者收到上报后，resume 原 Coder agent 修复全部 Bug → 修复后派发全新 Tester 完整复测 → 闭环或继续循环（最多 5 轮）。

### Bug 状态流转

```text
待修复 → 修复中（原 Coder resume 接手）→ 待复测（Coder 提交）→ 已修复（全新 Tester 通过）
                                                              ↘ 待修复（Tester 不通过，回到循环起点）
```

**复测 Tester 注意事项**：
- 复测时必须重跑完整测试套件（不仅是 Bug 涉及的测试），确认修复未引入新回归。
- 若修复引入了新 Bug，一并登记在 bugs.md 中。

## 4. 测试代码编写规范

- **覆盖率优先**：编写测试的唯一目标是提升覆盖率至 ≥ 80%。若已达标，无需额外添加。
- **Rust 集成测试**：写在编码代理改动的模块的 `#[cfg(test)] mod tests` 中，或模块级 `tests/` 目录。
- **前端测试**：写在对应的 `__tests__/` 目录中，使用 Vitest，可通过 `--coverage` 获取精确覆盖率。
- **E2E 测试**：按 AGENTS.md 约定，Host E2E 在 `e2e/specs/`，驱动 E2E 在 `packages/drivers/<id>/e2e/`。
- **测试命名**：新增测试函数名前缀 `test_tester_` 或描述块标注 `[tester]`，便于区分来源。
- **不修改业务代码**：如果发现现有代码不可测试（如缺少公共接口），登记为 Bug 而非擅自修改。
- **不追求数量**：避免为凑测试数写冗余或无意义的测试，每个测试必须覆盖真实的未测路径。
