# AI-refine 实施方案

> 状态：Draft
>
> 依据 [PRD](./prd.zh-CN.md) 与 [技术设计](./design.zh-CN.md)。分支：`feature/ai-refine`。原则：P0 先行、每 FR 独立可验、单测先行、不改默认安全行为。

## 1. 实施原则

- M1（P0）合并前保持 `AI_PROTOCOL_VERSION` 兼容过渡：先加新 trait 方法默认实现，再切版本号。
- 每个 Phase 结束：`cargo test -p datazen --lib ai` + 相关 `npx vitest run` + 手工验证表打勾。
- 安全门重构期间新旧并存（feature flag `AI_GATE_V2`，默认开），M1 末删除旧路。
- 不动 `drivers-registry / resolve-drivers / Pro 构建`。

## 2. M1：P0（FR-01~FR-06）

### Phase 1.1 取消 + 显示修正（FR-01 部分、FR-03 修复项）

文件：

- 新 `src-tauri/src/ai/cancel.rs`（`CancellationRegistry` + 单测）。
- 改 `src-tauri/src/ai/mod.rs`（export）、`src-tauri/src/state.rs` 或 `AppState` 定义处（加字段）、`src-tauri/src/commands/ai/mod.rs`（注册 `ai_cancel`）。
- 改 `packages/ai-api/src/traits.rs`（`stream_complete` 加 `cancel: CancellationToken`，默认实现忽略；加 `AiError::Cancelled`）、`src-tauri/src/ai/protocol/*.rs`（SSE 循环 `select!` cancel）。
- 改 `src-tauri/src/commands/ai/chat.rs`、`generate.rs`（register/select/unregister，`StreamChunk{cancelled}`）。
- 改 `src/commands/ai.ts`（`cancel()`）、`src/stores/aiStore.ts`（`stopChat/stopNl2sql`，三处补清 `streamReasoning`）、`src/components/ai/AiChatPanel.tsx`（补 `onStop` + 思考条件）。

测试：Rust：registry 注册/取消/注销单测 + protocol cancel 中断测试（wiremock 慢流）；前端：`aiStore.stop` 单测（草稿保留 + 推理清空）。

验证：生成中停止 ≤500ms；草稿含已产出；后端日志无残留轮次。

### Phase 1.2 NL2SQL 流式预览（FR-02）

文件：`src/stores/aiStore.ts`（`streamingSql`）、`src/lib/extractSql.ts`（加 `extractSqlStreaming`）、`src/components/ai/Nl2SqlPanel.tsx`（预览区 + 停止态 + `useResizable` 复用）、`src/components/ai/SqlPreview.tsx`（新，可先内联后抽）。

测试：`extractSqlStreaming` 单测（fence 未闭合/无 fence/混合文本）；`Nl2SqlPanel.test.tsx` 补流式累积断言。

验证：10s 生成全程可见；停止保留；完成写入编辑器一致。

### Phase 1.3 会话隔离（FR-03）

文件：新 `src/stores/ai/sessions.ts`（key 计算 + 持久化 + LRU），改 `src/stores/aiStore.ts`（`chatSessions/activeChatKey/requestId->key` 路由）、`src/components/ai/AiChatPanel.tsx`（按 active key 读写）、`src/components/ai/NlFilterInput.tsx` + `ExplainPanel.tsx`（按 key 缓存）。

测试：多 key 隔离单测；持久化 round-trip 单测；Explain LRU 单测。

验证：双连接各 3 轮互不串；刷新恢复；切 Tab 不丢。

### Phase 1.4 JSON + 进度（FR-04）

文件：`src-tauri/src/commands/ai/util.rs`（`find_first_balanced_json` + `strip_fences_all` + repair 重问 + 进度包装）、各 `generate.rs` 非流式命令接入。

测试：解析器单测（前导文本/多 fence/截断 `length` 提示）；repair mock provider 测试。

验证：前导“好的…”可解析；60s 超时条出现。

### Phase 1.5 Tool Loop 护栏（FR-05）

文件：`src-tauri/src/commands/ai/chat.rs`（`ToolLoopGuard` + 预算 + 并行 + confirm 流程 + activeTool 栈）、新 IPC `ai_confirm_tool`、前端确认 Dialog（`src/components/ai/McpConfirmDialog.tsx`）、`aiStore` loading 文案改栈顶。

测试：mock provider 发散 tool 6 轮收敛；大结果摘要化单测；Unknown tool 终止单测。

验证：手工 MCP 写工具弹窗；10 轮发散被截断收尾。

### Phase 1.6 安全门统一（FR-06）

文件：`src-tauri/src/ai/safety.rs`（删旧路 + 新规则 + gate 可配常量）、全部 `redact_for_egress` 调用点切换、单测更新（白名单 + 前缀正则 + 行为一致性）。

测试：`cargo test -p datazen --lib ai::safety` 全过；新增 `keyboard/monkey` 豁免、`resultRows` 前缀用例。

验证：旧严格输出 diff 为空（跑一致性脚本）。

M1 合并门槛：AC-01~AC-06 全过；`AI_PROTOCOL_VERSION=2`；旧安全代码删除。

## 3. M2：P1（FR-07~FR-13）

### Phase 2.1 渲染与代码块（FR-07/FR-08）

