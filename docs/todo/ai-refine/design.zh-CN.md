# AI-refine 技术设计

> 状态：Draft
>
> 对应 [PRD](./prd.zh-CN.md) FR-01~FR-20。现状引用：`src-tauri/src/ai/*`、`src-tauri/src/commands/ai/*`、`packages/ai-api`、`src/stores/aiStore.ts`、`src/components/ai/*`。

## 1. 总体架构（增量）

```text
前端                          IPC                        后端
AiInput/AiChatPanel/Nl2SqlPanel ── ai_chat/ai_generate_sql ─▶ resolve_ai
  │ onStop ── ai_cancel{requestId} ─▶ CancellationRegistry ─▶ stream_complete(tx, cancel)
  │                                    ▲
  │                            run_streaming_tool_loop(guard: rounds/token/time)
  │                              ├─ execute_db_tool (只读并行)
  │                              └─ execute_mcp_tool (写类需 confirm_token)
  └─ sessions: Record<sessionKey,_> ◀── ai:stream-chunk{content,reasoning,done,usage}
```

新增/改造模块一览：

| 模块 | 变更 |
|------|------|
| `packages/ai-api` | `stream_complete` 加 `cancel: CancellationToken` 参数（默认 trait 方法保持兼容）；`ModelInfo` 加 `max_window/supports_reasoning`；`AiError::Cancelled` |
| `src-tauri/src/ai/cancel.rs`（新） | `CancellationRegistry: request_id -> CancellationToken` |
| `src-tauri/src/commands/ai/chat.rs` | tool loop guard（rounds/token/time）+ tool 结果预算 + 只读并行 + 写确认 |
| `src-tauri/src/ai/safety.rs` | 删除旧路，统一 `redact_for_gate`，敏感/结果键新规则 |
| `src-tauri/src/ai/context.rs` | DDL 补 FK/索引/注释；token 估算；表召回排序 |
| `src-tauri/src/ai/schema_pipeline.rs` + `budget.rs` | 动态 budget（model_window×0.15） |
| `src-tauri/src/ai/prompt_resolver.rs` | 全 async 加载，`{{dialect_notes}}`，残留断言 |
| `src/stores/aiStore.ts` + `src/stores/ai/*` | 会话按 key 隔离 + 持久化；NL2SQL streamingSql 分离；错误/清空补清 reasoning |
| `src/components/ai/*` | 流式预览、Markdown、代码块动作、自动跟随、虚拟化 ContextPicker、常驻 Egress |

`AI_PROTOCOL_VERSION`：`stream_complete` 签名变更 → `1 -> 2`（`packages/ai-api/src/lib.rs`），Provider 插件同步升级（本仓库内置 5 个同步改）。

## 2. 流式取消（FR-01）

### 2.1 后端

```rust
// ai/cancel.rs
#[derive(Clone, Default)]
pub struct CancellationRegistry {
  inner: Arc<Mutex<HashMap<String, CancellationToken>>>,
}
impl CancellationRegistry {
  pub async fn register(&self, request_id: &str) -> CancellationToken;
  pub async fn cancel(&self, request_id: &str) -> bool;
  pub async fn unregister(&self, request_id: &str);
}
```

- `AppState` 新增 `cancel_registry: CancellationRegistry`。
- `ai_generate_sql/ai_chat` 入口：`register(request_id)` → `tokio::select!{ run => , cancel.cancelled() => Err(Cancelled) }` → `unregister`。
- `AiProvider::stream_complete(&self, req, tx, cancel: CancellationToken)`：各 `protocol/*` 的 SSE 循环每次 `recv` 前 `select!` 检查 cancel，取消即停发并返回 `Ok(())`（由上层发 `done{cancelled:true}`）。
- 新 IPC `ai_cancel{request_id}` → `registry.cancel()`，返回是否命中。
- `run_streaming_tool_loop` 每轮 tool 执行前检查 cancel；tool 为 MCP 网络调用时同样 `select!`。

`StreamChunk` 新增 `cancelled: bool`（默认 false），前端据此标草稿态。

### 2.2 前端

