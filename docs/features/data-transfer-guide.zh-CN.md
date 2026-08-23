# Data Transfer 用户指南（V1 草案）

> 产品定义见 [data-transfer-prd.zh-CN.md。  
> 与 **Data Synchronization**（同族 + 结构一致 + 相同 PK 的行级 Diff）和 **Schema Diff**（结构对齐）是独立产品。

## 入口

- **Tools → Data Transfer…**（macOS 菜单）
- 连接 / 库 / 表右键 → **Data Transfer…**（预填 Source）

## 向导步骤

1. **Source / Target** — 允许异构 SQL 对（如 MySQL → PostgreSQL）；SQL ↔ Redis 等跨类别会被拒绝。
2. **Mode** — `structure` | `data` | `structure+data`
3. **Objects** — 选择要传输的表（同名自动匹配）
4. **Mapping** — 列同名自动映射（V1 暂不支持手改 UI，后续补齐）
5. **Options** — 写入模式：`insert` | `truncate+insert` | `drop+create`（破坏性需勾选确认）
6. **Preview** — DDL 摘要 + 写入计划（只读）
7. **Execute** — 执行（目标 `read_only` 时禁用）
8. **Result** — 每表行数与错误

## V1 执行能力

| 场景 | 状态 |
|------|------|
| 同方言族 + `data` / `structure+data` + `insert` + 目标表已存在 | ✅ 批量 INSERT |
| 跨方言（IR）+ 两端均有 sync adapter + `data` / `structure+data` | ✅ IR 字面量批量 INSERT |
| `truncate+insert` 执行（同族） | ✅ TRUNCATE + INSERT |
| `drop+create` 执行（需 `confirmedDestructive` + IR adapter） | ✅ DROP + CREATE (IR) + INSERT |
| 目标新建表 CREATE（`structure` / `structure+data`，IR adapter） | ✅ `table_to_ir` → `build_create_table_ddl` → execute |
| Preview（IR adapter 可用时） | ✅ 真实 CREATE DDL；跨族数据不再 blocked |
| 取消（job id） | ✅ |
| 目标 read_only | ❌ Execute blocked |
| 同连接同库同表自覆盖 | ❌ blocked |
| 跨类别（如 SQL ↔ Redis） | ❌ pairing unsupported |

## 与 Sync 的分工

- **结构不一致或无 PK** → 用 Transfer 或 Schema Diff，不要用 Data Sync。
- **结构一致且有相同 PK** → 用 Data Sync 做增量对齐。

## 相关架构

- 后端：`src-tauri/src/data_transfer/` + `src-tauri/src/commands/data_transfer/`
- IR / DDL：`src-tauri/src/transfer/`（与 Sync 引擎分离）
- 前端：`src/windows/data-transfer/DataTransferWindow.tsx`
