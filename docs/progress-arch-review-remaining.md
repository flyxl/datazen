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

- Sync：`inventory` 注册；IR compare；PG `full_column_types_query`；别名 cloudberry / rqlite / turso / doris / starrocks；SQL Server 最小 adapter；DDL 列 comment
- `query_with_params` 真绑定（PG/MySQL/SQLite）；PG/MySQL 连接级事务
- Redis 视图进驱动包；`useConnectionForm` 去 kiwi/redis hardcode；Kiwi 登录进插件
- `resolve_session` 统一 config_id / connection_id；AI `@` 读文件去重 + 路径穿越
- `prompt_db_type`、`ai/budget`；连接分组 `preset:*`；Host 选择态语义色 `accent`；原生 `<select>` → 封装 Select
- Export Dialog / `isSchemaGroupingSchema` DRY

### 本轮补齐（A→D，2026-08-10）

| Track | 内容 | 状态 |
|-------|------|------|
| **A** | Sync IR/traits → `datazen-driver-api::sync`；PG/MySQL/SQLite/SQL Server adapter 迁入 `packages/drivers/*/src/sync_adapter.rs` 并 `inventory` 自注册；Trino/Presto 仍留宿主 | ✅ |
| **B** | `sql_dump::split_sql_statements`（引号/注释/`$$`）；restore 使用智能切分；扩展 dump/restore options（`no-owner`/`single-transaction`/`routines`/`triggers`）；去掉 UI `format-custom`；MySQL routines/triggers 尽力导出 | ✅（见下方残留） |
| **C** | 驱动 `meta.iconColor`/`iconBg` 语义色；IPC 文档改为 camelCase invoke 键；连接分组硬编码扫尾（无额外 UI 修复） | ✅ |
| **D** | `commands/sync/` 拆分；`store/` 拆分；`aiStore` 类型抽出；SettingsWindow 未拆（风险高） | ✅（Window 可选） |

相关计划：`docs/superpowers/plans/2026-08-10-arch-review-remaining.md`。

相关提交（节选）：`94810aa` … `a897fa0`。

---

## 剩余待办

### P1 — 仍暂缓 / 未做完

1. **ClickHouse Sync adapter**  
   需 `ENGINE` / `ORDER BY` 等 DDL 扩展；当前暂缓。SQL Server / 已下沉 adapter 可作参考。

2. **更多 Sync 方言与高级能力**  
   - 方言：duckdb、elasticsearch 等  
   - 高级：`strategy` / 断点续传、`transform_value`  
   - 无 adapter 时 schema compare 仍可能回退原始 `data_type` 字符串  
   - 宿主仍保留 Trino/Presto adapter（无 path 驱动包）

3. **Backup 保真残留**  
   - 未接外部 `pg_dump` / custom format（有意不做 shell-out）  
   - PG `dump_table_ddl` 仍偏 schema 重建，未达 mysqldump/`SHOW CREATE` 全量保真  
   - `no-owner`：仅 header 记录（本就不发射 OWNER）  
   - `single-transaction`：restore 可包事务；dump 侧仅记 flag

4. **超大 Window 组件**  
   `SettingsWindow` / `DataSyncWindow` 等仍可按需拆分（本轮跳过）。

### P3 — 无需再做

5. `DB_TYPE_POPULARITY_ORDER`：可保留含未注入驱动的列表。  
6. Schema token 预算、`{{db_type}}` Debug 引号：已修。  
7. 连接分组 `preset:*` + 硬编码扫描：已确认仅 locales / legacy alias。

### 流程债

8. **与 `main` 合并** — 曾发起合并并产生冲突，未完成；需要时再单独处理。  
9. **推远程** — 确认 `fix/critical-arch-review` 是否已 push 最新提交。

---

## 建议优先级（剩余）

1. ClickHouse Sync（需单独设计 ENGINE DDL）  
2. Backup PG DDL 保真 / 可选外部 `pg_dump`  
3. 超大 Window 拆分  
4. 与 `main` 合并

## 相关文档

- 架构总览：`docs/architecture/README.md`  
- 代码审查进度（一期～四期+）：`docs/progress-code-review-fix.md`  
- Sync IR 计划：`docs/plan-sync-ir.md`  
- 本轮实施计划：`docs/superpowers/plans/2026-08-10-arch-review-remaining.md`
