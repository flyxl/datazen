# Track `rem-frontend-split` — bugs

> Bug ID 格式：`rem-frontend-split-BUG-nnn`；状态流转：待修复 → 修复中 → 待复测 → 已修复。

## rem-frontend-split-BUG-001
- **描述**：`effectivePendingIdentity` 在传入包含非主键列的 columns 列表时，由于内部未筛选 `column.isPrimaryKey` 直接传入 `buildRowIdentity`，且 `values` 未包含非主键列（仅含 rowIdentity 和 modified currentValues），导致 `buildRowIdentity` 返回 null
- **状态**：已修复
- **重现**：`npx vitest run src/stores/tableData/__tests__/pendingChanges.test.ts` 失败（`effectivePendingIdentity applies PK overrides from currentValues`）
- **影响**：若调用方传入全量 columns 时会无法计算 effective row identity
