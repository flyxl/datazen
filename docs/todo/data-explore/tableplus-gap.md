# DataZen vs TablePlus 差距分析

> 基于 DataZen 当前版本能力与 TablePlus 核心工作流进行对比。

## 总体

DataZen 的能力覆盖面已经很完整，在 Schema Diff、Data Sync、Data Transfer、Workflow、Dashboard、AI、MCP、Extension 等方面形成了明显的差异化。

TablePlus 的主要优势集中在数据库日常操作的深度、交互成熟度和细节打磨。

## 核心差距

| 能力 | DataZen | TablePlus | 差距 |
|---|---|---|---|
| Data Grid / CRUD | 支持数据查看、编辑、CRUD | 内联编辑、多行编辑、筛选、排序、分页、NULL/Boolean/JSON 编辑、修改预览、Commit/Discard、Safe Mode | **大** |
| Schema Explorer | 支持数据库/Schema/对象浏览 | Open Anything、模糊搜索、Recent、Favorites/Pin、数据库切换、Workspace/Tab | **大** |
| Table Structure | 支持 Schema Diff / Migration | Data / Structure / Definition 一体化，直接编辑字段/索引、Preview SQL、Commit、Safe Mode | **较大** |
| DB Object 管理 | 以查询、Schema、Migration 为主 | Tables / Views / Functions / Procedures / Triggers 的完整对象级操作 | **较大** |
| Tab / Workspace | 已有多窗口、Tab、Workspace 基础能力 | Workspace、Tab、Window、Recent 等交互更成熟 | **中等** |
| SQL Editor | Pro Editor、Autocomplete、Linter、Intentions、事务等 | SQL 编辑、对象联动、结果交互等细节更成熟 | **中等** |

## 1. Data Grid / CRUD

这是 DataZen 与 TablePlus 差距最大的核心区域。

TablePlus 的典型流程：

```text
打开表
  ↓
Data Grid
  ↓
筛选 / 排序 / 分页
  ↓
直接编辑单元格
  ↓
查看修改
  ↓
生成 SQL
  ↓
Commit / Discard
```

重点差距：

- 内联编辑体验
- 多行批量编辑
- NULL / Boolean / JSON 等类型编辑
- 修改状态与修改预览
- 生成 SQL
- Commit / Discard
- Safe Mode
- 大数据量 Grid 的交互细节

DataZen 当前已经具备 Data Grid / CRUD 基础能力，但需要进一步强化“直接操作数据”的完整闭环。

## 2. Schema Explorer

TablePlus 的优势不是单纯的对象树，而是“快速找到并打开对象”。

核心体验包括：

- Open Anything
- 模糊搜索表、视图等对象
- Recent
- Favorites / Pin
- 快速切换数据库
- 对象与 Tab / Workspace 联动

DataZen 当前 Schema Explorer 已能完成对象浏览，但“搜索 → 打开 → 最近访问 → 收藏 → Tab”这一套对象导航体系仍有提升空间。

## 3. Table Structure

TablePlus 将日常表结构修改设计成直接操作流程：

```text
Table
 ├─ Data
 ├─ Structure
 └─ Definition
```

Structure 中直接修改字段、索引等对象，并可以：

```text
编辑
 ↓
Preview SQL
 ↓
Commit
```

DataZen 的 Schema Diff / Migration 能力更强，但对于“只改一个字段”这种日常操作，TablePlus 的交互链路更直接。

## 4. DB Object 管理

TablePlus 对数据库对象提供较完整的对象级操作：

- Open
- Edit
- Structure
- Definition
- Copy Script
- Drop
- Duplicate
- Pin
- New Tab
- New Object

覆盖：

- Tables
- Views
- Functions
- Procedures
- Triggers

DataZen 当前更偏向“数据库开发工作台”，对象级管理深度仍需要加强。

## 5. Tab / Workspace

TablePlus 的 Workspace / Tab / Window 模型与数据库对象导航结合较紧密。

重点体验：

```text
Connection
  ↓
Workspace
  ↓
Tabs
  ↓
Object / SQL / Data
```

DataZen 已有多窗口、Tab、Workspace 基础设施，但对象、Tab、Workspace 三者之间的联动体验仍可继续完善。

## 6. SQL Editor

DataZen 的 SQL Editor 已经属于核心竞争力：

- Pro Editor
- Autocomplete
- Linter
- Intentions
- SQL 执行
- 事务能力
- Workflow 集成

与 TablePlus 的差距主要在日常使用细节，而不是基础能力缺失。

重点可以继续完善：

- SQL 与 Schema Explorer 的联动
- SQL / Object / Result Tab 之间的导航
- 查询历史与最近查询
- 结果集交互
- 编辑器快捷操作

## DataZen 的差异化能力

以下能力不应简单按照“追平 TablePlus”的思路建设：

- Schema Diff
- Data Sync
- Data Transfer
- Workflow
- Dashboard
- AI
- MCP Server / Client
- Extension / Driver API
- 多数据库批量操作

这些能力更适合作为 DataZen 与传统数据库客户端的差异化方向。

## 差距优先级

| 优先级 | 能力 | 原因 |
|---|---|---|
| P0 | Data Grid / CRUD | 数据库客户端最高频核心操作 |
| P0 | Schema Explorer / Object Search | 决定对象发现与日常操作效率 |
| P1 | Table Structure Editor | 高频数据库结构修改场景 |
| P1 | DB Object Management | 完善数据库对象生命周期 |
| P1 | Tab / Workspace / Navigation | 提升复杂工作区效率 |
| P2 | SQL Editor 细节 | DataZen 已有较强基础，重点补交互细节 |
