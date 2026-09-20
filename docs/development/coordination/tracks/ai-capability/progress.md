# Track: ai-capability (Phase 3.1 能力矩阵 FR-14)

## 状态: IN_PROGRESS

## 目标
1. `registry.rs` ProviderListItem 补字段 — model_list / supports_streaming / supports_tools / max_tokens
2. 前端设置页 AI 区动态开关 — 根据 provider 能力灰度
3. 兼容矩阵文档 — 哪些 provider 支持哪些功能
4. `ai-api/types.rs` 能力枚举

## 文件清单
- 后端：`src-tauri/src/ai/registry.rs`（ProviderListItem 扩展）、`packages/ai-api/src/types.rs`（能力枚举）
- 前端：`src/windows/settings/` AI 配置区
- 测试：registry 单测 + 前端能力开关断言
