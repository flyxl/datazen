# Schema Diff Deploy 使用手册

> 把「源库结构 = 期望态」对比结果，变成可审阅的 DDL，并受控部署到目标库。  
> 入口：菜单 **Schema Diff**，或主窗口相关快捷入口。

---

## 1. 概述

| 步骤 | 做什么 |
|------|--------|
| **对比** | 选源/目标连接、库、schema（若支持）与表，查看列/索引差异 |
| **计划** | 生成部署 SQL（默认仅加法） |
| **审阅 / 部署** | 在右栏 Deploy 标签确认事务与破坏性确认词后，在目标库执行 |

**方向约定：** 源 = 期望结构（desired）；目标 = 应用站点（apply site）。

**diff 标签：** 目标缺失（ADD）· 目标多余（DROP，需 allowDestructive）· 变更列。

---

## 2. 界面概览

Schema Diff 为 **5 步向导**（与 Data Transfer 相同的 stepper 视觉）：

```text
┌ TitleBar（标题 + 帮助）────────────────────────────────┐
├ Stepper：Endpoints → Objects → Compare → Plan → Deploy ┤
├ ① Endpoints：源/目标（连接 · database · schema）        ┤
├ ② Objects：源库表多选（全选/取消）                      ┤
├ ③ Compare：左表列表 │ 列级 diff 详情                   ┤
├ ④ Plan：选项 + SQL 列表 + 导出/导入配置                 ┤
├ ⑤ Deploy：事务选项 · DEPLOY 确认 · 部署结果             ┤
└ StatusBar ───────────────────────────────────────────────┘
```

- **首次打开**会弹出当前版本**能力限制**说明（可勾选「不再显示」）。
- **Endpoints** 使用 dedicated session 拉取 database 列表；PostgreSQL 等会列出 schema（**Swap 暂未启用**，见 backlog）。
- **Objects** 从源库加载表列表，勾选要比对的表。
- **Plan** 步可勾选 `allowDestructive`、`includeIndexes`，查看 SQL 与风险 badge。
- **Deploy** 步：`DEPLOY` 确认词、事务与部署状态。

源码：`src/windows/schema-diff/`（`SchemaDiffWindow`、`SchemaDiffObjectsStep`、`SchemaDiffTableListPanel`、`SchemaDiffRightPanel`）。

---

## 3. 快速入门

1. 打开 **Schema Diff** 窗口（若首次打开，阅读限制说明并关闭弹窗）  
2. **Endpoints** 步：选择 **源**、**目标**连接及 **database**（不能是同一连接；PG 可选 **schema**），点 **下一步**  
3. **Objects** 步：勾选要比对的表，点 **下一步**（Compare 会自动运行）  
4. **Compare** 步：左侧表列表显示变更 badge，中间面板列出缺失/多余/变更列，点 **下一步**  
5. **Plan** 步：点击 **生成部署脚本**，核对 SQL 与风险标记（`additive` / `destructive` / `rewrite`）  
6. **Deploy** 步：按需勾选事务与回滚完整性；若含破坏性或改写类语句，输入 **`DEPLOY`** 后再点 **部署到目标**  
7. 查看部署状态：`committed` / `rolled_back` / `mixed` / `failed`

Plan 步可随时 **复制 SQL** 或 **导出/导入配置**，不必真正执行部署。

---

## 4. 安全默认值

- 默认计划为 **仅加法**：`ADD COLUMN`、放宽可空、`CREATE INDEX` 等  
- 勾选 **允许破坏性变更** 后才会生成：`DROP COLUMN` / `DROP INDEX`、收窄类型、`SET NOT NULL` 等  
- 计划中只要出现 `destructive` 或 `rewrite`，部署前必须输入 **`DEPLOY`**  
- 可选 **要求完整回滚脚本**：任一条语句缺少 `rollbackSql` 时禁止部署  

生产库建议：先复制 SQL 到变更窗口人工审阅，并做好备份。

