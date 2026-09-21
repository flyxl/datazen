# 扩展性 — 数据库类型与驱动系统

> [返回架构总览](../README.md)

### 1.1 设计原则

1. **注册表驱动**：`src/lib/databaseTypes.ts` 的 `DB_REGISTRY` 是所有 DB 类型行为的单一数据源（驱动条目经 codegen 的 `DRIVER_DB_ENTRIES` 在此合并，见 1.4）
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
├── ConnectionPage.tsx        # 统一主工作区壳：导航 + 连接 Tab + Workflow/Dashboard
├── ConnectionNavigatorTree.tsx # 左侧连接/Schema 导航
├── SqlConnectionView.tsx       # SQL 连接 UI
├── DocumentConnectionView.tsx  # 文档型连接 UI
└── schema-tree/
    ├── SchemaTree.tsx
    └── UnifiedSchemaTree.tsx
```

### 1.3 DB_REGISTRY 行为标志

| 字段 | 用途 |
|------|------|
| `connectionView` | 路由到 `CONNECTION_VIEWS`（sql / keyvalue / document） |
| `connectionForm` | 路由到连接表单 Fields 组件（standard / kiwi / file / index） |
| `sqlDialect` | 路由到 `sqlDialects/` 策略 |
| `hasMultiDatabase` | 驱动**能力**标志。`UnifiedSchemaTree` 内部通过 `isSingleDbMode` 切换单库/多库渲染 |
| `databaseFieldType` | `name` / `path` / `index` / `domain`（Kiwi 实例域名） |
| `defaultPageSize` | 覆盖默认分页（如 Kiwi 1000 行） |
| `supportsBackup` | BackupWindow 过滤 + 方言备份选项 |

### 1.4 添加新 DB 类型检查清单

新 DB 类型一律以**驱动包**（`packages/drivers/<id>/` 或独立 git 仓库）形式添加，不再手改宿主注册表：

1. `drivers-registry.json`（或 gitignored 的 `.drivers-dev.json`）— 注册驱动（`source: "path"` / `"git"`）
2. 驱动 Rust crate — `datazen-driver-api` trait 实现 + `inventory` 注册
3. 驱动前端入口 `ui/meta.ts`（或 `ui/shared/meta.ts`）— `DatabaseTypeMeta` 条目（含 `connectionForm` / `connectionView`）；`DatabaseType` 联合与 `DB_REGISTRY` 条目由 `scripts/resolve-drivers.mjs` codegen 的 `src/extensions/generated.ts`（`DRIVER_DB_ENTRIES`）自动合并，**禁止**在 `src/types/index.ts` / `src/lib/databaseTypes.ts` 手工添加
4. （可选）驱动 `ui/` — 连接表单 Fields、连接视图、Schema 树、SQL 方言策略、设置分区
5. （可选）驱动 `locales/` — 前端词条（契约见 [驱动 ↔ 宿主解耦契约](../../development/driver-api-dependency-boundary.md) 2.4）

宿主内置的表单/视图路由组件（`components/connection/`、`connectionViews/`、`schema-tree/`）仍按下表标志分发。前端 import 面与能力注入规则见 [独立驱动开发指南](../../development/independent-driver-development.zh-CN.md) 与解耦契约 Part 2。

### 1.5 主题包 vs 数据库驱动

主题包与数据库驱动**不共享**安装路径或注册表：

| | 数据库驱动 | 主题包 |
|---|---------|--------|
| 路径 | `packages/drivers/<id>/`（path 已提交；git 构建时 clone，gitignored） | Wapp 包内 `contributes.themes[]` 声明（运行时安装到 `{appData}/wapps/{publisher}.{name}/`） |
| 注册 | `drivers-registry.json` + `DB_REGISTRY` | 文件系统 + `manifest.json` |
| 扩展 | Rust crate + 前端 meta | CSS / JSON / SVG\|PNG\|WebP / 字体（无 JS） |

驱动在无主题包（`packId: null`）下正常工作；主题包可覆盖 `db.<type>` 图标与 `--dt-*`（含 `--dt-binary`）DataTable 单元格色，但不改变驱动协议。Redis 深度 UI 位于 `packages/drivers/redis/ui/`（非 Host `src/windows/connection/`）。
