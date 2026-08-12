# Native Context Menu 统一改造进度

> 分支：`feat/native-context-menus`  
> Worktree：`/Users/wuxiaolong/code/rust-projects/datazen-native-ctx-menus`  
> 目标：全部 ContextMenu 统一为系统原生菜单；完成 P0/P1/P2。

## 状态图例

- `pending` — 未开始
- `dev` — 开发中
- `testing` — 独立测试 agent 执行中
- `fixing` — 编码 agent 修复中
- `done` — 开发 + 测试通过 + 已提交

## 功能清单

| ID | 功能 | 优先级 | 状态 | Commit | 测试结果 |
|----|------|--------|------|--------|----------|
| F1 | 原生 ContextMenu 共享辅助层（TS popup helper + 事件约定） | P0 基建 | done | （本提交） | PASS：单测 8/8，lines 100%；E2E 设计 4 条（待 UI 接入后跑） |
| F2 | SQL 编辑器原生菜单：Cut/Copy/Paste/SelectAll + 收藏 + 完整 i18n | P0 | fixing | — | BUG-F2-001：disabled 项应灰显保留，勿从 normalize 中删除 |
| F3 | 移除 SqlConnectionView 整区 Web ContextMenu，消除双菜单 | P0 | pending | — | — |
| F4 | Schema 树原生菜单（表/视图/库/空白；按 nodeKind 分支） | P1 | pending | — | — |
| F5 | DataTable 原生菜单（导出/复制单元格/复制行） | P1 | pending | — | — |
| F6 | 连接窗口 Tab 栏原生菜单（关闭/关闭其他/关闭全部） | P1 | pending | — | — |
| F7 | 收藏 / 历史侧栏原生菜单 | P2 | pending | — | — |
| F8 | Redis key 列表原生菜单（驱动 UI） | P2 | pending | — | — |
| F9 | Workflow 列表 / 历史原生菜单 | P2 | pending | — | — |
| F10 | ER 图节点原生菜单 | P2 | pending | — | — |
| F11 | 清理 Web ContextMenu / uiStore 死代码 + 架构文档 / AGENTS.md | P2 | pending | — | — |
| F12 | 合并到 main 并 push | 收尾 | pending | — | — |

## 测试约定

- 每个功能开发必须附带单元测试。
- 功能完成后由**新开的独立测试 agent**执行验证（禁止开发 agent 自测充当本步）。
- 测试 agent 输出：E2E 用例、结果、覆盖率（目标 ≥80%）、失败时的复现步骤；**本步不修复**。
- 若测试不通过，另开编码 agent 修复，再开新测试 agent 复测，通过后提交。

## 变更日志

### F1 — 原生 ContextMenu 共享辅助层
- 新增 `src/lib/nativeContextMenu.ts`：`showNativeContextMenu` / `normalizeNativeMenuItems` / `nativeEditMenuItems` / `createNativeContextMenuHandler`
- 单测：`src/lib/__tests__/nativeContextMenu.test.ts`
- 独立测试 agent：PASS（覆盖率 lines 100%）

### F2 — SQL 编辑器原生菜单
- `normalizeNativeMenuItems` **保留** `enabled: false` 项（灰显），仅过滤空 submenu / 规范化分隔符；`buildItem` 已传 `enabled` 给 `MenuItem.new`
- 修复 BUG-F2-001：空 SQL 时「加入收藏」应灰显出现，而非整项消失
