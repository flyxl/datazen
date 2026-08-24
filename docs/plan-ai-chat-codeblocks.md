# AI Chat 代码块内联显示 — 开发进度

分支: `feature/ai-chat-codeblocks`  
Worktree: `../datazen-ai-chat-codeblocks`

## 目标

AI Chat 输出中的代码块（尤其是 SQL）应像 ChatGPT 一样内联渲染，Copy / Insert SQL 按钮放在每个代码块头部，而非消息底部。

## 功能清单

| # | 功能 | 状态 | 单元测试 | E2E 测试 | 提交 |
|---|------|------|----------|----------|------|
| 1 | 消息解析工具 `aiMessageBlocks.ts` | ✅ 完成 | ✅ 9/9 | N/A | 待提交 |
| 2 | `AiCodeBlock` 组件（内联代码块 + 头部操作） | ⏳ 待开始 | — | — | — |
| 3 | `ChatBubble` 集成新渲染器 | ⏳ 待开始 | — | — | — |
| 4 | Host E2E 测试 | ⏳ 待开始 | — | — | — |

## Bug 记录

（测试 agent 发现的问题记录于此）

## 测试记录

（各功能测试 agent 的结果记录于此）
