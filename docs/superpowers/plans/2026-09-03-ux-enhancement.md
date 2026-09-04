# SQL Editor、Database Object 与 Connection 交互优化实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 对齐 DBeaver 的常用数据库操作体验（Table 拖拽生成 SQL、右键生成常见 SQL、快捷键优化、Connection 分组拖拽及绿色按钮规范），同时严格避让 `post-review-hardening-plan`（PRH）进行中的改动，将有写锁冲突的 AI 出域控制部分暂缓。

**Architecture:** 
1. 建立纯前端领域级 `sqlGenerator.ts`，基于已有的 `TableSchema` 与数据库元数据（`DB_REGISTRY` 中的 `quoteChar` / `escapeIdent`）统一生成 SELECT / INSERT / UPDATE / DELETE 模板，DDL 复用现有的 `getObjectDdl`。
2. 将 SQL Editor 与 SchemaTree 通过 HTML5 Drag & Drop 打通，右键菜单与拖拽复用同一个 SQL 生成器。
3. 扩展 CodeMirror Keymap 与按钮 Tooltip，提供精准的快捷键体系（支持选区/单语句执行与全量执行分离）。
4. 在 ConnectionNavigatorTree 中打通 Connection 到自定义分组与 Pinned 分组的拖拽，拦截 Recent 分组放置。
5. 全局审计按钮样式，确保只有“SQL 执行 / Workflow 执行”享有绿色（`variant="run"`）。

**Tech Stack:** React 18, TypeScript, Zustand, CodeMirror 6, Tailwind CSS, Tauri v2 IPC, Vitest.

---

## 冲突隔离与范围划定 (PRH Conflict Boundary)

依据 `docs/development/coordination/post-review-hardening-plan.md` 当前正在进行的波次及写锁：
1. **冲突部分（本期不执行，延后）**：
   - **PRD §19 - §24「AI Database Access Settings & Context Indicator」**：该部分需修改 `src-tauri/src/ai/safety.rs`、`src-tauri/src/ai/context.rs` 及前端 `AiSettingsSection` / `AiChatPanel`。当前 PRH 的 `prh-ai-egress` 轨正持有写锁进行安全加固，**严禁在本阶段并发改动，本计划完全排除此模块，待 PRH Wave 1 合并后再行调度**。
2. **非冲突部分（本期完全可执行）**：
   - **Task 1: SQL Generator 核心模块**（纯前端 `src/lib/sqlGenerator.ts` 与单测，不碰后端，不触碰 `prh-split-dcmd`）。
   - **Task 2: Table → SQL Editor Drag & Drop**（前端 SchemaTree + SqlEditor 拖拽集成）。
   - **Task 3: Table ContextMenu → Generate SQL**（前端 SchemaTreeContextMenu 子菜单集成）。
   - **Task 4: SQL Editor 快捷键与视觉提示**（CodeMirror 键位与按钮 Tooltip / 菜单提示）。
   - **Task 5: Connection Group 拖拽归类**（ConnectionNavigatorTree 拖拽与 connectionStore 持久化）。
   - **Task 6: 全局绿色按钮样式规范审计**（纯前端样式统一，确保只有 Execute SQL / Workflow 按钮为绿色）。

---

## Global Constraints

- 严禁修改 `src-tauri/src/ai/**`、`src-tauri/src/mcp/**`、`src-tauri/src/commands/driver_command.rs` 等被 PRH 占用的文件。
- SQL Generator 不允许在前端按 `if (mysql) ... else if (pg)` 硬编码拼接语法，必须从 `DB_REGISTRY` 读取元数据与引号规则（`quoteChar`），方言差异走元数据驱动。
- UPDATE / DELETE 必须默认使用 Primary Key；若无 Primary Key，严禁生成无 WHERE 语句，必须显式生成带 warning 注释的 `WHERE /* condition required */` 占位。
- Drag & Drop 与 ContextMenu 必须调用同一个 `sqlGenerator`，禁止维护两套 SQL 生成逻辑。
- 所有 UI 变更必须保持 i18n 规则：仅修改 `src/locales/en/` 与 `src/locales/zh-CN/`，保持 key 完全一致。

---

### Task 1: 统一 SQL Generator 核心模块

**Files:**
- Create: `src/lib/sqlGenerator.ts`
- Test: `src/lib/__tests__/sqlGenerator.test.ts`

