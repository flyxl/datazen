# e2e-regression-fix bugs

| Bug ID | 描述 | 状态 | 记录时间 | 重现 | 验证 |
|--------|------|------|----------|------|------|
| e2e-regression-fix-BUG-001 | Host Select listbox id 改为 `dz-select-listbox-<useId>` 后 E2E helper 仍查找固定 id，导致 data-sync/transfer/schema-diff 等大量失败 | 待验证(修复后) | 2026-09-02 | `pnpm e2e` → Select listbox did not open | — |
| e2e-regression-fix-BUG-002 | `get_connection_commands` IPC 参数已改为 `dbSessionId`，`driver-commands.ts` 仍传 `connectionId` | 待验证(修复后) | 2026-09-02 | `driver-commands.ts` spec | — |
| e2e-regression-fix-BUG-003 | 全局对象搜索入口已从 Navigator 移除，connection-window DB-008 用例过时 | 待验证(修复后) | 2026-09-02 | `connection-window.ts` DB-008 | — |
| e2e-regression-fix-BUG-004 | client-parity SSH 用例未展开高级设置/SSH 折叠区 | 待验证(修复后) | 2026-09-02 | `client-parity.ts` SSH test | — |
| e2e-regression-fix-BUG-005 | bugfix-verification 图表用例未等待 recharts 渲染且依赖 product 表 | 待验证(修复后) | 2026-09-02 | `bugfix-verification.ts` FIX-003 | — |
