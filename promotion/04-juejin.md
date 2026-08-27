# 掘金文章

**标题：** DataZen：一款 AI 原生的开源数据库客户端，基于 Tauri + Rust 构建

---

## 为什么需要另一个数据库客户端？

市面上已经有 DBeaver、TablePlus、Beekeeper Studio 等优秀的数据库客户端，但它们都有一些共同的问题：

1. **AI 集成是「外挂式」的** — 需要手动复制 Schema 和错误信息到 ChatGPT，再把结果粘贴回来
2. **体积偏大** — DBeaver ~200MB+，TablePlus ~50MB，Beekeeper ~80MB
3. **缺少自动化能力** — 没有内置的工作流引擎，无法将 SQL 和 AI 操作串联起来
4. **MCP 支持缺失** — 无法作为 AI Agent 的数据库工具

DataZen 试图解决这些问题。

## DataZen 是什么？

DataZen 是一款基于 **Tauri v2 + Rust** 构建的桌面数据库客户端，核心定位是「AI 原生数据库工具」。

### 核心功能一览

#### 1. AI 原生集成

这不是一个「在侧边栏放个 ChatGPT 输入框」的实现。DataZen 的 AI 深度集成在数据库工作流中：

**自然语言 → SQL**
用自然语言描述需求，DataZen 自动获取当前数据库的 Schema 作为上下文，生成可执行的 SQL。生成的 SQL 可以直接执行，也可以插入编辑器修改。

**SQL 错误诊断**
SQL 执行失败后，AI 会结合数据库返回的错误信息和 Schema 上下文，分析问题原因并给出修正建议。

**EXPLAIN 执行计划分析**
将 EXPLAIN 结果可视化，AI 可以识别全表扫描、索引使用情况、JOIN 策略等瓶颈，给出优化建议。

**数据库感知的 AI Chat**
AI Sidebar 可以自动获取当前连接的 Schema 信息，对话中出现的 SQL 可以一键插入编辑器。

支持 OpenAI、Anthropic、DeepSeek 以及兼容协议的自定义 Endpoint。

#### 2. 轻量级

Tauri + Rust 带来 <15MB 的安装包和快速的启动速度。没有 Electron 的内存开销，没有 Java 运行时依赖。

#### 3. 数据可视化

查询结果可以直接转换成图表，支持：
- 折线图、柱状图、饼图、散点图、面积图
- 聚合和分组
- PNG / SVG 导出

不需要导出到 Excel 再做图表。

#### 4. YAML Workflow 自动化

这是 DataZen 最有特色的功能之一。你可以用 YAML 描述可复用的数据库操作流程：

```yaml
name: 每日报表
steps:
  - id: query_orders
    command: query
    connection_id: postgres-prod
    args:
      sql: "SELECT date, SUM(amount) FROM orders WHERE date >= CURRENT_DATE - 7 GROUP BY date"

  - id: query_logistics
    command: query
    connection_id: mysql-logistics
    args:
      sql: "SELECT status, COUNT(*) FROM shipments GROUP BY status"

  - id: summarize
    command: ai
    args:
      prompt: "基于以下数据生成日报摘要：\n订单数据：{{query_orders}}\n物流数据：{{query_logistics}}"
```

关键特性：
- 不同 Step 可以使用不同的数据库连接
- 支持条件分支和循环
- 可以从 UI、AI Sidebar、MCP 运行
- 可以由 AI 根据需求自动生成

#### 5. MCP 集成

DataZen 同时支持 **MCP Server** 和 **MCP Client**：

**MCP Server**
向外部 AI Agent 暴露数据库查询、Schema 浏览、EXPLAIN、Workflow 等能力。支持 stdio headless 模式，方便在 CI/CD 或 Agent 框架中使用。

**MCP Client**
将外部 MCP Server 接入 DataZen AI Chat，把文件系统搜索、Web 搜索等能力带入数据库对话。

#### 6. 可扩展 Driver 架构

DataZen 通过 **DataZen Driver API** 将应用核心与具体数据库实现解耦。

```
                     DataZen
                        │
          ┌─────────────┴─────────────┐
          │       DataZen Core        │
          │  UI · Query · AI · MCP   │
          └─────────────┬─────────────┘
                        │
                DataZen Driver API
                        │
          ┌─────────────┼─────────────┐
          │             │             │
       PostgreSQL     MySQL      外部 Driver
```

Driver 不是通过不稳定的 Rust 动态库 ABI 在运行时加载，而是在编译期被编译并链接进应用。因此 Driver 可以同时包含 Rust 数据库实现和前端页面，同时保持独立 Git 仓库。

Driver API 已作为 MIT License 的 crate 发布到 crates.io：https://crates.io/crates/datazen-driver-api

## 支持的数据库

| 数据库 | 类型 | 说明 |
|---|---|---|
| PostgreSQL | 默认 | SQL、Schema、EXPLAIN、AI 上下文 |
| MySQL / MariaDB | 默认 | SQL、Schema、EXPLAIN |
| SQLite | 默认 | 嵌入式数据库工作流 |
| Redis | 默认 | Key 浏览、命令台、Monitor、Pub/Sub |
| MongoDB | 可选 | 原生 Driver |
| ClickHouse | 可选 | 原生 Driver |
| DuckDB | 可选 | 原生 Driver |
| SQL Server | 可选 | 原生 Driver |

## 与竞品对比

| 特性 | DataZen | DBeaver | TablePlus | Beekeeper |
|---|---|---|---|---|
| AI 原生 SQL 生成 | ✅ | ❌ | ❌ | ❌ |
| MCP Server/Client | ✅ | ❌ | ❌ | ❌ |
| YAML Workflows | ✅ | ❌ | ❌ | ❌ |
| 跨数据库工作流 | ✅ | ❌ | ❌ | ❌ |
| 编译时 Driver 架构 | ✅ | ❌ | ❌ | ❌ |
| 安装体积 | <15MB | ~200MB+ | ~50MB | ~80MB |
| 开源 | GPLv3 | Apache 2.0 | 免费增值 | MIT |
| 本地优先 | ✅ | ✅ | ✅ | ✅ |

## 快速开始

### 下载安装

从 [GitHub Releases](https://github.com/flyxl/datazen/releases) 下载对应平台的安装包：
- macOS：`.dmg`（Apple Silicon / Intel）
- Windows：`.exe` / `.msi`
- Linux：`.deb` / `.rpm` / `.AppImage`

### 从源码构建

前置条件：Node.js >= 20、pnpm >= 9、Rust >= 1.77

```bash
git clone https://github.com/flyxl/datazen.git
cd datazen
pnpm install
pnpm tauri dev
```

构建指定 Driver：
```bash
# 默认 Driver 集合
pnpm tauri:build

# 全部 Driver
DATAZEN_DRIVERS=all pnpm tauri:build

# 自定义 Driver
DATAZEN_DRIVERS=postgres,mongodb pnpm tauri:build
```

## 安全设计

- 数据库凭据 AES-256-GCM 加密存储
- 主密钥在系统钥匙串（开发模式下用本地 `.key` 文件）
- 无云服务，无遥测
- SSH 隧道支持

## 链接

- GitHub：https://github.com/flyxl/datazen
- 官网：https://flyxl.github.io/datazen/
- Driver API：https://crates.io/crates/datazen-driver-api

---

DataZen 目前是 v0.1.0 早期阶段，欢迎试用、反馈和贡献！
