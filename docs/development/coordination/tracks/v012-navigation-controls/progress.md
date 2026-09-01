# v0.1.2 navigation/controls 进度

## 功能摘要

- 状态：编码完成，主线定向复验通过
- 范围：PanelTabBar、Select、MenuBar、WindowControls 的键盘与 ARIA 语义。

### 编码结果

- PanelTabBar 提供 tablist/tab 语义、选中态、方向键切换和可访问的关闭按钮。
- Select、MenuBar、WindowControls 增加键盘操作、ARIA 状态/标签和焦点可见性。
- 修复 MenuBar popup 定位 effect 的自依赖问题，避免打开菜单后重复触发渲染。
- 未改变数据库、IPC、驱动或窗口控制语义。

## E2E 登记

- 留待 R 阶段验证真实键盘导航与窗口控制。

## 测试结果与覆盖率

- 定向 Vitest：4 个文件，11/11 通过。
- `npx tsc --noEmit`：通过。
- `git diff --check`：通过。
- 覆盖率：待 R 阶段统一采集。
- 编码 commit：`70d798790`；合入主线：`783ed9bb4`。
- 原独立测试轮因 MenuBar effect 自依赖超时，修复后由协调者在独立 worktree 复验通过；该超时问题未进入主线。

## 设计决策 / 遗留

- 不修改共享 locale 文件；使用 i18n contract 轨道预先冻结的 key。
