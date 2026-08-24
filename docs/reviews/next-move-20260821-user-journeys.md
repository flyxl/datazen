# next-move-20260821 功能 User Journey（按真实界面绘制）

> 所有界面文案均取自 src/locales/zh-CN.ts 与实际组件代码，括号内标注对应 IPC / 组件，可直接作为测试走查脚本。
> 注：原 docs/progress/ 因文档重构移除，本文件迁移至 docs/reviews/。

## 0. 入口地图（哪些地方能进到这些功能）

主窗口
├─ 连接列表 · 连接节点右键
│   ├─ 查询历史 / 执行 SQL 文件            ← P1
│   ├─ 新建数据库 / 新建 Schema / 新建用户  ← P1 管理命令
│   └─ 置顶 / 对象过滤 / 进程列表 / 服务器状态 / 备份 / 恢复 ← 5.4 运维
└─ 连接视图 · 导航树
    ├─ 数据库节点右键：查询历史 / 执行 SQL 文件 / 新建 Schema /
    │   数据传输 / 结构对比 / 比较数据 / 备份 / 恢复 / 删除数据库
    └─ Schema 节点右键：数据传输 / 结构对比 / 比较数据 / 删除 Schema

单例子窗口：「数据同步 - DataZen」（P0）、「数据传输」8 步向导（P2 5.1）

## 1. Data Sync V1 ——「把开发库的数据差异修到生产库」

入口：导航树数据库节点右键 →「比较数据」→ 打开单例子窗口「数据同步 - DataZen」。

界面结构（EndpointsBar / OptionsBar / MappingPanel / TableListPanel / CompareSummary / DiffDetail / SqlPreview / ExecuteBar）：
1. 选源连接+数据库+Schema（PG 有「选择 schema」下拉；同连接同库被拦：「同一连接上源与目标不能选择相同的数据库」）；可点「交换源与目标」
2. 点「比较」→「正在检查表映射…」→「正在比较数据库结构…」（inspect_data_sync → compare_data_sync，keyset 分页）
3. 映射徽标：已匹配 / 不兼容 / 源未映射 / 目标未映射 / 已排除；不兼容行提示「不兼容的表需使用结构对比或数据传输。」并给「结构对比」「数据传输」两个 CTA
4. 表清单筛选 chips（全部/插入/更新/删除/无变化/不兼容）+「搜索表…」+ 复选框参与比较
5. 右侧「行差异 | SQL 预览」双 tab：行级新旧值对照、全选插入/更新/删除、分页；SQL 预览只读
6. 「执行」：含 DELETE 过两道确认（「启用删除操作？」→「确认删除…不可撤销」）；目标只读则禁执行
7. 验收闭环：再点「比较」应全部无差异

分支：异方言族选择端点即拒：「该组合请使用数据传输（Transfer）」；已配 AI 时可「解释差异」「复制报告」

## 2. Data Transfer —— 8 步向导

入口：树节点右键「数据传输」。①源/目标 → ②模式(仅结构/仅数据/结构+数据) → ③对象(预计行数) → ④映射(改目标表名/创建新表；列映射编辑器：源列↔目标列、「— 未映射 —」跳过、按名称自动匹配、清除未映射) → ⑤选项(Insert/Truncate+Insert/Drop+Create+Insert；破坏性须勾「我了解这可能破坏目标数据」；批大小、遇错即停) → ⑥预览 → ⑦执行 → ⑧结果(已插入行数)

与 Sync 本质区别：无 PK 表可搬、可建新表、跨方言走 IR（table_to_ir → build_create_table_ddl → format_literal 转义）

## 3. 查询历史 ——「找回昨天那条 SQL」

入口：连接右键「查询历史」或编辑器工具条「历史」按钮 → 左侧边栏面板

- 新增分组能力（本次被测功能）：默认「当前库」作用域（query.historyScopeCurrent），仅显示面板当前 database 的记录并显示「数据库: <名称>」标签；可切「全部」（historyScopeAll）显示按库分组头（data-testid=history-group-label）；无 database 的旧记录归「未记录库」组
- 搜索框实时过滤；条目右键「应用 SQL」回填编辑器；头部右键/垃圾桶「清空历史」
- 执行即自动记录（语句/耗时/行数/成败），SQLite 持久化重启仍在

## 4. 执行 SQL 文件

入口：连接/数据库节点右键「执行 SQL 文件」→ 对话框「执行 SQL 文件」显示「目标数据库：xxx」→ [选择 SQL 文件]（原生对话框）→ 点运行前弹确认 → 日志滚动逐条执行 → 「SQL 文件执行成功」；失败报错定位，之前语句已生效。E2E 用 webdriver-only 命令 execute_sql_file（路径直调）绕开原生对话框

## 5. 管理 DDL 命令

连接右键「新建数据库/新建 Schema/新建用户」→ 对话框确认 → admin_commands → 树刷新；删除类带确认；菜单按驱动 capability 动态显隐；权限管理在 PrivilegeView 面板

## 6. 5.4 运维四件套（开在主窗口连接视图的面板 tab 区，非弹窗）

连接节点右键
├─「服务器状态」ServerStatusView：卡片(版本|运行时间|连接数/最大连接数|活动查询|数据库大小) + MySQL/PG 实时趋势图表(QPS/会话/网络/事务) + 刷新 + 自动刷新(关闭/5秒/10秒/30秒) + 状态变量详情
├─「进程列表」ProcessListView：PID|用户|数据库|状态|查询|持续时间|客户端 + 刷新/终止（确认「终止进程 {pid}？…」）
├─「对象过滤器」ObjectFilterDialog：隐藏系统 schema 和数据库 + 表名包含/排除（通配符 *），保存后树立即生效
└─「置顶 Pin」组内排最前，菜单变「取消置顶」，持久化
「备份/恢复」→ 打开备份单例窗口且 configId/database 自动预填

底层命令：server_status_snapshot、list_processes/kill_process、estimate_table_rows（只读，PG/MySQL）

## 7. 大 DDL 警告 ——「改列类型前的最后防线」

TableStructureEditor 应用且计划含高风险语句 → 弹「确认结构变更」：
⚠ 此计划包含 {risks} 语句，可能会锁定或重写表。／估计行数约 {rows}。对大表执行在线 DDL 可能耗时较长并阻塞写入。／继续前请仔细审阅生成的 SQL。生产环境建议安排维护窗口。〔取消〕〔应用变更〕
行数估计来自 estimate_table_rows；无该命令的驱动仍按 plan risk 提示

## 8. Journey ↔ 测试用例对照

见 docs/reviews/next-move-20260821-test-plan.md 的 TS-* 用例编号（TS-SYNC-E03/E04/E05/E08、TS-XFER-E02/E04、TS-QH-E01~06、TS-SF-E01~05、TS-OPS-E01~E04）
