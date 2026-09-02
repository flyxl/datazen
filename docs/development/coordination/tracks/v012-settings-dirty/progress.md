# v0.1.2 Settings dirty 进度

## 功能摘要

- 状态：编码完成，主线全量复验通过
- 范围：统一 SettingsContent 的 draft/save/dirty 状态，并保护未保存离开。
- 编码 commit：`17981b0c6`
- 测试 commit：本轨编码代理已完成自验；独立测试代理待协调者派发。

已完成：

- 基础设置、主题、MCP Server、插件设置统一写入页面 draft，保存时只提交变化字段。
- 保存中禁用 Save/Close，保存失败保留 draft 并展示可读错误。
- 返回、关闭、分区切换统一提供“保存 / 放弃 / 取消”未保存确认。
- 更新检查开关不再绕过页面 Save 直接持久化；独立挂载 UpdateSection 仍保留兼容行为。
- 不改变 AI provider、Prompt、MCP runtime 的独立运行时协议。

## E2E 登记

- 留待 R 阶段验证设置修改、离开确认、保存和撤销；本轮未构建桌面 E2E。

## 测试结果与覆盖率

- `npx tsc --noEmit`：通过。
- `npx vitest run src/windows/settings/__tests__`：5 files / 46 tests，通过。
- 定向覆盖率：`windows/settings` 行 91.17%，分支 86.66%，函数 83.01%。覆盖率命令的全局阈值因只选 Settings 测试而失败；本轨变更文件达到 80% 目标。
- `npx vitest run --config vitest.drivers.config.ts`：14 files / 84 tests，通过。
- Host 全量 `pnpm exec vitest run`：285 files / 2351 tests，通过。
- Driver UI Vitest：14 files / 84 tests，通过。
- `git diff --check`：通过。

## 设计决策 / 遗留

- 不修改共享 locale 文件；使用 i18n contract 轨道预先冻结的 key。
- `pnpm test:unit:drivers`：14 files / 84 tests，通过。
