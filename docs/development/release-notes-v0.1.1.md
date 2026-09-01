# DataZen v0.1.1

**Patch release** — bug fixes and UX polish on top of v0.1.0.  
User manual: [flyxl.github.io/datazen](https://flyxl.github.io/datazen/) · Source: [github.com/flyxl/datazen](https://github.com/flyxl/datazen)

---

## What's new

### Fixes

- **Superset / path-hierarchy queries** — pin catalog/schema into query context (`hive.snap.*` instead of defaulting to `hive.default.*`); auto-complete single-option path levels; searchable Catalog/Schema selectors with reserved toolbar space
- **Kiwi / Superset export UI** — hide data export and table-structure export when driver `exportScope` is `none`
- **Superset connection tree** — bootstrap navigator after connect; use display names for path-hierarchy databases (no duplicate catalog nodes)
- **Path-hierarchy navigation** — selecting a table from Kiwi / Superset tree passes correct schema path (fixes *No active schema context*)
- **Connection navigator** — expand/collapse state is independent per section (e.g. Recent vs saved group)
- **Slow connect UX** — single loading state during connection (no duplicate spinners)
- **Query toolbar** — show expanded labels when the panel has room; fix toolbar width estimation for context selectors
- **DataTable** — default row height 28px (was 40px)
- **Object search** — hide search entry for unloaded object types
- **Recent connections** — keep recent connections visible in their original groups

### Also

- Homebrew tap and WinGet manifest templates updated for v0.1.1 distribution

---

## Install variants

Same as v0.1.0 — pick the installer that matches your drivers:

- **Basic** (no suffix): PostgreSQL, MySQL, SQLite, Redis
- **All** (`-all`): all path native drivers (MongoDB, SQL Server, ClickHouse, DuckDB, etc.)
- **Akulaku** (`-akulaku`): Basic + MongoDB + Kiwi + Superset

### macOS

Not notarized. If Gatekeeper blocks launch:

```bash
xattr -cr /Applications/DataZen.app
```

Or right-click → **Open** once.

---

# DataZen v0.1.1（中文）

**补丁版本** — 在 v0.1.0 基础上的问题修复与体验优化。  
使用手册：[flyxl.github.io/datazen/zh/](https://flyxl.github.io/datazen/zh/) · 源码：[github.com/flyxl/datazen](https://github.com/flyxl/datazen)

---

## 更新内容

### 修复

- **Superset / path-hierarchy 查询** — 将 catalog/schema 正确写入查询上下文（避免落到 `hive.default.*`）；单选项路径层级自动选中；可搜索的 Catalog/Schema 选择器，并在工具栏预留显示空间
- **Kiwi / Superset 导出** — 驱动 `exportScope` 为 `none` 时隐藏数据导出与表结构导出
- **Superset 连接树** — 连接后自动引导导航树；path-hierarchy 数据库使用显示名（避免重复 catalog 节点）
- **Path-hierarchy 导航** — 从 Kiwi / Superset 树打开表时传递正确 schema 路径（修复 *No active schema context*）
- **连接导航树** — 各分组（如「最近」与原分组）的展开/折叠状态互不干扰
- **连接体验** — 连接过程中统一加载状态（去除重复 spinner）
- **查询工具栏** — 空间充足时显示完整按钮文字；修正上下文选择器的宽度估算
- **DataTable** — 默认行高 28px（原 40px）
- **对象搜索** — 未加载的对象类型不显示搜索入口
- **最近连接** — 最近使用的连接仍保留在原分组中显示

### 其他

- 更新 Homebrew tap 与 WinGet 清单模板以支持 v0.1.1 分发

---

## 安装变体

与 v0.1.0 相同，按驱动需求选择安装包：

- **Basic**（无后缀）：PostgreSQL、MySQL、 SQLite、Redis
- **All**（`-all`）：全部 path 原生驱动
- **Akulaku**（`-akulaku`）：Basic + MongoDB + Kiwi + Superset

### macOS

未公证。若 Gatekeeper 拦截：

```bash
xattr -cr /Applications/DataZen.app
```

或右键 → **打开** 一次。
