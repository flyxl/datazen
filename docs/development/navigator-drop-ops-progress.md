# ConnectionNavigatorTree 删除表/Schema 进度

> 轨道：`feature/navigator-drop-ops` @ `/Users/wuxiaolong/code/rust-projects/datazen-navigator-drop-ops`

## 功能总览

| 编号 | 功能 | 状态 | 编码 commit | 测试 commit |
|------|------|------|-------------|-------------|
| F1 | 表节点右键删除表 | 已完成 | c3784b8a | — |
| F2 | Schema 节点右键删除 Schema | 已完成 | （既有） | — |

## Bug 台账

| Bug ID | 所属功能 | 描述 | 状态 | 记录时间 | 验证记录 |
|--------|----------|------|------|----------|----------|
| — | — | — | — | — | — |

## 测试约定

- Host 单测：`ConnectionNavigatorTree.test.tsx`、`schemaTreeContextMenu.test.ts`
- TS 改动文件覆盖率目标 ≥80%（vitest --coverage 摘取）
- E2E 用例设计登记于各功能小节，标注【留待 R 回归】

---

## F1：表节点右键删除表

**范围**：`ConnectionNavigatorTree` 的 `table`/`view` 行右键菜单增加 Truncate/Drop；自包含实现（不依赖 ContentView `nodeContextMenuRef`）。

### E2E 用例（设计）

| 编号 | 前置 | 步骤 | 断言 | 备注 |
|------|------|------|------|------|
| E2E-F1-1 | PG 连接，存在测试表 | 侧栏表节点右键 → Drop → Web 确认 | 表从树消失 | NCM-046 |

### 测试结果

- vitest：`ConnectionNavigatorTree.test.tsx` 68 passed
- tsc：`--noEmit` 通过
- E2E：`navigator-context-menu.ts` NCM-023/046（Web ConfirmDialog）

---

## F2：Schema 节点右键删除 Schema

**范围**：确认 `handleSchemaContextMenu` 已有 `drop_schema` 流程与单测；补缺口（如有）。

### E2E 用例（设计）

| 编号 | 前置 | 步骤 | 断言 | 备注 |
|------|------|------|------|------|
| E2E-F2-1 | PG 多 schema | 侧栏 schema 右键 → Drop Schema → Web 确认 | schema 从树消失 | NCM-023 |

### 测试结果

- vitest：`ConnectionNavigatorTree.test.tsx` 68 passed
- E2E：`navigator-context-menu.ts` NCM-022/023
