# E2E 测试已知问题与稳定性缺陷清单 (E2E Bugs & Flaky Issues)

> 记录在单实例与多实例并行跑 E2E 测试（尤其是 Smoke 测试集）时发现的失败问题、错误现象、触发路径与排查修复建议。

---

## 1. 概览统计

在最新一次执行 `pnpm e2e:skip-build -- --suite smoke`（全量 31 个 smoke spec 串行）和 `pnpm e2e:parallel:smoke`（3 进程并行）中，共定位出 **9 个存在稳定性或定位器缺陷的 Spec 文件**。

---

## 2. 缺陷明细清单

### 1) `e2e/specs/settings.ts` - 设置分区导航按钮不可见
- **用例**: `Settings (SS-001~SS-006).SS-NAV-001: 设置侧栏应能进入行为 / 日志 / AI / MCP / 扩展分区`
- **错误信息**:
  ```
  Error: element ("button*=MCP Server") still not displayed after 8000ms
  ```
- **复现原因**:
  - 设置页面左侧导航在默认测试窗口大小下可能存在垂直溢出，需要滚动才能看到底部的 MCP Server / 扩展等入口；
  - 或者是定位器使用了文案模糊匹配 `button*=MCP Server`，在界面中该按钮被子组件或其他 DOM 遮挡或处于折叠状态。
- **修复建议**:
  - 在点击目标设置分栏前，先执行 `scrollIntoView()` 确保元素在可视区域内；
  - 改用稳定的 `data-testid`（如 `[data-testid="settings-nav-mcp"]`）进行定位。

---

### 2) `e2e/specs/table-data.ts` - 空表状态显示超时
- **用例**: `表数据视图 (TD-001~TD-008).TC-TABLE-009: 空表应显示空状态而非崩溃`
- **错误信息**:
  ```
  等待表 "_e2e_empty_table" 在侧边栏出现或点击超时
  ```
- **复现原因**:
  - 前序测试动态执行 DDL 创建了 `_e2e_empty_table`，但由于前端 Schema 缓存机制，侧边栏未能自动刷新出新创建的表；
  - `waitForTableInSidebar` 在重试时可能未能成功展开当前的 worker schema 节点或未触发 schema 刷新按钮。
- **修复建议**:
  - 在创建表后，显式通过 IPC 或 UI 刷新按钮触发一次 Schema Tree 重新加载；
  - 确保 `waitForTableInSidebar` 正确传递当前 worker 隔离的 schema 参数。

---

### 3) `e2e/specs/table-filter.ts` - 筛选面板初始化等待超时
- **用例**: `"before all" hook: wrappedHook for "打开筛选面板应显示添加与 Apply (TF-001)"`
- **错误信息**:
  ```
  等待表 "_e2e_filter_test" 并在侧边栏点击打开超时
  ```
- **复现原因**:
  - `before all` 中执行 DDL 创建了表 `_e2e_filter_test`，随后立即尝试在侧边栏查找并点击该表；
  - 数据库刚完成 DDL，前端未收到 Schema 变更通知，导航树仍显示旧列表，导致一直找不到新表节点超时。
- **修复建议**:
  - 在创建表后，使用 `refreshSchemaTree()` 显式刷新侧边栏节点；
  - 增加重试展开机制。

---

### 4) `e2e/specs/table-edit.ts` - 测试表在工作区打开超时
- **用例**: `表数据编辑 (DE-002~DE-005).应能在侧边栏看到测试表并打开数据标签`
- **错误信息**:
  ```
  Error: waitUntil condition failed with the following reason: 等待表 "_e2e_edit_test" 工作区打开超时
  ```
- **复现原因**:
  - 双击侧边栏表节点时，由于 macOS Webkit 渲染性能或焦点未就绪，双击事件可能被判定为两次单击或未触发打开 tab；
  - 或者表工作区 DOM 节点渲染延时大于预设的 waitUntil 超时时间。
- **修复建议**:
  - 优化 `clickTableInSidebar` 实现，增加右键点击选择“打开数据”或更具弹性的双击重试逻辑；
  - 适度延长工作区挂载等待时间（如从 10s 调整至 15s）。

---

### 5) `e2e/specs/client-parity.ts` - 初次点击表超时
- **用例**: `Client parity P0–P2.table filter editor opens AND/OR controls`
- **错误信息**:
  ```
  Error: 等待第一个表工作区打开超时 (at clickFirstTable)
  ```
