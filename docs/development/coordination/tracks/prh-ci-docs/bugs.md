# Track `prh-ci-docs` — bugs

> Bug ID 格式：`prh-ci-docs-BUG-nnn`

| ID | 描述 | 状态 | 重现步骤 | 验证记录 |
|----|------|------|----------|----------|
| prh-ci-docs-BUG-001 | `windows.md` §4 代码片段 `WindowKind` 缺少 `'data-transfer'`（`windowKind.ts` 已包含） | 待验证(新发现) | 运行 `node scripts/check-ci-docs-consistency.mjs` 或 `pnpm test:scripts -- check-ci-docs-consistency` | `[check-ci-docs] windows.md §4 WindowKind snippet missing 'data-transfer'` |
| prh-ci-docs-BUG-002 | `windows.md` §6.1/§6.3 称 data-sync/schema-diff 子窗口监听 `datazen:connection-ready`；代码实际监听 `datazen:connections-changed` / `datazen:connection-closed`（`grep datazen:connection-ready src/windows/{data-sync,schema-diff,data-transfer}` 无匹配） | 待验证(新发现) | 阅读 `DataSyncWindow.tsx`、`useSchemaDiffEndpoints.ts` 与 §6.1 表格对比 | 事件仅由 main 侧 emit（`ConnectionPage.tsx`、`activeConnectionStore.ts`） |
| prh-ci-docs-BUG-003 | `windows.md` §1 Label 表：`schema-diff` / `data-transfer` 未列出 singleton label（`schema-diff-singleton`、`data-transfer-singleton`），与 backup/data-sync 格式不一致 | 待验证(新发现) | 对比 §1 表格与 `windowManager.ts` `openSingletonWindow` 调用 | 代码使用 `*-singleton` label |
| prh-ci-docs-BUG-004 | `ci-test-matrix.md` §2 Rust 顺序将 `cargo test -p datazen-ai-api` 列在 `driver-file-stash restore` 之前；`ci.yml` 在 restore 之后执行 ai-api 测试 | 待验证(新发现) | diff `ci-test-matrix.md` §2/§4 与 `.github/workflows/ci.yml` steps 113–118 | 行为无功能影响，文档顺序与 workflow 不一致 |
