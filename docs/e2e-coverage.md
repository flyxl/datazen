# Host E2E 覆盖矩阵

> 与 [AGENTS.md](../AGENTS.md)「Host E2E 覆盖规则」、[e2e-testing.md](./e2e-testing.md) §1.1 配套。  
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
| 主页操作面板、搜索、分组 | `main-window.ts`, `homepage-features.ts` | Covered |
| 新建 / 编辑 / 删除连接 | `new-connection.ts`, `edit-delete-connection.ts` | Covered |
| 连接窗口工具栏、表树、子标签 | `connection-window.ts` | Covered |
| 查询执行 / 历史 / 收藏 | `sql-query.ts` | Covered |
| 绑定参数面板填值并执行 | `sql-query.ts` (SQ-BIND-*) | Covered |
| EXPLAIN 面板 | `sql-query.ts` (SQ-EXPLAIN-*) | Covered |
| 表数据分页 / 排序 / 选择 | `table-data.ts` | Covered |
| 表筛选：打开 / 添加 / Apply / Clear / AND·OR / 收起 / chip / 空值不报错 | `table-filter.ts` | Covered |
| 表内联编辑 | `table-edit.ts` | Covered |
| 详情面板 | `detail-panel.ts` | Covered |
| 结构只读 + 内嵌编辑 + 返回 + 保存列变更 | `table-structure.ts` | Covered |
| 索引列表 / 新建对话框 / 删除 / 在结构中编辑 | `table-indexes.ts`, `connection-window.ts` | Covered |
| FK / DDL 子标签 | `connection-window.ts` | Covered |
| Objects / Privileges 面板 | `object-browser.ts` | Covered |
| 侧栏导出 / 导入 | `export-import.ts` | Covered |
| DataTable 工具栏导出对话框 | `export-import.ts` (EI-GRID-*) | Covered |
| ER 图 | `er-diagram.ts` | Partial |
| 图表 | `chart-views.ts`, `chart-expand.ts` | Covered |
| AI Chat / @ 上下文 | `ai-context*.ts`, `ai-features.ts` | Covered / Partial（需 API Key 的路径见 Exception） |
| 智能筛选未配置提示 | `table-filter.ts` (TF-AI-*) | Covered |

## 其他窗口 / 设置

| 用户路径 | Spec | 状态 |
|----------|------|------|
| 设置：主题 / 持久化 / 分区导航（通用·浏览·编辑器·行为·日志·AI·Prompt·MCP·扩展） | `settings.ts` | Covered |
| Workflow 列表 / 执行 / 历史 | `workflow.ts`, `workflow-window.ts` | Covered |
| Workflow 可视化 ↔ YAML 切换与保存入口 | `workflow-window.ts` (WF-YAML-*) | Covered |
| 备份窗口打开与连接选择 UI | `backup-window.ts` | Covered |
| 备份执行（IPC） | `backup-database.ts` | Covered |
| Schema Diff 窗口打开与步骤控件 | `schema-diff-window.ts` | Covered |
| 数据同步窗口 | `homepage-features.ts`, `data-sync-real.ts` | Partial |
| 数据看板 | `data-dashboard-*.ts` | Covered |
| 应用数据备份标签 | `app-data-backup.ts` | Covered |
| 路径 IPC 加固 | `path-ipc-hardening.ts` | Covered |

## 例外登记（自动化限制）

| 路径 | 原因 | 替代覆盖 |
|------|------|----------|
| 真实系统 IME 拼音组字 | WebDriver 无法稳定模拟系统输入法 | `FilterEditor.test.tsx` composition 事件；E2E 覆盖 Apply/空值路径 |
| 原生「另存为 / 打开文件」对话框点选 | OS 对话框不可靠 | webdriver 路径 IPC（`write_file` / `export_app_data` 等） |
| 依赖真实 LLM Key 的 AI 深度路径 | 环境无 Key 时跳过 | `ai-features.ts` 条件执行；无 Key 时仍测未配置 UI |
| `ConnectionSettingsDialog` | 当前未挂到可点击入口（非用户可达） | 组件单测；挂接 UI 后须立刻补 E2E |

## 维护约定

1. 新增 Host UI → 更新本矩阵 + 新增/扩展 spec，同 PR。  
2. 将 Gap 清为零；Partial 须有明确后续用例 ID。  
3. 驱动深度能力写入驱动 crate 的 `e2e/`，并在驱动文档维护，不占用本矩阵。
