# DataZen 架构重构实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 DataZen 重构为符合 SOLID 原则、易于扩展新数据库类型的架构；前端 if-else 仅用于组件路由选择，不同 DB 的 UI 逻辑抽取为独立组件。

**Architecture:** 采用「注册表驱动 + 策略模式」双层扩展机制。前端以 `DB_REGISTRY` 元数据 + `ConnectionViewRegistry` / `sqlDialects` 模块驱动 UI 选择；后端以 `DriverRegistry` + 接口隔离（`SqlDriver` / `KeyValueDriver`）驱动行为分发。每个 Phase 独立可合并，不破坏现有功能。

**Tech Stack:** Tauri 2 + Rust (async-trait, tokio) / React 18 + TypeScript + Zustand + Vitest + WebdriverIO e2e

**设计原则（贯穿全程）：**
1. if-else 只允许出现在**路由/选择层**（选哪个组件、哪个策略）
2. 新增数据库类型 = registry 条目 + driver 实现 +（可选）专用视图组件
3. 每个 PR/Phase 必须 `cargo test` + `npm test` + 相关 e2e 通过
4. 行为变更用测试锁定，纯搬移用 e2e 回归

---

## 文件结构总览

### 前端新增

```
src/
├── lib/
│   ├── databaseTypes.ts          # 扩展行为标志
│   ├── sqlDialects/
│   │   ├── index.ts              # 方言策略入口
│   │   ├── types.ts
│   │   ├── postgresql.ts
│   │   ├── mysql.ts
│   │   └── sqlite.ts
│   └── connectionViews/
│       ├── index.ts              # ConnectionViewRegistry
│       └── types.ts
├── components/connection/
│   ├── ConnectionFormBody.tsx    # 共享连接表单
│   ├── KiwiConnectionFields.tsx
│   ├── FileConnectionFields.tsx
│   ├── IndexConnectionFields.tsx
│   ├── StandardConnectionFields.tsx
│   └── useConnectionForm.ts      # 共享表单 hook
└── windows/connection/
    ├── SqlConnectionView.tsx     # 从 ConnectionWindow 抽出
    └── schema-tree/
        ├── SchemaTree.tsx        # 仅路由
        ├── StandardSchemaTree.tsx
        └── MultiDatabaseSchemaTree.tsx
```

### 后端新增

```
src-tauri/src/
├── commands/
│   ├── mod.rs                    # 薄层：re-export + generate_handler 列表
│   ├── connection.rs
│   ├── schema.rs
│   ├── query.rs
│   ├── kv.rs
│   ├── backup.rs
│   ├── sync.rs
│   ├── kiwi.rs
│   ├── config.rs
│   └── file.rs
├── db/
│   ├── traits/
│   │   ├── mod.rs
│   │   ├── sql.rs                # SqlDriver（从 DatabaseDriver 拆分）
│   │   └── kv.rs                 # KeyValueDriver
│   └── menu.rs                   # 从 lib.rs 抽出共享菜单构建
└── sync/
    ├── mod.rs
    └── mappers/
        ├── mod.rs
        ├── pg_to_mysql.rs
        └── mysql_to_pg.rs
```

---

## Phase 0：基线与安全网（1 天）

在任何重构前建立回归基线，确保后续搬移可验证。

### Task 0.1：记录当前测试基线

**Files:**
- Create: `docs/superpowers/plans/2026-07-13-refactor-baseline.md`

- [ ] **Step 1: 运行全量测试并记录结果**

```bash
cd /Users/flyxl/code/datazen
cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tee /tmp/rust-test-baseline.txt
npm test 2>&1 | tee /tmp/vitest-baseline.txt
```

- [ ] **Step 2: 记录 e2e 覆盖的数据库类型**

当前 e2e specs 覆盖：`postgresql`(implicit), `mysql`, `sqlite`, `redis`, `kiwi`。将各 spec 文件路径写入 baseline 文档。

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/plans/2026-07-13-refactor-baseline.md
git commit -m "docs: add architecture refactor test baseline"
```

---

## Phase 1：扩展 DB_REGISTRY 行为标志（2 天）

**目标：** 消除 `databaseType === 'kiwi'` 等硬编码，让所有行为差异由 registry 驱动。

### Task 1.1：扩展 DatabaseTypeMeta 接口

**Files:**
- Modify: `src/lib/databaseTypes.ts`
- Test: `src/lib/__tests__/databaseTypes.test.ts`（新建）

- [ ] **Step 1: 写失败测试**

```typescript
// src/lib/__tests__/databaseTypes.test.ts
import { describe, it, expect } from 'vitest';
import { DB_REGISTRY } from '../databaseTypes';

