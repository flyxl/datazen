# DataZen 官网文案优化完整文档

> 版本：基于 v0.0.8 站点现状  
> 原则：**只写用户能感知的结果与约束；实现选型（Recharts / Tauri / 编译期驱动等）下沉到 GitHub README / 架构文档**  
> 范围：首页、AI、Charts、Workflow、Download（中英对照）

---

## 0. 改写原则（统一执行）

| 保留 | 删除或改写 |
|------|------------|
| 体积、启动速度、本地安全 | 技术栈名称当主卖点（Rust / Tauri / React / Recharts） |
| 能做什么、怎么帮我 | 库对比（ECharts 400KB）、协议名（Responses / Chat Completions） |
| 真实约束（需自备 API Key、平台支持） | CLI flag（`--mcp-stdio`）、编译期/运行时黑话 |
| 场景与结果 | max_tokens、token budget、compact DDL 等配置术语 |

---

## 1. 首页 Home

### 1.1 Hero

**EN（优化后）**

```text
Let AI handle the database work.
Turn data into charts.

DataZen is a desktop database client under 10 MB.
Query in natural language, diagnose errors in one click,
understand slow queries, chart every result, and automate
cross-database jobs with reusable workflows.

[Download]  [Star on GitHub]

No account. No telemetry. Your data stays on your machine.
```

**ZH（优化后）**

```text
把数据库的活儿交给 AI，
让数据自己开口说话。

DataZen 是一款不足 10 MB 的桌面数据库客户端：
自然语言写查询、一键诊断报错、读懂慢查询、
结果一键成图，再用可复用流程把跨库任务自动化。

[立即下载]  [Star on GitHub]

无需注册 · 无遥测 · 数据不出本机
```

**改动说明**
- 去掉「YAML」在首屏的强制出现（详情页再展）
- 「read EXPLAIN plans like a pro」→「understand slow queries / 读懂慢查询」更克制
- 信任句保留，表述更干净

---

### 1.2 数据条 Stats

**EN**

| 指标 | 原文 | 优化后 |
|------|------|--------|
| 体积 | `<10 MB` installer size — Rust core | **`<10 MB`** installer size |
| 语言 | 10 UI languages out of the box | **10** UI languages out of the box |
| 数据库 | 5+ databases, plugin extensible | **5+** databases, extensible |
| 价格 | 0 € free & open source forever | **Free** · open source forever |

**ZH**

| 指标 | 优化后 |
|------|--------|
| 体积 | **`<10 MB`** 安装包 |
| 语言 | **10** 种界面语言开箱即用 |
| 数据库 | **5+** 种数据库，可扩展 |
| 价格 | **永久免费** · 开源 |

删除：「Rust 内核 / Rust core」

---

### 1.3 核心能力三卡片

**EN**

```text
Core capabilities
An AI teammate for your daily database work
Not a chatbot bolted onto a grid — AI is built into how you query, fix, and explore data.

AI Assistant
Natural-language SQL, one-click error diagnosis, EXPLAIN insights,
chat with database context, and plain-language table filters.
→ Explore AI

Charts
One-click chart view with smart type recommendations.
Line, bar, pie, scatter, area — export PNG / SVG.
→ Explore charts

Workflow Automation
Reusable flows across databases: queries, AI steps, conditions, and loops.
Run from the app, chat, or external AI tools.
→ Explore Workflow
```

**ZH**

```text
核心能力
日常数据库工作的 AI 队友
不是在表格上套一个聊天框——能力长在真实查询工作流里。

AI 助手
自然语言转 SQL、一键错误诊断、执行计划解读、
带库上下文的对话、自然语言筛选表格。
→ 了解 AI

图表可视化
结果集一键切图表，智能推荐图型；
折线 / 柱状 / 饼图 / 散点 / 面积，支持导出 PNG、SVG。
→ 了解图表

Workflow 自动化
可复用的跨库流程：查询、AI、条件、循环。
可在应用内、对话里或外部 AI 工具中触发。
→ 了解 Workflow
```

**改动说明**
- Workflow 卡片首屏弱化「YAML」「MCP」专有名词，改为能力描述
- 「MCP」改为「external AI tools / 外部 AI 工具」

---

### 1.4 Why DataZen

**EN**

```text
Why DataZen
More than “yet another database client”
Lightweight, secure, and extensible — built for real developer workflows.

Under 10 MB
Tiny installers and near-instant startup — far lighter than typical heavy clients.

Local-first security
Passwords stay in the OS keychain. No cloud account. Your data never leaves your machine.

Extensible drivers
Add databases you need (including OLAP and specialty engines) without bloating the default app.

10 languages
en / zh-CN / zh-TW / ja / ko / de / es / fr / pt / ru — follows system language on first launch.
```

