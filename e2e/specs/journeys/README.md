# DataZen 用户 Journey 测试

这里放跨模块的连续用户路径。单个 Journey 应从用户可见入口开始，经过真实 UI 操作，最后断言可观察的结果；模块级 spec 仍保留，用于定位失败点和覆盖边界。

## 主路径 Journey

| Journey | 覆盖路径 | 入口 |
| --- | --- | --- |
| First Run → Query | 首次安装欢迎页 → 取消并重新建连 → 查看连接 → 首条查询 | `welcome-query-journey.ts` |
| Create Connection | 连接工作区 → 驱动切换 → 填写/测试/保存 → 重载持久化 | `connection-create-journey.ts` |
| Browse Connection | 连接列表 → 连接首页 → Schema 树 → 数据/结构/索引/外键/DDL | `connection-browse-journey.ts` |
| Create → Query | 新建连接 → 连接 → 新建查询 → SQL 结果 | `connection-query-journey.ts` |
| Navigator | 最近连接分组展开 → 折叠 → 恢复 | `../connection-navigator-expansion.ts` |
| Query Recovery | 查询失败 → 复制错误 → 修正 SQL → 成功结果 | `query-recovery-journey.ts` |
| Query → Chart | 查询 → 表格结果 → 图表类型 → 导出对话框 | `query-result-chart-journey.ts` |
| Query Toolbar | 缩窄窗口 → compact 工具栏 → 查询/历史 → 恢复窗口 | `query-toolbar-responsive-journey.ts` |
| Schema Diff | PG→PG、PG→MySQL、MySQL→PG | `schema-diff-*-journey.ts` |
| Data Sync | PG、MySQL 的比较→Review→执行闭环 | `data-sync-journey.ts` |
| Data Transfer | PG→PG、PG→MySQL、MySQL→PG | `data-transfer-*-journey.ts` |

## Edge Journey

| Journey | 连续状态路径 | 入口 |
| --- | --- | --- |
| First Run Recovery | 必填校验 → 连接失败 → 修正成功 → 删除最后连接 → Welcome | `first-run-edge-journey.ts` |
| Query Recovery | 错误 SQL → 复制错误 → 修正 → 成功 | `query-recovery-journey.ts` |
| Query State | 长查询取消 → 恢复执行 → 多 Tab 隔离 → 切回保留结果 | `query-edge-journey.ts` |
| Navigator Race | 慢连接 pending → 精确展开 → 分组折叠恢复 | `../connection-navigator-expansion.ts` |
| Connection Validation | 空 Host、无效 Host、错误密码、空密码、分组切换 | `../connection-validation.ts` |
| Connection Lifecycle | 快速增删、编辑刷新、失败重试、重复打开、分组重命名 | `../connection-edge-cases.ts` |

运行全部 Journey：

```bash
pnpm e2e:journeys

# 只跑异常恢复与状态边界 journey
pnpm e2e:journeys:edge
```

这组测试需要 WebDriver 构建和 PostgreSQL；Schema Diff、Data Sync、Data Transfer 还需要各自的夹具数据库。尚未实现的示例 SQLite 向导、真实 LLM 修复闭环不在这里伪造测试。