- **复现原因**:
  - 依赖 `clickFirstTable()`，该辅助方法假设连接已经自动展开且表类别已展示出具体表项；
  - 串行连续运行多个用例后，当前活动连接或 Schema 树状态处于收起状态，`clickFirstTable` 无法命中任何有效的表 DOM。
- **修复建议**:
  - 在 `clickFirstTable` 执行前，强制执行 `expandConnectedConnectionInNavigator()` 和 `expandSchemaTableCategory()`，确保树节点已展开。

---

### 6) `e2e/specs/object-browser.ts` - 例程列表右键菜单超时
- **用例**: `对象浏览器与权限 (OBJ/PRV).例程列表右键应打开 Web 菜单 (OBJ-003)`
- **错误信息**:
  ```
  等待例程列表项右键上下文菜单可见超时
  ```
- **复现原因**:
  - PostgreSQL 系统例程/存储过程加载依赖 `list_objects` driver command，在串行运行后期或数据库连接池繁忙时，获取 routines 列表有网络/查询延迟；
  - WebDriver 模拟 `contextClick()` 时，如果目标元素发生 DOM 重排（stale element），右键事件未能冒泡到自定义菜单监听器。
- **修复建议**:
  - 在触发右键前先等待例程列表项状态稳定（`isDisplayed` 且文本不为空）；
  - 增加针对 stale element 的重试保护机制。

---

### 7) `e2e/specs/er-diagram.ts` - ER 图面板未按时展示
- **用例**: `ER 图功能 E2E 测试 (ER-001~ER-008).ER-003: 点击 ER 入口应打开 ER 面板`
- **错误信息**:
  ```
  Error: element ("[data-testid="er-diagram-view"]") still not displayed after 15000ms
  cmd="get_er_data" error=DB session '...' not found
  ```
- **复现原因**:
  - 错误日志中出现核心后端报错：
    `cmd="get_er_data" error=DB session '...' not found`；
  - 用例在前置逻辑中对数据库连接执行了重连或生命周期切换，但 ER 图面板加载时仍然持有了已经销毁的旧 `db_session_id`，导致请求被后端拦截，前端卡在加载态。
- **修复建议**:
  - 检查 ER 面板在切换数据库或重新连接时更新 `dbSessionId` 的响应式逻辑，确保不会传递已释放的会话 ID；
  - 在 E2E 中确保连接处于稳定已连接状态再触发 ER 图入口。

---

### 8) `e2e/specs/app-data-backup.ts` - 导出/导入文案断言不匹配
- **用例**: `App Data Backup (ADB-001~ADB-005).ADB-007: export label locale keys match current UI language`
- **错误信息**:
  ```
  主页导出/导入入口文案与预期 locale 字典不匹配
  ```
- **复现原因**:
  - 最近的 i18n 翻译更新或重构调整了备份恢复相关的展示文案/键值，而测试用例中硬编码了旧的文案或错误的 locale 预期。
- **修复建议**:
  - 同步更新测试中的文案断言，使用当前 `src/locales/` 中实际定义的文本。

---

### 9) `e2e/specs/drag-drop-groups.ts` - HTML5 拖拽 dropEffect 判定为 none
- **用例**: `连接分组中的 HTML5 拖拽排序 (DND).DND-009: 通过 HTML5 拖拽将连接 A 拖到 GroupB 头部，连接 A 归类到 GroupB`
- **错误信息**:
  ```
  Expected: "move"
  Received: "none"
  ```
- **复现原因**:
  - WebDriver 无法完美触发 HTML5 原生 `DragEvent` 的全部生命周期（`dragstart -> dragover -> drop -> dragend`）；
  - 测试中自定义派发 `DataTransfer` 事件时，拖拽目标元素可能由于窗口大小或容器滚动未完全处于可交互坐标范围内，导致浏览器的默认行为返回了 `none`。
- **修复建议**:
  - 使用更健壮的合成 HTML5 拖拽辅助函数（通过 `browser.execute` 完整注入并派发包含坐标与 types 的 `DragEvent` 序列）。

---

## 3. 改进计划与后续任务

1. **统一导航树辅助方法**:
   - 对 `waitForTableInSidebar` / `clickTableInSidebar` / `clickFirstTable` 增加强制刷新 Schema 与自动重试展开机制。
2. **修复会话销毁时序**:
   - 修复 `er-diagram` 中使用失效 `db_session_id` 调用 `get_er_data` 的问题。
3. **滚动进可视区支持**:
   - 在 `settings.ts` 等长页面中统一加入 `scrollIntoView()` 兜底。
