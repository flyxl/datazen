# v0.1.2 Settings dirty 进度

## 功能摘要

- 状态：编码完成，主线定向复验通过
- 范围：统一 SettingsContent 的 draft/save/dirty 状态，并保护未保存离开。

## E2E 登记

- 留待 R 阶段验证设置修改、离开确认、保存和撤销。

## 测试结果与覆盖率

- Settings 定向 Vitest：2 个文件，27/27 通过。
- `pnpm typecheck`：通过（与本轮 UI polish 联合复验）。
- `git diff --check`：通过。
- 编码 commit：`c11ab2e73`。
- 独立测试代理台账：尚未产生独立 commit；当前结果为协调者复验，待后续全新测试实例补录。

## 设计决策 / 遗留

- 不修改共享 locale 文件；使用 i18n contract 轨道预先冻结的 key。