**ZH**

```text
为什么是 DataZen
不止是“又一个数据库客户端”
轻量、安全、可扩展——围绕真实开发工作流设计。

安装包 <10 MB
安装包小、启动快，远轻于常见重量级客户端。

本地优先安全
密码存系统钥匙串，无云端账户；数据永远不离开你的电脑。

可扩展驱动
按需增加数据库支持（含 OLAP 与专用引擎），默认安装保持精简。

10 种语言
简繁中文 / 英 / 日 / 韩 / 德 / 西 / 法 / 葡 / 俄，首次启动跟随系统语言。
```

**改动说明**
- 删除：Tauri v2 + Rust、JVM-based、Compile-time drivers、runtime extension pages
- 「Plugin ecosystem」→ 用户能理解的「可扩展驱动 / Extensible drivers」

---

### 1.5 对比表

**EN（表头优化）**

| | DataZen | Typical heavy clients | Native desktop tools |
|--|---------|----------------------|----------------------|
| Installer size | < 10 MB | ~ 100–400 MB | 100–300 MB |
| Startup time | < 1 s | 5–15 s | 1–3 s |
| Built-in AI (NL2SQL, diagnosis, EXPLAIN) | ✓ | — / paid | Partial / paid |
| Cross-database workflows | ✓ | — | — |
| Charts on any result set | ✓ | Paid add-ons | Paid editions |
| Price | Free · open source | Free / paid | $99+ / seat |

脚注保留：Comparison figures are indicative…

**ZH**

| | DataZen | 常见重量级客户端 | 原生桌面工具 |
|--|---------|------------------|--------------|
| 安装包体积 | < 10 MB | 约 100–400 MB | 100–300 MB |
| 启动耗时 | < 1 秒 | 5–15 秒 | 1–3 秒 |
| 内置 AI（NL2SQL / 诊断 / EXPLAIN） | ✓ | — / 付费 | 部分需付费 |
| 跨库工作流 | ✓ | — | — |
| 任意结果集出图 | ✓ | 付费插件 | 付费版本 |
| 价格 | 免费 · 开源 | 免费/付费混合 | $99+/席 |

**改动说明**
- 「JVM-based clients」→「Typical heavy clients / 常见重量级客户端」（用户不关心是否 JVM）
- 「YAML workflows」→「workflows / 工作流」

---

### 1.6 底部 CTA

**EN**

```text
Ready to simplify your database work?
Download free, connect in seconds, and let AI handle the tedious parts.

[Get DataZen]
```

**ZH**

```text
准备好简化数据库工作了吗？
免费下载，秒级连库，把枯燥的部分交给 AI。

[获取 DataZen]
```

（「zen your databases」双关可保留作品牌彩蛋，或改为更直白如上。）

---

## 2. AI 功能页

### 2.1 页头

**EN**

```text
AI Assistant
From writing SQL to understanding data — AI is built into querying, diagnosis, analysis, and automation.

You bring your own API key. Works with OpenAI, Anthropic, DeepSeek, and compatible endpoints.
```

**ZH**

```text
AI 助手
从写 SQL 到读懂数据——AI 贯穿查询、诊断、分析与自动化。

使用你自己的 API Key。支持 OpenAI、Anthropic、DeepSeek 及兼容接口。
```

**改动说明**：首屏诚实说明 BYOK，这是用户约束，不是实现细节。

---

### 2.2 NL2SQL

**EN**

```text
Natural language → SQL
Describe what you need. DataZen uses the current database schema as context and streams SQL you can run or edit.

• Schema of the current connection is included automatically
• Streaming output — stop anytime
• One-click run, or send to the editor
• OpenAI, Anthropic, DeepSeek, and custom endpoints
```

**ZH**

```text
自然语言 → SQL
用平常话描述需求。DataZen 自动带上当前库结构，流式生成可执行 SQL，可直接跑或再改。

• 自动使用当前连接的库结构作为上下文
• 流式输出，可随时停止
• 一键执行，或填入编辑器
• 支持 OpenAI、Anthropic、DeepSeek 与自定义接口
```

**删除**：compact DDL、token budget control

---

### 2.3 错误诊断

**EN**

```text
SQL error diagnosis
When a query fails, one click analyzes the error with schema context and suggests a fix you can apply immediately.

• Clear breakdown: original error + analysis + suggested SQL
• Apply the fix in one click
• Strong on common syntax, column, and type issues
```

**ZH**

