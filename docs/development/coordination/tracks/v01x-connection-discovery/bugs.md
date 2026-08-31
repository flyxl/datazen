# v01x-connection-discovery Bug 清单

| Bug ID | 描述 | 状态 | 验证记录 |
|---|---|---|---|
| — | 本轮未发现新的业务 bug | — | 定向 Vitest 97/97 通过；待 R 执行桌面 E2E |

## 环境/遗留风险

- 当前 worktree 缺少 ignored 生成文件 `src/locales/builtinLocales.ts`。普通 Vitest 和 `tsc --noEmit` 因此分别无法直接启动/留下缺失模块诊断；定向测试使用 `/private/tmp` 临时 alias 验证，未修改 codegen。
- `npx tsc --noEmit` 另有既有 `src/windows/settings/SettingsContent.tsx:66` implicit-any 诊断，与本轨无关。
- Host 桌面 E2E 未在本环境执行，搜索层级/表单折叠留待 R 回归；本轨未修改 ConnectionPage 最终接线。
