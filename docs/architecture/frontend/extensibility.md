# 扩展性 — 数据库类型与插件系统

> [返回架构总览](../README.md)

### 1.1 设计原则

1. **注册表驱动**：`src/lib/databaseTypes.ts` 的 `DB_REGISTRY` 是所有 DB 类型行为的单一数据源
2. **路由层 if-else**：仅在组件选择边界使用 if-else，不在组件内部散布方言/类型分支
3. **策略模式**：SQL 方言逻辑集中在 `src/lib/sqlDialects/`，连接视图集中在 `src/lib/connectionViews/`

### 1.2 核心模块

```
src/lib/
├── databaseTypes.ts          # DB_REGISTRY + 行为标志 + normalizeRedisDatabaseField
├── sqlDialects/              # DDL / 索引 / 备份 方言策略
│   ├── postgresql.ts, mysql.ts, sqlite.ts
│   └── index.ts              # getSqlDialect(dbType)
└── connectionViews/          # 连接窗口视图注册表
    └── index.ts              # CONNECTION_VIEWS + getConnectionView(mode)

src/components/connection/    # 共享连接表单
├── useConnectionForm.ts      # 表单状态 + Kiwi 登录 + draft 构建
├── ConnectionFormBody.tsx    # 按 connectionForm 路由到 Fields 组件
├── KiwiConnectionFields.tsx
├── FileConnectionFields.tsx
├── IndexConnectionFields.tsx
└── StandardConnectionFields.tsx

src/windows/connection/
├── ConnectionWindow.tsx        # 薄壳：TitleBar + getConnectionView()
├── SqlConnectionView.tsx       # SQL 连接全部 UI
├── RedisConnectionView.tsx     # KV 连接 UI
└── schema-tree/
    ├── SchemaTree.tsx          # 路由（< 30 行）
    ├── StandardSchemaTree.tsx
    └── MultiDatabaseSchemaTree.tsx
```

### 1.3 DB_REGISTRY 行为标志

| 字段 | 用途 |
|------|------|
| `connectionView` | 路由到 `CONNECTION_VIEWS`（sql / keyvalue / document） |
| `connectionForm` | 路由到连接表单 Fields 组件（standard / kiwi / file / index） |
| `sqlDialect` | 路由到 `sqlDialects/` 策略 |
| `hasMultiDatabase` | 驱动**能力**标志。未配置逻辑库时走 `MultiDatabaseSchemaTree`；配置了逻辑库且该库 ∈ `get_databases` 时锁定 `StandardSchemaTree`。`databaseFieldType: 'domain'`（Kiwi）时 `connection.database` 为实例域名，**不**触发锁定 |
| `databaseFieldType` | `name` / `path` / `index` / `domain`（Kiwi 实例域名） |
| `defaultPageSize` | 覆盖默认分页（如 Kiwi 1000 行） |
| `supportsBackup` | BackupWindow 过滤 + 方言备份选项 |

### 1.4 添加新 DB 类型检查清单

1. `types/index.ts` — 添加 `DatabaseType` 联合成员
2. `databaseTypes.ts` — 添加 `DB_REGISTRY` 条目（含 `connectionForm` / `connectionView`）
3. （可选）`components/connection/` — 新表单 Fields 组件
4. （可选）`connectionViews/` — 注册新视图组件
5. （可选）`sqlDialects/` — 新方言策略文件
6. （可选）`schema-tree/` — 新 Schema 树变体

### 1.5 主题包 vs 驱动插件

主题包与驱动插件**不共享**安装路径或注册表：

| | 驱动插件 | 主题包 |
|---|---------|--------|
| 路径 | `packages/drivers/<id>/`（path 已提交；git 构建时 clone，gitignored） | `{appData}/themes/{id}/`（运行时 ZIP） |
| 注册 | `plugins-registry.json` + `DB_REGISTRY` | 文件系统 + `manifest.json` |
| 扩展 | Rust crate + 前端 meta | CSS / JSON / SVG\|PNG\|WebP / 字体（无 JS） |

驱动在无主题包（`packId: null`）下正常工作；主题包可覆盖 `db.<type>` 图标但不改变驱动协议。