**Interfaces:**
- Consumes: `TableSchema`, `ColumnSchema` from `src/types/index.ts`, `escapeIdent` from `src/lib/databaseTypes.ts`.
- Produces:
  ```ts
  export type GeneratedSqlType = 'select' | 'insert' | 'update' | 'delete';
  export function generateTableSql(
    schema: TableSchema,
    type: GeneratedSqlType,
    databaseType: string,
    options?: { schemaPrefix?: string }
  ): string;
  ```

- [ ] **Step 1: 编写失败测试**

在 `src/lib/__tests__/sqlGenerator.test.ts` 中编写单测：
```ts
import { describe, it, expect } from 'vitest';
import { generateTableSql } from '../sqlGenerator';
import type { TableSchema } from '../../types';

describe('sqlGenerator', () => {
  const sampleSchema: TableSchema = {
    tableName: 'users',
    primaryKeys: ['id'],
    columns: [
      { name: 'id', dataType: 'bigint', nullable: false, isPrimaryKey: true, isAutoIncrement: true },
      { name: 'name', dataType: 'varchar(255)', nullable: false },
      { name: 'email', dataType: 'varchar(255)', nullable: true },
      { name: 'created_at', dataType: 'timestamp', nullable: false, defaultValue: 'CURRENT_TIMESTAMP' },
    ],
    indexes: [],
    foreignKeys: [],
  };

  it('generates SELECT with columns in ordinal order without SELECT *', () => {
    const sql = generateTableSql(sampleSchema, 'select', 'postgresql');
    expect(sql).toBe('SELECT "id", "name", "email", "created_at"\nFROM "users";');
  });

  it('generates INSERT excluding auto-increment / identity columns', () => {
    const sql = generateTableSql(sampleSchema, 'insert', 'mysql');
    expect(sql).toContain('INSERT INTO `users`');
    expect(sql).not.toContain('`id`');
    expect(sql).toContain('`name`');
    expect(sql).toContain('`email`');
  });

  it('generates UPDATE with primary key in WHERE clause', () => {
    const sql = generateTableSql(sampleSchema, 'update', 'postgresql');
    expect(sql).toContain('UPDATE "users"');
    expect(sql).toContain('WHERE "id" = ');
  });

  it('generates UPDATE with warning placeholder if no primary key exists', () => {
    const noPkSchema: TableSchema = { ...sampleSchema, primaryKeys: [] };
    const sql = generateTableSql(noPkSchema, 'update', 'postgresql');
    expect(sql).toContain('WHERE /* WARNING: Primary Key not found. Specify condition */');
  });

  it('generates DELETE with primary key in WHERE clause', () => {
    const sql = generateTableSql(sampleSchema, 'delete', 'postgresql');
    expect(sql).toBe('DELETE FROM "users"\nWHERE "id" = ;');
  });
});
```

- [ ] **Step 2: 运行测试验证失败**

运行：`npx vitest run src/lib/__tests__/sqlGenerator.test.ts`  
预期：FAIL（模块尚未创建）

- [ ] **Step 3: 编写最小实现**

在 `src/lib/sqlGenerator.ts` 中实现：
```ts
import type { DatabaseType, TableSchema } from '../types';
import { escapeIdent } from './databaseTypes';

export type GeneratedSqlType = 'select' | 'insert' | 'update' | 'delete';

export interface SqlGeneratorOptions {
  schemaPrefix?: string;
}

export function formatTableIdentifier(
  tableName: string,
  databaseType: string,
  schemaPrefix?: string,
): string {
  const dbType = databaseType as DatabaseType;
  if (schemaPrefix && schemaPrefix.trim()) {
    return `${escapeIdent(schemaPrefix.trim(), dbType)}.${escapeIdent(tableName, dbType)}`;
  }
  return escapeIdent(tableName, dbType);
}

export function generateTableSql(
  schema: TableSchema,
  type: GeneratedSqlType,
  databaseType: string,
  options?: SqlGeneratorOptions,
): string {
  const dbType = databaseType as DatabaseType;
  const tableRef = formatTableIdentifier(schema.tableName, databaseType, options?.schemaPrefix);

  switch (type) {
    case 'select': {
      const cols = schema.columns.map((c) => escapeIdent(c.name, dbType)).join(', ');
      return `SELECT ${cols}\nFROM ${tableRef};`;
    }
    case 'insert': {
      const insertableCols = schema.columns.filter((c) => !c.isAutoIncrement);
      const colList = insertableCols.map((c) => `  ${escapeIdent(c.name, dbType)}`).join(',\n');
      const valList = insertableCols
        .map((c) => {
          if (c.defaultValue) return `  ${c.defaultValue}`;
          if (c.dataType.toLowerCase().includes('int') || c.dataType.toLowerCase().includes('numeric')) return '  0';
          return "  ''";
        })
        .join(',\n');
      return `INSERT INTO ${tableRef} (\n${colList}\n) VALUES (\n${valList}\n);`;
    }
    case 'update': {
      const nonPkCols = schema.columns.filter((c) => !schema.primaryKeys.includes(c.name));
      const targetCols = nonPkCols.length > 0 ? nonPkCols : schema.columns;
      const setClauses = targetCols.map((c) => `  ${escapeIdent(c.name, dbType)} = ''`).join(',\n');

      let whereClause: string;
      if (schema.primaryKeys.length > 0) {
        whereClause = schema.primaryKeys.map((pk) => `${escapeIdent(pk, dbType)} = `).join(' AND ');
      } else {
        whereClause = '/* WARNING: Primary Key not found. Specify condition */';
      }
      return `UPDATE ${tableRef}\nSET\n${setClauses}\nWHERE ${whereClause};`;
    }
    case 'delete': {
      let whereClause: string;
      if (schema.primaryKeys.length > 0) {
        whereClause = schema.primaryKeys.map((pk) => `${escapeIdent(pk, dbType)} = `).join(' AND ');
      } else {
        whereClause = '/* WARNING: Primary Key not found. Specify condition */';
      }
      return `DELETE FROM ${tableRef}\nWHERE ${whereClause};`;
    }
  }
}
```

