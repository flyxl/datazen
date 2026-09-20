# AI-Refine 功能代码审查报告

> 审查时间：2026-02-14
> 审查分支：`feature/ai-refine`（commit `c268fa919`）
> 对照文档：`docs/todo/ai-refine/prd.zh-CN.md`、`docs/todo/ai-refine/design.zh-CN.md`

---

## 一、审查总览

| 类别 | 数量 |
|------|------|
| 🔴 阻塞级 BUG（必须修复） | 5 |
| 🟡 中等严重度 BUG（建议修复） | 6 |
| 🟢 低严重度 / 建议项 | 5 |
| ✅ 已正确实现的 FR | 12/20 |

---

## 二、阻塞级 BUG（必须修复）

### BUG-01: AiChatPanel 缺少 onStop — Chat 模式无法停止流式生成

- **严重度**: 🔴 高
- **文件**: `src/components/ai/AiChatPanel.tsx`
- **PRD 要求**: FR-01 "AiChatPanel 补传 onStop（当前缺失）"
- **问题描述**: `AiInput` 组件有 `onStop` prop 和 stop button（当 `onStop` 传入时显示），但 `AiChatPanel` 没有传 `onStop` 给 `AiInput`。对比 `Nl2SqlPanel` 正确实现了 `handleStop` → `aiCommands.cancel(requestId)`。
- **影响**: Chat 模式下流式生成无法停止，用户必须等完成或刷新页面。
- **修复方案**:
  ```tsx
  // AiChatPanel.tsx: 在 handleSend 附近添加
  const handleStop = useCallback(() => {
    if (chatSession?.requestId) {
      void aiCommands.cancel(chatSession.requestId);
    }
  }, [chatSession?.requestId]);

  // 传给 AiInput
  <AiInput onStop={handleStop} ... />
  ```

### BUG-02: ProviderCapabilities 未实现 — FR-14 能力矩阵缺失

- **严重度**: 🔴 高
- **文件**: `packages/ai-api/src/types.rs`、`packages/ai-api/src/traits.rs`
- **PRD 要求**: FR-14 "ai_get_providers 返回实测 supports_tools/supports_streaming/supports_reasoning/max_window"
- **问题描述**: PRD 要求 `ProviderCapabilities` struct 和 `capabilities()` trait method。当前代码中完全没有这两个概念。`ProviderListItem`（`config.rs`）也没有 `capabilities` 字段。前端无法按矩阵动态禁用 tool 开关与推理折叠。
- **影响**: P2 需求未实现，前端无法根据 provider 能力动态调整 UI。
- **修复方案**: 需要新增 `ProviderCapabilities` struct 和 `capabilities()` trait method，并在每个 provider 中实现 override。

### BUG-03: FeedbackButton 前端组件不存在 — FR-17 反馈闭环缺失

- **严重度**: 🔴 高
- **文件**: `src/components/ai/` 目录
- **PRD 要求**: FR-17 "每条 assistant 消息 👍/👎 + 复制 + 重试 + 固定 SQL"
- **问题描述**: 后端 `feedback.rs` 已实现 JSONL 存储和 CSV 导出，但前端完全没有 `FeedbackButton` 组件。assistant 消息气泡没有反馈按钮。
- **影响**: 反馈闭环完全不可用，用户无法对 AI 回复进行评价。
- **修复方案**: 需要创建 `FeedbackButton.tsx` 并集成到 `ChatBubble` 或 `AiMessageContent`。

### BUG-04: truncate_user_message UTF-8 panic

- **严重度**: 🔴 高
- **文件**: `src-tauri/src/ai/protocol/mod.rs:135-141`
- **问题描述**: `&msg[..MAX_LEN]` 在多字节 UTF-8 字符（如中文）截断时会 panic。当 `MAX_LEN=500` 落在多字节字符中间时，`&str` 切片 panic。
- **影响**: 用户输入含中文时，超过 500 字符即触发 panic，导致请求崩溃。
- **修复方案**:
  ```rust
  fn truncate_user_message(msg: &str) -> String {
      const MAX_LEN: usize = 500;
      if msg.len() <= MAX_LEN {
          msg.to_string()
      } else {
          let end = msg.char_indices()
              .take_while(|(i, _)| *i < MAX_LEN)
              .last()
              .map(|(i, c)| i + c.len_utf8())
              .unwrap_or(MAX_LEN);
          format!("{}…", &msg[..end])
      }
  }
  ```

