# 进度：使用说明 / 导入卡死 / 导出格式 / 补全 / Superset SQL

分支：`fix/docs-import-export-autocomplete`  
日期：2026-08-12

| # | 功能 | 开发 | 单测 | 独立测试 Agent | 提交 |
|---|------|------|------|----------------|------|
| 1 | 菜单「使用说明」打开两个窗口，其中一个无法拖动 | 开发完成 | 通过 | 通过 | 已提交 |
| 2 | 导入连接点「浏览文件」弹出 Finder 后应用卡死 | 开发完成 | 通过 | 用户复现后已二次修复 | 已提交 |
| 3 | 导出连接改为 RNCryptor 二进制 `.datazenconnection`（非明文 JSON；导入仍兼容 TablePlus） | 开发完成 | 通过 | | 已提交 |
| 4 | Kiwi / Superset：未写完整路径时 SQL 补全应给出当前库对象 | 开发完成 | 通过 | | 已提交 |
| 5 | Superset：SQL 若带了当前 database 前缀，发给 API 前剥掉 | 开发完成 | 通过 | | 插件仓提交 |
| 6 | Superset 懒加载树：补全时触发加载并展示 loading | 开发完成 | 通过 | | 已提交 |

## 后续：PostgreSQL WHERE 补全混入 schema/表名

`SELECT * FROM product WHERE p` 曾同时列出 `price`（列）和 `product` / `public`（表/schema）。
CodeMirror `completeFromSchema` 在顶层总是合并 tables/schemas + `defaultTable` 列。
现用 `contextualSchemaCompletion` 按光标前关键字过滤：`WHERE`/`SELECT` 只保留 `type === "property"`。

## 阻塞

无。#3 导出扩展名为 `.datazenconnection`（RNCryptor v3，与 TablePlus 同算法；导入仍接受 `.tableplusconnection` / 旧 JSON）。#5/#6 树与补全共用 `schemaStore.pathItems`。

## 测试记录

### #1 使用说明窗口 — [DocsWindow Tester](c8fd3f44-6c6d-46a7-8f95-3b07d6ce6028)

环境：已运行 `pnpm tauri:dev` debug 二进制，zh-CN。

| 用例 | 结果 | 观察 |
|------|------|------|
| TC-DOCS-001 单击只开一个窗口 | PASS | 新增 1 个「使用说明 - DataZen」920×680 |
| TC-DOCS-002 标题栏可拖动 | PASS | 从 (500,171) 拖到 (700,266) |
| TC-DOCS-003 可关闭 | PASS | Cmd+W 后窗口消失，主窗口仍在 |
| TC-DOCS-004 再次打开复用 | PASS | 同一 windowId，未新开 |

无 Bug。
