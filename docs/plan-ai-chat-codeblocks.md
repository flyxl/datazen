# AI Chat 代码块内联显示 — 开发进度

分支: `feature/ai-chat-codeblocks`  
Worktree: `../datazen-ai-chat-codeblocks`

## 目标

AI Chat 输出中的代码块（尤其是 SQL）应像 ChatGPT 一样内联渲染，Copy / Insert SQL 按钮放在每个代码块头部，而非消息底部。

## 功能清单

| # | 功能 | 状态 | 单元测试 | E2E 测试 | 提交 |
|---|------|------|----------|----------|------|
| 1 | 消息解析工具 `aiMessageBlocks.ts` | ✅ 完成 | ✅ 9/9 | N/A | ✅ 14b6f78a |
| 2 | `AiCodeBlock` 组件（内联代码块 + 头部操作） | ✅ 完成 | ✅ 5/5 | N/A | ✅ c5a9bf62 |
| 3 | `ChatBubble` 集成新渲染器 | ✅ 完成 | ✅ 23/23 | — | ✅ 2caf6bef |
| 4 | Host E2E 测试 | ✅ 完成 | — | ✅ 4/4 | 待提交 |

## Bug 记录

| Bug | 发现者 | 状态 | 重现步骤 |
|-----|--------|------|----------|
| `sqlDialect={databaseType}` 传参错误 | 9a747e64 | ✅ 已修复 | QuestDB 等类型高亮降级为 StandardSQL |
| E2E 新建连接按钮 locale 不匹配 | 764c8f57 | ✅ 已修复 | 英文 UI 下 `button*=新建连接` 找不到 |
| E2E 需先打开 Query 面板才能看到 AI 按钮 | 编码 agent | ✅ 已修复 | 连接后未开 Query tab，ContentToolbar 不渲染 |

## 测试记录

### Feature 1 — aiMessageBlocks.ts
- 单元测试: **PASS** 9/9
- 测试 agent: Overall PASS

### Feature 2 — AiCodeBlock.tsx
- 单元测试: **PASS** 5/5
- 测试 agent: 组件隔离 PASS，集成待 Feature 3

### Feature 3 — ChatBubble 集成
- 单元测试: **PASS** 23/23
- 测试 agent: 集成 PASS（sqlDialect bug 已修复）

### Feature 4 — E2E ai-code-block.ts
- E2E-CB-01: ✅ 内联 SQL 代码块渲染
- E2E-CB-02: ✅ 无底部 legacy 操作按钮
- E2E-CB-04: ✅ Insert SQL 写入编辑器
- E2E-CB-05: ✅ JSON 块无 Insert 按钮