### BUG-05: resolve_with_dialect 缓存路径跳过 dialect_notes 替换

- **严重度**: 🔴 高
- **文件**: `src-tauri/src/ai/prompt_resolver.rs:250-255`
- **问题描述**: `resolve_with_dialect` 步骤 4（文件缓存命中）直接 `return tpl.clone()`，跳过了 `{{dialect_notes}}` 替换。步骤 3 和步骤 5 均正确替换。
- **影响**: 使用文件缓存模板时，`{{dialect_notes}}` 占位符原样发送给 LLM，方言提示失效。
- **修复方案**: 在步骤 4 返回前执行 dialect_notes 替换：
  ```rust
  if let Some(tpl) = cache.get(&key) {
      let mut result = tpl.clone();
      if let Some(driver) = driver {
          if let Some(notes) = driver.dialect_notes() {
              result = result.replace("{{dialect_notes}}", &notes);
          }
      }
      return result;
  }
  ```

---

## 三、中等严重度 BUG

### BUG-04: ContextPicker 使用 window capture 劫持键盘事件

- **严重度**: 🟡 中
- **文件**: `src/components/ai/ContextPicker.tsx:243`
- **PRD 要求**: FR-10 "keydown 监听改到 textarea 本地，不再 window capture 劫持"
- **问题描述**: `window.addEventListener('keydown', handleKeyDown, true)` 使用 capture=true 劫持全局键盘事件，会拦截其他组件的按键（如输入框的 Escape、Tab 等）。
- **修复方案**: 改为在 `AiInput` 的 textarea 上绑定 keydown 事件，通过 prop 传递 picker 导航逻辑。

### BUG-05: ContextPicker 缺少 debounce — 每次输入立即触发搜索

- **严重度**: 🟡 中
- **文件**: `src/components/ai/ContextPicker.tsx`
- **PRD 要求**: FR-10 "输入防抖 200ms"
- **问题描述**: 无任何 debounce 实现，每次 query 变化立即调用后端 `context_list_files` 和 `getTables`，可能导致性能问题和不必要的 IPC 调用。
- **修复方案**: 使用 `useDebouncedCallback` 或 `setTimeout` 实现 200ms debounce。

### BUG-06: format_compact_ddl 列注释未截断至 40 字

- **严重度**: 🟡 中
- **文件**: `src-tauri/src/ai/context.rs:240-243`
- **PRD 要求**: FR-11 "COMMENT 'text'(截 40 字)"
- **问题描述**: `comment.replace('\'', "''")` 直接全量输出，长注释会膨胀 token 预算，可能挤占其他表的 DDL 配额。
- **修复方案**: 添加截断逻辑：
  ```rust
  let truncated = if comment.len() > 40 { &comment[..40] } else { comment };
  parts.push(format!("COMMENT '{}'", truncated.replace('\'', "''")));
  ```

### BUG-07: FR-13 build_connections_context 未从普通 chat 中删除

- **严重度**: 🟡 中
- **文件**: `src-tauri/src/commands/ai/chat.rs:820,865`
- **PRD 要求**: FR-13 "普通 ai_chat 不再发送全量连接清单；仅 workflow_generate 需要时发送"
- **问题描述**: `build_connections_context` 在普通 chat 路径中仍被调用（L820, L865），发送全量连接清单给模型。
- **修复方案**: 普通 chat 路径删除 `build_connections_context` 调用，仅 `workflow_generate` 保留。

### BUG-08: feedback.rs CSV 导出 .unwrap() + 文件覆盖竞态