- 引入 `marked + DOMPurify`（先核 `pnpm` 已有无，无则加），改 `AiMessageContent`，加 `AiCodeBlock` toolbar（运行/插入/新建/复制/全屏/方言）。
- 单测：XSS 过滤（`javascript:` 链接、script 标签）；流式未闭合 fence 快照。

### Phase 2.2 Chat 交互（FR-09）

- 新 `useAutoScroll` + pill；`ChatBubble` 按 id key；tool role 样式；`QuestionBlock` 折叠保留 + answers 存 id。
- E2E/单测：滚动跟随（mock 几何）、问答回看。

### Phase 2.3 ContextPicker（FR-10）

- 后端 `context_list_files(query, limit)`；前端防抖 + 虚拟化 + keydown 下沉 + recent 分桶 + ctx.yaml 校验/通配。
- 性能验证：千表库 picker 打开 <300ms，输入过滤 <100ms。

### Phase 2.4 Schema 上下文（FR-11）

- `context.rs` DDL 升级 + 估算 + `rank_tables`；`budget.rs` 动态化；`schema_pipeline` 截断参数化。
- 单测：FK/注释样例 DDL 快照；召回排序（users/orders 场景）；预算 clamp 边界。
- 抽测：20 问 NL2SQL 准确率对比（旧 vs 新，目标 ≥80% 新版胜/平）。

### Phase 2.5 Prompt（FR-12）

- `prompt_resolver.rs` async 化 + `{{dialect_notes}}` + 残留断言；`driver-api` `PromptTemplate.dialect_notes`（默认空）；启动预热。
- 各内置驱动按需补方言小抄（PG/MySQL/SQLite 优先）。

### Phase 2.6 Egress（FR-13）

- `egress_summary` 首 chunk 下发；`AiEgressNotice` 常驻化 + i18n；删除普通 chat 的 `build_connections_context`。
- 验证：严格模式仍有单行提示；抓包/prompt 日志确认无全量连接清单。

M2 合并门槛：AC-07~AC-08；包体积增量 <150KB（markdown 相关）。

## 4. M3：P2（FR-14~FR-20）

| Phase | 内容 | 文件 |
|-------|------|------|
| 3.1 能力矩阵 | `ProviderListItem` 补字段，前端动态开关，兼容矩阵文档 | `registry.rs`、`ai-api/types.rs`、设置页 AI 区 |
| 3.2 超时重试 | 超时常量可配，429 退避，400 脱敏透出 | `protocol/mod.rs` + 各 protocol |
| 3.3 用量 | done chunk 带 usage/elapsed，气泡 footer，设置页估算 | `util.rs emit`、`ChatBubble`、单价资源 |
| 3.4 反馈 | `ai_feedback` JSONL + 导出 | 新 `commands/ai/feedback.rs` + 消息 toolbar |
| 3.5 Workflow | 复用 seed，统一连接解析，dry-run 预览 | `WorkflowChatPanel`、`chat.rs resolve_seed` 抽取 |
| 3.6 复用+MCP治理 | 共享组件抽取，MCP 读写分类 | `components/ai/shared/*`、settings MCP 区 |

## 5. 测试策略

- Rust：`cargo test -p datazen --lib ai`（安全/解析/预算/召回/cancel/guard），`cargo test -p datazen-ai-api`。
- 前端：`npx vitest run src/stores/__tests__/aiStore.test.ts src/components/ai/__tests__/` 全过，新增单测与实现同 PR。
- 手工（M1/M2 必做）：取消、NL2SQL 流式、双连接隔离、发散 tool、写 MCP 确认、Egress 提示、大库 picker。
- 回归：`interaction-and-testing-principles.md` 三维自查（解决否/误伤否/下一步顺否）每 Phase 记录。

## 6. 文件清单（汇总）

后端新增：`ai/cancel.rs`、`components/ai/McpConfirmDialog.tsx`（前端）、`stores/ai/sessions.ts`（前端）、`components/ai/shared/*`（前端 M3）、`components/ai/SqlPreview.tsx`（前端，可内联起步）。

后端改造：`ai-api/traits.rs + types.rs + lib.rs`、`ai/mod.rs`、`ai/protocol/*`、`ai/safety.rs`、`ai/context.rs`、`ai/schema_pipeline.rs`、`ai/budget.rs`、`ai/prompt_resolver.rs`、`commands/ai/chat.rs + generate.rs + util.rs + mod.rs`、`AppState`。

前端改造：`commands/ai.ts`、`stores/aiStore.ts`、`components/ai/{AiChatPanel,Nl2SqlPanel,AiMessageContent,AiCodeBlock,AiEgressNotice,AiInput,ContextPicker,ExplainPanel,NlFilterInput,WorkflowChatPanel,DiagnosisPanel}.tsx`、`lib/extractSql.ts + extractQuestions.ts`（answers 结构）、i18n `en/zh-CN`（Egress/进度/确认/用量文案）。

## 7. 风险与应对

| 风险 | 应对 |
|------|------|
| trait 签名变更破坏外部 Provider | 默认方法过渡 + `AI_PROTOCOL_VERSION=2` + 加载期版本校验禁用旧件 |
| Markdown XSS | DOMPurify + 白名单 + CSP 不动；默认关闭 HTML 直通 |
| 会话存储爆配额 | LRU + 4KB/消息截断 + 版本 key 失败自清空 |
| 预算动态导致 prompt 骤变 | 上限 clamp + 灰度开关 + 新旧 diff 日志一周 |
