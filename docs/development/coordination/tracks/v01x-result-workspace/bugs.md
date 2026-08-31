# F：Result Workspace 轨道 Bug 清单

## 当前清单

### v01x-result-workspace-BUG-001

- 描述（高）：首次得到可推断图表的查询结果时，`chartConfig` 通常尚未存在；ResultWorkspace 却把缺少 config 判为 `chartAvailable: false` 并禁用 Chart 入口，阻断现有 `ChartView` 在缺省 config 下的推荐/初始化流程。
- 状态：待验证（修复后）
- 记录时间：2026-08-31
- 重现步骤：
  1. 构造含数值列和至少一行数据的 `StatementResult`。
  2. 以 `view="table"`、`chartConfig={undefined}` 渲染 `ResultWorkspace`。
  3. 观察 Chart 按钮：实际为 disabled，无法触发 `onViewChange('chart')`。
  4. 对照现有 query 状态：`QueryExecState.chartConfig` 是可选字段，成功查询路径不会预先创建 config；现有 `ChartView` 会在 `savedConfig` 缺省时根据 recommendation 生成 config 并回调保存。
- 验证记录：修复 `chartAvailable` 和 view resolution，使可推断结果不再依赖已保存的 `chartConfig`；无 config 时交由现有 `ChartView` 生成推荐配置。新增回归断言覆盖 helper 和壳组件的可用性/config callback；定向 3 个测试文件、15 个测试全通过。待独立测试代理复验。

### v01x-result-workspace-BUG-002

- 描述（中）：Chart 数据点点击只调用 `onRowDetail(rowIndex)`，没有切换回 Table。现有 QueryPanel 的等价路径同时切换 `resultViewMode` 到 Table 并设置 detail row，因此该组件不能完整保留已有数据点回看行为。
- 状态：待验证（修复后）
- 记录时间：2026-08-31
- 重现步骤：
  1. 以 chartable result、有效 `chartConfig`、`view="chart"` 渲染 ResultWorkspace。
  2. 点击 ChartView 的数据点。
  3. 实际只收到 `onRowDetail(rowIndex)`；组件没有调用 `onViewChange('table')`，仍停留在 Chart 视图，无法显示 ResultTableView 的高亮定位。
  4. 对照现有 `QueryPanel.tsx:1159-1162`，其数据点回调会先切 Table，再设置 row detail。
- 验证记录：新增独立 Chart 数据点 handler，严格先调用 `onViewChange('table')` 再调用 `onRowDetail(rowIndex)`；Table 行详情仍只转发 row detail，并覆盖缺失回调不抛异常。定向 3 个测试文件、15 个测试全通过。待独立测试代理复验。

## 环境/回归记录

- `pnpm typecheck` 被 worktree 缺失的生成文件 `src/locales/builtinLocales.ts` 阻断，并连带报告既有 `SettingsContent.tsx` 隐式 any；未修改 codegen。
- 全量 Vitest 为 195/266 文件通过、1393/1563 测试通过，主体失败由同一缺失生成文件导致；不登记为本轨业务 Bug。
