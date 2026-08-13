# 删除行 / 主窗口关闭 / 导入提示 / 备份体验

> 分支：`fix/delete-row-backup-import-windows`

## 功能清单

| ID | 功能 | 状态 | 测试 |
|----|------|------|------|
| 1 | 删除行：补齐键盘 Delete + E2E（IPC 已有） | done | vitest + e2e TD-DEL-001 |
| 2 | 主窗口仅在全部子窗口关闭后才能关闭 | done | window.rs 单测 |
| 3 | `.datazenconnection` 导入成功提示为 DataZen 而非 TablePlus | done | cargo roundtrip |
| 4 | 备份窗口分组与主窗口一致，列出全部连接 | done | BackupWindow.test + BKU-005 |
| 5 | 备份导出显示分步进度 | done | backupProgress.test |
| 6 | 视图不写 `INSERT INTO`（改为 VIEW DDL） | done | sql_dump view_like 单测 |
| 7 | 备份 vs 批量导出：产品说明（非同一入口） | done | 见下方 |
| 8 | 备份/恢复包含函数与存储过程 | done | sql_dump DELIMITER 单测 + e2e BACKUP-011 |
| 9 | 主页恢复：打开选连接/选库窗口，再选文件 | done | MainWindow + BackupWindow restore mode + HOME-RESTORE-001 |
| 10 | MySQL/多级树：SQL 限定路径自动同步执行栏选择框 | done | queryContextPath vitest + SQ-CTX-001 |
| 11 | path-hierarchy（Superset）多级联动 catalog/schema 选择框 | done | QueryContextSelectors + queryContextPath |
| 12 | 数据同步：选库、错误时关闭进度框、比较区分 table/view/function/procedure、同步索引 | done | compare/table_sync + DataSyncWindow |
| 13 | 恢复：目标已有对象时确认覆盖；按语句上报真实进度 | done | BackupWindow + restore-progress + BACKUP-012 |
| 14 | 新建查询不闪现「正在加载对象」补全框（所有 SQL multidb 共用 SqlEditor） | done | namespaceLoadingCompletion + SQ-AC-001 / MY-AC-001 |
| 15 | 恢复进度条下方执行日志 | done | BackupWindow progress log |
| 16 | Workflow / 数据看板 SQL 编辑复用 SqlEditor | done | WorkflowForm + WidgetEditorDrawer |

## #7 备份数据库 vs 连接窗口批量导出

**不是同一功能的两个入口，不必合并，但交互应一致（进度、分组文案、视图不写 INSERT）。**

| | 主页「备份数据库」 | 连接窗口「批量导出」 |
|--|--|--|
| 范围 | 整库逻辑备份（类似 pg_dump / mysqldump） | 当前库已选表 |
| 产出 | 单个 `.sql` / `.sql.gz` | csv / json / sql，可 ZIP |
| 用途 | 灾难恢复、整库迁移 | 日常取数、部分表交付 |
| 视图 | 应导出 `CREATE VIEW`，禁止 `INSERT` | 用户显式勾选视图时可导出查询结果 |

## 变更日志

- 进行中
