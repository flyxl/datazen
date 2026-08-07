# 黑盒测试报告 — 配置了 database 只显示单库

| 字段 | 内容 |
|------|------|
| **功能** | 连接配置了 `database` 时 Schema 树锁定单库（StandardSchemaTree）；未配置时列出全部可见库（MultiDatabaseSchemaTree） |
| **日期** | 2026-08-07 |
| **测试者** | test-agent（report-only，未改业务代码） |
| **分支** | `feat/multi-database-session-ui` |
| **范围** | 黑盒 TC-A～TC-D + Vitest 交叉验证 |

## 环境

| 项目 | 配置 |
|------|------|
| **操作系统** | macOS 15.7.3 (Darwin 24.6.0, Apple Silicon) |
| **应用** | DataZen v0.0.8，`pnpm tauri:dev --plugins=superset,kiwi` |
| **进程** | `/Users/flyxl/code/datazen/target/debug/datazen` (pid 49743) |
| **自动化** | `user-use-computer` MCP（accessibility tree + 窗口截图） |
| **MySQL** | 127.0.0.1:3306，用户 `root`，无密码；可见库 6 个 |
| **PostgreSQL** | 127.0.0.1:5432（`/tmp:5432` 接受连接）；可见库 9 个 |

## 实现要点（静态核对）

- `resolveVisibleDatabases`：配置了 `preferredDatabase` 时返回 `databases: [configured]`、`lockedToConfigured: true`
- `SchemaTree.tsx`：`hasMultiDatabase && !initialDatabase?.trim()` → `MultiDatabaseSchemaTree`，否则 → `StandardSchemaTree`

## 自动化交叉验证

### Vitest — PASS

```text
$ npx vitest run \
    src/stores/__tests__/schemaStore.test.ts \
    src/windows/connection/schema-tree/__tests__/SchemaTree.test.tsx

 Test Files  2 passed (2)
      Tests  19 passed (19)
   Duration  1.25s
```

覆盖：`resolveVisibleDatabases` 锁定逻辑、`mysql/postgresql` 有/无 `initialDatabase` 的 SchemaTree 路由与 `isMultiDatabase`。

## 黑盒用例结果

### TC-A：MySQL/MariaDB — 配置了 database

| 字段 | 内容 |
|------|------|
| **结果** | **PASS** |
| **连接** | `E2E-MySQL-Types` — 127.0.0.1:3306，Database=`datazen_test` |
| **操作** | 主窗口双击连接 → 打开 Connection Window |

**预期 vs 实际**

| 预期 | 实际 |
|------|------|
| 单库风格，有「Tables」分组 | ✅ 侧栏显示 `datazen_test` 标题 + **表 (6)** 分组，列出 6 张表 |
| 无多个数据库可展开节点 | ✅ 无 `mysql` / `information_schema` / `sys` 等库节点 |
| 看不到其它库名列表 | ✅ 仅当前配置库 `datazen_test` |

**截图描述**：Connection Window 标题 `E2E-MySQL-Types - MySQL - DataZen`；左侧 Schema 树为 StandardSchemaTree 布局（表分组 + 表名列表），状态栏 `MySQL · E2E-MySQL-Types · datazen_test`。MCP 截图时间约 17:23（窗口 ID 125368）。

---

### TC-B：MySQL/MariaDB — 未配置 database

| 字段 | 内容 |
|------|------|
| **结果** | **PASS** |
| **连接** | `E2E-MySQL-MultiDb` — 127.0.0.1:3306，Database 留空（主窗口显示 `127.0.0.1 :`） |
| **操作** | 双击连接 → 展开 `datazen_test` 节点 |

**预期 vs 实际**

| 预期 | 实际 |
|------|------|
| 侧栏出现多个数据库节点 | ✅ 列出 6 个库：`datazen_sync_mysql_tgt`、`datazen_test`、`information_schema`、`mysql`、`performance_schema`、`sys` |
| 多库树（MultiDatabaseSchemaTree） | ✅ 无「表 (N)」顶层分组，直接以库名为可展开节点 |
| 可展开某个库加载表 | ✅ 点击 `datazen_test` 后展开 6 张表（active_users … users），状态栏切换为 `… · datazen_test` |

**截图描述**：侧栏为 6 个并列数据库节点（MultiDatabaseSchemaTree）；展开 `datazen_test` 后其下出现表名。MCP 截图时间约 17:24（窗口 ID 125372）。

---

### TC-C：PostgreSQL — 配置了 database

| 字段 | 内容 |
|------|------|
| **结果** | **PASS** |
| **连接** | `Postgre-Local` — 127.0.0.1:5432，Database=`goecoride` |
| **操作** | 双击连接 → 观察 Schema 树 |

**预期 vs 实际**

| 预期 | 实际 |
|------|------|
| 单库 Tables 树 | ✅ **表 (5)** + **VIEWS (1)** 分组 |
| 无其它库节点 | ✅ 无 `postgres` / `logto` / `template1` 等库列表 |
| 锁定到配置库 | ✅ 标题 `goecoride`，状态栏 `PostgreSQL · Postgre-Local · goecoride` |

**截图描述**：StandardSchemaTree 单库视图，表与视图分组清晰，无多库节点。MCP 截图时间约 17:25（窗口 ID 125376）。

---

### TC-D：PostgreSQL — 未配置 database

| 字段 | 内容 |
|------|------|
| **结果** | **PASS** |
| **连接** | `E2E-PostgreSQL-MultiDb` — 127.0.0.1:5432，Database 留空 |
| **操作** | 双击连接 → 展开 `goecoride` 节点 |

**预期 vs 实际**

| 预期 | 实际 |
|------|------|
| 多库节点 | ✅ 列出 9 个库：`datazen_sync_src`、`datazen_sync_tgt`、`goecoride`、`logto`、`postgres`、`vikunja`、`winamz`、`winamz_site_medusa`、`winamz_site_payload` |
| MultiDatabaseSchemaTree | ✅ 库名为顶层可展开节点 |
| 可展开加载表 | ✅ 展开 `goecoride` 后出现 6 张表 + 1 视图（active_users … users），状态栏 `… · goecoride` |

**截图描述**：侧栏 9 个 PostgreSQL 库节点并列；展开 `goecoride` 后子级为表/视图名。MCP 截图时间约 17:26（窗口 ID 125384）。

---

## 用例汇总

| 用例 | 场景 | 结果 |
|------|------|------|
| TC-A | MySQL 配置 database | **PASS** |
| TC-B | MySQL 未配置 database | **PASS** |
| TC-C | PostgreSQL 配置 database | **PASS** |
| TC-D | PostgreSQL 未配置 database | **PASS** |

## 发现的 Bug

**无。** 本次黑盒与 Vitest 均未发现回归或功能缺陷，未新建 `test/bugs/` 条目。

## 总结

| 维度 | 结论 |
|------|------|
| **功能是否达标** | ✅ 是 — 四种场景均符合设计 |
| **Vitest** | 19/19 通过 |
| **黑盒** | 4/4 通过，0 BLOCKED |

**明确结论：配置了 `database` 的连接在 Connection Window 侧栏只显示该单库的 Tables/Views（StandardSchemaTree），不会列出其它数据库节点；未配置 `database` 时正常显示多库树并可展开加载表。**
