# v01x-object-actions Bug 清单

| Bug ID | 描述 | 状态 | 重现步骤 | 验证记录 |
|---|---|---|---|---|
| — | 本编码轮未发现功能性 bug | — | — | 定向 Vitest 2 文件 / 13 测试通过；等待独立测试轮与 R 回归 |

## 基线验证遗留（非本轨功能 bug）

- 目标 worktree 没有被 git 跟踪的 `src/locales/builtinLocales.ts`；它被现有 `src/locales/index.ts` 引用，导致 tsc/未 mock 的模块测试无法完整加载。该文件是 ignored 生成文件，本轨不生成、不提交。
- 现有 `src/windows/settings/SettingsContent.tsx:66` 仍有隐式 `any` 诊断；本轨不修改共享设置页面。
- Host 桌面 E2E 依 playbook 登记留待 R；本轨不接共享页面，也不直接执行 SQL。
