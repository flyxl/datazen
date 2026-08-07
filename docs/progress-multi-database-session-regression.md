# 回归测试报告 — feat/multi-database-session-ui

日期：2026-08-07  
分支：`feat/multi-database-session-ui`  
Binary：`tauri build --debug --features webdriver`（plugins=none）

## 总览

| 套件 | 结果 | 说明 |
|------|------|------|
| Vitest（无插件） | **PARTIAL** | 298 pass / 5 fail（kiwi 未注入） |
| Vitest（`--plugins=kiwi`） | **PARTIAL** | 302 pass / 1 fail（AiInput @picker） |
| Cargo `datazen` | **PASS** | lib 206 + mysql IT 1 + postgres IT 1 + workflow 13 + provider 11 |
| E2E core | **PASS** | 9/9 specs |
| E2E db | **FAIL** | 3/11 specs 全过；8 specs 有失败（见下） |

## 单元 / 集成细节

### Vitest + kiwi
- 通过：302
- 失败：`AiInput > detects @ in input and opens picker`（picker 未出现，疑似既有 flaky）
- 无插件时额外失败：kiwi 相关 `useConnectionForm` / `getSqlDialect`（环境未注入插件，非本分支逻辑回归）

### Cargo
全部通过，含 `TEST_MYSQL_*` / `TEST_PG_*` 本地 gated IT。

## E2E core — PASS

`pnpm e2e:core`：main-window、new-connection、edit-delete、search-group、settings、i18n-menu、homepage、drag-drop、backup — **全部通过**。

## E2E db — FAIL（含本分支相关回归迹象）

| Spec | 结果 | 备注 |
|------|------|------|
| mysql-multi-database.ts | **PASS** 3/3 | 新功能 |
| postgres-multi-database.ts | **PASS** 3/3 | 新功能 |
| sql-query.ts | **PASS** 22 | |
| data-sync-real.ts | **PARTIAL** 19 pass / 1 fail | SYNC-PERM：`datazen_readonly` 无密码 Access denied（环境） |
| connection-window.ts | **FAIL** 7p/31f | 大量「等待 schema 树加载超时」 |
| table-data.ts | **FAIL** 4p/7f | schema 树超时 / 分页 |
| table-edit.ts | **FAIL** 3p/3f | schema 树超时 |
| table-structure.ts | **FAIL** 9p/3f | 侧栏新表 / schema 超时 |
| export-import.ts | **FAIL** | schema 树超时 |
| data-types.ts | **FAIL** 16p/34f | 多为断言/结构 tab |
| mysql.ts | **FAIL** 11p/9f | schema 树超时等 |

### 根因判断（待修）

多数 DB E2E 仍假设 **StandardSchemaTree**：连接后侧栏直接出现表行。  
本分支对 MySQL/PG 改为 **MultiDatabaseSchemaTree**：需先展开数据库节点才加载表。  
`e2e/helpers.ts` 的 `waitForSchemaTreeLoaded` / `clickTableInSidebar` 未适配展开步骤 → 超时级联失败。

**建议修复（非本次回归执行范围）**：更新 helpers：连接后若存在多库节点，先展开 `preferred`/`initial`/第一个业务库，再等表；或 E2E 连接时带明确 database 且 helpers 自动 expand。

## 新功能相关用例

- F2 MySQL 多库 E2E：**PASS**
- F4 PostgreSQL 多库 E2E：**PASS**

## 未跑

- `e2e:kiwi`（当前 binary plugins=none，无 kiwi 驱动）
- `e2e:ai`（可选，未纳入本次）
