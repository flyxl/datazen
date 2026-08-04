# DataZen Workflow 使用指南

Workflow（工作流）是 DataZen 的自动化功能，允许你将多个 SQL 查询、AI 分析步骤串联成可复用的执行流程。工作流以 YAML 文件存储，支持变量、条件分支、循环和跨库查询。

## 快速入门

### 在 UI 中创建

1. 打开数据库连接窗口
2. 点击右上角的消息图标打开 AI 侧边栏
3. 切换到「工作流」标签
4. 点击「新建工作流」

### 通过 YAML 文件创建

将 `.yaml` 文件放入工作流存储目录（可在 UI 中查看路径），DataZen 启动时会自动加载。

```yaml
id: daily-report
name: 日报查询
description: 查询今日订单数据并生成摘要
version: "1.0"

variables:
  - name: date
    type: string
    description: 查询日期 (YYYY-MM-DD)
    required: true
    default: "2024-01-01"

steps:
  - type: query
    id: get_orders
    sql: |
      SELECT COUNT(*) as total, SUM(amount) as revenue
      FROM orders
      WHERE order_date = '{{date}}'

  - type: ai
    id: summary
    prompt: |
      根据以下查询结果生成一份简短的中文日报摘要：
      {{steps.get_orders.result}}

output:
  format: text
  template: "{{steps.summary.result}}"
```

## 步骤类型

### query — SQL 查询

执行一条 SQL 语句并将结果存入上下文。

```yaml
- type: query
  id: user_count
  sql: SELECT COUNT(*) as cnt FROM users WHERE status = 'active'
  connection: my-pg-conn  # 可选，不指定则使用当前连接
```

查询结果通过 `{{steps.user_count.result}}` 引用。

### ai — AI 分析

调用 AI 模型处理数据。需要在设置中配置好 AI Provider。

```yaml
- type: ai
  id: analyze
  prompt: |
    分析以下用户数据趋势：
    {{steps.user_stats.result}}
    给出 3 条优化建议。
```

### condition — 条件分支

根据条件表达式选择不同的执行路径。

```yaml
- type: condition
  id: check_threshold
  if: "{{steps.user_count.result[0].cnt}} > 1000"
  thenSteps:
    - type: query
      id: detail_query
      sql: SELECT * FROM users LIMIT 100
  elseSteps:
    - type: query
      id: full_query
      sql: SELECT * FROM users
```

### foreach — 循环

遍历数组中的每个元素执行子步骤。

```yaml
- type: foreach
  id: process_tables
  items: "{{steps.get_tables.result}}"
  asVar: table
  maxIterations: 50
  steps:
    - type: query
      id: count_rows
      sql: "SELECT COUNT(*) FROM {{table.name}}"
```

## 变量系统

### 定义变量

```yaml
variables:
  - name: db_name
    type: string
    description: 数据库名称
    required: true

  - name: limit
    type: number
    description: 结果条数限制
    default: 100

  - name: target_conn
    type: connection    # 特殊类型：连接选择器
    description: 目标数据库连接
    required: true
```

变量类型：
- `string` — 文本输入
- `number` — 数值输入
- `connection` — 数据库连接选择器（在 UI 中显示为下拉框）

### 引用变量

使用 `{{variable_name}}` 语法引用变量值：

```yaml
sql: "SELECT * FROM {{db_name}}.users LIMIT {{limit}}"
```

### 引用步骤结果

查询步骤的结果通过 `rows` 访问：

```yaml
# 访问第一行的 name 字段
sql: "SELECT * FROM {{steps.get_tables.rows[0].name}}"

# 通配符：展开所有行的字段用于 IN 子句
sql: "SELECT * FROM orders WHERE id IN ({{steps.get_ids.rows.*.id}})"

# 行数
if: "steps.get_data.rows_count > 0"
```

AI 步骤的结果通过 `result` 访问：

```yaml
prompt: "分析数据：{{steps.get_data.result}}"
```

## 错误处理

### 全局错误策略

```yaml
errorHandling:
  strategy: abort  # abort | skip | fallback
```

- `abort` — 遇错立即停止（默认）
- `skip` — 跳过失败步骤继续执行
- `fallback` — 执行备选步骤

### 步骤级错误处理

```yaml
steps:
  - type: query
    id: risky_query
    sql: SELECT * FROM maybe_missing_table
    timeoutSecs: 30
    onError:
      strategy: fallback
      fallbackSteps:
        - type: query
          id: safe_fallback
          sql: SELECT 'table not found' as error
```

## 跨库查询

工作流支持在不同数据库连接间执行查询。通过 `connection` 变量类型指定目标连接：

```yaml
id: cross-db-sync
name: 跨库数据同步检查

variables:
  - name: source
    type: connection
    description: 源数据库
    required: true
  - name: target
    type: connection
    description: 目标数据库
    required: true

steps:
  - type: query
    id: source_count
    sql: SELECT COUNT(*) as cnt FROM orders
    connection: "{{source}}"

  - type: query
    id: target_count
    sql: SELECT COUNT(*) as cnt FROM orders
    connection: "{{target}}"

  - type: ai
    id: compare
    prompt: |
      源库订单数: {{steps.source_count.result[0].cnt}}
      目标库订单数: {{steps.target_count.result[0].cnt}}
      判断数据是否同步，差异是否在可接受范围内。
```

## 执行记录

每次执行工作流的结果会自动保存到历史记录中，包括：
- 每个步骤的执行状态（成功/失败/跳过/超时）
- 执行时间
- 错误信息（如有）
- 最终输出

可在 UI 的「执行记录」标签页中查看历史执行详情。

## MCP 集成

工作流通过 MCP (Model Context Protocol) 暴露为工具，可被外部 AI 代理调用：

- `list_workflows` — 列出所有可用工作流
- `run_workflow` — 执行指定工作流

## 超时控制

```yaml
# 全局超时（秒）
timeoutSecs: 300

steps:
  - type: query
    id: slow_query
    sql: SELECT * FROM huge_table
    timeoutSecs: 60  # 步骤级超时覆盖全局设置
```

## 完整示例

### 数据库健康检查

```yaml
id: db-health-check
name: 数据库健康检查
description: 检查数据库连接状态、表空间和慢查询
version: "1.0"

variables:
  - name: slow_threshold
    type: number
    description: 慢查询阈值（秒）
    default: 5

steps:
  - type: query
    id: connection_count
    sql: SHOW STATUS LIKE 'Threads_connected'

  - type: query
    id: slow_queries
    sql: |
      SELECT * FROM information_schema.PROCESSLIST
      WHERE TIME > {{slow_threshold}}
      ORDER BY TIME DESC
      LIMIT 10

  - type: query
    id: table_sizes
    sql: |
      SELECT table_name, 
             ROUND(data_length/1024/1024, 2) AS size_mb
      FROM information_schema.tables
      WHERE table_schema = DATABASE()
      ORDER BY data_length DESC
      LIMIT 10

  - type: ai
    id: report
    prompt: |
      生成数据库健康报告：
      
      当前连接数：{{steps.connection_count.result}}
      慢查询（超过 {{slow_threshold}} 秒）：{{steps.slow_queries.result}}
      前 10 大表：{{steps.table_sizes.result}}
      
      请给出健康评估和优化建议。

output:
  format: text
  template: "{{steps.report.result}}"

timeoutSecs: 120
errorHandling:
  strategy: skip
```
