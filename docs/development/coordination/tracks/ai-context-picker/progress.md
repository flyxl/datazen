# Track: ai-context-picker (Phase 2.3 — ContextPicker FR-10)

## 状态: IN_PROGRESS

## 目标
1. 后端：`context_list_files(query, limit)` IPC — 搜索 .ctx.yaml 文件
2. 后端：ctx.yaml 校验（schema + 通配符 + 重复检测）
3. 前端：`AiContextPicker` 组件 — 防抖搜索 + 虚拟化列表 + keyboard 导航
4. 前端：recent 分桶 + 点击插入上下文引用
5. 集成：aiStore 接入 picker 状态
6. 性能：千表库打开 <300ms，过滤 <100ms

## 文件清单
- 后端：`src-tauri/src/ai/context.rs`（新增 `context_list_files`）、`src-tauri/src/commands/ai/mod.rs`（注册 IPC）
- 前端：`src/components/ai/AiContextPicker.tsx`（新）、`src/stores/aiStore.ts`（picker 状态）
- 测试：Rust `context.rs` 单测 + 前端 `AiContextPicker.test.tsx`