---

## 5. 事务与原子性

| 方言 | DDL 原子性 | 中途失败时的状态 |
|------|------------|------------------|
| PostgreSQL | 支持事务 | 通常为 `rolled_back`（先前语句撤销） |
| SQLite | 支持事务（ALTER 能力有限） | 通常为 `rolled_back` |
| MySQL / MariaDB | 语句级自动提交 | 部分成功时为 `mixed`（**不会**假装整批回滚） |
| 其他 | 按自动提交处理 | 类似 `mixed` / `failed` |

目标方言不支持事务型 DDL 时，界面会禁用「在事务中执行」。

---

## 6. 多表与索引

- 一次可填多张表，计划会按表拼接列级与索引语句  
- 可取消勾选 **包含索引** 只生成列变更  
- `DROP INDEX` 等往往没有完整反向 `CREATE`，会进入「回滚不完整」列表  

主键结构变更目前**不会**自动进计划，请手工处理并留意警告。

---

## 7. 跨方言

源与目标方言不同时，列类型会经 **数据同步 IR**（`column_to_ir` → `ir_type_to_native`）映射。

- 映射成功：出现在目标方言的 `ADD`/`MODIFY` 语句中  
- 映射失败：写入计划 **警告** 并跳过该语句（不静默生成错误 SQL）  

SQLite 侧仍以 `ADD COLUMN` / 索引为主；复杂 `DROP`/`MODIFY` 会提示不支持。

---

## 8. 配置 JSON

可用剪贴板 **导出 / 导入** 配置（不含密码，只含连接配置 ID 与选项）：

```json
{
  "version": 2,
  "sourceConnectionId": "...",
  "targetConnectionId": "...",
  "tables": ["users", "orders"],
  "allowDestructive": false,
  "includeIndexes": true,
  "requireRollback": false
}
```

导入后进入 **Objects** 步并恢复表勾选；**database / schema 需手动重选**（v2 尚未持久化，见 [backlog](../todo/migration-tools-backlog.md#schema-diff)）。连接须已存在于本机配置列表。

---

## 9. 与 Data Sync / Data Transfer 的关系

| 产品 | 做什么 | 用户手册 |
|---|---|---|
| **Schema Diff（本文）** | 结构对齐与受控 DDL 部署，不灌行数据 | 本文 |
| **Data Synchronization** | 结构完全一致 + 相同 PK → 行 Diff → 审查 → Execute | [data-sync-guide.md](./data-sync-guide.zh-CN.md) |
| **Data Transfer** | 异构 / 结构不同 / 无 PK → 单向搬运（V1 基础） | [data-transfer-guide.md](./data-transfer-guide.zh-CN.md) |

结构不一致时不要用 Data Sync「只同步部分列」；应先 **Schema Diff** 对齐 DDL，或改用 **Data Transfer**。

跨库建表 DDL 属于 Transfer / 适配器 IR；Deploy 路径是已有表上的 ALTER，不要和 Sync 的 Change Set 混用。

---

## 10. 当前不做

- 视图 / 函数 / 触发器 / 存储过程同步  
- 在线改表工具（pt-osc / gh-ost）  
- 按相似度猜测重命名  
- 部署前自动备份  
- 经 MCP 一键部署（后续可能以高风险工具封装）  

---

## 11. 排错

| 现象 | 可能原因 |
|------|----------|
| 计划为空 | 结构已一致，或破坏性差异被默认跳过 |
| 部署被拒 | 未输入 `DEPLOY`，或开启了「要求完整回滚」但回滚不全 |
| MySQL 显示 `mixed` | DDL 已逐条提交，失败前的语句留在目标库 |
| 跨方言缺语句 | 类型映射失败，查看计划警告 |
| SQLite 警告多 | 引擎 ALTER 能力有限，属预期 |

更偏架构的说明见仓库 `docs/architecture/backend/schema-diff.md`。