describe('DB_REGISTRY behavioral flags', () => {
  it('kiwi has multi-database and fixed page size', () => {
    expect(DB_REGISTRY.kiwi.hasMultiDatabase).toBe(true);
    expect(DB_REGISTRY.kiwi.defaultPageSize).toBe(1000);
    expect(DB_REGISTRY.kiwi.connectionForm).toBe('kiwi');
  });

  it('redis uses index form and keyvalue view', () => {
    expect(DB_REGISTRY.redis.connectionForm).toBe('index');
    expect(DB_REGISTRY.redis.connectionView).toBe('keyvalue');
  });

  it('sqlite uses file form', () => {
    expect(DB_REGISTRY.sqlite.connectionForm).toBe('file');
  });

  it('standard sql dbs use standard form', () => {
    expect(DB_REGISTRY.postgresql.connectionForm).toBe('standard');
    expect(DB_REGISTRY.mysql.connectionForm).toBe('standard');
  });
});
```

- [ ] **Step 2: 运行测试确认失败**

```bash
npm test -- src/lib/__tests__/databaseTypes.test.ts
```
Expected: FAIL — `hasMultiDatabase` undefined

- [ ] **Step 3: 扩展接口并为每个 DB 赋值**

在 `DatabaseTypeMeta` 中新增：

```typescript
/** 是否支持多数据库/多实例树（如 Kiwi） */
hasMultiDatabase?: boolean;
/** 默认分页大小；未设置则使用全局默认 */
defaultPageSize?: number;
/** 连接表单变体 */
connectionForm: 'standard' | 'kiwi' | 'file' | 'index';
```

各条目赋值：
- `postgresql`, `mysql`: `connectionForm: 'standard'`
- `sqlite`: `connectionForm: 'file'`
- `redis`: `connectionForm: 'index'`
- `kiwi`: `connectionForm: 'kiwi'`, `hasMultiDatabase: true`, `defaultPageSize: 1000`

- [ ] **Step 4: 运行测试确认通过**

```bash
npm test -- src/lib/__tests__/databaseTypes.test.ts
```

- [ ] **Step 5: Commit**

```bash
git add src/lib/databaseTypes.ts src/lib/__tests__/databaseTypes.test.ts
git commit -m "feat: add behavioral flags to DB_REGISTRY"
```

### Task 1.2：替换硬编码引用

**Files:**
- Modify: `src/windows/connection/SchemaTree.tsx:30`
- Modify: `src/stores/tableDataStore.ts:191`
- Modify: `src/windows/main/NewConnectionDialog.tsx:215-216`
- Modify: `src/windows/new-connection/NewConnectionWindow.tsx:269,298`

- [ ] **Step 1: SchemaTree 改用 registry**

```typescript
// 替换
const hasMultiDb = databaseType === 'kiwi';
// 为
const hasMultiDb = DB_REGISTRY[databaseType]?.hasMultiDatabase ?? false;
```

- [ ] **Step 2: tableDataStore 改用 registry**

```typescript
// 替换
const pageSize = databaseType === 'kiwi' ? 1000 : existing.pageSize;
// 为
const defaultPageSize = DB_REGISTRY[databaseType]?.defaultPageSize;
const pageSize = defaultPageSize ?? existing.pageSize;
```

- [ ] **Step 3: NewConnectionDialog / NewConnectionWindow 改用 connectionForm**

```typescript
// 替换
const isKiwi = databaseType === 'kiwi';
const hasUsername = !!meta.defaultUser || isKiwi;
// 为
const formVariant = meta.connectionForm;
const hasUsername = !!meta.defaultUser || formVariant === 'kiwi';
```

表单 JSX 中 `isKiwi ?` 改为 `formVariant === 'kiwi' ?`，`isFileMode` 改为 `formVariant === 'file' ?`，`isIndexMode` 改为 `formVariant === 'index' ?`。

- [ ] **Step 4: 运行测试**

```bash
npm test
npm run test:e2e -- --spec e2e/specs/kiwi.ts
npm run test:e2e -- --spec e2e/specs/redis.ts
```

- [ ] **Step 5: Commit**

```bash
git commit -am "refactor: replace databaseType string checks with DB_REGISTRY flags"
```

---

## Phase 2：SQL 方言策略模块（3 天）

**目标：** 将 DDL、索引、备份中的方言逻辑集中到 `src/lib/sqlDialects/`，视图组件只做渲染。

### Task 2.1：创建方言类型与策略接口

**Files:**
- Create: `src/lib/sqlDialects/types.ts`
- Create: `src/lib/sqlDialects/index.ts`
- Test: `src/lib/sqlDialects/__tests__/dialects.test.ts`

- [ ] **Step 1: 定义类型**

```typescript
// src/lib/sqlDialects/types.ts
import type { DatabaseType } from '../../types';

export type SqlDialectFamily = 'postgresql' | 'mysql' | 'sqlite';

