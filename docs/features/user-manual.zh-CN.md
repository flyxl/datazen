# DataZen 操作手册

> 从安装到进阶的逐步操作指南，口径基于 **v0.3.0**。想先了解 DataZen 有哪些功能，请读 [产品功能总览](product-features.zh-CN.md)；本文只讲「怎么做」与「为什么这么设计」。
>
> 领域深度文档见文末索引；不含 Workspace App 相关内容。

## 目录

1. [安装与启动](#1-安装与启动)
2. [界面总览](#2-界面总览)
3. [连接管理](#3-连接管理)
4. [SQL 编辑器与查询](#4-sql-编辑器与查询)
5. [数据浏览与编辑](#5-数据浏览与编辑)
6. [导入与导出](#6-导入与导出)
7. [对象管理与 ER 图](#7-对象管理与-er-图)
8. [图表可视化](#8-图表可视化)
9. [AI 助手](#9-ai-助手)
10. [MCP 与 datazen CLI](#10-mcp-与-datazen-cli)
11. [Workflows 自动化](#11-workflows-自动化)
12. [运营看板](#12-运营看板)
13. [数据同步 · 迁移 · 结构对比](#13-数据同步--迁移--结构对比)
14. [备份与恢复](#14-备份与恢复)
15. [Redis 工具](#15-redis-工具)
16. [数据库隧道](#16-数据库隧道)
17. [设置参考](#17-设置参考)
18. [安全与隐私](#18-安全与隐私)
19. [键盘快捷键](#19-键盘快捷键)
20. [故障排查 FAQ](#20-故障排查-faq)

---

## 1. 安装与启动

1. 从[官网下载页](https://flyxl.github.io/datazen/download.html)或 [GitHub Releases](https://github.com/flyxl/datazen/releases) 下载安装包：

   | 平台 | 安装包 |
   | --- | --- |
   | macOS（Apple Silicon / Intel） | `.dmg` |
   | Windows x64 | NSIS `.exe` / 便携 `.zip` |
   | Linux x86_64 | `.deb` / `.rpm` / `.AppImage` |

2. DataZen 免费、无需注册账号；自动更新可在设置里关闭。
3. **macOS 提示「已损坏」或「无法验证开发者」**：应用尚未公证，终端执行 `xattr -cr /Applications/DataZen.app`，或右键 → 打开一次。
4. 首次启动进入欢迎向导：可「创建第一个连接」或「导入连接」；内置示例 SQLite 库（Sample Playground）开箱即用，建议先用它走一遍本文第 4、5 章。

从源码构建：Node 24 + pnpm 11 + Rust stable，`pnpm tauri:build:community --drivers=all` 可自由选择打包哪些驱动。

## 2. 界面总览

- **左侧功能栏**：连接（数据库）、Workflows、看板、设置；底部还有日志入口。
- **主内容区**：连接树（可搜索、分组、置顶）+ 多标签页查询区 + 右侧 AI 侧边栏。
- **菜单栏**：文件（新建连接、Data Sync、Schema Diff、Workflow、备份…）、视图（主题、全屏）、工具（查看日志、导入导出应用数据 / 连接配置）、帮助。
- **独立子窗口**：备份数据库、Data Sync、Data Transfer、Schema Diff、Workflow、看板——这些重流程功能在单独窗口运行，主窗口不受影响。
- 标签页支持多连接并行；窗口级故障隔离，单个页面崩溃不影响其他标签。

## 3. 连接管理

### 3.1 新建连接

1. 左侧栏「连接」→「新建连接」（或菜单 文件 → 新建连接）。
2. 在类型列表选择数据库类型（可搜索）；未随当前安装包启用的驱动会标注。
3. 按类型填写表单：服务器型填主机 / 端口 / 用户名 / 密码 / 数据库；文件型（SQLite / DuckDB）选文件；Redis 填密码与库索引。
4. 展开「高级设置」：SSL 模式（禁用 / 首选 / 必须）、**只读连接**（驱动层拒绝写入）、颜色标记、分组。
5. 「测试连接」通过后保存。连接树里可拖动分组、置顶常用连接。

### 3.2 快速建连与迁移

- 直接**粘贴连接 URL 或连接串**，DataZen 自动识别并填充表单。
- 「导入连接」支持从 **TablePlus、Navicat（.ncx）、DataGrip、DBeaver、DBX** 批量导入（连接分享对话框中操作）。
- 应用数据整体搬家：菜单 工具 → 导出 / 导入应用数据（ZIP 档案，含连接、Workflow 与历史；**加密主密钥有意不打包**，向导会提示单独备份密钥）。

## 4. SQL 编辑器与查询

### 4.1 写 SQL

- 补全基于当前库的真实表 / 列 / 函数；`Tab` 接受候选。输入 `JOIN` 时会按外键推荐关联表并自动生成 `ON` 子句。
- 悬停表名 / 列名弹出结构卡片（列清单、主键、索引、注释）；⌘/Ctrl+点击表名直接跳到该表数据页。
- 写 INSERT 时每个值前有灰色列名提示；调用函数有参数名与签名提示。
- 写错即时提示：未知表 / 未知列 / 类型不匹配 / 无 WHERE 的 UPDATE / DELETE 会画红色波浪线；选中问题区域按 `Alt+Enter` 可用快速修复（展开 `SELECT *`、生成 INSERT 模板、加/去表别名等）。
- 从 Excel 复制一列 ID 后，在编辑器按 `⌘/Ctrl+Shift+V` 自动转成 `IN (...)` 子句（数字不加引号，字符串加引号）。

### 4.2 执行

1. 按 `⌘/Ctrl+Enter`。若脚本包含多条语句，会弹出「**选择执行范围**」：仅执行当前语句（标注行号范围）或执行整段脚本；在设置 → 编辑器的**执行策略**里可固定行为，不再每次询问。
2. 选中文本后执行，只运行所选。
3. 当前语句有背景高亮；行号栏有单语句 ▶ 按钮，执行中变为加载动画。
4. 多语句执行产生多个结果标签页，显示总耗时与流式加载行数；执行中按钮变为「停止」可取消。
5. 执行 .sql 文件：菜单入口选择文件对目标库执行，破坏性内容需二次确认。

### 4.3 结果、事务与安全

- **EXPLAIN**：驱动支持时可用，展示执行计划，可点「AI 分析」解读瓶颈。
- **事务工具栏**：Begin / Commit / Rollback，未闭合事务有提醒；自动提交默认可在设置 → Behavior 切换。
- **Safe Mode**：开启时拦截无 WHERE 的 UPDATE / DELETE 及 TRUNCATE / DROP（尽力防护）；关闭后高危 SQL 执行前弹框确认。
- **参数绑定**：SQL 含 `:name` / `$1` / `?` 时下方出现参数面板，带类型提示与最近值历史（password / token 等敏感参数不入历史）。
- **查询历史**：按状态 / 时间范围 / 数据库 / Schema 四维筛选，支持批量导出、复制 SQL、一键收藏；**SQL 收藏**以 `.sql` 文件存储，可用网盘 / Git 同步。

## 5. 数据浏览与编辑

> 本节细节的权威文档：[数据浏览使用手册](data-browsing-guide.zh-CN.md)。表数据标签页与查询结果标签页的网格行为基本一致，差异在「改动如何生效」。

### 5.1 浏览

1. 连接树点表名打开表数据标签页（按页读取，默认 50 行）。
2. **筛选**：工具条「筛选」逐条添加条件（并且 / 或者），操作符按列类型给出；也可点单元格右键「按值筛选」，或用 **AI 智能筛选**输入自然语言条件。条件没填完时应用按钮不可用——不会发出错误 SQL。
3. **排序**：点表头在 升 → 降 → 无 之间循环；列头右键「排序（多列）」追加排序列（带序号徽标）；排序按表记忆，重开应用还在。
4. **行数与分页**：页脚显示 `1-50 / 总数`（精确）、`≈` 估算或「未知」；点「计数」换精确值。快速分页不可用时页脚会给出退化原因。没有跳页输入框是有意设计：快速分页靠上一页/下一页维护位置。
5. **列工具**：拖拽换位、右键「冻结到此列」、拖宽列（列宽与列序按表持久化）、右键「列统计」（行数 / 非空 / NULL / 去重 / 最小 / 最大 / 均值，服务端聚合，不看当前页）。
6. **详情面板**：`Space` 开关，查看整行与 BLOB。
7. **全库值搜索**：从搜索入口跨表找值，范围随 Schema 提示。

### 5.2 编辑单元格（表数据标签页）

1. **双击**或选中后 `Enter` / `F2` 进入编辑；编辑控件按列类型自动选择（数字 / 布尔 / 日期时间 / JSON / UUID / 枚举下拉 / 长文本 / 二进制）。
2. `Enter` 写入暂存；`Esc` 取消；`Tab` 写入并移到下一格。
3. **NULL 三态**：「设为 NULL」写 SQL NULL，「设为空字符串」写空串，新增行每列还可选「默认值（不写这一列）」——三者互不等同。
4. 输入不合法（数字格式、JSON、UUID、不允许 NULL 的列留空）会给出原因并拒绝应用。
5. 中文输入法选词的回车只确认候选词，选词结束后再按回车提交。

### 5.3 暂存 → 预览 → 提交

1. 编辑（含粘贴）先进暂存区：出现「N 项未保存更改」条，脏单元格有角标（悬停看原值 → 新值）。
2. 「预览」列出待执行语句、参数与警告；之后若又改了别的，旧预览会失效，需重新预览。
3. 「提交」确认后整批在一个事务里执行；「回滚」丢弃全部暂存。
4. 手动事务：面板上「开始事务」后，每次提交并入该事务，直到提交/回滚事务。
5. **删除行**：选中行按 `Delete`（需主键），确认后进入暂存，整行划掉可撤销；**插入行**：`⌘/Ctrl+I` 或在末尾空白双击，`⌘/Ctrl+D` 复制当前行。

### 5.4 复制与粘贴

- 复制入口：`⌘C`（TSV）、右键 → 复制选中单元格为 → TSV / CSV / JSON / 纯文本 / **HTML 表格**（粘进 Notion / Excel 变真表格）/ **Markdown 表格** / SQL INSERT / SQL UPDATE。
- 粘贴（`⌘V`）：同表数据按列名对齐；从外部（Excel 等）粘进来按位置对齐**且一定先询问**；超过 2000 格先确认；`NULL`/`\N` 写 NULL、真空单元格写空串、自增列整列跳过；粘贴同样进暂存区。
- 失败会逐格标红并说明原因，畸形数据一条都不会写入。

### 5.5 外键跳转

- 外键单元格右侧有小箭头；点击打开目标表并按外键值筛选，面包屑显示来源，`⌘/Ctrl+⌥+←/→` 在跳转历史中后退/前进（最多 32 步）。
- 复合外键整组一起跳；成员为 NULL 时不能跳（NULL 表示「没有引用」）。

### 5.6 查询结果编辑（live）

- 查询结果是单表、含全部主键列的直接投影时（无聚合 / DISTINCT / UNION / 跨表），结果网格可直接编辑。
- 修改先**暂存**（脏标记）→ **预览** → 显式**提交 / 回滚**，支持一次提交多处修改；提交后可在同一 live 结果继续编辑。
- 不可编辑时悬停只读单元格会给出原因（聚合 / 跨表 / 缺主键列 / 连接只读等）。

### 5.7 为什么不能编辑（速查）

| 提示 | 处理 |
| --- | --- |
| 安全模式下不能编辑表格单元格 | 设置 → Behavior 关闭 Safe Mode |
| 此连接为只读 / 驱动不支持写入 | 换可写连接 |
| 视图与物化视图为只读 | 换基表 |
| 此表没有主键 | 用横幅里的「用 SQL 编辑」模板：先 SELECT 定位，再写 UPDATE/DELETE |
| 自增列 / 生成列 / 二进制过大 | 该列本身不支持就地编辑 |

## 6. 导入与导出

### 6.1 导出

1. 表数据 / 查询结果右上「导出」，或表右键导出。
2. 选格式：CSV / TSV / **XLSX** / **Parquet** / JSON / Markdown / SQL INSERT / **SQL UPDATE**；不可用格式会禁用并说明原因。
3. 选范围：当前页 / 已选中行 / 整表流式导出；多表批量导出可选结构+数据、单文件或 ZIP。
4. 流式进度可**中途取消**；写入用 `.part` 中转原子保存，失败保留原文件。
5. 只依赖已加载数据的导出（无会话导出）不再要求连接处于打开状态。

### 6.2 导入

1. 目标表右键或工具条「导入」，选择 CSV / JSON / XLSX 文件。
2. 向导解析预览（RFC-4180），做**列映射**，确认后分批事务执行。
3. 出错时报告可解释的行 / 列 / 原因；CSV 公式注入在解析与写出两侧都有防护。

## 7. 对象管理与 ER 图

- **对象树**：浏览库 / Schema / 表 / 视图 / 函数 / 存储过程 / 触发器 / 序列 / 类型；右键菜单提供建库、建 Schema、建用户、授权等（随驱动能力出现）。
- **表结构**：顶部五个视图——数据 / 结构 / 索引 / 外键 / DDL；结构页可改列、索引、注释；「新建表」向导建表。
- **权限**：Privileges 视图查看与授予 / 回收权限。
- **ER 图**：工具条「ER 图」打开，按外键自动布线；支持搜索、缩放适配、折叠列，导出 PNG / SVG。
- **服务器状态**：查看实时趋势图与进程列表，可 Kill 会话（视驱动支持）。

## 8. 图表可视化

1. 执行查询后，结果区切换 表格 ⇄ 图表（可在设置开「自动切换图表」）。
2. DataZen 按结果集推荐图表类型并给理由；面板可改类型（折线 / 柱状 / 饼 / 散点 / 面积）、X/Y 轴多序列、分组、聚合、排序、配色、图例与数值标签。
3. 可以直接用自然语言改图（「改成柱状图」「按 X 升序」）。
4. 导出 PNG / SVG；点「添加到看板」转入持续监控（见第 12 章）。

## 9. AI 助手

### 9.1 配置 Provider

1. 设置 → AI Assistant：选 Provider（OpenAI / Anthropic / DeepSeek / Ollama / 自定义兼容端点）、协议、模型、API Key，点 Validate 校验。
2. 配置加密存于本地 `ai_config.enc`，不出现在日志；也可以把 Claude Code / Codex / Cursor 等本地 CLI 配成模型后端。
3. 未配置时侧边栏会引导跳转设置页。用 Ollama 时数据完全不出本机。

### 9.2 三大入口

- **NL2SQL**：编辑器 AI 按钮 / 侧栏输入自然语言需求 → 生成 SQL 流式回填编辑器 → 可应用 / 复制 / 「应用并生成图表」。生成基于当前连接的真实 Schema；`.ctx.yaml` 表组与 `@` 引用可控制上下文。
- **错误诊断**：SQL 执行报错后一键诊断，得到原因与可直接运行的修正 SQL。
- **EXPLAIN 分析**：解读执行计划，指出昂贵扫描与缺失索引建议。

### 9.3 AI Chat：Ask / Agent 双模式与记忆

- 侧栏对话（`Enter` 发送 / `Shift+Enter` 换行）；流式输出、思考过程展示、随时停止。
- 输入 `@` 引用上下文：当前连接下的表，以及 AI Context 目录中的文件（最近引用自动记忆）。
- **Ask 模式**：问答与生成 SQL，执行由你决定；**Agent 模式**：AI 自主调用只读查询与采样工具，多步完成（未绑定连接时 fail-closed）。
- **AI 记忆**：对话中让它「记住」表间关系或口径；设置 → AI 记忆可管理；检索结果自动注入对话与 NL2SQL，修正模式会过滤陈旧记忆。
- 会话按连接绑定并持久化，重开应用可继续；回复中的 SQL 代码块可直接执行。
- 每次生成可取消；响应带 `egress_summary` 出网摘要，流量只去你配置的 Provider。

## 10. MCP 与 datazen CLI

### 10.1 把数据库给 AI 编辑器用（MCP Server）

1. 设置 → MCP Server：开启，选**权限模式**——只读 / **安全写入（默认）** / 高危写入；按需开关工具、配置连接 allowlist。
2. 复制设置页提供的 Cursor / Claude Desktop 配置片段到对应 MCP 配置；或使用无头模式 `datazen --mcp-stdio` 接入 CI / Agent 管线。
3. 之后在 Claude Code / Cursor 里直接问数据问题，Agent 会调用 `list_tables` / `get_schema` / `query` 等工具读真实库。非只读调用会逐次请求确认；工具调用有审计记录。

### 10.2 接外部工具（MCP Client）

设置 → MCP Client 添加外部 MCP Server（name / command / args），其工具折入 AI 对话；Streamable HTTP 传输需显式开启。

### 10.3 datazen CLI（脚本与 Agent）

```bash
cargo build -p datazen-cli --release          # 构建，放入 PATH
datazen doctor --json                         # 体检
datazen connections --json                    # 列连接（不输出凭据）
datazen context prod-pg --database app --json # 给 LLM 的紧凑 Schema 快照
datazen query prod-pg "SELECT ..." --limit 100 --json
datazen open prod-pg --query "SELECT 1" --json # 唤起桌面打开查询
datazen agent setup                           # 安装官方 Agent Skill
```

安全模型：默认只读（写语句报 `E_READ_ONLY`）；`--allow-writes` 放行 DML；DDL 还要 `--allow-dangerous-sql`；**生产连接强制拒绝写入**；`--limit` / `--timeout` 控制行数与时长。CLI 复用桌面连接档案与密钥，本构建链接 PostgreSQL / MySQL（含兼容）/ SQLite 驱动。

## 11. Workflows 自动化

### 11.1 创建与运行

1. 入口：左侧栏「Workflows」新建，或连接窗口 AI 侧栏 →「工作流」标签；也可「AI Create」用自然语言生成草稿。
2. 编辑器提供可视化表单 + YAML 双模式；`variables`（string / number / connection）作为运行时入参。
3. 点 Run 执行：逐步显示每个 Step 的结果表、SQL 与耗时；历史可回看。

最小示例（完整语法见 [Workflow 使用指南](workflow-guide.zh-CN.md)，YAML 字段为 snake_case）：

```yaml
id: daily-report
name: 日报
description: 查询今日订单并由 AI 生成摘要
variables:
  - name: date
    type: string
    required: true
steps:
  - type: query
    id: get_orders
    sql: "SELECT count(*) AS total FROM orders WHERE order_date = '{{date}}'"
  - type: ai
    id: summary
    prompt: "订单数 {{steps.get_orders.rows.0.total}}，请写一句话日报。"
output:
  template: "{{steps.summary.result}}"
```

### 11.2 能力与边界

- 步骤类型：`query` / `command` / `ai` / `condition` / `foreach`（上限 100 次）/ `merge` / `transform`（纯数据变换，无任意代码）。
- 连接继承：Workflow 默认连接 + Step 覆盖，天然跨库；`merge` 可并表多库结果。
- 错误策略：`abort`（默认）/ `skip` / `fallback`，Step 级可覆盖；全局超时默认 300 秒。
- 运行入口：界面、MCP `run_workflow`、无头模式；支持定时调度（可按迁移档案定时执行）。

## 12. 运营看板

1. 左侧「看板」→ 新建，进入 12 列网格画布。
2. 「添加组件」：绑定已保存连接 + 监控 SQL + 图表配置，设置刷新间隔（最低 30 秒）。
3. 配置告警：指标列与聚合、比较符与阈值、冷却时间；通知渠道选桌面通知 / Webhook。
4. 工具栏支持全部刷新、暂停/恢复监控（全局，托盘同步）、导入导出看板定义（不含凭据与历史）。
5. 关闭窗口不停止后台调度；每次运行的历史快照落盘，可在组件「历史」抽屉回放。

## 13. 数据同步 · 迁移 · 结构对比

三个独立子窗口（菜单 文件 / Tools，或连接、数据库右键）打开。先按场景选择：

| 场景 | 用什么 |
| --- | --- |
| 两库结构不一致，要把结构对齐 | Schema Diff（生成依赖 DAG 排序的 DDL 部署计划） |
| 同族库（PG⇄PG、MySQL⇄MySQL）行级对齐 | Data Sync |
| 异构库搬数据、导出 SQL 文件 | Data Transfer |
| 定期备份 | 备份窗口（第 14 章） |

通用操作流：**选源与目标 → 比较生成计划 → 审阅（可勾选对象、保存 Profile）→ 预览 → 执行**；运行历史共享，可挂 Workflow 调度器定时执行。注意：

- Schema Diff 的部署计划按依赖 DAG 排序（视图 / 例程 / 触发器 / 外键不会错序）；目标侧删表有外键守卫。
- Data Sync 同族配对才可用；比较计划指纹锁定审阅内容，冲突策略需显式选择。
- Data Transfer 支持断点续传与外键写入排序；源/目标任一端非同族时界面标注「实验性」。

## 14. 备份与恢复

1. 菜单 文件 → 备份数据库（独立窗口）：选连接与数据库、改文件名模式、按方言加 dump 选项、可选 Gzip。
2. 开始后按阶段显示进度，执行日志可复制。
3. 恢复：在备份窗口切恢复模式，选择备份文件重放；目标库已有对象时要求破坏性覆盖确认。
4. 换机迁移用 工具 → 导出/导入应用数据（ZIP）；**主密钥不打包**，按向导提示单独备份。

## 15. Redis 工具

1. 建立 Redis 连接（支持库索引与 TLS；集群 / 哨兵自动感知）。
2. **键浏览**：键树或扁平模式（SCAN 游标保持位置）、按类型过滤、查 TTL、按类型查看与编辑值（String / Hash / List / Set / ZSet / Stream）、删除键。
3. **Console**：直接执行命令；危险命令会分类并 fail-closed，需要显式确认。
4. **工作台附加面**：Slowlog 慢查询、MEMORY USAGE、压缩值解码（gzip / zlib / deflate / base64）。
5. **Monitor**：实时命令流；**Pub/Sub**：订阅频道看消息。

## 16. 数据库隧道

1. 设置 → 隧道 → 新建：按需选 SSH（密码 / 私钥 / Agent，可配跳板机）、HTTP CONNECT 或 WebSocket 隧道。
2. 保存后可「测试」连通性；连接编辑时挂上对应隧道即可直达内网库，无需额外工具。
3. 不再使用时在设置页删除（有删除确认）。

## 17. 设置参考

设置页分「应用」与「集成」两组十二节：

| 节 | 常用项 |
| --- | --- |
| General | 界面语言（10 种内置；「安装语言包」可装外部包）、默认页大小、连接池大小、历史清理、自动更新与空闲提醒 |
| Appearance | 主题：亮 / 暗 / 跟随系统；可安装主题包 |
| Data Browsing | 页大小、SELECT 结果上限、自动切换图表、网格密度 / 交替行 / 行号 / 滚动条；**值呈现规则**（全局时间戳格式 / 时区 / 毫秒精度，可按列覆盖） |
| Editor | 字号、字体、**执行策略**（多语句脚本的执行范围行为） |
| Behavior | 删除确认、自动提交、**Safe Mode**、高危 SQL 弹框开关 |
| Logging | 日志级别与路径、查看日志（修改后需重启生效） |
| AI Assistant | Provider / 协议 / 模型 / Key / 校验；**AI 记忆管理**；本地 CLI Agent Provider |
| Prompt Management | 全局 / 每驱动提示词覆盖与重置 |
| MCP Server | 启用、工具开关、权限模式（只读 / 安全写入 / 高危写入）、连接 allowlist、配置片段复制 |
| MCP Client | 外部 MCP Server 增删 |
| Tunnels | SSH / HTTP CONNECT / WebSocket 隧道管理 |
| Extensions | 内置扩展点（EP）的设置 |

## 18. 安全与隐私

- 凭据 AES-256-GCM 加密；主密钥在 macOS 登录钥匙串 / Windows DPAPI（升级自动迁移，开发可用文件后端）。
- 只读连接在驱动层拒绝写入；Safe Mode 拦截高危 SQL；行删除、破坏性部署、覆盖恢复一律二次确认。
- AI 流量只去你配置的 Provider；MCP 权限模式 + 工具开关 + allowlist + 审计。
- 无账号、无遥测、无云端；日志与错误不输出凭据、敏感路径与原始连接串。

## 19. 键盘快捷键

### 全局与查询

| 快捷键 | 作用 |
| --- | --- |
| ⌘/Ctrl + N | 新建查询标签页 |
| ⌘/Ctrl + W | 关闭当前标签页 |
| ⌘/Ctrl + Enter | 执行（多语句脚本弹出执行范围选择；选中文本只执行所选） |
| ⌘/Ctrl + Shift + V | 粘贴为 IN 子句 |
| ⌘/Ctrl + Shift + F | 格式化 SQL |
| Alt + Enter | 打开快速修复（意图） |
| ⌘/Ctrl + 点击 | 多光标（编辑器内）；跳转到表（悬停卡片） |
| @ | AI 上下文选择器（Esc 关闭） |

### 数据网格

| 快捷键 | 作用 |
| --- | --- |
| 方向键 / Shift+方向键 | 移动 / 扩展选择 |
| Home / End / ⌘Home / ⌘End / PageUp / PageDown | 行内与整页导航 |
| Enter / F2 | 编辑当前单元格 |
| Esc | 取消编辑或清空选择 |
| ⌘/Ctrl + A | 全选本页行 |
| ⌘C / ⌘V | 复制选区 / 粘贴 |
| ⌘/Ctrl + I | 插入行（草稿） |
| ⌘/Ctrl + D | 复制当前行 |
| Delete | 删除选中行（需主键，确认） |
| Space | 开关行详情面板 |
| ⌘/Ctrl + ⌥ + ← / → | 外键跳转历史后退 / 前进 |

### AI 输入框

| 快捷键 | 作用 |
| --- | --- |
| Enter | 发送 |
| Shift + Enter | 换行 |

## 20. 故障排查 FAQ

**macOS 提示「已损坏」？**
应用尚未公证：`xattr -cr /Applications/DataZen.app`，或右键 → 打开一次。

**连不上数据库？**
先看连接表单的测试错误；内网库用设置 → 隧道挂 SSH / HTTP CONNECT / WebSocket；驱动未随安装包启用时换个含该驱动的构建（Basic 只含四个核心驱动）。

**编辑单元格被拒绝？**
见 5.7 速查表：最常见是 Safe Mode 开着、连接只读、表无主键、目标列是自增/生成列或二进制。

**改完数据刷新后"丢了"？**
表数据标签页的改动是**暂存**的，必须「提交」才写库；刷新不会丢暂存。查询结果编辑同样走暂存 → 提交。

**行数显示 `≈` 或「未知」？**
估算来自数据库统计（PostgreSQL 走 `pg_class`），点「计数」换精确值；数据库不提供估算时如实显示「未知」并回落精确计数。

**页脚出现「快速分页已退化」？**
快速分页（keyset）需要稳定排序键；无主键、排序键可空、跳页等场景会自动退回偏移分页并在页脚说明原因。要稳定翻页就设置一个排序。

**AI 生成的 SQL 不准？**
确认当前连接选对了库；把常用表加进 `.ctx.yaml` 表组或用 `@` 显式引用表；在 AI 记忆里固化表间关系与口径。

**MCP 调用被拒绝？**
检查权限模式（只读 / 安全写入 / 高危写入）、工具开关与连接 allowlist；非只读调用需要逐次确认。

**CLI 报 `E_READ_ONLY` / `E_PRODUCTION_WRITE`？**
CLI 默认只读；写 DML 加 `--allow-writes`，DDL 再加 `--allow-dangerous-sql`；名称/分组含 `prod` 的连接一律拒绝写入，这是有意设计。

**团队部署（Web 服务）怎么评估？**
`deploy/dev/up.sh` 一键拉起 server + dex（OIDC）+ seed 冒烟栈；生产用 `deploy/` 的 Dockerfile / docker-compose。桌面团队认证接入仍在后续版本。

## 相关文档

- 产品功能总览：[product-features.zh-CN.md](product-features.zh-CN.md)
- 数据浏览与编辑：[data-browsing-guide.zh-CN.md](data-browsing-guide.zh-CN.md)
- Workflow 指南：[workflow-guide.zh-CN.md](workflow-guide.zh-CN.md)
- 查询构建器：[query-builder.md](query-builder.md)
- Schema Diff：[schema-diff-guide.zh-CN.md](schema-diff-guide.zh-CN.md) · Data Sync：[data-sync-guide.zh-CN.md](data-sync-guide.zh-CN.md) · Data Transfer：[data-transfer-guide.zh-CN.md](data-transfer-guide.zh-CN.md)
- 运营看板：[ops-dashboard-guide.zh-CN.md](ops-dashboard-guide.zh-CN.md)
- 隧道：[tunnel-guide.zh-CN.md](tunnel-guide.zh-CN.md)
- CLI 与 Agent Skill：[ai-cli.md](ai-cli.md)
- 无会话导出：[published-result-export.md](published-result-export.md)
- 架构系列：[../blogs/README.md](../blogs/README.md)