- `aiCommands.cancel(requestId)` 封装 `ai_cancel`。
- `useAiStreamControl(requestId)` hook：`isStreaming + stop()`，`stop()` 先发 cancel IPC，再本地把 `streamContent/Reasoning` 落为 `cancelled` 草稿消息。
- `AiChatPanel` 补 `onStop`；`AiInput` streaming 时右下按钮即停止；NL2SQL 生成按钮变停止态。
- 超时：前端 60s 无 chunk 自动提示“似乎卡住，可停止/重试”（不自动 cancel）。

## 3. NL2SQL 流式预览（FR-02）

- `aiStore.nl2sql` 新增 `streamingSql: string`（原始累积）与既有 `generatedSql`（终值清洗）分离；`handleStreamChunk` 对 NL2SQL requestId 增量拼 `streamingSql`。
- `Nl2SqlPanel` 新增预览区（默认折叠？不，生成中自动展开，平时折叠）：
  ```tsx
  { (nl2sql.isGenerating || nl2sql.generatedSql) && <SqlPreview streaming code onApply onApplyChart onCopy onStop/> }
  ```
- 增量 SQL 容错显示：`extractSqlStreaming(accumulated)` —— fence 未闭合取 fence 内全部；无 fence 取 SQL 行（含 continuation），不做终值 trim。
- `done` 时 `generatedSql = extractSqlFromResponse(streamingSql)` + `onSqlChange(generatedSql)`（默认）；设置项 `nl2sql.streamIntoEditor`（默认 false）为 true 时每 500ms 节流同步编辑器。
- 沿用 `useResizable(nl2sql-panel-height)` 高度记忆（frontend/ai.md §1.4 既有）。

## 4. 会话隔离与显示修正（FR-03）

### 4.1 数据模型

```ts
type SessionKey = string; // `${connectionId ?? dbSessionId}::${database ?? ''}`
interface AiSessions {
  chatSessions: Record<SessionKey, ChatSessionState>;
  activeChatKey: SessionKey | null;
}
```

- `sendChatMessage(key, ...)`、`handleStreamChunk` 按 `payload.requestId -> key` 路由（维护 `requestId -> key` 映射，done/error 后清理）。
- 持久化：`localStorage 'datazen.aiSessions.v1'` 存各 key 最近 50 轮（content+reasoning+toolCalls 摘要，不存大 tool 结果原文，超 4KB 截断），版本化 key 便于迁移。
- NL Filter：`parsedFilters: Record<tableKey, FilterCondition[]>`，`tableKey = dbSessionId::database::table`。
- Explain：`explainCache: Record<hash(sql+plan), ExplainAnalysis>`，LRU 20。
- 修复：`AiChatPanel` 思考条件 `!streamContent && !streamReasoning`；三处补清 `streamReasoning`。

### 4.2 兼容

- 旧单例读法 `s.chatSession` 保留为 getter（返回 active key 会话），测试不 break；新代码走 `getChatSession(key)`。

## 5. JSON 解析与长任务进度（FR-04）

### 5.1 `parse_ai_json` v2

```rust
fn find_first_balanced_json(s: &str) -> Option<&str> // {}/[] 平衡 + 字符串/转义感知
```

流程：`strip_fences_all`（多个 fence 逐个试）→ 直接 parse → `find_first_balanced_json` parse → repair 重问 1 次（system 追加 only-JSON，temp 0）→ 报错（含 scenario/finish_reason/长度，不含原文）。

### 5.2 长任务进度

- 非流式 6 命令统一包一层 `run_with_progress(cmd_label, fut, on_chunk_like)`：前端收 `ai:task-progress{taskId,phase:started|done,label}`；`started` 后 60s 无 `done` 前端显示超时条（重试/取消——取消仅对流式有效，非流式为“忽略结果”）。
- `taskId = Uuid`，与现有 `requestId` 区分（非流式无 requestId）。

## 6. Tool Loop 护栏（FR-05）

```rust
pub struct ToolLoopGuard {
  pub max_rounds: usize,      // 默认 6，上限 10（配置 ai_tool_max_rounds）
  pub max_total_tokens: usize,// 默认 24000（usage 累加，未知按 chars/4 估）
  pub max_duration: Duration, // 默认 180s
  pub per_tool_cap: usize,    // 2048 bytes
  pub per_round_cap: usize,   // 12288 bytes
}
```