export interface DdlQueries {
  getTableDdl(tableName: string, schema?: string): string;
}

export interface IndexDialect {
  getDropIndexSql(indexName: string, tableName: string, schema?: string): string;
  getCreateIndexSql(opts: {
    indexName: string;
    tableName: string;
    columns: string[];
    unique?: boolean;
    method?: string; // GIN, GiST, etc.
    schema?: string;
  }): string;
  supportedIndexMethods: string[]; // e.g. ['btree', 'gin', 'gist'] or ['btree']
}

export interface BackupDialectOptions {
  value: string;
  labelKey: string; // i18n key
}

export interface SqlDialectStrategy {
  family: SqlDialectFamily;
  ddl: DdlQueries;
  index: IndexDialect;
  backupOptions: BackupDialectOptions[];
}
```

- [ ] **Step 2: 实现 postgresql.ts**

从 `DDLView.tsx` 提取 PG 的 catalog 查询（`pg_get_tabledef` 或现有内联 SQL），从 `IndexesView.tsx` 提取 `DROP INDEX` PG 语法。

```typescript
// src/lib/sqlDialects/postgresql.ts
import type { SqlDialectStrategy } from './types';

export const postgresqlDialect: SqlDialectStrategy = {
  family: 'postgresql',
  ddl: {
    getTableDdl(tableName, schema = 'public') {
      return `SELECT pg_get_tabledef('${schema}.${tableName}')`; // 使用项目现有查询
    },
  },
  index: {
    supportedIndexMethods: ['btree', 'gin', 'gist', 'hash'],
    getDropIndexSql(indexName, _tableName, schema) {
      const q = '"';
      const qualified = schema ? `${q}${schema}${q}.` : '';
      return `DROP INDEX ${qualified}${q}${indexName}${q}`;
    },
    getCreateIndexSql(opts) { /* 从 IndexesView 提取 */ return ''; },
  },
  backupOptions: [
    { value: 'pg_dump_plain', labelKey: 'backup.pg.plain' },
    { value: 'pg_dump_custom', labelKey: 'backup.pg.custom' },
  ],
};
```

- [ ] **Step 3: 实现 mysql.ts 和 sqlite.ts**

`mysql.ts`：从 `DDLView` 提取 `SHOW CREATE TABLE`，从 `IndexesView` 提取 `DROP INDEX ... ON table`。

`sqlite.ts`：新增 `SELECT sql FROM sqlite_master WHERE type='table' AND name='...'`（修复当前 SQLite DDL 缺失问题）。

- [ ] **Step 4: 创建入口函数**

```typescript
// src/lib/sqlDialects/index.ts
import { DB_REGISTRY } from '../databaseTypes';
import type { DatabaseType } from '../../types';
import type { SqlDialectStrategy } from './types';
import { postgresqlDialect } from './postgresql';
import { mysqlDialect } from './mysql';
import { sqliteDialect } from './sqlite';

const DIALECTS: Record<string, SqlDialectStrategy> = {
  postgresql: postgresqlDialect,
  mysql: mysqlDialect,
  sqlite: sqliteDialect,
};

export function getSqlDialect(dbType: DatabaseType): SqlDialectStrategy | null {
  const family = DB_REGISTRY[dbType]?.sqlDialect;
  return family ? DIALECTS[family] ?? null : null;
}
```

- [ ] **Step 5: 写测试**

```typescript
describe('getSqlDialect', () => {
  it('returns mysql dialect for kiwi (mysql family)', () => {
    expect(getSqlDialect('kiwi')?.family).toBe('mysql');
  });
  it('sqlite ddl uses sqlite_master', () => {
    const sql = getSqlDialect('sqlite')!.ddl.getTableDdl('users');
    expect(sql).toContain('sqlite_master');
  });
});
```

- [ ] **Step 6: Commit**

```bash
git add src/lib/sqlDialects/
git commit -m "feat: add sqlDialects strategy module"
```

### Task 2.2：重构 DDLView 使用方言策略

**Files:**
- Modify: `src/windows/connection/DDLView.tsx`

- [ ] **Step 1: 移除组件内方言 if-else**

```typescript
// DDLView.tsx — loadDdl effect 内
import { getSqlDialect } from '../../lib/sqlDialects';

