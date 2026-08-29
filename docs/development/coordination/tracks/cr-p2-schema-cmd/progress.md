# cr-p2-schema-cmd — 进度

**轨 ID：** cr-p2-schema-cmd | **状态：** 完成

## 交付

| 项 | 状态 | 说明 |
|---|---|---|
| Schema 目录 Driver Command | ✅ | `list_databases` / `list_tables` / `get_table_schema` in `schema_catalog_commands.rs` |
| GUI IPC 收敛 | ✅ | `commands/schema.rs` 经 `execute_driver_command` |
| Trait 快路径文档 | ✅ | `docs/architecture/backend/drivers.md` §Schema 目录 |
| 驱动 dispatch | ✅ | SQL 驱动 + Redis + mock_driver + 默认 trait |
| SchemaCache | ✅ | `try_get_cached_schema` / `store_table_schema`；IPC cache hit 跳过 dispatch |

## 验证

```bash
CARGO_TARGET_DIR=../datazen/target cargo test -p datazen-driver-api
CARGO_TARGET_DIR=../datazen/target cargo test -p datazen --lib schema::
```
