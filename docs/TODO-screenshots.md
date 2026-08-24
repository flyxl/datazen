# E2E 截图修复 TODO

> 目标：`e2e/specs/zz-screenshots.ts` 全部 11 个用例通过，捕获 11 张高清截图。
> 更新时间：2026-08-24

## 如何截图

```bash
# 完整流程（构建 + 启动 app + 跑截图用例）
pnpm e2e -- --spec e2e/specs/zz-screenshots.ts

# 如果已经构建过，跳过构建
pnpm e2e:skip-build -- --spec e2e/specs/zz-screenshots.ts
```

截图输出到 `site/assets/screenshots/`，分辨率 2400×1600（2x retina）。

## 通过的用例（6/11）

| 用例 | 文件 | 大小 |
|------|------|------|
| 01-main-window | 01-main-window.png | 159KB |
| 02-query-chart | 02-query-chart.png | 365KB |
| 04-workflow | 04-workflow.png | 258KB |
| 16-er | 16-er.png | 425KB |
| 19-backup-sync | 19-backup-sync.png | 171KB |
| 09-ai-more | 09-ai-more.png | 239KB |

## 失败的用例（5/11）

### 1. 14-multidb — 多数据库树
**错误**：`tables not listed under postgres`
**根因**：虚拟滚动树中 `button[data-tree-node="db"][data-db-name="postgres"]` 点击后，表格行未在超时内出现。
**可能原因**：
- `toggleDb` 需要 `configId` + `connectionId` + `dbName` 三个参数，click handler 从 row data 取值，但 React 事件可能未正确触发
- 虚拟滚动只渲染可见行，`scrollIntoView` 后按钮可能还未被 React 挂载
- postgres 可能已展开，点击反而折叠了

**修复方向**：
1. 先用 `browser.execute` 调用 Tauri IPC `list_tables` 确认 postgres 数据库有表
2. 尝试用 `dblclick` 而非 `click` 触发展开
3. 如果单击无效，用 `dispatchEvent(new PointerEvent(...))` 模拟更真实的用户交互
4. 备选：截图时只展示数据库列表（不展开 postgres），仍然能体现多数据库功能

---

### 2. 17-sql-editor — SQL 编辑器
**错误**：`结果行数未达到 10`
**根因**：SQL 查询执行了但 `[data-dt-row]` 行数为 0。可能是：
- 连接未激活（14-multidb 失败后工作区状态异常）
- SQL 查询在错误的数据库上执行（连接默认数据库不是 `postgres`，`demo_sales` 表不存在于默认库）
- 新建查询 tab 后编辑器未获得焦点

**修复方向**：
1. 测试开头用 IPC `use_database` 显式切到 `postgres`
2. 等待 `[data-dt-row]` 出现前先检查 `exec.running` 状态
3. 14-multidb 修复后此用例可能自动通过（级联失败）

---

### 3. 10-chart-types — 图表类型切换
**错误**：`toolbar button 柱状图 not found`
**根因**：`clickToolbarButton('柱状图')` 在所有 `<button>` 中搜索 `aria-label` 或 `textContent` 包含"柱状图"的按钮，未找到。
**可能原因**：
- ChartToolbar 渲染在 ChartView 内部，图表按钮用的是 icon-only + `aria-label`
- `aria-label` 依赖 i18n `t('chart.type.bar')` 返回"柱状图"，但语言可能未生效
- 图表视图未打开（02-query-chart 的 `clickToolbarButton('图表')` 可能未正确触发 `setResultViewMode('chart')`）

**修复方向**：
1. 检查 `aria-label` 值：`browser.execute(() => Array.from(document.querySelectorAll('button')).filter(b => b.getAttribute('aria-label')?.includes('Chart') || b.getAttribute('aria-label')?.includes('图')))`
2. 改用 icon SVG 类名选择器：`button:has(svg.lucide-bar-chart-3)` 或按位置选择
3. 直接用 `browser.execute` 点击第二个 chart type button（bar 在 CHART_TYPES 数组索引 0）

---

### 4. 11-chart-export — 饼图导出
**错误**：`toolbar button 饼图 not found`
**根因**：与 10-chart-types 相同，`clickToolbarButton('饼图')` 找不到按钮。

**修复方向**：同上，改用可靠的选择器。

---

### 5. 08-ai-filter — 自然语言筛选
**错误**：`demo_sales not found in sidebar`
**根因**：侧边栏中 `button[data-tree-node="table"]` 元素不存在，因为 postgres 数据库未展开（与 14-multidb 同源）。

**修复方向**：
1. 14-multidb 修复后此用例可能自动通过
2. 或者先用 IPC `get_tables('postgres')` 确认表存在，再展开

---

### 6. 07-ai-chat — AI 对话
**错误**：`AI input not found`
**根因**：点击 AI 按钮后未找到 `<textarea>` 输入框。
**可能原因**：
- AI 按钮选择器不准确（toolbar 中无文字，仅有 MessageSquare icon）
- AI 聊天面板未打开
- `<textarea>` 的 `placeholder` 包含"输入"或"问"的判断条件未匹配

**修复方向**：
1. 检查 AI 面板是否打开：`document.querySelector('[data-testid*="ai"], [class*="ai-chat"]')`
2. 找 textarea 时放宽匹配条件：`document.querySelector('textarea')`
3. AI 按钮可能在 toolbar 右侧，用更精确的位置选择器

---

## 其他待办

### 截图后
- [ ] 检查每张截图质量和内容
- [ ] 确认分辨率 2400×1600（2x retina）

### GitHub Pages 更新
- [ ] 更新 `site/manual.html` — 替换截图引用
- [ ] 更新 `site/zh/manual.html` — 替换中文版截图引用
- [ ] 更新 `site/index.html` + `site/zh/index.html` — 首页展示截图
- [ ] 确认所有图片路径正确

### 提交
- [ ] `git add` 所有变更
- [ ] 写 commit message
- [ ] `git push`