- 每轮后累加 `usage`（无 usage 则估算），超限 → 推送已产出 + `"(truncated: tool budget exceeded)"` 收尾 done。
- tool 结果 `truncate_and_summarize`：保留前 N 行 schema/列名，行样本截断并注 `…N more rows omitted`。
- `Unknown` tool：记 warn，直接 `done{error}` 不回塞。
- 只读 DB tool 同轮 `join_all` 并行（`list_*`/`search`/`get_schema` 白名单）；写类 MCP（`x-write:true` 或名单）需 `confirm_token`：后端返回 `need_confirm{toolCallId, preview}`，前端弹窗确认后 `ai_confirm_tool{toolCallId}` 才执行（超时 120s 自动拒绝）。
- loading 文案：`streamMcpToolName` 改为栈（当前执行中 tool 名），chunk 携带 `activeTool` 字段，前端显示栈顶。

## 7. 安全门统一 + Egress（FR-06/FR-13）

### 7.1 `safety.rs` 重构

- 删除 `redact_for_ai/redact_for_egress/sanitize_json/sanitize_json_with_gate` 旧路；唯一入口 `redact_for_gate(value, gate)`。
- 调用点（chat/generate/diagnose/explain/filter/schema_doc/connection/history）全改为 gate（gate 来自 active profile `safetyGate`，默认 strict）。
- 敏感 key：`key_words` 切分后 token 全等匹配 SECRET_WORDS（`key` 仅独立 token 命中），白名单 `{"keyboard","monkey","donkey","turnkey","hockey"}` 豁免；`is_result_key` 改正则 `^(results?_?|rows?|records?|data|payload|samples?_?)` 前缀 + 规范化后匹配。
- 常量：`MAX_JSON_DEPTH/ARRAY/OBJECT` 进 gate 可配（默认 4/100/100）。

### 7.2 Egress 常驻

- 后端 `ai_chat/ai_generate_sql` 返回前计算 `egress_summary{tables_sent, tokens_est, rows_sent:0|sampled, truncated}`，随首个 chunk 下发，前端存 `session.egress`。
- `AiEgressNotice` 改为常驻单行（严格也显示）+ 展开详情；文案走 i18n（中英）。
- `build_connections_context` 仅 workflow_generate 调用；普通 chat 删除该调用，依赖 `list_connections` tool 按需拉取。

## 8. 渲染与 Chat 交互（FR-07/FR-08/FR-09）

- Markdown：引入 `marked`（或现有依赖复用）+ `DOMPurify` sanitize，仅白名单标签（p/b/i/code/pre/ul/ol/li/table/a），链接默认 `target=_blank` 禁 `javascript:`；流式按 block 增量（未闭合 fence 按文本）。
- 代码块：`AiCodeBlock` 加 toolbar（运行/插入/新建查询/复制/全屏/方言），SQL 默认 `SqlCodeBlock` 高亮 + 行号；yaml/json 加基础高亮（复用 CodeMirror 按需加载，避免首屏增重）。
- 自动跟随 hook：
  ```ts
  useAutoScroll(deps, { threshold: 120 }) -> { atBottom, unread, jumpToBottom }
  ```
  非底部显示 pill `↓ N 条新消息`。
- `ChatSession.messages[].id = uuid`，key 用 id；`tool` role 浅色可折叠；`QuestionBlock` 提交后折叠保留，answers 存 `{id,label,text?}[]`。
- 气泡 footer（FR-16 预留）：`usage{prompt,completion,total} + elapsedMs`，由 done chunk 携带。

## 9. ContextPicker（FR-10）

- 后端新增 `context_list_files(query, limit)`（现有 `context_list_files` 加 query 参数，前缀/子串匹配 + 白名单 + 上限 100）。
- 前端：query 防抖 200ms 调后端；表走 `search_tables`（大库）/`getTables`（小库）；列表虚拟化（`@tanstack/virtual` 或手写 windowing，>200 行启用）。
- `keydown` 下沉到 textarea（`AiInput.handleKeyDown` 内处理 picker 导航），删除 window capture。
- recent：`localStorage 'datazen.contextRecent.<connectionId|'global'>'`，8/桶。
- `.ctx.yaml`：解析后调 `getTables` 校验，缺失标红；支持 `schema.*` 与 `*`（展开上限 200，超限提示收窄）。