const dialect = getSqlDialect(databaseType as DatabaseType);
if (!dialect) {
  setError('DDL not supported for this database type');
  return;
}
const sql = dialect.ddl.getTableDdl(tableName, schema);
```

- [ ] **Step 2: 运行 e2e**

```bash
npm run test:e2e -- --spec e2e/specs/table-structure.ts
```

- [ ] **Step 3: Commit**

```bash
git commit -am "refactor: DDLView delegates to sqlDialects module"
```

### Task 2.3：重构 IndexesView 和 BackupWindow

**Files:**
- Modify: `src/windows/connection/IndexesView.tsx`
- Modify: `src/windows/backup/BackupWindow.tsx`

- [ ] **Step 1: IndexesView 移除 isMySQLDialect 分支**

用 `getSqlDialect(databaseType)?.index` 获取 drop/create SQL 和 `supportedIndexMethods`。GIN/GiST 选项渲染改为：

```typescript
{dialect?.index.supportedIndexMethods.includes('gin') && (
  <option value="gin">GIN</option>
)}
```

- [ ] **Step 2: BackupWindow 使用方言 backupOptions + supportsBackup 过滤**

```typescript
const backupOptions = useMemo(() => {
  if (!selectedConn) return [];
  const meta = DB_REGISTRY[selectedConn.databaseType];
  if (!meta.supportsBackup) return [];
  return getSqlDialect(selectedConn.databaseType)?.backupOptions ?? [];
}, [selectedConn]);
```

连接列表过滤：仅显示 `supportsBackup: true` 的连接。

- [ ] **Step 3: 运行 e2e**

```bash
npm run test:e2e -- --spec e2e/specs/backup-database.ts
```

- [ ] **Step 4: Commit**

```bash
git commit -am "refactor: IndexesView and BackupWindow use sqlDialects"
```

---

## Phase 3：连接表单组件抽取（3 天）

**目标：** 消除 `NewConnectionDialog` 与 `NewConnectionWindow` 约 85% 重复代码。

### Task 3.1：抽取 useConnectionForm hook

**Files:**
- Create: `src/components/connection/useConnectionForm.ts`
- Test: `src/components/connection/__tests__/useConnectionForm.test.ts`

- [ ] **Step 1: 从 NewConnectionDialog 提取共享状态和逻辑**

Hook 返回：

```typescript
export interface UseConnectionFormOptions {
  initialDraft?: Partial<ConnectionConfig>;
  onSaved?: () => void;
}

