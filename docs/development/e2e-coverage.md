# Host E2E 覆盖矩阵

> 与 [AGENTS.md](../../AGENTS.md)「Host E2E 覆盖规则」、[e2e-testing.md](./e2e-testing.md) §1.1 配套。  
> **规则：** Host 内所有 UI 交互、所有用户可走到的路径，都必须有 `e2e/specs/` 覆盖；驱动专属用例不进本表。

## 状态图例

| 状态 | 含义 |
|------|------|
| **Covered** | 有交互 + 结果断言 |
| **Partial** | 仅打开/可见或 IPC，缺完整路径 |
| **Gap** | 用户可走但尚无 E2E（须补齐） |
| **Exception** | 自动化例外（见文末），须有替代覆盖 |

## Host Connection Contract × Driver

> 目标：每个 SQL 驱动各开连接窗口，跑同一套 Host UI/IPC journeys（适配验证）。

| 入口 | 说明 |
|------|------|
| `pnpm e2e:contract:matrix` | PG + MySQL + SQLite 全矩阵 |
| `pnpm e2e:contract:pg` | 仅 PostgreSQL（冒烟） |
| `pnpm test:unit:e2e-contract:coverage` | fixtures/plan 单测覆盖率 ≥80% |

| Journey | 内容 | sqlite |
|---------|------|--------|
| HC-CONN | 工具栏 / 子标签 | run |
| HC-QUERY | 执行 SQL | run |
| HC-DATA | 表数据分页 | run |
| HC-FILTER | Apply / 空值不炸 | run |
| HC-EDIT | 内联编辑 | run |
| HC-STRUCT | 结构内嵌编辑+返回 | run |
| HC-INDEX | 新建索引对话框 | run |
| HC-EXPORT | DataTable 导出对话框 | run |
| HC-OBJ | 例程面板 | **skip**（无 objects） |
| HC-EXPLAIN | EXPLAIN 面板 | run |

实现：`e2e/contract/` + `e2e/specs/host-contract-matrix.ts`。方言深度仍在 `packages/drivers/<id>/e2e/`。

## 主窗口 / 连接 / SQL

| 用户路径 | Spec | 状态 |
|----------|------|------|
| 主页操作面板、搜索、分组 | `main-window.ts`, `homepage-features.ts`, `unified-main-window.ts` | Covered |
| 主页空白右键 Web 菜单（含边缘不截断） | `homepage-features.ts` (HOME-021) | Covered（需 webdriver 二进制；无二进制时 BLOCKED） |
| 统一工作区导航（连接 / 工作流 / 看板） | `unified-main-window.ts` | Covered |
| 连接工作区首页（无 panel 空状态） | `unified-main-window.ts`, `unified-tab-bar.ts` (UTB-005) | Covered |
| 新建 / 编辑 / 删除连接 | `new-connection.ts`, `edit-delete-connection.ts` | Covered |
| 连接工具栏、表树、子标签（统一主窗口内） | `connection-window.ts`, `unified-tab-bar.ts` | Covered |
| 侧栏删表后树立即刷新（不再需关窗） | `ConnectionNavigatorTree.test.tsx` / `schemaStore.test.ts` | Covered（原生 Drop 确认框见例外） |
| 连接导航树右键菜单（连接/库/Schema/表·视图） | `navigator-context-menu.ts` (NCM-*) | Covered（Drop 确认执行见例外 NCM-046） |
| 查询执行 / 历史 / 收藏 | `sql-query.ts` | Covered |
| 绑定参数面板填值并执行 | `sql-query.ts` (SQ-BIND-*) | Covered |
| EXPLAIN 面板 | `sql-query.ts` (SQ-EXPLAIN-*) | Covered |
| 表数据分页 / 排序 / 选择 / 删除行按钮 | `table-data.ts` | Covered |
| 文本选择：内容可复制、交互控件不可选中（user-select 计算样式） | `table-data.ts` (TD-SEL-001) | Covered |
| 表筛选：打开 / 添加 / Apply / Clear / AND·OR / 收起 / chip / 空值不报错 | `table-filter.ts` | Covered |
| 表内联编辑 | `table-edit.ts` | Covered |
| 详情面板 | `detail-panel.ts` | Covered |
| 结构只读 + 内嵌编辑 + 返回 + 保存列变更 + 导出表结构按钮 | `table-structure.ts` | Covered |
| 索引列表 / 新建对话框 / 删除 / 在结构中编辑 | `table-indexes.ts`, `connection-window.ts` | Covered |
| FK / DDL 子标签 | `connection-window.ts` | Covered |
| Objects / Privileges 面板 / 例程 Web 右键菜单 | `object-browser.ts` (OBJ-003) | Covered |
| 管理命令（创建/删除数据库·Schema·用户·授权/撤销权限） | `bugfix-admin-commands.ts` | Covered |
| 侧栏导出 / 导入 | `export-import.ts` | Covered |
| 顶栏导出对话框 / DataTable 导出 / 整表范围 / Schema 树 Web 菜单导出 | `export-import.ts` (EI-BE / EI-GRID / EI-001 / EI-002) | Covered |
| ER 图 | `er-diagram.ts` (ER-001~ER-008) | Covered |
| 图表 | `chart-views.ts`, `chart-expand.ts` | Covered |
| AI Chat / @ 上下文 | `ai-context*.ts`, `ai-features.ts` | Covered（需 API Key 的路径见 Exception） |
| AI 无 Key 降级 | `ai-no-key-fallback.ts` (TC-AI-007~009) | Covered |
| 智能筛选未配置提示 | `table-filter.ts` (TF-AI-*) | Covered |

