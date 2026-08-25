# i18n 翻译去重计划

> 生成日期：2026-08-25
> 基于 `src/locales/en.ts`（1510 条）+ `src/locales/zh-CN.ts` 交叉比对

## 概要

| 指标 | 数值 |
|------|------|
| 总翻译条目 | 1510 |
| 值完全重复组数（英+中均相同） | 125 组 |
| 涉及重复 key 数 | 306 个 |
| 可节省 key 数（去重后） | 181 个（-12%） |
| 英文相同但中文不同（需保留） | 33 组 |

## 策略

### 合并原则

1. **通用动词/名词** → 收归 `common.*` 命名空间
2. **功能标题**（菜单/导航/页面标题相同） → 保留一个最语义化的 key，其余改为引用
3. **AI 提示类** → 收归 `common.ai.*` 或相似前缀
4. 合并时需同步更新所有 13 个 locale 文件 + 所有引用该 key 的组件

### 不合并的情况

英文相同但中文不同的 33 组**必须保留**——它们在不同语境下有不同的翻译需求。

## Category A：可安全合并（125 组，节省 181 key）

### 高优先级（重复 4 次以上，收益最高）

| 建议 canonical key | EN | ZH | 当前重复 key |
|---|---|---|---|
| `common.refresh` | Refresh | 刷新 | `main.ctx.refresh`, `connWin.refresh`, `objects.refresh`, `processList.refresh`, `serverStatus.refresh`, `sync.refreshPreview`, `dashboard.refreshWidget` |
| `common.newConnection` | New Connection | 新建连接 | `menu.newConnection`, `main.newConnection`, `main.ctx.newConnection`, `action.newConnection`, `newConn.title` |
| `common.export` | Export | 导出 | `connShare.exportAction`, `export.export`, `batchExport.title`, `batchExport.export`, `chart.export` |
| `common.aiNotConfigured` | Please configure an AI provider in Settings first | 请先在设置中配置 AI 服务 | `chat.notConfigured`, `nl2sql.notConfigured`, `explain.notConfigured`, `diagnosis.notConfigured`, `smartFilter.notConfigured` |
| `common.delete` | Delete | 删除 | `common.delete`, `sync.optionDelete`, `sync.filter.delete`, `workflows.delete` |
| `common.selectAll` | Select All | 全选 | `common.selectAll`, `menu.selectAll`, `dataTable.selectAll`, `batchExport.selectAll` |
| `common.backupDatabase` | Backup Database | 备份数据库 | `menu.backup`, `main.ctx.backup`, `action.backup`, `backup.title` |
| `common.restoreDatabase` | Restore Database | 恢复数据库 | `menu.restore`, `main.ctx.restore`, `action.restore`, `backup.restoreTitle` |
| `common.copyName` | Copy Name | 复制名称 | `main.ctx.copyName`, `schemaTree.copyName`, `objects.copyName`, `workflows.copyName` |
| `common.password` | Password | 密码 | `connShare.password`, `newConn.password`, `newConn.authPassword`, `createUser.password` |
| `common.columns` | Columns | 列 | `connWin.fields`, `erDiagram.columns`, `indexes.columns`, `indexes.colColumns` |
| `common.execute` | Execute | 执行 | `query.execute`, `sync.execute`, `workflows.execute`, `transfer.step.execute` |
| `common.dashboards` | Dashboards | 数据看板 | `menu.dashboard`, `nav.dashboard`, `action.dashboard`, `dashboard.title` |

### 中优先级（重复 3 次）