- **严重度**: 🟡 中
- **文件**: `.worktrees/datazen-ai-feedback/src-tauri/src/commands/ai/feedback.rs:47,80,82`
- **问题描述**:
  1. `submit_feedback` 使用 `tokio::fs::write()` 全量覆盖文件，并发提交会丢数据（应改用 `tokio::fs::OpenOptions::new().append(true)`）
  2. CSV 导出 `w.write_record().unwrap()` 在写入失败时 panic
- **修复方案**: 改用 append 模式写入 JSONL；CSV 导出用 `.map_err()` 替代 `.unwrap()`

### BUG-09: render_template 不检查残留 {{}} 占位符

- **严重度**: 🟡 中
- **文件**: `src-tauri/src/ai/prompt_resolver.rs:397-403`
- **PRD 要求**: FR-12 "render_template 后断言无残留 {{}}（残留即 warn + 替换空串）"
- **问题描述**: `render_template` 返回渲染后字符串，但不检查是否有未替换的 `{{}}` 占位符。缺失变量会原样发给 LLM。
- **修复方案**: 渲染后用正则检测 `{{...}}` 残留，warn 日志并替换为空串。

---

## 四、低严重度 / 建议项

### BUG-10: ContextPicker 缺少虚拟化

- **严重度**: 🟢 低
- **文件**: `src/components/ai/ContextPicker.tsx`
- **PRD 要求**: FR-10 "列表虚拟化（>200 行）"
- **问题描述**: 当前使用简单的 `overflow-y-auto`，无虚拟化。大库表多时可能卡顿。
- **影响**: 低，实际表数通常 <200。

### BUG-11: sanitize_json_with_gate 被 PRD 标记为"死代码"但仍在使用

- **严重度**: 🟢 低（设计分歧，非 BUG）
- **文件**: `src-tauri/src/ai/safety.rs:257-308`
- **PRD 要求**: FR-06 "删除 sanitize_json_with_gate 恒真条件死代码"
- **问题描述**: PRD 说要删除，但实际 `sanitize_json_with_gate` 在 `redact_json_text_with_gate`（L308）中被调用，不是死代码。所有 match 分支均可达，`depth >= MAX_JSON_DEPTH` 条件非恒真。
- **建议**: 确认 PRD 设计变更，更新文档标注此函数保留。

### BUG-12: consume_assigned_value 中 4 处 .expect()

- **严重度**: 🟢 低
- **文件**: `src-tauri/src/ai/safety.rs:155,174,181,196`
- **PRD 要求**: AGENTS.md "生产路径禁止裸 unwrap/expect"
- **问题描述**: `consume_assigned_value` 在生产路径 `redact_for_gate → redact_plain_text → redact_sensitive_assignments` 中使用 `.expect("valid char boundary")`。虽然逻辑上 cursor 始终有效，但不符合策略。
- **修复方案**: 改为 `if let Some(ch) = ... { } else { break; }` 安全降级。

### BUG-13: sanitize_400_error 重复死代码块

- **严重度**: 🟢 低
- **文件**: `src-tauri/src/ai/protocol/mod.rs:107-127`
- **问题描述**: `sanitize_400_error` 中有两个完全相同的 match 分支（重复代码块），应合并。

### 建议-01: DOMPurify 已安装但未使用

- **文件**: `src/lib/htmlSanitizer.ts`
- **描述**: DOMPurify 作为依赖已安装（`package.json`），但当前使用自定义 `DOMParser`+DOM 遍历实现。建议统一到 DOMPurify 并配置 `ALLOWED_TAGS` / `ALLOWED_ATTR`。

### 建议-02: FR-19 共享组件未抽取

- **文件**: `src/components/ai/`
- **PRD 要求**: FR-19 "抽 AiEmptyState / ChatBubble / useAutoScroll / EgressLine 共享"
- **描述**: `src/components/ai/shared/` 目录不存在。`ChatBubble` 定义在 `AiChatPanel.tsx` 内部，`useAutoScroll` 已抽取为独立 hook，`EgressLine` 未抽取。

