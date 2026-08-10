# 全库架构审查 — 进度

> 对照会话审查结论（Critical / Important / Minor）。  
> 分支：`fix/critical-arch-review`（已与远程同步）  
> 更新日期：2026-08-10

## 已完成

### Critical（全部完成）

| # | 问题 | 处理 |
|---|------|------|
| 1 | Schema 缓存失效未接线 | DDL 路径失效 |
| 2 | 多库 `get_table_data` cache key | 使用 active DB |
| 3 | SQLite `commit`/`rollback` 空实现 | 真实事务 |
| 4 | Workflow 绕过 SQL 权限/行数上限 | 权限 + 1000 行 |
| 5 | Redis flush 仅信前端 | 进程门控 |
| 6 | Host 自拼备份 SQL | `dump_database` / `sql_dump` |

### Important / 架构债收尾

| 领域 | 内容 |
|------|------|
| **Sync 下沉** | IR/traits → `datazen-driver-api::sync`；适配器在驱动包 `inventory` 自注册；宿主仅保留 registry + DDL + orchestration |
| **Sync IR 覆盖** | 见下方「IR 注册表」 |
| **Backup** | 智能 SQL 切分；options 扩展；PG catalog `dump_table_ddl`；MySQL `SHOW CREATE` + routines/triggers |
| **一致性** | 驱动 meta 语义色；IPC 文档 camelCase；连接分组 `preset:*` |
| **God modules** | `commands/sync/`、`store/`、`aiStore` 类型、Settings / DataSync 窗口拆分 |
| **高级 Sync** | `transform_value`；表级断点 + 行级 `resume_offset` |

相关计划：`docs/superpowers/plans/2026-08-10-arch-review-remaining.md`。  
近期提交（节选）：`1766380` → `a4ad700` → `e4864cf` → `f72f591`。

### Sync IR 注册表（`databaseType`）

| 来源 | 类型 |
|------|------|
| postgres 包 | `postgresql`, `cloudberry`, `questdb` |
| mysql 包 | `mysql`, `mariadb`, `doris`, `starrocks`, `manticore`, `ob_oracle` |
| sqlite 包 | `sqlite`, `rqlite`, `turso` |
| 其它 path | `sqlserver`, `clickhouse`, `duckdb`, `elasticsearch`, `mongodb`, `influxdb`, `victoriametrics`, `hbase`, `vector` |
| olap git 包 | `trino`, `presto`（`datazen-driver-olap@58b5bd2`） |

**不接关系 IR（有意）：** `redis`、`kiwi`、`superset`。

> 非 SQL 引擎（ES / Mongo / 时序 / HBase / Vector）为**最小 IR**：用于 schema compare / 导出到 SQL 目标；完整 `DROP/CREATE/INSERT` 往返仍可能失败。

---

## 剩余待办

### 流程债

1. **与 `main` 合并** — 曾有冲突史；需单独处理（当前未合并）。

### 有意不做

| 项 | 原因 |
|----|------|
| 外部 `pg_dump` / `--format=custom` | 不做 shell-out；用进程内 dump |
| Redis / Kiwi / Superset Sync IR | 非表模型或代理 |

### 可选后续（非阻塞）

- ClickHouse：Replicated* / Distributed 等 ENGINE 专项保真与测试  
- ES / Mongo 等「完整数据同步」产品设计（非仅 IR 类型映射）  
- Backup：`no-owner` / dump 侧 `single-transaction` 仍偏 header 记录  
- Settings 更细粒度懒加载  

---

## 相关文档

- 架构总览：`docs/architecture/README.md`  
- 代码审查进度：`docs/progress-code-review-fix.md`  
- Sync IR 计划：`docs/plan-sync-ir.md`  
- 实施计划：`docs/superpowers/plans/2026-08-10-arch-review-remaining.md`