```text
SQL 错误诊断
查询报错后一键分析：结合库结构定位原因，并给出可一键应用的修正 SQL。

• 结构化展示：原始错误 + 分析 + 修正建议
• 一键应用到编辑器
• 对常见语法、列名、类型问题效果好
```

---

### 2.4 EXPLAIN

**EN**

```text
EXPLAIN insights
See the plan, spot the bottleneck, and get plain-language optimization hints.

• Visual execution plan — scan type, rows, cost at a glance
• AI highlights slow nodes and suggests improvements
• Supported on built-in drivers such as PostgreSQL and MySQL
```

**ZH**

```text
执行计划解读
看清计划、找出瓶颈，并用通俗语言给出优化建议。

• 可视化执行计划：扫描方式、行数、代价一目了然
• AI 标出慢节点并给出改进方向
• 内置驱动（如 PostgreSQL / MySQL）支持 EXPLAIN
```

---

### 2.5 AI Chat / Smart Filters / More

**EN（节选优化）**

```text
AI Chat sidebar
Chat with full context of the current connection — schema-aware answers, not generic guesses.
Streaming replies; reasoning models supported where the provider allows.

Smart filters
Type “amount > 1000 and status is shipped” in the filter box.
AI turns it into structured filters you can still edit — validated against real column names.

More
• One-click Markdown schema documentation
• Connection troubleshooting (SSL / SSH / timeouts)
• Summarize recent query history
• Prompts follow your UI language; advanced users can customize
```

**ZH**

```text
AI 对话侧栏
对话自动带上当前连接上下文——基于真实库结构，而不是空泛回答。
流式回复；在供应商支持时可用推理模型。

智能筛选
在筛选框输入「金额大于 1000 且状态为已发货」。
AI 转为可编辑的结构化条件，并对照真实列名校验。

更多
• 一键生成库结构 Markdown 文档
• 连接失败时的 SSL / SSH / 超时排查
• 汇总近期查询历史
• 回复跟随界面语言；高级用户可自定义提示词
```

**删除**：max_tokens、reasoning content 作为主文案、协议表中的协议列

---

### 2.6 模型支持（简化表）

**EN**

| Provider | Notes |
|----------|--------|
| OpenAI | Official API or compatible gateways |
| Anthropic | Claude models |
| DeepSeek | Including reasoning-capable models |
| Custom endpoint | Any OpenAI-compatible service (e.g. local inference) |

```text
After you add an API key, available models are listed automatically — no manual model names required.
Streaming is supported.
```

**ZH**

| 服务商 | 说明 |
|--------|------|
| OpenAI | 官方 API 或兼容网关 |
| Anthropic | Claude 系列 |
| DeepSeek | 含推理类模型 |
| 自定义接口 | 任意 OpenAI 兼容服务（如本地推理） |

```text
配置 API Key 后自动拉取可用模型列表，无需手填模型名。支持流式输出。
```

**删除**：Responses / Chat Completions / Messages / Three protocols / max_tokens

---

### 2.7 MCP（能力向，非 CLI 向）

**EN**

```text
Work with external AI tools
• Expose database query, schema, EXPLAIN, and workflows to MCP-compatible clients — including headless use
• In chat, connect external MCP servers (files, search, third-party tools) to extend what AI can do
```

**ZH**

```text
与外部 AI 工具协作
• 向兼容 MCP 的客户端开放查询、库结构、EXPLAIN 与工作流——支持无界面接入
• 在对话中连接外部 MCP 服务（文件、搜索、第三方工具），扩展 AI 可用能力
```

**删除**：`--mcp-stdio`、具体 tool 名列表（放到文档）

---

## 3. Charts 功能页

### 3.1 页头

**EN**

```text
Charts
No need to export to Excel — one click turns a result table into a chart.
Field types are inferred and chart types recommended for you.
```

**ZH**

```text
图表可视化
查询结果不用导出到 Excel，点一下就从表格变成图。
字段类型自动推断、图表智能推荐。
```

---

### 3.2 能力块（保持结构，微调措辞）

**EN**

```text
One-click switch
Table ↔ chart, minimal setup
After a query, switch above the results. On first switch, recommendations run from field types and row count.

• Time + numeric → line
• Category + numeric → bar / pie
• Multiple numerics → scatter
• Single numeric → area

Five chart types
Line, bar, pie, scatter, area — axes, aggregation, and grouping from the toolbar.

• Aggregations: sum / avg / count / min / max
• Group by a second category as series
• Tooltips with names, values, percentages
• Preset palettes + dark theme

Export & reuse
PNG or SVG for docs and slides. Chart settings remembered per query; Workflow results can be charted too.
```