## 10. Schema 上下文（FR-11）

- DDL 行格式升级：
  ```
  users (id int PK, name varchar NOT NULL, org_id int FK->orgs(id) /*索引:idx_users_org*/, created_at timestamptz DEFAULT now() /*备注:创建时间*/)
  ```
  注释截 40 字并过 `redact_for_gate`；默认值过敏感规则（含 secret/token 即隐去）。
- token 估算：`tokens ≈ cjk*1.6 + ascii*0.3`（CJK 按 Unicode 范围计），替代 `len/4`。
- 表召回（`context.rs::rank_tables(question, tables, schema_cache)`）：token 化问题关键词 × 表名/列名/注释命中加权，Top30 + pinned + 当前表（去重），fallback 扫描按此序。
- 动态 budget：`budget::for_model(max_window) = clamp(max_window*0.15, 2000, 16000)`；`ModelInfo.max_window` 缺失回退静态常量。
- 表名列表截断 200（`format_table_names_block` max_names 参数化），阈值判断前移召回前。

## 11. Prompt（FR-12）

- `load_templates` 改 `tokio::fs` async；`resolve` 用 `driver.driver_type()` 直接比较（去 `serde_json::to_value`）；`render_template` 返回 `(String, Vec<missing>)`，missing 非空 warn 并替换空串。
- 模板新增 `{{dialect_notes}}`；驱动 `prompt_overrides()` 可选 `dialect_notes` 字段（`driver-api` `PromptTemplate` 加字段，默认空，`PROTOCOL_VERSION` 不变——纯加字段且 serde 默认）。
- App 启动空闲（`tauri::async_runtime::spawn` + 延迟 2s）预热 `ensure_ready`。

## 12. P2（FR-14~FR-20）要点

- 能力矩阵：`ProviderListItem{supports_streaming,supports_tools,supports_reasoning,max_window}`（`registry.rs` 已有前两项，补后两项：reasoning 按 provider 静态声明，max_window 按 model 前缀表 + 远程 `ModelInfo`）。
- 超时：`protocol/mod.rs` 常量改为可配（connect 8s / first-token 60s / chunk 60s），`map_http_error` 400 透出脱敏 `{code,message}`（截 300 字）；429 读 `Retry-After` 退避 1 次。
- 用量：done chunk 固定带 `usage+elapsedMs`；设置页本地单价表估算（`model_pricing.json` 资源文件，可覆盖）。
- 反馈：`ai_feedback{messageId, kind, note?}` 存 `{data_dir}/ai_feedback.jsonl`（脱敏后 prompt hash，不存原文）；设置页导出。
- Workflow：`sendWorkflowChatMessage` 改走同一 `resolve_seed(dbSessionId, database, pinned)`（`SchemaContextPipeline` 复用），连接解析统一 `resolveDbSession(connectionId)` helper；YAML dry-run 用现有 `validateWorkflowFields + parseValidatedWorkflowDefinition` 不落盘预览。
- 复用：新建 `src/components/ai/shared/{AiEmptyState,ChatBubble,AutoScroll,EgressLine}.tsx`，两面板迁移；连接清单文案进 i18n。
- MCP 治理：`McpToolInfo` 加 `write: bool`（启发式：名含 write/delete/drop/update/create/insert + `input_schema.x-write` + 手动覆盖存 settings）；AI 面板 header 显示 `MCP n 个可用`。

## 13. 风险与回退

- `stream_complete` 签名变更：内置 Provider 同步改；`AI_PROTOCOL_VERSION 1->2`，旧外部 Provider 加载时 warn 并禁用。
- Markdown 引入：包体积 +XSS 面，`DOMPurify` 必备 + CSP 保持。
- 会话持久化：localStorage 配额，超限 LRU 丢弃 + 版本 key 迁移失败则清空重建（可接受）。
- 每 FR 独立开关（settings 高级或常量），P0 默认全开，P1 可灰度。
