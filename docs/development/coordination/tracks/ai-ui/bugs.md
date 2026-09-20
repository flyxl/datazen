# Bugs — Track ai-ui

## ai-ui-BUG-001: AiCodeBlock 使用原生 `<select>` 而非 `components/ui/Select`

- **状态**: 待修复
- **严重度**: Medium（规范违反）
- **文件**: `src/components/ai/AiCodeBlock.tsx`
- **描述**: `AiCodeBlock` 中的方言选择器使用了原生 HTML `<select>` 元素（第 84-94 行和第 178-190 行），违反项目约定。项目规范要求使用 `components/ui/Select` 组件（保持暗色主题一致性与统一交互风格）。
- **影响**: `noNativeSelect.test.ts` 检测失败。视觉上原生 `<select>` 在暗色主题下可能显示异常。
- **重现步骤**: 运行 `npx vitest run src/lib/__tests__/noNativeSelect.test.ts` → 失败
- **修复建议**: 将 `<select>` + `<option>` 替换为 `components/ui/Select` 组件。
- **关联**: `src/lib/__tests__/noNativeSelect.test.ts`

## ai-ui-BUG-003: Missing `onRunCode` / `onNewQuery` prop forwarding in ChatBubble

- **状态**: 待修复
- **严重度**: Medium（功能缺失）
- **文件**: `src/components/ai/AiChatPanel.tsx` — `ChatBubble` 函数
- **描述**: `ChatBubble` 渲染 `AiMessageContent` 时只传递了 `onInsertSql`，未传递 `onRunCode` 和 `onNewQuery`。`AiMessageContent` 接受这两个 prop 并转发给 `AiCodeBlock`，但由于上游未提供，代码块工具栏中的"运行"和"新查询"按钮在聊天上下文中完全无效。
- **影响**: 用户无法在聊天代码块中直接运行 SQL 或打开新查询标签页。
- **重现步骤**:
  1. 发送一条消息，AI 返回包含 SQL 的代码块
  2. 观察代码块工具栏中的 Run（运行）和 New Query（新查询）按钮
  3. 点击按钮 — 无任何响应
- **修复建议**: 在 `ChatBubble` 中从父组件接收 `onRunCode` 和 `onNewQuery` 并传递给 `AiMessageContent`。需同步更新 `AiChatPanelProps` 接口。
- **关联**: AiCodeBlock toolbar 功能完整性

## ai-ui-BUG-004: AiMessageContent.test.tsx 测试隔离失败

- **状态**: 待修复
- **严重度**: Low（测试基础设施）
- **文件**: `src/components/ai/__tests__/AiMessageContent.test.tsx`
- **描述**: 测试文件缺少 `afterEach(cleanup)` 调用，导致渲染的组件在测试之间累积，产生重复元素导致测试失败。
- **影响**: 10 个测试中有 2 个失败：
  1. `renders text around code blocks` — `getByText('Before')` 找到多个匹配元素
  2. `renders multiple code blocks` — `getAllByTestId('ai-code-block')` 返回 4 个元素而非预期的 2 个
- **修复建议**: 在测试文件顶部导入 `cleanup`，并添加 `afterEach(cleanup)` 调用（参考 `AiCodeBlock.test.tsx` 的做法）。
- **实测错误日志**:
  ```
  TestingLibraryElementError: Found multiple elements with the text: Before
  AssertionError: expected [ ... ] to have a length of 2 but got 4
  ```
