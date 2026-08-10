# 全库架构审查 — 剩余待办

> 对照会话审查结论（Critical / Important / Minor）。  
> 分支：`fix/critical-arch-review`  
> 更新日期：2026-08-10

## 已完成（摘要）

### Critical（全部完成）

| # | 问题 | 处理 |
|---|------|------|
| 1 | Schema 缓存失效未接线 | DDL 路径失效 |
| 2 | 多库 `get_table_data` cache key | 使用 active DB |
| 3 | SQLite `commit`/`rollback` 空实现 | 真实事务 |
| 4 | Workflow 绕过 SQL 权限/行数上限 | 权限 + 1000 行 |
| 5 | Redis flush 仅信前端 | 进程门控 |
| 6 | Host 自拼备份 SQL | `dump_database` / `sql_dump` |

### Important / 后续已落地（节选）

- Sync：`inventory` 注册；IR compare；PG `full_column_types_query`；别名；SQL Server / ClickHouse / DuckDB adapter；DDL 列 comment + `table_options` 后缀
- `query_with_params` 真绑定；PG/MySQL 连接级事务
- Redis 视图进驱动包；连接分组 `preset:*`；语义色；IPC camelCase 文档
- Backup：智能切分、options、PG catalog `dump_table_ddl`、MySQL SHOW CREATE / routines·triggers
- God modules：`commands/sync/`、`store/`、`aiStore`、Settings / DataSync 窗口拆分

### 本轮收尾（2026-08-10 续）

| 项 | 状态 |
|----|------|
| ClickHouse Sync + ENGINE/ORDER BY（`table_options` / `create_table_suffix` / `table_options_query`） | ✅ |
| DuckDB Sync adapter | ✅ |
| `transform_value` 钩子 + 表级断点 + **行级 resume_offset**（失败保存 offset，继续跳过已插入行） | ✅ |
| PG `dump_table_ddl` catalog 保真 | ✅ |
| SettingsWindow / DataSyncWindow 拆分 | ✅ |
| Elasticsearch Sync | ✅ 最小 IR（schema compare / 导出到 SQL 目标）；完整 SQL 往返仍可能失败 |
| DuckDB / ClickHouse / 协议别名 questdb·manticore·ob_oracle | ✅ |
| MongoDB / Influx / VictoriaMetrics / HBase / Vector Sync IR | ✅ 最小 IR |
| Redis / Kiwi / Superset Sync IR | ❌ 非表模型或代理，不接关系 IR |
| 外部 `pg_dump` / custom format | ❌ 有意不做（不 shell-out） |
| 与 `main` 合并 | ⏸ 按需求单独处理（本轮不合并） |

相关计划：`docs/superpowers/plans/2026-08-10-arch-review-remaining.md`。

---

## 剩余待办

### 流程债

1. **与 `main` 合并** — 需要时再单独处理（曾有冲突史）。  
2. **推远程** — 提交后 push `fix/critical-arch-review`（见最新 commit）。

### 可选后续（非阻塞）

- ClickHouse：更复杂 ENGINE 族（Replicated* / Distributed）专项测试  
- Elasticsearch / 其它非表模型「同步」另开产品设计  
- Settings 更细粒度懒加载

## 相关文档

- 架构总览：`docs/architecture/README.md`  
- 代码审查进度：`docs/progress-code-review-fix.md`  
- Sync IR 计划：`docs/plan-sync-ir.md`  
- 实施计划：`docs/superpowers/plans/2026-08-10-arch-review-remaining.md`