## 其他窗口 / 设置

| 用户路径 | Spec | 状态 |
|----------|------|------|
| 设置：主题 / 持久化 / 分区导航（通用·浏览·编辑器·行为·日志·AI·Prompt·MCP·扩展） | `settings.ts` | Covered |
| Workflow 列表 / 执行 / 历史 / 列表右键无运行 / 历史无菜单 | `workflow.ts`, `workflow-window.ts` (WF-CTX-*) | Covered |
| 嵌入主窗口的工作流工作区（非独立 OS 窗口） | `workflow-window.ts`, `unified-main-window.ts` | Covered |
| Workflow 可视化 ↔ YAML 切换与保存入口 | `workflow-window.ts` (WF-YAML-*) | Covered |
| Workflow / 数据看板 SQL 编辑（SqlEditor 高亮 + Web 右键） | `workflow-window.ts` (WF-SQL-001 / WF-SQL-002), `data-dashboard-widget-ux.ts` (UJ-06) | Covered |
| 恢复执行日志（virtual scroll + 复制） | `BackupWindow.test.tsx` | Covered（原生文件对话框为例外） |
| 备份窗口打开与连接选择 UI / 分组文案 | `backup-window.ts` | Covered |
| 备份执行（IPC） | `backup-database.ts` | Covered |
| 恢复：覆盖确认 + 分步进度 | `backup-database.ts` (BACKUP-012) + `BackupWindow.test.tsx` | Covered / Exception（原生 ask + 打开文件） |
| 新建查询不弹出对象加载补全框 | `connection-window.ts` (SQ-AC-001), `mysql.ts` (MY-AC-001) | Covered |
| SQL 补全只拉取语句中已加载完整表名的列（禁止前缀 get_columns） | `schemaStore.test.ts` / `sqlEditorDefaults.test.ts` / `buildEditorSchema.test.ts` | Covered |
| Schema Diff 窗口打开与步骤控件 | `schema-diff-window.ts` | Covered |
| 数据同步 Diff Workspace（Options / Swap / Compare / Summary / row-diff / preview / Execute chrome） | `data-sync-window.ts` (DSW-001~008, DSW-MAP/WS) | Covered（Execute 对 live DB 见 IPC spec；无 PG 夹具时 MAP/WS 用例 soft-skip） |
| 数据同步 UI 执行闭环（compare→preview→execute 后回查目标行数） | `data-sync-window.ts` (DSW-EXEC-001) | Covered |
| 数据同步 IPC（inspect / compare / generate SQL / apply / revalidate） | `data-sync-real.ts` | Covered（需 `e2e/setup-sync-dbs.sh` PG 夹具；SYNC-REAL-009 apply→recompare） |
| 数据传输窗口 + PG→PG 迁移闭环 | `data-transfer-window.ts` (DTW-001~003, DTW-CL) | Covered（跨方言 Execute 见 V1 限制 / 例外） |
| 连接 Pin 置顶 | `ops-pin.ts` | Covered |
| 连接边界：快速新建/删除、并发 tab、生命周期 | `connection-edge-cases.ts` (TC-EDGE-009~013) | Covered |
| 设置持久化：主题/语言/字体/确认删除开关 | `settings-persistence.ts` (TC-SET-007~010) | Covered |
| 窗口操作：单窗口模式、tab 状态 | `window-operations.ts` (TC-WIN-001~005) | Covered |
| SQL 多 Tab：独立查询结果 | `sql-multi-tab.ts` (TC-QUERY-009~012) | Covered |
| Schema 树完整性：加载/右键/DDL 后刷新 | `schema-tree-completeness.ts` (TC-TREE-001~006) | Covered |
| 表批量操作：多选/分页边界 | `table-batch-ops.ts` (TC-TABLE-009~014) | Covered |
| 工作流完整生命周期 | `workflow-lifecycle.ts` (TC-WF-007~012) | Covered |
| AI 无 Key 降级 UI | `ai-no-key-fallback.ts` | Covered |
| 对象过滤器对话框与树过滤持久化 | `object-filter.ts` | Covered |
| 进程列表 / 服务器状态面板 | `ops-process-server.ts` | Covered |
| DB 节点备份/还原预填入口 | `ops-ddl-backup.ts` | Covered |
| 数据看板 | `data-dashboard-*.ts` | Covered（表格视图底部导出按钮交互见 UJ-05） |
| 应用数据备份标签 | `app-data-backup.ts` | Covered |
| 路径 IPC 加固 | `path-ipc-hardening.ts` | Covered |
| 插件系统：安装（两步对话框）/ 管理卡片与权限徽标 / Workspace 导航+Tab / 桥往返（探针落盘或 shell 级降级断言，见例外）/ 双 Tab 体系独立 / 外观主题切换 / 停用联动关 Tab / 卸载确认 | `plugins.spec.ts`（J1/J2/J3/J5/J4）+ fixture `e2e/fixtures/sample-plugin/`（fixture 校验锚点：`plugins::fixture_tests` Rust 单测） | Covered（安装走 PathInput 键入路径，原生目录选择器见例外；`ui.notify` 限频、iframe 崩溃恢复与 iframe 内容加载见例外） |

