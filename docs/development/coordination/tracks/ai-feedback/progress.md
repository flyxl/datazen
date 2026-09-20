# Track: ai-feedback (Phase 3.4 反馈 FR-17)

## 状态: PENDING

## 目标
1. `ai_feedback` JSONL 存储 — 用户对 AI 回复的 thumbs up/down + comment
2. 导出功能 — JSONL → CSV/JSON
3. 消息 toolbar 反馈按钮
4. 隐私：仅本地存储，不上传

## 文件清单
- 后端：`src-tauri/src/commands/ai/feedback.rs`（新）、`src-tauri/src/commands/ai/mod.rs`（注册）
- 前端：`src/components/ai/FeedbackButton.tsx`（新）、消息 toolbar
- 测试：feedback 写入/读取/导出单测
