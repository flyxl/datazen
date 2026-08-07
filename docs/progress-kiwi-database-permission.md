# 进度：修复 Kiwi「无权限 / 加载数据库失败」

> 分支：`fix/kiwi-database-permission-load`  
> 现象：连接成功、`get_databases` 返回 8 个库后，`get_tables` 报  
> `The user does not have permission for the database afi-ph-useraccount-dbreader.aku`

## 根因（已确认）

Kiwi 连接配置里 `config.database` 表示的是 **实例 domain**（如 `afi-ph-useraccount-dbreader.aku`），不是逻辑库名。

F6「配置了 database 则锁定单库」之后：

1. `SchemaTree` 见 `initialDatabase=domain` → 走 `StandardSchemaTree`
2. `resolveVisibleDatabases` 把可见库锁成 `[domain]`
3. `get_tables(database=domain)` → API 200 但业务错误无权限

日志证据：`domain` 与 `database` 查询参数同为 domain 字符串。

## 功能拆分

| ID | 功能 | 状态 | 提交 |
|----|------|------|------|
| F1 | `resolveVisibleDatabases`：仅当配置库 ∈ `get_databases` 结果时才锁定 | ✅ PASS | `50820b1` |
| F2 | `databaseFieldType: 'domain'` + SchemaTree 不对 domain 做单库锁定路由；Kiwi meta | ✅ PASS | `389d93a` |

## 测试记录

| ID | 测试 agent | 报告 | 结论 |
|----|------------|------|------|
| F1 | [F1 test](72d6c2ef-88c1-4a71-96ea-3f444391cd01) | [f1-test](./progress-kiwi-database-permission-f1-test.md) | **PASS**（Vitest 12/12；黑盒 BLOCKED） |
| F2 | [F2 test](bf183664-8573-49f2-9eb2-3169722a50e3) | [f2-test](./progress-kiwi-database-permission-f2-test.md) | **PASS**（Vitest 27/27） |

## 变更日志

### F1 — `50820b1`
- `resolveVisibleDatabases`：仅 `allDatabases.includes(configured)` 时锁定
- 单元测试覆盖 Kiwi domain 样例

### F2 — `389d93a`
- `databaseFieldType: 'domain'`；Kiwi meta + SchemaTree `shouldUseMultiDatabaseTree`
- domain 时 strip `initialDatabase` 再交给 Multi 树
- kiwi 插件 commit `b9cc1bd`（已合入 kiwi `main`）；`plugins-registry.json` pin 更新