---

## 五、已正确实现的 FR 清单

| FR | 描述 | 状态 | 备注 |
|----|------|------|------|
| FR-01 | 流式可取消 | ⚠️ 部分 | 后端完整，AiChatPanel 缺 onStop（BUG-01） |
| FR-02 | NL2SQL 流式预览 | ✅ | streamingSql/generatedSql 分离正确，extractSqlStreaming fence-tolerant |
| FR-03 | 会话按连接隔离 | ✅ | SessionKey 格式正确，LRU 20/50，thinking condition 已修复，streamReasoning 清理完整 |
| FR-04 | JSON 鲁棒解析 | ✅ | find_first_balanced_json 实现完整，平衡括号+字符串感知 |
| FR-05 | Tool Loop 护栏 | ✅ | ToolLoopGuard 6 轮/24k/180s，Unknown tool 终止，写工具确认，MCP loading 文案 |
| FR-06 | 安全门统一 | ✅ | redact_for_egress 已删除，唯一入口 redact_for_gate，白名单 5 词，is_result_key 正则 |
| FR-07 | Markdown 渲染 | ✅ | marked + 自定义 sanitize，GFM 支持 |
| FR-08 | 代码块动作 | ✅ | run/insert/new/copy/fullscreen/dialect 六动作齐全 |
| FR-09 | Chat 交互细节 | ✅ | useAutoScroll threshold=120，unread pill |
| FR-10 | ContextPicker | ⚠️ 部分 | 有搜索和 recent，缺 debounce/虚拟化/本地 keydown（BUG-04/05/08） |
| FR-11 | Schema 上下文质量 | ⚠️ 部分 | FK/索引/默认值完整，rank_tables Top30，token 估算正确，注释截断缺失（BUG-06） |
| FR-12 | Prompt 方言 | ✅ | async 化，dialect_notes，驱动 hints，ensure_ready |
| FR-13 | Egress 常驻提示 | ⚠️ 部分 | 后端 egress_summary chunk 完成，前端 AiEgressNotice 存在但仅显示 context/relaxed，未消费 egress_summary 数据；普通 chat 仍发全量连接清单（BUG-07） |
| FR-14 | Provider 能力矩阵 | ❌ 未实现 | ProviderCapabilities struct 和 capabilities() trait method 缺失（BUG-02） |
| FR-15 | 超时重试 | ✅ | RetryConfig(3,1s,30s)，retry_with_backoff，map_http_error 400/429，parse_retry_after |
| FR-16 | 用量观测 | ❌ 未实现 | done chunk 未携带 usage+elapsedMs 给前端，设置页无用量估算 |
| FR-17 | 反馈闭环 | ⚠️ 部分 | 后端 feedback.rs 完成，前端 FeedbackButton 缺失（BUG-03） |
| FR-18 | Workflow 生成复用 | ❌ 未实现 | WorkflowChatPanel 未复用 PromptSeed/SchemaContextPipeline |
| FR-19 | 组件复用 | ❌ 未实现 | shared 目录不存在，ChatBubble 未抽取 |
| FR-20 | MCP 工具治理 | ✅ | mcp_needs_confirm 启发式分类（名称+x-write），AI 面板显示 |

---

## 六、BUG 汇总表

