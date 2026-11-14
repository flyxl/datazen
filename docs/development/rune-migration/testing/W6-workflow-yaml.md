# TN W6 工作流 YAML 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`6a0a4c1c3` `feat(rune): workflow YAML through the editor's document model (issue-rune W6)`
- 两个文件：`crates/datazen-ui/src/sql_editor.rs`（`plain_panel`：编辑器文档模型 + 行渲染 + IME 光标，无 SQL 高亮——空 spans）、`crates/datazen-ui/src/lib.rs`（`workflow_yaml` 序列化、`workflow_yaml` 编辑器状态与种子、app_key/app_ime 路由到 YAML 文档 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 99 passed（净增 1：`the_workflow_yaml_edits_and_undoes_through_the_document_model`），5.49s；`datazen-ui-api` 45 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「工作流 YAML 用 SQL Editor 的文档模型，不做 SQL 高亮。测：撤销仍可用」核对：打开 audit 编辑器 → YAML 面板呈现 "name: audit
steps:
  - compare structures…"；行渲染复用编辑器的行渲染器（sql-line-* 键）且 spans 为空（无 SQL 高亮，代码核对）；键入 "X" 进文档；Command+Z 撤销回到原文——Q1 的历史机制在 YAML 文档上同样可用。通过

## 缺陷

无。

## 结论与遗留

**放行。** YAML 走同一文档模型（多光标/IME/撤销共享），高亮按任务要求关闭。遗留：YAML 修改不回写步骤列表（单向视图）；运行结果是 W7。
