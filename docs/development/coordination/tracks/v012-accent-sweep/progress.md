# v0.1.2 accent/i18n sweep 进度

## 功能摘要

- 状态：编码完成，主线全量复验通过
- 范围：生产 UI 的 accent token sweep、permission labels 和残留 UI 文案。

## E2E 登记

- 留待 R 阶段验证主题切换后的视觉与键盘反馈。

## 测试结果与覆盖率

- 定向 Host Vitest：4 个文件，26/26 通过（Input/Select、DataTable、QueryErrorPanel、MenuBar）。
- `tsc --noEmit`：通过。
- `git diff --check`：通过。
- Host 全量 `pnpm exec vitest run`：285 files / 2351 tests，通过。
- Driver UI Vitest：14 files / 84 tests，通过。
- E2E 留待 R 阶段验证主题切换后的真实视觉反馈。

## 设计决策 / 遗留

- 在前置 UI 轨道合入后完成一次有限范围 token sweep：通用输入/选择焦点、主题选中态、表格选择框、查询错误动作和菜单勾选态统一使用 `accent`。
- 数据库对象、差异和连接状态颜色仍保留语义色，避免把状态信息机械改成品牌色。
- Select 的触发器使用 button 语义，相关测试同步使用 `button`/`combobox` 的实际角色；未改变选择行为。
