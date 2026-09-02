# e2e-regression-fix bugs

| Bug ID | 描述 | 状态 | 记录时间 | 重现 | 验证 |
|--------|------|------|----------|------|------|
| e2e-regression-fix-BUG-001 | Host Select listbox id 改为 `dz-select-listbox-<useId>` 后 E2E helper 仍查找固定 id | 已修复 | 2026-09-02 | `pnpm e2e` → Select listbox did not open | spot-check: data-sync-edge-cases PASSED |
| e2e-regression-fix-BUG-002 | `get_connection_commands` IPC 参数已改为 `dbSessionId` | 已修复 | 2026-09-02 | `driver-commands.ts` spec | spot-check: driver-commands PASSED |
| e2e-regression-fix-BUG-003 | 全局对象搜索入口已从 Navigator 移除 | 已修复 | 2026-09-02 | `connection-window.ts` DB-008 | 用例改为 hidden + skip |
| e2e-regression-fix-BUG-004 | client-parity SSH 用例未展开高级设置/SSH 折叠区 | 已修复 | 2026-09-02 | `client-parity.ts` SSH test | SSH 子用例通过；query toolbar 仍失败 |
| e2e-regression-fix-BUG-005 | bugfix-verification 图表用例依赖 product 表 | 已修复 | 2026-09-02 | `bugfix-verification.ts` FIX-003 | spot-check PASSED |
| e2e-regression-fix-BUG-006 | 结构/索引子标签下 sidebar 表右键菜单项断言失败 (CTX-002~006) | 待验证 | 2026-09-02 | `connection-window.ts` | web-context-item-open 未出现 |
| e2e-regression-fix-BUG-007 | 连接右键子菜单剪贴板/编辑用例不稳定 | 待验证 | 2026-09-02 | `conn-ctx-menu-submenus.ts` | clipboard timeout / submenu |
| e2e-regression-fix-BUG-008 | `clickTableInSidebar` 工作区检测超时（table-data/sqlite 等） | 待验证 | 2026-09-02 | 多 spec | 等待表工作区打开超时 |
