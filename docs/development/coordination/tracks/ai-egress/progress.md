# Track: ai-egress (Phase 2.6 Egress FR-13 + Phase 2.2 QuestionBlock FR-09)

## 状态: IN_PROGRESS

## 目标
1. `egress_summary` 首 chunk 下发 — AI 首次回复包含连接/表数量摘要
2. `AiEgressNotice` 组件常驻化 — 非严格模式底部横幅
3. 严格模式单行提示保留
4. 删除普通 chat 的 `build_connections_context`（防泄露全量连接清单）
5. QuestionBlock 折叠保留 + answers 存 id（Phase 2.2 补全）
6. i18n：en/zh-CN Egress/进度/确认/用量文案

## 文件清单
- 后端：`src-tauri/src/commands/ai/chat.rs`（egress_summary chunk）、`src-tauri/src/commands/ai/util.rs`（emit 增强）
- 前端：`src/components/ai/AiEgressNotice.tsx`（新）、`src/components/ai/AiChatPanel.tsx`（集成）、`src/components/ai/QuestionBlock.tsx`（折叠）
- i18n：`src/locales/en/ai.ts`、`src/locales/zh-CN/ai.ts`
- 测试：前端组件测试 + egress 行为断言
