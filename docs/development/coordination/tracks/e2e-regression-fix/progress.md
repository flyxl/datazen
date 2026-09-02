# e2e-regression-fix 进度

## 功能摘要

| 项 | 值 |
|----|-----|
| 范围 | 全量 E2E 回归失败修复（R 阶段） |
| 状态 | 测试中·修复轮 |
| 编码 commit | — |
| 测试 commit | — |

## E2E 用例

| 用例 | 状态 |
|------|------|
| 全量 `pnpm e2e` | 【留待 R 回归】主检出首轮跑通中 |

## 修复项

1. `e2e/helpers.ts` — Select listbox 选择器前缀匹配
2. 多处 spec — 同步 listbox 选择器
3. `driver-commands.ts` — `dbSessionId` 契约
4. `connection-window.ts` — DB-008 与全局搜索隐藏对齐
5. `client-parity.ts` — SSH 高级设置展开
6. `bugfix-verification.ts` — 图表 fixture
7. `conn-ctx-menu-submenus.ts` — 子菜单 mouseenter/focus 打开
