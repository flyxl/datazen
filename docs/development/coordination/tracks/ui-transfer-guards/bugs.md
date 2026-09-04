# Track `ui-transfer-guards` — bugs

> Bug ID 格式：`ui-transfer-guards-BUG-nnn`；状态流转：待修复 → 修复中 → 待复测 → 已修复。

| ID | 来源 | 描述 | 状态 |
|----|------|------|------|
| ui-transfer-guards-BUG-001 | P0-3 | `objects` 步骤 `tables.length === 0` 时 Next 未禁用，且无空状态提示与重新检测入口 | 已修复 |
| ui-transfer-guards-BUG-002 | P0-2 | `preview` 步骤 preview 失败或为 null 时界面空白，缺少错误提示及重试/返回映射操作 | 已修复 |

### 复测记录（2026-09-04）

- **BUG-001**：`canNext` `objects` 分支要求 `tables.length > 0`；空表展示 `data-transfer-objects-empty` + `data-transfer-reinspect`；单测 `disables next and shows empty guidance when no tables are detected` 通过。
- **BUG-002**：preview 失败时展示 `data-transfer-preview-error`，重试与返回映射按钮可用；单测 4 项（失败态、重试成功、重试失败、返回映射）均通过。