- [ ] **Step 4: 运行测试验证通过**

运行：`npx vitest run src/lib/__tests__/sqlGenerator.test.ts`  
预期：PASS

- [ ] **Step 5: 提交 Commit**

```bash
git add src/lib/sqlGenerator.ts src/lib/__tests__/sqlGenerator.test.ts
git commit -m "feat(sql): implement unified SQL generator for table statements"
```

---

### Task 2: SchemaTree Table 拖拽与 SqlEditor 放置集成 (Drag & Drop)

**Files:**
- Modify: `src/windows/connection/schema-tree/SchemaTree.tsx`
- Modify: `src/components/SqlEditor.tsx`
- Modify: `src/windows/connection/QueryPanel.tsx`
- Test: `src/windows/connection/schema-tree/__tests__/SchemaTreeDrag.test.tsx`
- Test: `src/components/__tests__/SqlEditorDrop.test.tsx`

**Interfaces:**
- Drag MIME Type: `'application/datazen-table'`
- Payload: `{ tableName: string; schema?: string; dbSessionId: string; connectionId: string; databaseType: string }`

- [ ] **Step 1: 编写拖拽测试用例**

在 `src/components/__tests__/SqlEditorDrop.test.tsx` 中编写测试：验证传递 table payload 时，SqlEditor `onDropTable` 能正确在光标位置生成并插入 SELECT。

- [ ] **Step 2: 运行测试验证失败**

运行：`npx vitest run src/components/__tests__/SqlEditorDrop.test.tsx`  
预期：FAIL

- [ ] **Step 3: 实现 SchemaTree 拖拽源与 SqlEditor 放置目标**

1. 在 `SchemaTree.tsx` 中为 table / view 节点添加 `draggable` 属性与 `onDragStart` 事件，写入 MIME `'application/datazen-table'`。
2. 在 `SqlEditor.tsx` 中添加 `onDropTable` 回调或在容器绑定 `dragover` / `drop` 处理：通过 `view.posAtCoords` 计算 drop 光标位置，调用 `databaseCommands.getTableSchema` 获取字段列表，使用 `generateTableSql(schema, 'select', dbType)` 生成 SQL，并利用 `view.dispatch({ changes: { from: pos, insert: generatedSql } })` 插入。
3. 若无有效光标，在当前 document 尾部换行追加。

- [ ] **Step 4: 运行测试验证通过**

运行：`npx vitest run src/components/__tests__/SqlEditorDrop.test.tsx`  
预期：PASS

- [ ] **Step 5: 提交 Commit**

```bash
git add src/windows/connection/schema-tree/SchemaTree.tsx src/components/SqlEditor.tsx src/windows/connection/QueryPanel.tsx src/components/__tests__/SqlEditorDrop.test.tsx
git commit -m "feat(editor): support dragging tables into SQL editor to generate SELECT query"
```

---

### Task 3: Table 右键菜单 Generate SQL (SELECT / INSERT / UPDATE / DELETE / DDL)

