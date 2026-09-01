# v0.1.2 i18n contract 进度

## 功能摘要

- 状态：编码完成，待独立测试代理复验
- 范围：为 UI polish 轨道冻结英文/中文新增 key；不修改业务组件。

### 编码结果

- 为 Dialog/ErrorBoundary、Settings dirty/save、DataTable 空/加载、Workflow 错误、Select/Panel 可访问性和权限说明预留英文/简体中文 key。
- 只修改 `src/locales/en.ts` 与 `src/locales/zh-CN.ts`，保持 host locale key parity。
- 新增 locale contract 测试，覆盖本轮 key 的双语解析和带参数文案插值。
- 编码 commit：待本轮提交后填写。

## E2E 登记

- locale parity 与组件文案由单测覆盖；真实 UI E2E 留待 R 阶段。

## 测试结果与覆盖率

- locale 定向测试：1 个文件，18/18 通过（含 UI polish key contract）。
- `git diff --check`：通过。
- typecheck：通过。
- 覆盖率：待独立测试代理按 playbook 复验。

## 设计决策 / 遗留

- 仅修改 `src/locales/en.ts`、`src/locales/zh-CN.ts` 和必要的 locale 测试。