export function useConnectionForm(opts: UseConnectionFormOptions) {
  // 状态: draft, databaseType, testing, testResult, kiwiInstances, kiwiToken, ...
  // 方法: onTest, onSave, setField, handleKiwiLogin, normalizeRedisDatabaseField
  // 派生: meta, formVariant, hasUsername, sslOptions
  return { draft, databaseType, meta, formVariant, /* ... */ onTest, onSave, setField };
}
```

将 `normalizeRedisDatabaseField` 从两个文件移到 `src/lib/databaseTypes.ts`：

```typescript
export function normalizeRedisDatabaseField(value: string): string {
  const n = parseInt(value, 10);
  return Number.isNaN(n) ? '0' : String(Math.max(0, n));
}
```

- [ ] **Step 2: 写 hook 单元测试**（至少覆盖 redis index 归一化、kiwi form variant 检测）

- [ ] **Step 3: Commit**

```bash
git add src/components/connection/ src/lib/databaseTypes.ts
git commit -m "feat: extract useConnectionForm hook"
```

### Task 3.2：抽取表单字段组件

**Files:**
- Create: `src/components/connection/KiwiConnectionFields.tsx`
- Create: `src/components/connection/FileConnectionFields.tsx`
- Create: `src/components/connection/IndexConnectionFields.tsx`
- Create: `src/components/connection/StandardConnectionFields.tsx`
- Create: `src/components/connection/ConnectionFormBody.tsx`

- [ ] **Step 1: KiwiConnectionFields** — 从 NewConnectionDialog 285-331 行提取 Kiwi URL、登录按钮、实例选择器

- [ ] **Step 2: 其他三个 Fields 组件** — 按 `connectionForm` 变体拆分

- [ ] **Step 3: ConnectionFormBody 作为路由组件**

```typescript
// ConnectionFormBody.tsx
export function ConnectionFormBody({ form }: { form: ReturnType<typeof useConnectionForm> }) {
  const { formVariant, meta, draft, setField, /* kiwi props */ } = form;

  return (
    <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
      {/* DB type selector — 共有 */}
      {formVariant === 'file' && <FileConnectionFields ... />}
      {formVariant === 'index' && <IndexConnectionFields ... />}
      {formVariant === 'kiwi' && <KiwiConnectionFields ... />}
      {formVariant === 'standard' && <StandardConnectionFields ... />}
      {/* SSH / SSL — 共有，由 meta.supportsSSH / supportsSSL 控制 */}
    </div>
  );
}
```

**关键：** if-else 仅在此路由组件出现，各 Fields 组件内部无 DB 类型分支。

- [ ] **Step 4: 重构 NewConnectionDialog 和 NewConnectionWindow**

两个文件各缩减到 ~100 行：Dialog/Window 外壳 + `useConnectionForm` + `ConnectionFormBody`。

- [ ] **Step 5: 运行 e2e**

```bash
npm run test:e2e -- --spec e2e/specs/new-connection.ts
npm run test:e2e -- --spec e2e/specs/edit-delete-connection.ts
```

- [ ] **Step 6: Commit**

```bash
git commit -am "refactor: consolidate connection form into shared components"
```

---

## Phase 4：Schema 树与连接视图拆分（3 天）

**目标：** `SchemaTree` 和 `ConnectionWindow` 只做路由，业务 UI 在子组件中。

### Task 4.1：拆分 SchemaTree

**Files:**
- Create: `src/windows/connection/schema-tree/StandardSchemaTree.tsx`
- Create: `src/windows/connection/schema-tree/MultiDatabaseSchemaTree.tsx`
- Modify: `src/windows/connection/schema-tree/SchemaTree.tsx`（从原文件移入并瘦身）
- Delete: `src/windows/connection/SchemaTree.tsx`（搬移后）

- [ ] **Step 1: 将 SchemaTree.tsx 95-185 行（hasMultiDb 分支）移到 MultiDatabaseSchemaTree.tsx**

- [ ] **Step 2: 将默认单库树逻辑移到 StandardSchemaTree.tsx**

`isKeyValue` 标签差异通过 props 传入：

```typescript
interface StandardSchemaTreeProps {
  labels: { tables: string; views: string; database: string };
  // ...其他 props
}
```

路由组件：

```typescript
// schema-tree/SchemaTree.tsx
export function SchemaTree(props: SchemaTreeProps) {
  const meta = DB_REGISTRY[props.databaseType];
  if (meta?.hasMultiDatabase) {
    return <MultiDatabaseSchemaTree {...props} />;
  }
  const labels = meta?.isKeyValue
    ? { tables: t('schema.keys'), views: t('schema.sets'), database: t('schema.db') }
    : { tables: t('schema.tables'), views: t('schema.views'), database: t('schema.database') };
  return <StandardSchemaTree {...props} labels={labels} />;
}
```

- [ ] **Step 3: 更新 import 路径**（`ConnectionWindow.tsx` 等）

- [ ] **Step 4: 运行 e2e**

```bash
npm run test:e2e -- --spec e2e/specs/connection-window.ts
npm run test:e2e -- --spec e2e/specs/kiwi.ts
npm run test:e2e -- --spec e2e/specs/redis.ts
```

- [ ] **Step 5: Commit**

```bash
git commit -am "refactor: split SchemaTree into variant components"
```

### Task 4.2：创建 ConnectionViewRegistry + SqlConnectionView

**Files:**
- Create: `src/lib/connectionViews/index.ts`
- Create: `src/lib/connectionViews/types.ts`
- Create: `src/windows/connection/SqlConnectionView.tsx`
- Modify: `src/windows/connection/ConnectionWindow.tsx`

- [ ] **Step 1: 定义 registry**

```typescript
// src/lib/connectionViews/index.ts
import type { ComponentType } from 'react';
import { RedisConnectionView } from '../../windows/connection/RedisConnectionView';
import { SqlConnectionView } from '../../windows/connection/SqlConnectionView';
import type { ConnectionViewProps } from './types';

export const CONNECTION_VIEWS: Record<string, ComponentType<ConnectionViewProps>> = {
  sql: SqlConnectionView,
  keyvalue: RedisConnectionView,
  // document: DocumentConnectionView, // future
};

export function getConnectionView(mode: string) {
  return CONNECTION_VIEWS[mode] ?? SqlConnectionView;
}
```

- [ ] **Step 2: 从 ConnectionWindow 487-792 行抽出 SqlConnectionView**

包含：工具栏、Schema 侧栏、Panel Tabs、TableView/QueryPanel/StructureView 等 SQL 连接全部 UI。

- [ ] **Step 3: ConnectionWindow 瘦身**

```typescript
export function ConnectionWindow() {
  // ...参数解析、TitleBar、detail panel 逻辑保留
  const viewMode = dbMeta?.connectionView ?? 'sql';
  const ViewComponent = getConnectionView(viewMode);

  return (
    <div className="flex h-screen min-h-0 flex-col bg-surface text-fg">
      <TitleBar ... />
      <ViewComponent
        connectionId={connectionId}
        connectionName={connectionName}
        databaseType={dbType}
        initialDatabase={initialDatabase}
        // detail panel props
      />
    </div>
  );
}
```

- [ ] **Step 4: 运行 e2e**

```bash
npm run test:e2e -- --spec e2e/specs/connection-window.ts
```

- [ ] **Step 5: Commit**

```bash
git commit -am "refactor: extract SqlConnectionView and ConnectionViewRegistry"
```

---

## Phase 5：后端 commands 模块拆分（4 天）

**目标：** 将 1,719 行的 `commands/mod.rs` 拆为职责单一的子模块，不改 IPC 接口名。

### Task 5.1：创建子模块骨架

**Files:**
- Create: `src-tauri/src/commands/connection.rs`
- Create: `src-tauri/src/commands/schema.rs`
- Create: `src-tauri/src/commands/query.rs`
- Create: `src-tauri/src/commands/kv.rs`
- Create: `src-tauri/src/commands/backup.rs`
- Create: `src-tauri/src/commands/sync.rs`
- Create: `src-tauri/src/commands/kiwi.rs`
- Create: `src-tauri/src/commands/config.rs`
- Create: `src-tauri/src/commands/file.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: 按函数列表搬移（保持 pub async fn 签名不变）**