**Files:**
- Modify: `src/lib/schemaTreeContextMenu.ts`
- Modify: `src/windows/connection/navigator/useNavigatorContextMenus.ts`
- Modify: `src/locales/en/contextMenu.ts`（若无则对应 locale 模块）
- Modify: `src/locales/zh-CN/contextMenu.ts`
- Test: `src/lib/__tests__/schemaTreeContextMenu.test.ts`

**Interfaces:**
- Consumes: `generateTableSql` from `src/lib/sqlGenerator.ts`, `databaseCommands.getObjectDdl`, `addPanel` from `panelStore`.
- Produces: `Generate SQL >` 子菜单定义与对应的执行 handler。

- [ ] **Step 1: 编写失败测试**

在 `src/lib/__tests__/schemaTreeContextMenu.test.ts` 中增加测试：验证当节点为 `table` 时，右键菜单中包含 `generateSql` 子菜单及其 5 项子操作（SELECT / INSERT / UPDATE / DELETE / DDL）。

- [ ] **Step 2: 运行测试验证失败**

运行：`npx vitest run src/lib/__tests__/schemaTreeContextMenu.test.ts`  
预期：FAIL

- [ ] **Step 3: 实现 Generate SQL 右键菜单与 Tab 打开动作**

1. 在 `schemaTreeContextMenu.ts` 中增加 `generateSql` 相关 label 定义与子菜单生成逻辑。
2. 在 `useNavigatorContextMenus.ts` 中实现对应 handler：
   - 获取目标 table 的 schema（或 DDL）。
   - 调用 `generateTableSql(schema, kind, dbType)` 生成 SQL。
   - 打开新的 Query Tab，将生成 SQL 放入编辑器，且不自动执行。
3. 补齐 `en` 与 `zh-CN` 对应菜单文案。

- [ ] **Step 4: 运行测试验证通过**

运行：`npx vitest run src/lib/__tests__/schemaTreeContextMenu.test.ts`  
预期：PASS

- [ ] **Step 5: 提交 Commit**

```bash
git add src/lib/schemaTreeContextMenu.ts src/windows/connection/navigator/useNavigatorContextMenus.ts src/locales/en/ src/locales/zh-CN/ src/lib/__tests__/schemaTreeContextMenu.test.ts
git commit -m "feat(schema-tree): add Generate SQL sub-menu for table objects"
```

---

### Task 4: SQL Editor 快捷键优化与 Discoverability

**Files:**
- Modify: `src/components/SqlEditor.tsx`
- Modify: `src/windows/connection/QueryPanel.tsx`
- Modify: `src/lib/sqlEditorContextMenu.ts`
- Test: `src/components/__tests__/SqlEditorShortcuts.test.ts`

**Interfaces:**
- `Mod-Enter`: 有选中时执行选中区；无选中时仅执行当前 Statement。
- `Mod-Shift-Enter`: 执行全文所有 Statement。
- `Mod-s`: 保存 Query。
- Execute 按钮 Tooltip 显示对应快捷键（Mac 为 `⌘ Enter` / `⌘ ⇧ Enter`，Windows 为 `Ctrl+Enter` / `Ctrl+Shift+Enter`）。

- [ ] **Step 1: 编写失败测试**

在 `src/components/__tests__/SqlEditorShortcuts.test.ts` 中测试快捷键事件绑定与派发。

- [ ] **Step 2: 运行测试验证失败**

运行：`npx vitest run src/components/__tests__/SqlEditorShortcuts.test.ts`  
预期：FAIL

- [ ] **Step 3: 实现快捷键与提示**

1. 在 `SqlEditor.tsx` 的 keymap 中增加 `Mod-Shift-Enter` 触发 `onExecuteAll`。
2. 完善 `Mod-Enter`：若无 selection，通过分号划分提取当前光标所在的单条 SQL 语句执行，避免误执行整个脚本。
3. 在 `QueryPanel.tsx` 的 Execute 按钮 Tooltip 与右键菜单中增加对应快捷键标签。

- [ ] **Step 4: 运行测试验证通过**

运行：`npx vitest run src/components/__tests__/SqlEditorShortcuts.test.ts`  
预期：PASS

- [ ] **Step 5: 提交 Commit**

```bash
git add src/components/SqlEditor.tsx src/windows/connection/QueryPanel.tsx src/lib/sqlEditorContextMenu.ts src/components/__tests__/SqlEditorShortcuts.test.ts
git commit -m "feat(editor): optimize execution shortcuts and shortcut discoverability"
```

---

### Task 5: Connection Group 拖拽归类与 Recent 分组拦截

**Files:**
- Modify: `src/windows/connection/ConnectionNavigatorTree.tsx`
- Modify: `src/stores/connectionStore.ts`
- Test: `src/windows/connection/__tests__/ConnectionNavigatorTreeDragGroup.test.tsx`

