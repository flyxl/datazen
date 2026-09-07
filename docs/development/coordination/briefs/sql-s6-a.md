# Brief: sql-s6-a — Ask in Chat

> 依赖：S3-B2（ContentView/AiChatPanel bridge，只消费不改）。
> **不修改** locale（只消费 S1-D 已定义 key）。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S6-A + §6.9（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s6-a/progress.md`。

## 独占写锁

可建/可改：`src/components/query/QueryErrorPanel.tsx`、
`src/windows/connection/query/queryErrorChatPrompt.ts`、QueryPanel error-section 薄接线、
`PanelContentRenderer.tsx` 的 bridge prop 透传及 tests。
**禁止**：locale、新聊天引擎。

## 步骤

1. `QueryErrorPanelProps` 加 optional `onAskInChat`；既有 Retry/Fix/Explain/Diagnose 条件布局不变。
2. 复用 `src/lib/aiQueryActions.ts` 的 `buildQueryDiagnosisContext` 构建脱敏有界 prompt
   （错误信息、SQL、dialect、database/schema）；不重复实现脱敏。
3. 点击发带 panel/connection/session/context fingerprint 的 `AiChatDraftRequest`；
   展开 Chat、切 chat tab、预填并 focus；只有 textarea 成功写入后 ack。
4. 不自动发送；未配置时保留草稿并显示原配置入口。
5. 确认历史参数值、结果行、session IDs、secret fixture 不进 prompt。
   prompt SQL/error 各限 4,000 字符（见 §6.9）。

## 测试

按钮显隐、一次点击一次 request、已有 actions 不变、脱敏/长度、侧栏/focus/未配置/streaming/非空草稿冲突。

## 完成定义

- 错误上下文能进现有 Chat 草稿；无新增聊天引擎、无自动数据外发。
- 相关验收：AC-16。
