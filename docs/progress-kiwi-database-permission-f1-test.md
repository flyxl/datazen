# F1 测试报告：`resolveVisibleDatabases` Kiwi domain 误锁修复

**分支**：`fix/kiwi-database-permission-load`  
**Commit**：`b07d243` — fix: harden plugin stash restore, window show, and workflow docs  
**测试时间**：2026-08-07  
**Agent 模式**：report-only（未改业务代码、未 commit）

## 测试目标（F1）

`resolveVisibleDatabases` 仅当 `preferredDatabase` 出现在 `get_databases` 返回列表中才锁定单库；否则列出全部库（修复 Kiwi 将 instance domain 误当成逻辑库并触发 `get_tables(domain)` 的问题）。

---

## 必跑：Vitest

```bash
cd /Users/flyxl/code/datazen
npx vitest run src/stores/__tests__/schemaStore.test.ts
```

### 命令输出摘要

```
 RUN  v4.1.10 /Users/flyxl/code/datazen

 Test Files  1 passed (1)
      Tests  12 passed (12)
   Duration  ~686–809ms

✓ computeIsMultiDatabase / resolvePreferredDatabase / resolveVisibleDatabases > is multi only when capability and length > 1
✓ schemaStore.loadForConnection isMultiDatabase > locks to configured database and disables multi-db session
✓ schemaStore.loadForConnection isMultiDatabase > lists all databases when none configured (mysql)
✓ schemaStore.loadForConnection isMultiDatabase > sets isMultiDatabase false for mysql with a single database
✓ schemaStore.loadForConnection isMultiDatabase > sets isMultiDatabase true for postgresql with multiple databases
✓ schemaStore.loadForConnection isMultiDatabase > falls back to listing all when preferred is empty string
✓ schemaStore.loadForConnection isMultiDatabase > does not lock when configured database is absent from server list (e.g. Kiwi domain)
✓ schemaStore.loadTables > does not call getColumns during loadTables
✓ schemaStore.loadTables > calls useDatabase before getTables
✓ schemaStore.loadTables > populates tables but leaves columnMap empty after loadTables
✓ schemaStore.loadColumnMap > loads columns for all tables sequentially when called
✓ schemaStore.loadColumnMap > does nothing when connectionId is null
```

**结果**：12/12 PASS，exit code 0。

---

## 静态核对：`resolveVisibleDatabases`

**文件**：`src/stores/schemaStore.ts`（L33–49）

| 场景 | 预期 | 实现 | 核对 |
|------|------|------|------|
| `preferredDatabase` trim 后非空且在 `allDatabases` 中 | 锁定单库：`databases=[configured]`，`lockedToConfigured=true` | L38–43：`configured && allDatabases.includes(configured)` → 返回单库 | **PASS** |
| `preferredDatabase` 不在列表（如 Kiwi domain） | 不锁定：`databases=allDatabases`，`lockedToConfigured=false` | L45–49：走 else 分支，返回完整列表 | **PASS** |
| 不在列表时 preferred 回退 | 回退到列表首项（经 `resolvePreferredDatabase`） | L47：`resolvePreferredDatabase(allDatabases, configured \|\| undefined)`；该 helper 在 preferred 不在列表时返回 `databases[0]`（L22） | **PASS** |
| 空白 preferred | 视为未配置，列出全部 | L37：`preferredDatabase?.trim()` 为空则不进 lock 分支 | **PASS** |

**集成路径**（`loadForConnection`，L101–112）：

- `lockedToConfigured=false` 时 `isMultiDatabase` 由 `computeIsMultiDatabase(meta?.hasMultiDatabase, databases.length)` 决定（L105–107）。
- 仅当 `preferred` 非空且未 `skipLoadTables` 时才调用 `loadTables(preferred)`（L110–112）；domain 不在列表时 `preferred` 为列表首项，不会对 domain 调 `get_tables`。

**单元用例直接覆盖 F1**（`schemaStore.test.ts`）：

- Kiwi-style：`resolveVisibleDatabases(['app_db','other'], 'afi-ph-useraccount-dbreader.aku')` → 全列表 + `preferred:'app_db'` + `lockedToConfigured:false` — **PASS**
- 集成：`does not lock when configured database is absent from server list (e.g. Kiwi domain)` — **PASS**

---

## 用例表

| ID | 类型 | 用例 | 结果 |
|----|------|------|------|
| F1-UT-01 | 单元 | 配置库在 `get_databases` 列表中 → lock 单库 | **PASS** |
| F1-UT-02 | 单元 | 配置库不在列表（Kiwi domain 样例）→ 不 lock，preferred 为首项 | **PASS** |
| F1-UT-03 | 单元 | preferred 为空/空白 → 不 lock，列出全部 | **PASS** |
| F1-IT-01 | 集成 | `loadForConnection` + 配置库在列表 → `isMultiDatabase=false`，仅单库 | **PASS** |
| F1-IT-02 | 集成 | `loadForConnection` + 配置库不在列表 → 全列表、`isMultiDatabase=true`、current 为首项 | **PASS** |
| F1-BB-01 | 黑盒 | 连接 Kiwi → Connection Window 不对 domain 调 `get_tables`，能列出逻辑库 | **BLOCKED** |

### 黑盒 BLOCKED 原因

- DataZen 桌面应用当前未运行（`pgrep` 无 datazen/tauri 进程）。
- `e2e/.env` 存在但未配置 `E2E_KIWI_*` 凭证（无 Kiwi URL/token/username/domain）。
- 无法在 report-only 会话内启动带 Kiwi 插件的完整应用并完成手工验证。

---

## 结论

**F1 在单元/集成层验证通过（12/12 Vitest PASS，静态逻辑与 F1 需求一致）；黑盒 Kiwi 场景 BLOCKED，待有凭证与运行中的应用后补测。**