**ZH** 同结构中译即可，与现网接近，无需大改。

---

### 3.3 【重点删除】设计取舍整段

**删除原文（中英）：**

```text
Design trade-off: charts are built on Recharts — lightweight and consistent with React;
compared to ECharts (~400KB), only the necessary rendering capabilities are included…

设计取舍：图表基于 Recharts 构建……相比 ECharts（约 400KB）……
```

**若需保留一句“轻量”相关（可选，非必须）：**

```text
EN: Charting stays inside the app — no extra bloat, export when you need it.
ZH: 图表能力内嵌在客户端中，不额外拖慢体积与启动；需要时再导出。
```

**推荐：整段删除。** 页面其余内容已足够。

---

## 4. Workflow 功能页

### 4.1 页头

**EN**

```text
Workflow automation
Turn “query → analyze → decide” into reusable flows.
Run across databases, involve AI, and re-run anytime.
```

**ZH**

```text
Workflow 自动化
把「查数 → 分析 → 决策」固化成可复用流程。
跨库执行、可让 AI 参与、随时重跑。
```

（YAML 在正文再出现，不抢首屏。）

---

### 4.2 定义与跨库

**EN**

```text
Defined in YAML
One file describes variables, steps, and output.
Steps pass data through simple templates — query results can feed AI, and AI output can drive the next step.

• Step types: query, AI, condition, loop
• Variables with defaults and required checks
• Error strategy: stop, skip, or fallback; timeouts supported

Across databases
Each step can use a different connection.
Example: orders from PostgreSQL, logistics from MySQL, then one AI summary.
```

**ZH**

```text
用 YAML 描述
一个文件定义变量、步骤与输出。
步骤间用模板传数据：查询结果可交给 AI，AI 结论可驱动下一步。

• 步骤类型：查询、AI、条件、循环
• 变量支持默认值与必填校验
• 错误策略：中止、跳过或降级；可设超时

跨库编排
每一步可绑定不同连接。
例如：PostgreSQL 查订单，MySQL 补物流，最后 AI 汇总成一段说明。
```

示例代码可保留（目标用户能看懂），旁边说明用场景语言。

---

### 4.3 运行方式

**EN**

```text
Four ways to run
• From the connection window AI sidebar — pick a flow, fill variables, see results
• Standalone Workflow window — manage multiple flows
• From external AI tools that speak MCP — list and run your flows
• Let AI draft a runnable workflow from a plain-language request
```

**ZH**

```text
四种运行方式
• 连接窗口的 AI 侧栏：选流程、填变量、看结果
• 独立 Workflow 窗口：管理多个流程
• 通过兼容 MCP 的外部 AI 工具：列出并执行你的流程
• 用自然语言让 AI 生成一份可执行流程
```

**删除主文案中的**：`list_workflows` / `run_workflow` 代码名（可放手册）

---

### 4.4 典型场景（可保留现有）

订单+物流 / 循环巡检 / 容错降级 —— 场景向，保留。

---

### 4.5 边界说明（缩短）

**EN**

```text
Current focus is query and AI orchestration (no standalone HTTP or script steps).
Loops have sensible defaults to avoid runaway runs. Full syntax is in the user guide.
```

**ZH**

```text
当前聚焦查询与 AI 编排（暂无独立 HTTP / 脚本步骤）。
循环带有合理默认上限，避免失控。完整语法见使用手册。
```

**删除主文案中的**：具体「foreach 默认 100」等数字细节（写入手册即可；若保留也放脚注）

---

## 5. Download 页

### 5.1 标题区

**EN**

```text
Download DataZen
Free, open source, no signup. Latest release.

[macOS] Apple Silicon and Intel — .dmg
[Windows] x64 — .exe / .msi
[Linux] x86_64 — .deb / .rpm / AppImage   ← 建议补齐

All installers: github.com/flyxl/datazen/releases
```

**ZH**

```text
下载 DataZen
免费开源，无需注册。当前为最新正式包。

[macOS] Apple Silicon 与 Intel — .dmg
[Windows] x64 — .exe / .msi
[Linux] x86_64 — .deb / .rpm / AppImage

全部安装包见 GitHub Releases
```

---

### 5.2 安装说明（语气更友好）

**EN**

```text
First launch on macOS
If macOS says the app can’t be opened because it is from an unidentified developer:

1. Right-click the app → Open → Open
   or
2. In Terminal: xattr -cr /Applications/DataZen.app

We are working toward notarized builds to remove this step.
```

**ZH**