## 例外登记（自动化限制）

| 路径 | 原因 | 替代覆盖 |
|------|------|----------|
| 真实系统 IME 拼音组字 | WebDriver 无法稳定模拟系统输入法 | `FilterEditor.test.tsx` composition 事件；E2E 覆盖 Apply/空值路径 |
| 原生「另存为 / 打开文件」对话框点选 | OS 对话框不可靠 | webdriver 门控路径 IPC（`export_app_data` / `backup_database` 直连变体等）；纯文件读写 `write_file` 系已删除，fixture 改 Node fs |
| 依赖真实 LLM Key 的 AI 深度路径 | 环境无 Key 时跳过 | `ai-features.ts` 条件执行；无 Key 时仍测未配置 UI |
| `ConnectionSettingsDialog` | 当前未挂到可点击入口（非用户可达） | 组件单测；挂接 UI 后须立刻补 E2E |
| E2E 夹具 `DROP`/`TRUNCATE` | Safe Mode 默认开启会拦截 | `executeSQL` / `withSafeModeOff` 临时关闭；`client-parity` 断言 DROP 被拦 |
| 删除行确认框（`confirmOnDelete`） | 原生 `ask` 对话框无法点选 | `DataTable.test.tsx` 工具栏/Delete 键；`commit_row_deletes` Rust 单测；E2E 断言删除按钮出现 |
| 侧栏 Drop 表/视图/Schema 确认 | 原生 `ask` 无法点选 | `ConnectionNavigatorTree.test.tsx` drop/truncate 流程；`navigator-context-menu.ts` NCM-044/022/046 断言菜单项 |
| 主窗口在子窗口未关时关闭 | 原生窗口关闭 + 阻塞对话框 | `window.rs` `non_main_window_labels` 单测 |
| 恢复覆盖确认（原生 `ask`）+ 选 SQL 文件 | OS 对话框不可点选 | `BackupWindow.test.tsx` ask/overwrite；`backup-database.ts` BACKUP-012 IPC overwrite |
| 恢复执行日志截断（>1500 行省略标记 / 字符预算） | 需 >1500 条 SQL 语句的真实大备份，E2E 不可行 | `backupProgress.test.ts`（行/字符预算、头尾保留、累计省略数、超长单行截断）；`BackupWindow.test.tsx` 覆盖日志渲染路径 |
| 数据看板表格视图：大数据量下底部导出按钮不被容器裁剪 | 依赖真实渲染高度的几何断言，跨 WebView 平台不稳定 | 布局修复（`flex flex-col` 容器约束使 DataTable `flex-1` 生效、虚拟滚动开启）；E2E UJ-05 覆盖导出按钮可见 + 点击打开导出对话框 |
| 选区视觉样式（`::selection` 颜色、大面积选区外观） | 纯视觉外观，无法自动化断言颜色/观感 | 全局 CSS（`globals.css` A1 主题化选区 + A2 控件 `user-select: none`）；TD-SEL-001 覆盖计算样式（内容可选中/控件不可选中） |
| 插件安装原生目录选择器（PathInput 浏览按钮） | OS 对话框不可点选（同上通用条目，此处为具体落点） | E2E 在 PathInput 键入 fixture 绝对路径走同一 UI 链路 |
| 插件 `ui.notify` 5s 限频 / 系统通知弹出 | 依赖系统通知中心，自动化不可观测 | `extensionBridge.test.ts` 限频用例（冷却窗口内第二次回 `E_RATE_LIMIT`） |
| 插件 iframe 崩溃恢复条（10s watchdog → 重载按钮） | 需真实加载失败时序，WebKit 自动化不稳定 | `PluginPageShell.test.tsx` watchdog/reload 用例；E2E 断言 shell 存在与重开路径 |
| 插件 iframe 内元素自动化（J2 桥往返的帧内 DOM 断言） | macOS WebKit 自动化下 `datazen://` 子帧导航被拒（实测截图：帧内容永不渲染、fixture JS 永不执行、`.storage.json` 探针永不落盘；同 URL 顶层窗口直载则正常渲染执行——疑为宿主 CSP `default-src 'self'` 未豁免 `datazen:` 子帧或 WebKit 自定义协议子帧策略，已登记 BUG-F9-04 待宿主验证） | 补偿：fixture 经既有桥 `storage.set` 持久化三个探针（`probe.bridge`/`probe.dark`/`probe.connCount`），E2E 从 `{appData}/plugins/datazen.sample/.storage.json` 轮询对账（内容可加载的平台即全量断言）；本环境下自动降级为真实 shell 级行为断言——watchdog 失败条出现 / 重载按钮重挂 iframe / manifest entry URL 解析正确。桥逻辑另有宿主 `extensionBridge` 64 例 + SDK 69 例单测背书。iframe 存在性断言（顶层文档）保留 |

## 维护约定

1. 新增 Host UI → 更新本矩阵 + 新增/扩展 spec，同 PR。  
2. 将 Gap 清为零；Partial 须有明确后续用例 ID。  
3. 驱动深度能力写入驱动 crate 的 `e2e/`，并在驱动文档维护，不占用本矩阵。
