# Track Progress: sql-s1-d

- Task: i18n key 契约
- Plan: docs/todo/sql-editor/implementation-plan.md
- BaseCommit: 5589ba148
- Worktree: .worktrees/datazen-sql-s1-d
- Branch: feature/sql-s1-d
- Phase: PASSED
- Agent: tester-sql-s1-d
- CodingCommit: 4435b40c1
- TestCommit: ef5868a24
- MergeCommit: —
- LastHeartbeat: 2026-09-05T02:45:00+08:00
- UnresolvedBugs: 0

## 详细进展

- 2026-09-05 02:27 BOOTSTRAP: 轨道已创建，准备派发编码子代理
- 2026-09-05 02:32 CODING: 已读 AGENTS/coder/plan/progress；梳理 AC-01~AC-21 文案域，开始写入 en/zh-CN query.ts 与 settings.ts
- 2026-09-05 02:35 READY_FOR_TEST: 新增 94 个 `query.editor.*` + 2 个 `settings.editorInsertValueHints*` key；`npx tsc --noEmit` 0 报错；`npx vitest run src/locales/` 18/18 通过
- 2026-09-05 02:30 TESTING: BOOTSTRAP — worktree feature/sql-s1-d @ bc6dcfcf9，工作区 clean
- 2026-09-05 02:45 PASSED: 独立复验通过；en/zh-CN query.editor 94 key + settings 2 key 100% 对称；AC-01~AC-17 i18n 契约完备；AC-18~AC-21 非本轨 i18n 范围

## 测试报告（Tester）

| 套件 | Coder 自报 | 独立实测 |
|------|-----------|---------|
| `npx vitest run src/locales/` | 18/18 | **18/18** |
| `npx tsc --noEmit` | 0 错误 | **0 错误** |

- **结构对称性**：`query.editor.*` en=94 / zh-CN=94，零孤立 key；`settings.editorInsert*` en=2 / zh-CN=2，对称
- **占位符一致性**：96 个新增 key 的 `{placeholder}` 在 en/zh-CN 间完全匹配
- **契约审查**：AC-01~AC-17 所需文案 key 均已定义；AC-18~AC-21 为代码/E2E/性能/dbx 审计项，不由 S1-D 交付
- **代码审查**：变更范围仅限 locale 域文件，命名遵循 `query.editor.*` 契约，Mac/Win 平台分 key，无业务逻辑改动
- **覆盖率**：locale 数据文件由现有 `locales.test.ts` 基线 shipping pair parity 测试守护，无需额外单测

## i18n 契约摘要

- **域**：`query.editor.*`（语句 frame/gutter/快捷键、补全/签名、Intention、Paste as IN、拖拽、参数历史、执行门禁、Ask in Chat）；`settings.editorInsertValueHints*`（INSERT Inlay Hint 设置，AC-09）
- **平台快捷键**：macOS 与 Win/Linux 分 key（`.Mac` / `.Win` 后缀或独立 tooltip key）
- **发布前**：其他语言需通过 i18n-sync 补齐（本轨仅 en/zh-CN）
