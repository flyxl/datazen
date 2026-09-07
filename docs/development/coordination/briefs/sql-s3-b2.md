# Brief: sql-s3-b2 — 导航与 AI 草稿桥（S3-B1 合流后串行）

> 依赖：S3-B1 已 PASSED 并合流（消费其新建的 bridge contracts）。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S3-B2（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s3-b2/progress.md`。

## 独占写锁

可建/可改：S3-B1 新建的 bridge contracts/协调 hook、`ContentView.tsx` 薄装配、
`PanelContentRenderer.tsx` 薄装配、`src/components/ai/AiChatPanel.tsx` 及 tests。
QueryPanel 只消费 S1-B 预留的 optional boundary prop；若未预留，由协调者精确授权改
`src/windows/connection/query/contracts.ts`，不得改执行逻辑。

## 步骤

1. 定义 `openRelation`、`copyRelationDdl`、`openAiChatDraft` callbacks。
2. 复用现有 table selection / panel handler 打开数据与结构页。
3. ContentView 持有 pending draft request，下传 callback 到 QueryPanel boundary。
4. AiChatPanel 增加 backward-compatible draft / focus / ack props。
5. 未配置 AI 时不消费；已有非空草稿时执行 plan §4.7 冲突策略；streaming 时可预填但不发送。
6. 不发送消息、不清空历史、不建第二个 chat store。

## 测试

callback 透传、身份匹配、单次 ack、侧栏打开、focus、未配置 AI、streaming、已有未发送草稿冲突。

## 完成定义

- 深层功能可经 callback 请求导航/Chat；无 DOM query hack、无 store 越层调用。