| 模块 | 函数 |
|------|------|
| `connection.rs` | `get_connections`, `save_connection`, `delete_connection`, `test_connection`, `connect`, `ping_connection`, `disconnect`, `get_connection_info` |
| `schema.rs` | `get_databases`, `get_tables`, `get_columns`, `get_table_schema`, `get_table_data` |
| `kv.rs` | `kv_scan_keys`, `kv_get_key` |
| `query.rs` | `execute_query`, `get_explain`, `cancel_query`, `get_query_history`, `clear_query_history`, `get_favorite_queries`, `add_favorite_query`, `delete_favorite_query` |
| `backup.rs` | `backup_database`, `restore_database` |
| `sync.rs` | `compare_databases`, `sync_table`, `sync_tables`, `get_sync_tasks`, `save_sync_task_direct`, `delete_sync_task`, `check_sync_conflicts` |
| `kiwi.rs` | `kiwi_login`, `kiwi_list_instances` |
| `config.rs` | `get_groups`, `save_groups`, `get_settings`, `save_settings`, `export_connections`, `import_connections_preview` |
| `file.rs` | `write_file`, `read_file`, `show_editor_context_menu` |

- [ ] **Step 2: mod.rs 仅保留 re-export**

```rust
// src-tauri/src/commands/mod.rs
mod connection;
mod schema;
mod query;
mod kv;
mod backup;
mod sync;
mod kiwi;
mod config;
mod file;

pub use connection::*;
pub use schema::*;
// ... 其余 re-export
```

- [ ] **Step 3: 确认 lib.rs 的 generate_handler 列表不变**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
cargo build --manifest-path src-tauri/Cargo.toml
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/
git commit -m "refactor: split commands/mod.rs into domain modules"
```

### Task 5.2：抽取 sync 类型映射

**Files:**
- Create: `src-tauri/src/sync/mod.rs`
- Create: `src-tauri/src/sync/mappers/mod.rs`
- Create: `src-tauri/src/sync/mappers/pg_to_mysql.rs`
- Create: `src-tauri/src/sync/mappers/mysql_to_pg.rs`
- Modify: `src-tauri/src/commands/sync.rs`
- Modify: `src-tauri/src/lib.rs`（添加 `mod sync;`）

- [ ] **Step 1: 定义 TypeMapper trait**

```rust
pub trait TypeMapper: Send + Sync {
    fn map_type(&self, source_type: &str, nullable: bool) -> String;
    fn map_default(&self, source_default: Option<&str>) -> Option<String>;
}

pub struct PgToMysqlMapper;
impl TypeMapper for PgToMysqlMapper { /* 从 sync.rs 搬移 */ }

pub struct MysqlToPgMapper;
impl TypeMapper for MysqlToPgMapper { /* 从 sync.rs 搬移 */ }
```

- [ ] **Step 2: sync.rs 调用 mapper 而非内联 match**

- [ ] **Step 3: 运行测试 + e2e**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
npm run test:e2e -- --spec e2e/specs/data-sync-real.ts
```

- [ ] **Step 4: Commit**

```bash
git commit -am "refactor: extract PG/MySQL sync type mappers"
```

### Task 5.3：去重 lib.rs 菜单构建

**Files:**
- Create: `src-tauri/src/menu.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 提取共享函数**

```rust
// src-tauri/src/menu.rs
pub fn build_app_menu<R: Runtime>(
    app: &AppHandle<R>,
    theme: &str,
    lang: &str,
) -> Result<Menu<R>, tauri::Error> {
    // 合并 build_app_menu 和 rebuild_menu_for_handle 的共有逻辑
}