```text
macOS 首次打开
若提示无法验证开发者：

1. 右键应用 → 打开 → 仍要打开
   或
2. 终端执行：xattr -cr /Applications/DataZen.app

我们正在推进公证，以去掉该步骤。
```

---

### 5.3 可选 FAQ 短块（建议新增）

**EN**

```text
FAQ
• Do I need an account? No.
• Do I need an API key for AI? Yes — you use your own key; keys are not required for basic SQL/Redis work.
• Is my data uploaded? No. Connections and credentials stay on your machine.
• Linux? Yes — see Releases for packages.
```

**ZH**

```text
常见问题
• 需要注册吗？不需要。
• AI 需要 API Key 吗？需要，使用你自己的 Key；普通 SQL/Redis 使用不需要。
• 数据会上传吗？不会。连接与凭据仅保存在本机。
• 支持 Linux 吗？支持，见 Releases 安装包。
```

---

## 6. 全站统一替换对照表

| 原文关键词 | 优化为 | 备注 |
|------------|--------|------|
| Tauri v2 + Rust core | 删除或仅 README | 官网主文案不出现 |
| Rust core | 删除 | 保留「<10 MB」即可 |
| Compile-time drivers | 可扩展驱动 / extensible drivers | |
| runtime extension pages & themes | 可扩展主题与页面 / 删 | |
| JVM-based clients | heavy clients / 重量级客户端 | |
| Recharts / ECharts / 400KB | **整段删除** | |
| Design trade-off / 设计取舍 | **删除** | |
| YAML（首屏） | workflows / 可复用流程 | 详情页可写 YAML |
| `--mcp-stdio` | headless / 无界面接入 | |
| list_workflows / run_workflow | 外部 AI 工具中列出并运行 | 细节进文档 |
| compact DDL + token budget | 自动带上当前库结构 | |
| Responses / Chat Completions / Messages | 删除协议列 | |
| max_tokens | 删除 | |
| read EXPLAIN like a pro | understand slow queries / 读懂慢查询 | |
| zen your databases | simplify your database work（可选） | 品牌双关可保留 |

---

## 7. Meta / SEO 描述（顺带优化）

**EN**

```text
DataZen — lightweight free open-source AI database client.
Natural-language SQL, error diagnosis, EXPLAIN insights, charts, and cross-database workflows.
PostgreSQL, MySQL, MariaDB, SQLite, Redis. Under 10 MB. Local-first.
```

**ZH**

```text
DataZen — 轻量免费开源的 AI 数据库客户端。
自然语言 SQL、错误诊断、执行计划解读、图表与跨库工作流。
支持 PostgreSQL、MySQL、MariaDB、SQLite、Redis。安装包不足 10 MB，数据不出本机。
```

删除 meta 中的「Tauri-based」作为必需要点（可留在 GitHub）。

---

## 8. 实施优先级

| 优先级 | 项 | 原因 |
|--------|----|------|
| P0 | 删除 Charts「设计取舍 / Recharts」中英 | 零用户价值，最典型问题 |
| P0 | 首页去掉 Rust core / Compile-time / Tauri 主卖点表述 | 首屏信任与专业感 |
| P0 | Download 补 Linux + 统一平台表述 | 避免与产品能力不一致 |
| P1 | AI 页去掉协议名、token 术语；MCP 改能力表述 | 降低认知噪音 |
| P1 | Workflow 首屏弱化 YAML/MCP 专名；缩短边界说明 | 先讲价值再讲格式 |
| P1 | 对比表「JVM」改「重量级」 | 读者更广 |
| P2 | 新增 Download FAQ；Hero BYOK 一句 | 降低预期落差 |
| P2 | Meta 描述去 Tauri 必提 | SEO 仍面向用户收益 |

---

## 9. 不改动的内容（明确保留）

- `<10 MB`、启动时间对比  
- Local-first / 无账号 / 无遥测  
- 支持的数据库列表  
- 需自备 API Key（应说清楚）  
- 五种图表、导出 PNG/SVG  
- 跨库场景示例与 YAML 示例代码（放在 Workflow 详情）  
- GPLv3 / 开源（可弱化重复，不必删）  
- 中英文双语结构与 Manual 入口  

---

## 10. 验收标准（改完后自检）

1. 全站搜索不到：Recharts、ECharts、Tauri、Rust core、Compile-time、token budget、`--mcp-stdio`（主文案层）。  
2. 任意功能页第一屏能回答：「能帮我做什么？」而不是「用了什么技术」。  
3. 真实限制（API Key、平台、能力边界）仍然诚实可见。  
4. 中英文同一段落信息等价，无一侧残留实现黑话。

---