**Interfaces:**
- Consumes: `moveConnectionToGroup(connId, targetGroup)`, `toggleConnectionPinned(connId)`.
- Rules:
  - 拖拽 Connection 到普通 Group -> `moveConnectionToGroup(connId, targetGroup)` 并持久化。
  - 拖拽 Connection 到 Pinned Group -> 设为 pinned。
  - 拖拽 Connection 到 Recent Group -> 拦截并禁止（反馈 `not-allowed`）。

- [ ] **Step 1: 编写失败测试**

在 `src/windows/connection/__tests__/ConnectionNavigatorTreeDragGroup.test.tsx` 中编写测试：验证拖拽 connection 到普通分组与 Pinned 分组能成功调用 store 方法，而拖入 Recent 分组被忽略/拒绝。

- [ ] **Step 2: 运行测试验证失败**

运行：`npx vitest run src/windows/connection/__tests__/ConnectionNavigatorTreeDragGroup.test.tsx`  
预期：FAIL

- [ ] **Step 3: 实现 Connection 拖拽到 Group 的逻辑**

1. 在 `ConnectionNavigatorTree.tsx` 中为每个 Group Header 区域添加 `onDragOver` 与 `onDrop` 事件。
2. 若当前悬停的 Group 为 `RECENT_GROUP_KEY`，设置 `e.dataTransfer.dropEffect = 'none'`，显示不可放置状态样式。
3. 若为普通 Group 或 Pinned Group，放置后触发移动逻辑并保存。

- [ ] **Step 4: 运行测试验证通过**

运行：`npx vitest run src/windows/connection/__tests__/ConnectionNavigatorTreeDragGroup.test.tsx`  
预期：PASS

- [ ] **Step 5: 提交 Commit**

```bash
git add src/windows/connection/ConnectionNavigatorTree.tsx src/stores/connectionStore.ts src/windows/connection/__tests__/ConnectionNavigatorTreeDragGroup.test.tsx
git commit -m "feat(navigator): support dragging connections into groups and pin section"
```

---

### Task 6: 全局绿色按钮设计规范审计 (Green Button Audit)

**Files:**
- Audit & Modify: `src/windows/connection/QueryPanel.tsx`
- Audit & Modify: `src/windows/workflow/WorkflowPage.tsx`
- Audit & Modify: `src/components/ui/Button.tsx`
- Test: `src/components/ui/__tests__/ButtonVariantAudit.test.tsx`

**Constraint:**
- 绿色按钮（`variant="run"` 或包含 `bg-query-run` / `bg-green*` 的按钮）**仅限于 SQL 执行与 Workflow 执行**。
- 其他按钮统一转为 `secondary`、`ghost`、`primary`（accent 品牌色）或 `danger`（红）。
- 状态指示灯（如 Connected 绿点）保持不变。

- [ ] **Step 1: 编写按钮规范审计测试**

在 `src/components/ui/__tests__/ButtonVariantAudit.test.tsx` 中扫描/测试关键面板按钮的 variant，确保除 Execute/Run 外不存在其他绿色触发按钮。

- [ ] **Step 2: 运行测试排查不规范按钮**

运行：`npx vitest run src/components/ui/__tests__/ButtonVariantAudit.test.tsx`

- [ ] **Step 3: 修正非执行类按钮的绿色样式**

检查 `QueryPanel`、`WorkflowPage` 以及通用弹窗，将非 Execute 的按钮样式统一修正为规范样式。

- [ ] **Step 4: 运行测试验证通过**

运行：`npx vitest run src/components/ui/__tests__/ButtonVariantAudit.test.tsx`  
预期：PASS

- [ ] **Step 5: 提交 Commit**

```bash
git add src/windows/connection/QueryPanel.tsx src/windows/workflow/WorkflowPage.tsx src/components/ui/__tests__/ButtonVariantAudit.test.tsx
git commit -m "style(ui): enforce green button rule exclusively for SQL and workflow execution"
```

---

## 验收回归测试套件 (Self-Verification)

在所有任务完成后，执行以下综合检验：

```bash
# 1. 本地语言文件检查
node scripts/i18n-sync-check.mjs

# 2. TypeScript 类型检查（无新增类型错误）
npx tsc --noEmit

# 3. 前端相关单测全量通过
npx vitest run src/lib/__tests__/sqlGenerator.test.ts
npx vitest run src/components/__tests__/
npx vitest run src/windows/connection/
```
