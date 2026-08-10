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

相关提交（节选）：`94810aa` … `a897fa0`。

---

## 剩余待办

### P1 — 架构债

1. **Sync 迁入驱动包**  
   适配器仍在宿主 `src-tauri/src/sync/`，未进 `packages/drivers`，驱动侧尚未自注册 Sync adapter。

2. **ClickHouse Sync adapter**  
   需 `ENGINE` / `ORDER BY` 等 DDL 扩展；当前暂缓。SQL Server 已有最小实现可作参考。

3. **更多 Sync 方言与高级能力**  
   - 方言：duckdb、elasticsearch 等  
   - 高级：`strategy` / 断点续传、`transform_value`  
   - 无 adapter 时 schema compare 仍可能回退原始 `data_type` 字符串

4. **Backup 保真**  
   - UI 大量 option 仍为 no-op  
   - restore 仍粗暴按 `;` 切分  
   - 未接 `pg_dump` / 全量 `SHOW CREATE` 级保真

5. **God module 拆分**  
   `commands/sync.rs`、`Store`、前端 `aiStore`、超大 Window 组件。

### P2 — 一致性 / DRY

6. **驱动 `meta.iconColor` / `iconBg`**  
   多数仍为调色板类（如 `text-blue-400`）；Redis 已语义化。Host 选择态已改 `accent`，驱动角标品牌色未统一。

7. **IPC 文档对齐**  
   文档写 snake_case，前端 `commands/` 多为 camelCase，文档需与实现一致。

### P3 — Minor / 扫尾

8. 连接分组：已做 `preset:*`；其它入口若仍有中文硬编码需再扫一遍。  
9. `DB_TYPE_POPULARITY_ORDER`：可保留含未注入驱动的列表；无需大改。  
10. Schema token 预算、`{{db_type}}` Debug 引号：已修，无需再做。

### 流程债

11. **与 `main` 合并** — 曾发起合并并产生冲突，未完成；需要时再单独处理。  
12. **推远程** — 确认 `fix/critical-arch-review` 是否已 push 最新提交。

---

## 建议优先级

1. Sync 下沉驱动包 **或** Backup 保真  
2. God module 拆分  
3. 驱动 meta 语义色 + IPC 文档  
4. ClickHouse Sync（需单独设计 ENGINE DDL）

## 相关文档

- 架构总览：`docs/architecture/README.md`  
- 代码审查进度（一期～四期+）：`docs/progress-code-review-fix.md`  
- Sync IR 计划：`docs/plan-sync-ir.md`（若存在）
