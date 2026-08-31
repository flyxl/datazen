# UI 评审修复进度台账

> 评审依据：[docs/reviews/ui/00-summary.md](../reviews/ui/00-summary.md)
> 采用 [subagent-dev-playbook.md](./subagent-dev-playbook.md) 多轨并行流程
> 状态机：功能 `未开始→编码中→编码完成→测试中→已完成`；bug `待验证/验证不通过→修复后→已修复`

## 1. 功能总览表

| 编号 | 功能 | 来源项 | 轨道/分支 | 状态 | 编码 commit | 测试 commit |
|------|------|--------|-----------|------|------------|------------|
| T1 | Dialog α11y（焦点圈闭/Esc/aria-label） | P0-1 | feature/ui-t1 (datazen-ui-t1) | 编码中 | - | - |
| T2 | ErrorBoundary i18n | P0-2 | feature/ui-t2 (datazen-ui-t2) | 编码中 | - | - |
| T3 | 导航轨/PanelTab a11y | P1-2 | feature/ui-t3 (datazen-ui-t3) | 编码中 | - | - |
| T4 | Select useId+accent+a11y | P2-3 | feature/ui-t4 (datazen-ui-t4) | 编码中 | - | - |

> 波 1（2026-08-28 启动）：T1/T2/T4 编码代理已派发（worktree 见分支）
| T5 | Settings 保存模型统一+dirty 警告 | P1-1/P3 | feature/ui-t5 | 未开始 | - | - |
| T6 | window.alert 消除+Workflow i18n/硬编码 | P1-4/P1-5 | feature/ui-t6 | 未开始 | - | - |
| T7 | MenuBar 平台化+WindowControls i18n | P2-2 | feature/ui-t7 (datazen-ui-t7) | 编码中 | - | - |
| T8 | DataTable 空态/loading | P2-1 | feature/ui-t8 (datazen-ui-t8) | 编码中 | - | - |
| T9 | blue-500 → accent 全局 sweep | P1-3 | feature/ui-t9 | 未开始 | - | - |
| T10 | i18n 漏网清理（permissionLabels/FK/其余） | P1-5 | feature/ui-t10 | 未开始 | - | - |

## 2. Bug 台账

（空）

## 3. 测试约定

- 改动 TS 文件行覆盖率 ≥80%（全量 vitest --coverage 实测）
- i18n 只改 en.ts + zh-CN.ts；发布前统一 sync
- E2E：功能轮只登记用例【本机可执行】/【留待 R 回归】，真实 webdriver 构建统一到合并后 R 回归

## 4. 每功能小节

（每轨道合并后由编码/测试代理补写）
