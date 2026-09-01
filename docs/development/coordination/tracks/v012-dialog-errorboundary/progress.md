# v0.1.2 Dialog/ErrorBoundary 进度

## 功能摘要

- 状态：编码完成，主线全量复验通过
- 范围：Dialog 可访问性与 ErrorBoundary 本地化。

### 编码结果

- Dialog 增加初始焦点、Tab 环回、Escape 关闭、关闭后恢复 opener 焦点，以及 `aria-labelledby`/`aria-describedby` 和关闭按钮标签。
- ErrorBoundary 改为通过现有 i18n key 渲染错误状态，保留关闭/重试操作；日志只记录错误名、消息长度和组件栈，不输出错误正文。
- 编码 commit：`55934d2e7`。

## E2E 登记

- 留待 R 阶段验证真实键盘路径。

## 测试结果与覆盖率

- 定向 Vitest：2 个文件，8/8 通过。
- `pnpm typecheck`：通过。
- `git diff --check`：通过。
- Host 全量 `pnpm exec vitest run`：285 files / 2351 tests，通过。
- 本轮未形成独立测试代理 commit；主线全量回归已覆盖 Dialog/ErrorBoundary。
- 覆盖率：未单独采集。

## 设计决策 / 遗留

- 不修改共享 locale 文件；使用 i18n contract 轨道预先冻结的 key。