| # | 严重度 | 描述 | 文件 | PRD |
|----|--------|------|------|-----|
| BUG-01 | 🔴 高 | AiChatPanel 缺少 onStop，Chat 模式无法停止 | `AiChatPanel.tsx` | FR-01 |
| BUG-02 | 🔴 高 | ProviderCapabilities 未实现 | `ai-api/src/types.rs` | FR-14 |
| BUG-03 | 🔴 高 | FeedbackButton 前端组件不存在 | `src/components/ai/` | FR-17 |
| BUG-04 | 🔴 高 | truncate_user_message UTF-8 panic | `protocol/mod.rs:135` | FR-15 |
| BUG-05 | 🔴 高 | resolve_with_dialect 缓存跳过 dialect_notes | `prompt_resolver.rs:250` | FR-12 |
| BUG-06 | 🟡 中 | ContextPicker window capture 劫持 | `ContextPicker.tsx:243` | FR-10 |
| BUG-07 | 🟡 中 | ContextPicker 无 debounce | `ContextPicker.tsx` | FR-10 |
| BUG-08 | 🟡 中 | format_compact_ddl 注释未截断 40 字 | `context.rs:240` | FR-11 |
| BUG-09 | 🟡 中 | 普通 chat 仍发全量连接清单 | `chat.rs:820,865` | FR-13 |
| BUG-10 | 🟡 中 | feedback.rs CSV unwrap + 文件覆盖竞态 | `feedback.rs:47,80` | FR-17 |
| BUG-11 | 🟡 中 | render_template 不检查残留 {{}} | `prompt_resolver.rs:397` | FR-12 |
| BUG-12 | 🟢 低 | ContextPicker 缺虚拟化 | `ContextPicker.tsx` | FR-10 |
| BUG-13 | 🟢 低 | sanitize_json_with_gate 设计分歧 | `safety.rs:257` | FR-06 |
| BUG-14 | 🟢 低 | consume_assigned_value 4 处 .expect() | `safety.rs:155` | AGENTS.md |
| BUG-15 | 🟢 低 | sanitize_400_error 重复死代码 | `protocol/mod.rs:107` | FR-15 |

---

## 七、与设计文档一致性检查

| 设计文档要求 | 实现状态 | 差异 |
|-------------|---------|------|
| `CancellationRegistry` struct | ✅ 一致 | 完全匹配设计 |
| `ToolLoopGuard` struct | ✅ 一致 | 字段和默认值匹配 |
| `SessionKey = connectionId::database` | ✅ 一致 | 格式匹配 |
| `extractSqlStreaming` fence-tolerant | ✅ 一致 | 未闭合 fence 取全部 |
| `redact_for_gate` 唯一入口 | ✅ 一致 | 旧路已删 |
| `key_words()` token 边界匹配 | ✅ 一致 | 拆词后精确匹配 |
| `result_key_prefix_re()` | ✅ 一致 | 正则前缀匹配 |
| `budget::dynamic_budget` | ⚠️ 命名差异 | PRD 称 `for_model`，实现为 `dynamic_budget`，功能等价 |
| `ProviderListItem.capabilities` | ❌ 未实现 | 设计文档有，代码无 |
| `FeedbackButton.tsx` | ❌ 未实现 | 设计文档有，代码无 |
| `AiEmptyState/ChatBubble/EgressLine` 共享 | ❌ 未实现 | 设计文档有，代码无 |
| `PromptSeed/SchemaContextPipeline` 复用 | ❌ 未实现 | 设计文档有，代码无 |

---

## 七、总结

### 完成度评估

- **P0（FR-01~FR-06）**: 5/6 完成，FR-01 缺 AiChatPanel onStop
- **P1（FR-07~FR-13）**: 4/7 完成，FR-10/11/13 部分实现
- **P2（FR-14~FR-20）**: 3/7 完成，FR-14/16/17/18/19 未实现或部分实现

### 建议优先级

1. **立即修复**: BUG-01（AiChatPanel onStop）— 一行代码修复，阻塞用户核心操作
2. **尽快修复**: BUG-06（注释截断）、BUG-07（connections_context）
3. **计划修复**: BUG-02/03（P2 需求，需评估是否纳入当前版本）
4. **改进项**: BUG-04/05/08（ContextPicker 体验优化）

### 代码质量

- ✅ 生产路径无裸 unwrap（除 BUG-10 的 4 处 expect）
- ✅ 无 React hooks 违规
- ✅ 安全门实现完整，白名单/正则/边界匹配正确
- ✅ Token 估算 CJK/ASCII 双通道正确
- ✅ 流式取消生命周期完整（register/cancel/unregister 无泄漏）
