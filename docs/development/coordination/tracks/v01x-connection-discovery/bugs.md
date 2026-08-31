# v01x-connection-discovery Bug 清单

| Bug ID | 描述 | 状态 | 验证记录 |
|---|---|---|---|
| — | 本轮未发现新的业务 bug | — | 独立聚焦 Vitest 113/113 通过；待 R 执行桌面 E2E |

## 环境/遗留风险

- 当前 worktree 缺少 ignored 生成文件 `src/locales/builtinLocales.ts`。普通 Vitest 因此有 6 个相关 suite 无法导入，`pnpm typecheck` 留下缺失模块诊断；聚焦复跑使用临时 alias，未修改 codegen。
- `pnpm typecheck` 另有既有 `src/windows/settings/SettingsContent.tsx:66` implicit-any 诊断，与本轨无关。
- `ConnectionAdvancedSettings.tsx` focused 行覆盖率为 66.66%，仅记录为覆盖率遗留，不登记业务 bug；其已执行 Basic/Advanced/SSH 关键路径均通过。
- Host 桌面 E2E 未在本环境执行（无可调用桌面自动化工具），搜索层级/表单折叠留待 R 回归；本轨未修改 ConnectionPage 最终接线。