| 建议 canonical key | EN | ZH | 当前重复 key |
|---|---|---|---|
| `common.cancel` | Cancel | 取消 | `common.cancel`, `query.txUnclosedCancel`, `transfer.cancel` |
| `common.skip` | Skip | 跳过 | `common.skip`, `query.txAbortedSkip`, `transfer.mapping.skip` |
| `common.close` | Close | 关闭 | `common.close`, `traffic.close`, `connWin.closeTab` |
| `common.copied` | Copied | 已复制 | `common.copied`, `backup.logCopied`, `mcp.config.copied` |
| `common.preview` | Preview | 预览 | `common.preview`, `workflows.aiCreate.preview`, `transfer.step.preview` |
| `common.dataSync` | Data Sync | 数据同步 | `menu.dataSync`, `action.dataSync`, `sync.title` |
| `common.schemaDiff` | Schema Diff | 结构对比 | `menu.schemaDiff`, `sync.openSchemaDiff`, `schemaDiff.title` |
| `common.edit` | Edit | 编辑 | `menu.edit`, `settings.prompts.edit`, `workflows.edit` |
| `common.theme` | Theme | 主题 | `menu.theme`, `plugins.page.themeBadge`, `settings.theme` |
| `common.importAppData` | Import App Data | 导入应用数据 | `menu.importConfig`, `action.importConfig`, `appData.importConfirmTitle` |
| `common.importConnections` | Import Connections | 导入连接 | `menu.importConnections`, `welcome.importConnection`, `connShare.importTitle` |
| `common.workflows` | Workflows | 工作流 | `welcome.feature.workflow.title`, `nav.workflow`, `workflows.title` |
| `common.aiAssistant` | AI Assistant | AI 助手 | `welcome.feature.ai.title`, `settings.ai`, `chat.title` |
| `common.editConnection` | Edit Connection | 编辑连接 | `main.ctx.editConnection`, `conn.edit`, `newConn.editTitle` |
| `common.disconnect` | Disconnect | 断开连接 | `main.ctx.disconnect`, `conn.disconnect`, `mcpClient.disconnect` |
| `common.exportFailed` | Export failed | 导出失败 | `appData.exportFailed`, `connShare.exportFailed`, `batchExport.failed` |
| `common.schema` | Schema | Schema | `newConn.schema`, `query.schema`, `sync.schema` |
| `common.functions` | Functions | 函数 | `schemaTree.functions`, `objects.function`, `sync.group.functions` |
| `common.procedures` | Procedures | 存储过程 | `schemaTree.procedures`, `objects.procedure`, `sync.group.procedures` |
| `common.dataTransfer` | Data Transfer | 数据迁移 | `schemaTree.dataTransfer`, `sync.openDataTransfer`, `transfer.title` |
| `common.history` | History | 历史 | `query.history`, `workflows.history.title`, `dashboard.history` |
| `common.executing` | Executing… | 执行中… | `query.executing`, `structEditor.executing`, `workflows.executing` |
| `common.result` | Result | 结果 | `query.result`, `workflows.result`, `transfer.step.result` |
| `common.executionFailed` | Execution failed | 执行失败 | `query.executeFailed`, `structEditor.executeFailed`, `workflows.executionFailed` |
| `common.privileges` | Privileges | 权限 | `createUser.privileges`, `privileges.title`, `privileges.selectPrivileges` |

### 低优先级（重复 2 次，共 ~90 组）

略。完整列表可通过以下命令生成：

```bash
node -e "
const fs = require('fs');
const content = fs.readFileSync('src/locales/en.ts', 'utf8');
// ... (使用上方分析脚本)
"
```

## Category B：不可合并（33 组）

以下组英文值相同但中文翻译不同，体现了不同语境下的语义差异，**必须保留独立 key**：

| EN | ZH 变体 | 原因 |
|---|---|---|
| Database | "数据库名" / "数据库" | 表单 label vs 通用名词 |
| Tables | "表" / "表名（多表）" | 通用 vs 特定 UI |
| Back | "返回" / "返回列表" / "上一步" | 导航 vs 向导步骤 |
| Copy | "复制" / "副本" | 动作 vs 名词 |
| Open | "打开连接" / "打开" | 带宾语 vs 纯动词 |
| Connection | "连接" / "关联连接" | 通用 vs 专业术语 |
| Table | "表" / "表名" / "表格" | DB 对象 vs HTML table |
| Default | "默认" / "默认值" | 形容词 vs 名词短语 |
| Dashboards | "数据看板" / "看板" | 完整 vs 简写 |
| Settings | "设置" / "偏好设置" | 菜单项 vs 页面标题 |
| Disconnect | "断开连接" / "断开" | 完整表述 vs 省略宾语 |
| Continue | "继续" / "继续执行" | 通用 vs 带宾语 |
| History | "历史" / "执行记录" | 通用 vs 专业 |
| Next | "下一页" / "下一步" | 分页 vs 向导 |
| Clear | "清空" / "清除" | 近义词不同语境 |
| Name | "字段名" / "名称" | 专业 vs 通用 |
| Target | "目标数据库" / "目标" | 完整 vs 简写 |
| Compare | "比较" / "对比" | 近义词 |

## 执行步骤

1. **Phase 1 — 高优先级合并**（影响最大的 13 组，消除 ~40 个冗余 key）
   - 在 `common` 命名空间新增 canonical key
   - 全局搜索替换所有引用旧 key 的组件（使用 `t('old.key')` → `t('common.newKey')`）
   - 删除旧 key
   - 运行 `node scripts/i18n-sync-check.mjs` 确认无遗漏
   - 运行 `npx vitest run` 确认无破坏

2. **Phase 2 — 中优先级**（24 组，消除 ~48 个冗余 key）

3. **Phase 3 — 低优先级**（88 组，消除 ~93 个冗余 key）

4. **验证**
   - `npx vitest run` 通过
   - `node scripts/i18n-sync-check.mjs` 无 missing key
   - 肉眼抽查 UI 无错位

## 注意事项

- 部分 key 如 `sync.refreshPreview` 虽然翻译值与 `common.refresh` 一样，但未来可能需要独立化（如 "刷新预览"），合并前需评估
- `dashboard.refreshWidget` 同理——如果将来要改为 "刷新组件" 则不应合并
- 建议对 `menu.*` / `action.*` / `main.ctx.*` 这类「入口点」保持就近引用 `common.*`，而非定义独立 key
- 驱动 locales（`packages/drivers/*/locales/`）不在本计划范围内