pub fn rebuild_menu_for_handle<R: Runtime>(app: &AppHandle<R>) -> Result<(), tauri::Error> {
    let theme = /* read from settings */;
    let lang = /* read from settings */;
    let menu = build_app_menu(app, &theme, &lang)?;
    app.set_menu(menu)
}
```

- [ ] **Step 2: cargo build + 手动验证菜单**

- [ ] **Step 3: Commit**

```bash
git commit -am "refactor: deduplicate menu building in menu.rs"
```

---

## Phase 6：后端驱动接口隔离（5 天）

**目标：** 消除 `registry.redis` 硬依赖和胖 `DatabaseDriver` trait。

### Task 6.1：定义 KeyValueDriver trait

**Files:**
- Create: `src-tauri/src/db/traits/mod.rs`
- Create: `src-tauri/src/db/traits/kv.rs`
- Modify: `src-tauri/src/db/drivers/redis.rs`
- Modify: `src-tauri/src/db/registry.rs`
- Modify: `src-tauri/src/commands/kv.rs`

- [ ] **Step 1: 定义 KV trait**

```rust
// src-tauri/src/db/traits/kv.rs
#[async_trait]
pub trait KeyValueDriver: Send + Sync {
    fn driver_type(&self) -> DatabaseType;
    async fn scan_keys(&self, handle: &ConnectionHandle, pattern: &str, cursor: u64, count: usize)
        -> Result<KvScanResult, DriverError>;
    async fn get_key(&self, handle: &ConnectionHandle, key: &str)
        -> Result<KvKeyInfo, DriverError>;
}
```

- [ ] **Step 2: RedisDriver 实现 KeyValueDriver**

- [ ] **Step 3: DriverRegistry 改为按 category 分发**

```rust
impl DriverRegistry {
    pub fn get_kv_driver(&self, db_type: DatabaseType) -> Option<Arc<dyn KeyValueDriver>> {
        match db_type {
            DatabaseType::Redis => Some(self.redis.clone() as Arc<dyn KeyValueDriver>),
            _ => None,
        }
    }
}
```

- [ ] **Step 4: kv.rs 命令改用 get_kv_driver**

```rust
// 替换
if config.database_type != DatabaseType::Redis { ... }
let driver = state.registry.redis.clone();
// 为
let driver = state.registry.get_kv_driver(config.database_type)
    .ok_or_else(|| "Key-value operations not supported".to_string())?;
```

- [ ] **Step 5: cargo test + redis e2e**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
npm run test:e2e -- --spec e2e/specs/redis.ts
```

- [ ] **Step 6: Commit**

```bash
git commit -am "refactor: introduce KeyValueDriver trait for KV operations"
```

### Task 6.2：单元格更新 SQL 下沉后端

**Files:**
- Create: `src-tauri/src/commands/data.rs`（或放入 `schema.rs`）
- Modify: `src/stores/tableDataStore.ts`
- Modify: `src/commands/database.ts`

- [ ] **Step 1: 新增 Tauri 命令 update_cell**

```rust
#[tauri::command]
pub async fn update_cell(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
    column: String,
    value: serde_json::Value,
    row_id: serde_json::Value, // PK values
) -> Result<String, String> {
    let driver = state.connection_manager.get_driver(&connection_id).await?;
    let sql = driver.build_update_sql(&table, &column, &value, &row_id)?;
    // execute and return sql for UI feedback
}
```

在 `DatabaseDriver` trait 添加默认方法 `build_update_sql` 和 `format_value`，MySQL driver override 布尔值为 `1`/`0`。

- [ ] **Step 2: 前端 tableDataStore 调用新命令**

- [ ] **Step 3: 更新 tableDataStore 测试**

- [ ] **Step 4: Commit**

```bash
git commit -am "feat: move cell update SQL generation to backend driver"
```

### Task 6.3：消除 Kiwi 连接 ID 前缀 hack

**Files:**
- Modify: `src-tauri/src/commands/schema.rs`（`get_table_data`）
- Modify: `src-tauri/src/db/drivers/kiwi.rs`

- [ ] **Step 1: 在 driver 上添加行为标志**

```rust
// DatabaseDriver trait
fn skip_count_query(&self) -> bool { false }

// KiwiDriver
fn skip_count_query(&self) -> bool { true }
```

- [ ] **Step 2: 替换 connection_id.starts_with("kiwi_") 检查**

```rust
let driver = state.connection_manager.get_driver(&connection_id).await?;
let effective_skip_count = skip_count.unwrap_or(false) || driver.skip_count_query();
```

- [ ] **Step 3: cargo test + kiwi e2e**

- [ ] **Step 4: Commit**

```bash
git commit -am "refactor: replace kiwi_ prefix hack with driver metadata"
```

---

## Phase 7：文档与收尾（1 天）

### Task 7.1：更新架构文档

**Files:**
- Modify: `docs/frontend-architecture.md`
- Modify: `docs/backend-architecture.md`
- Modify: `README.md`（新增 DB 扩展指南章节）

