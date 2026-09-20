# Bugs — ai-safety-prompt

## ai-safety-prompt-BUG-001

- **描述**: `format_compact_ddl_basic` 测试断言与 PK NOT NULL 去重实现不一致（~5 行影响）
- **状态**: 待修复
- **文件**: `src-tauri/src/ai/context.rs:326`
- **重现步骤**:
  ```bash
  cargo test -p datazen --lib ai::context::tests::format_compact_ddl_basic
  ```
- **实测错误日志**:
  ```
  assertion failed: ddl.contains("id int PK NOT NULL")
  thread 'ai::context::tests::format_compact_ddl_basic' panicked at
  src-tauri/src/ai/context.rs:326:9
  ```
- **根因**: Commit `ca11d711a` 将 `format_compact_ddl` 改为"PK implies NOT NULL"（第 237 行：`if !c.nullable && !pk_set.contains(c.name.as_str())`），PK 列不再输出 `NOT NULL`。但 `format_compact_ddl_basic` 测试仍断言 `ddl.contains("id int PK NOT NULL")`，未同步更新。
- **修复方案**: 将测试断言改为 `ddl.contains("id int PK")` 并增加 `assert!(!ddl.contains("id int PK NOT NULL"))`。代码本身正确。
- **影响范围**: 仅测试断言，不影响生产代码逻辑。

---

## ai-safety-prompt-BUG-002

- **描述**: `resolve_safety_gate` 使用不存在的枚举变体 `AiDataEgressLevel::Relaxed` 导致编译失败（~3 行影响）
- **状态**: 待修复
- **文件**: `src-tauri/src/commands/ai/util.rs:64`
- **重现步骤**:
  ```bash
  # 在干净工作树（无本地未提交修改）下编译
  git stash && cargo check -p datazen --lib && git stash pop
  ```
- **实测错误日志**:
  ```
  error[E0599]: no variant or associated item named `Relaxed` found for enum
  `datazen_ai_api::AiDataEgressLevel` in the current scope
    --> src-tauri/src/commands/ai/util.rs:64:51
  ```
- **根因**: Commit `ca11d711a` 的提交信息称"safety gate fallback to Relaxed"，但 `AiDataEgressLevel` 枚举（`packages/ai-api/src/types.rs:49`）只有 `Strict`、`SampleMasked`、`Unrestricted` 三个变体，没有 `Relaxed`。Commit 中写入了 `AiDataEgressLevel::Relaxed`，导致编译失败。
- **修复方案**: 将 `AiDataEgressLevel::Relaxed` 改为 `AiDataEgressLevel::Unrestricted`（本地工作树已有未提交修改）。
- **影响范围**: 整个 crate 无法编译。

---

## 非回归失败（已知问题，非本轨引入）

### ai_chat_mcp_and_db_same_round

- **状态**: 已知问题（pre-existing）
- **说明**: 该集成测试在 `d888917c0`（ai-safety-prompt 的前序 commit）就已经失败，与本轨改动无关。`integration_tests.rs` 在 commit 范围 `d888917c0..ca11d711a` 中未被修改。推测是 `list_connections` 工具结果经 `redact_for_gate` 后丢失了连接名称，属于已有缺陷。
