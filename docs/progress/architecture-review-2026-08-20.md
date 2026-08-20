# Architecture Review Progress — 2026-08-20 (完整修复)

Branch: `feat/arch-review-all`  
Worktree: `../datazen-arch-review-all`  
Base: `main` @ 91283919

## Status legend

| Status | Meaning |
|--------|---------|
| pending | 未开始 |
| dev_done | 已开发 + 单测，待 QA |
| tested_fail | QA 失败，待修复 |
| tested_pass | QA 通过 |

---

| ID | 标题 | Status |
|----|------|--------|
| F1 | Host schema_objects SQL → Driver Command | dev_done |
| F2 | Redis KV IPC → execute_driver_command | dev_done |
| F3 | Admin UI 硬编码 → Command definitions | dev_done |
| F4 | EXPLAIN 方言解析下沉驱动 | dev_done |
| F5 | 测试落点迁移 | dev_done |
| F6 | 移除 query-* capabilities | tested_pass |
| F7 | Restore sql_guard + debug SQL 脱敏 | dev_done |
| F8 | 移除 legacy sync IPC | dev_done |
| F9 | 架构文档漂移 | tested_pass |
| F10 | 流式查询统一 execute_driver_command | dev_done |
| F11 | --dt-binary token | tested_pass |
| F12 | sync/ 模块重命名为 transfer/ | dev_done |

## F2 notes

- Removed Host `kv_scan_keys` / `kv_get_key` IPC (`commands/kv.rs`)
- Redis workbench/import UI now calls `scan_keys` / `get_key` via `execute_driver_command`
- KV invoke helpers: `packages/drivers/redis/ui/redisInvoke.ts`

## F5 notes

- `schema_objects` SQL + shared tests → `packages/driver-api/src/schema_objects.rs`
- Dialect tests → `packages/drivers/{postgres,mysql,sqlite}/tests/schema_objects_sql.rs`
- Removed Host `schema_objects` dialect tests + `sqlite_function_list_is_empty` IPC test
- `e2e/specs/data-sync-real.ts` documented as Host IPC contract (kept in `e2e/specs/`)

## F1 notes

- Added `list_objects`, `get_object_ddl`, `list_privileges` Driver Commands in `driver-api` + postgres/mysql/sqlite drivers
- Host `commands/schema.rs` dispatches via `execute_driver_command`; dialect SQL stays in driver-api for drivers only
- Driver tests: `packages/drivers/{postgres,mysql,sqlite}/tests/schema_object_commands.rs` (+ sqlite integration)

## QA log

| Date | Agent | Feature | Result |
|------|-------|---------|--------|

## Bugs

| ID | Feature | Status |