- [ ] **Step 1: 在前端架构文档中补充**
  - `DB_REGISTRY` 行为标志说明
  - `ConnectionViewRegistry` 用法
  - `sqlDialects` 扩展指南
  - 连接表单组件结构图

- [ ] **Step 2: 在后端架构文档中补充**
  - commands 模块划分
  - `KeyValueDriver` / `SqlDriver` 接口隔离
  - sync mappers 注册方式

- [ ] **Step 3: README 新增「添加数据库类型」清单**

```markdown
## 添加新数据库类型

### 后端
1. 在 `DatabaseType` 枚举添加变体
2. 实现 `DatabaseDriver`（SQL）或 `KeyValueDriver`（KV）
3. 在 `init_drivers()` 注册
4. （可选）实现 `TypeMapper` 用于数据同步

### 前端
1. 在 `types/index.ts` 添加联合类型
2. 在 `DB_REGISTRY` 添加元数据条目
3. （可选）若需专用 UI：`connectionViews/` 注册新视图组件
4. （可选）若需专用表单：`components/connection/` 添加 Fields 组件并在 `ConnectionFormBody` 路由
5. （可选）若 SQL 方言不同：在 `sqlDialects/` 添加策略文件
```

- [ ] **Step 4: Commit**

```bash
git commit -am "docs: update architecture docs for extensibility patterns"
```

---

## 实施顺序与 PR 策略

| Phase | 预估工期 | 可独立合并 | 依赖 |
|-------|---------|-----------|------|
| 0 基线 | 0.5 天 | ✅ | 无 |
| 1 DB_REGISTRY 扩展 | 2 天 | ✅ | Phase 0 |
| 2 sqlDialects | 3 天 | ✅ | Phase 1 |
| 3 连接表单抽取 | 3 天 | ✅ | Phase 1 |
| 4 Schema/View 拆分 | 3 天 | ✅ | Phase 1 |
| 5 commands 拆分 | 4 天 | ✅ | Phase 0 |
| 6 驱动接口隔离 | 5 天 | ✅ | Phase 5 |
| 7 文档 | 1 天 | ✅ | 全部 |

**推荐 PR 顺序：** 0 → 1 → (2 ∥ 3 ∥ 4 可并行) → 5 → 6 → 7

Phase 2、3、4 互相独立，可分配给不同开发者并行推进。

---

## 验收标准

### 架构合规

- [ ] 前端不存在 `databaseType === 'xxx'` 字符串比较（仅允许在 `DB_REGISTRY` 定义处）
- [ ] 前端组件内部不存在 `sqlDialect === 'mysql'` 分支（仅在 `sqlDialects/` 模块内）
- [ ] `ConnectionWindow` < 200 行，仅含 TitleBar + View 路由
- [ ] `SchemaTree` < 30 行，仅含变体路由
- [ ] `commands/mod.rs` < 50 行，仅含 re-export
- [ ] `kv_*` 命令不直接访问 `registry.redis`

### 功能回归

- [ ] `cargo test` 全部通过
- [ ] `npm test` 全部通过
- [ ] e2e specs 全部通过：`mysql`, `sqlite`, `redis`, `kiwi`, `connection-window`, `new-connection`, `backup-database`, `data-sync-real`, `table-structure`

### 扩展性验证（模拟添加新 DB）

- [ ] 在 `DB_REGISTRY` 添加一个 `mockdb` 条目 + 空 driver 桩，确认表单/视图路由无需修改其他文件即可识别新型
- [ ] 编写 `sqlDialects/mock.ts` 并注册，确认 DDLView 自动使用新方言

---

## 风险与缓解

| 风险 | 缓解措施 |
|------|---------|
| 大文件搬移引入回归 | Phase 0 基线 + 每 Task 跑相关 e2e |
| SqlConnectionView 抽出后 prop drilling 过深 | 使用现有 Zustand stores，View 组件直接从 store 读取 |
| 后端 trait 拆分破坏 Redis SQL 适配 | 保持 `DatabaseDriver` 作为 superset，KV trait 为附加而非替换 |
| 多人并行 Phase 2/3/4 冲突 | 先合并 Phase 1（registry 扩展），再并行 |

---

## Self-Review Checklist

| 审查项 | 覆盖 Task |
|--------|----------|
| SOLID — SRP (commands 拆分) | Phase 5 |
| SOLID — OCP (registry 驱动) | Phase 1, 4 |
| SOLID — ISP (KV trait) | Phase 6 |
| SOLID — DIP (方言策略) | Phase 2 |
| 前端 if-else 仅路由层 | Phase 3, 4 |
| 公共组件抽取 | Phase 3, 4 |
| sqlDialects 集中 | Phase 2 |
| 后端 sync 模块化 | Phase 5.2 |
| 文档更新 | Phase 7 |
| 无 placeholder | ✅ 全部 Task 含具体文件路径和代码 |
