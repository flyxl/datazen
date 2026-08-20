# DataZen Workflow 使用手册

> 本文档描述 DataZen **Workflow（工作流）** 的完整 YAML 语法与用法。  
> 事实来源：`src-tauri/src/workflow/workflows.rs`、仓库内测试 YAML、以及 `workflow_generate` Prompt。  
> **YAML 磁盘文件一律使用 snake_case 字段名**（如 `timeout_secs`、`then_steps`、`as_var`）。前端 TypeScript / IPC 响应里可能出现 camelCase，那是序列化层差异，**手写 YAML 时不要用 camelCase**。

---

## 1. 概述

Workflow 是可复用的自动化流程：把 **SQL 查询、AI 分析、条件分支、循环、数据整合** 串成 YAML，支持变量替换与**跨库**（每步可绑定不同连接）。

| 能力 | 说明 |
|------|------|
| 步骤类型 | `query` / `command` / `ai` / `condition` / `foreach` / `merge` / `transform` |
| 变量 | `string` / `number` / `connection`，可带默认值 |
| 模板 | `{{...}}` 替换 SQL、prompt、connection、output 等 |
| 跨库 | 步骤级 `connection` / `database`；多库结果可用 `merge` 并表 |
| 错误策略 | 全局或步骤级：`abort` / `skip` / `fallback` |
| 运行入口 | 连接窗口 AI 侧栏、独立 Workflow 窗口、MCP `run_workflow`、AI 对话生成 |

**不能做什么（当前实现边界）**

- 没有「脚本/HTTP」步骤类型；`merge` / `transform` 是**纯数据变换**，不支持任意代码
- `condition` 右侧比较值**不会**再解析为步骤路径（见 [§7](#7-条件表达式)）  
- `foreach` 默认最多 100 次迭代  
- MCP `run_workflow` 通常只返回最终文本输出，不含逐步详情  

---

## 2. 文件与生命周期

### 2.1 存储位置

- **主存储**：`{应用数据目录}/datazen.sqlite` 表 `workflows`（与看板共用同一库）  
- **可见性**：`user`（列表可见）或 `dashboardHidden`（看板 SQL 源，不进 Workflow 列表）  
- 执行历史：`workflow_history`（仅用户可见 Workflow 的手动/调度执行；看板刷新只写 `widget_runs`）  

编辑器提供 **可视化** 与 **YAML** 双模；YAML 仍使用 snake_case 字段。

### 2.2 加载规则

- 列表默认只返回 `visibility = user`  
- 看板引擎可通过 id 加载 hidden 定义  
- 外部改库后需刷新列表 / 重新打开编辑器  

### 2.3 校验要点

保存或解析时，至少需要：

- 非空的 `id`、`name`、`description`  
- 非空的 `steps` 数组  

`variables` 可省略（默认为空数组）。

---

## 3. 快速入门

### 3.1 在 UI 中创建

1. 打开数据库连接窗口  
2. 打开 AI 侧栏 →「工作流」标签 →「新建」  
3. 或打开独立 **Workflow 窗口**（菜单/快捷入口）  

说明：侧栏表单对 `condition` / `foreach` 支持较完整；独立窗口的简易表单可能只覆盖 `query` + `ai`，复杂步骤请直接编辑 YAML。

### 3.2 最小可运行示例

将下列内容保存为 `workflows/daily-report.yaml`（或通过 UI 保存），在已配置 AI、并选中连接的情况下执行：

```yaml
id: daily-report
name: 日报查询
description: 查询今日订单并生成摘要
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
      SELECT COUNT(*) AS total, COALESCE(SUM(amount), 0) AS revenue
      FROM orders
      WHERE order_date = '{{date}}'

  - type: ai
    id: summary
    prompt: |
      根据以下查询结果生成一份简短的中文日报摘要：
      行数: {{steps.get_orders.rows_count}}
      数据: {{steps.get_orders.result}}

output:
  format: text
  template: "{{steps.summary.result}}"
```

要点：

- 字段名是 `timeout_secs` 这类 **snake_case**，不是 `timeoutSecs`  
- query 结果用 `rows` / `rows_count`；`{{steps.xxx.result}}` 对 query 会**回退到 rows 的 JSON**（兼容写法）  
- AI 步骤的正文在 `{{steps.summary.result}}`  

---

## 4. 顶层 Schema

```yaml
id: string                 # 必填，唯一标识
name: string               # 必填，显示名称
description: string        # 必填
version: string            # 可选
author: string             # 可选
variables: []              # 可选，默认 []
steps: []                  # 必填，至少一个步骤
output:                    # 可选
  format: string           # 如 text / markdown（展示用，执行以 template 为准）
  template: string         # 可选；缺省则取最后一步结果
timeout_secs: number       # 可选，整条工作流超时，默认 300
error_handling:            # 可选，默认 strategy: abort
  strategy: abort | skip | fallback
  fallback_steps: []       # strategy 为 fallback 时使用
```

### 字段一览

| 字段 | 类型 | 必填 | 默认 | 说明 |
|------|------|------|------|------|
| `id` | string | 是 | — | 注册与 MCP `workflow_id` |
| `name` | string | 是 | — | UI 列表显示名 |
| `description` | string | 是 | — | 说明 |
| `version` | string | 否 | — | 文档用 |
| `author` | string | 否 | — | 文档用 |
| `variables` | array | 否 | `[]` | 运行时入参定义 |
| `steps` | array | 是 | — | 步骤列表（可嵌套于 condition/foreach） |
| `output` | object | 否 | 最后一步结果 | 最终输出 |
| `timeout_secs` | u64 | 否 | `300` | 全局超时（秒） |
| `error_handling` | object | 否 | `abort` | 默认错误策略 |

Serde 使用 `#[serde(tag = "type")]` 区分步骤：每个步骤对象必须有 `type: query|command|ai|condition|foreach|merge|transform`。

---

## 5. 变量（variables）

```yaml
variables:
  - name: uid
    type: string          # string | number | connection
    description: 用户 ID
    required: true        # 可选，默认 false
    default: "u001"       # 可选；运行时未传则填入
```

### 5.1 类型

| `type` | 含义 | UI 行为 |
|--------|------|---------|
| `string` | 普通字符串 | 文本输入 |
| `number` | 数值（仍以字符串形式进入模板上下文） | 数值输入 |
| `connection` | 连接配置 ID | 连接选择器；用于 `connection: "{{name}}"` |

### 5.2 必填与默认

执行开始时：

1. 用调用方传入的 `variables` JSON 填充上下文  
2. 写入内置变量（见下）  
3. 对定义中有 `default` 且尚未赋值的项填入默认值  
4. 若 `required: true` 且值缺失或为空字符串 → **直接失败**（`Required variable 'x' is missing`）  

### 5.3 内置变量（每次执行自动注入）

| 名称 | 格式示例 | 说明 |
|------|----------|------|
| `current_date` | `2026-08-07` | 本地日期 |
| `current_time` | `14:30:00` | 本地时间（时:分:秒） |
| `current_month` | `2026-08` | 本地年月 |
| `current_year` | `2026` | 本地年 |

用法：`WHERE d = '{{current_date}}'`，或 `{{current_time}}`。

---

## 6. 模板语法 `{{...}}`

引擎用正则 `\{\{([^}]+)\}\}` 替换；表达式两侧空白会 trim。

### 6.1 支持的模式

| 模式 | 示例 | 结果 |
|------|------|------|
| 入参 / 内置 | `{{uid}}`、`{{current_date}}` | 字符串 |
| 查询行字段 | `{{steps.get_orders.rows.0.order_id}}` | 第一行该列 |
| 查询行数 | `{{steps.get_orders.rows_count}}` | 如 `3` |
| 通配（IN 列表） | `{{steps.get_orders.rows.*.order_id}}` | `'a','b','c'`（每项加单引号、逗号拼接） |
| AI 文本 | `{{steps.summary.result}}` | AI 返回字符串 |
| Query 的 `result` | `{{steps.get_orders.result}}` | **无独立 result 字段**时回退为 `rows` 的 JSON 字符串 |
| 循环对象字段 | `{{order.order_id}}` | foreach 的 `as_var` |
| 循环标量 | `{{item}}` | 当前元素为简单值时 |
| 下标写法 | `{{steps.s1.result[0].name}}` | 兼容；`result`/`data` 空则回退 `rows` |

### 6.2 解析优先级（单个表达式）

1. 以 `steps.` 开头 → 取该步骤结构化结果，再按路径取值  
2. 否则若含 `.` 且左侧是 foreach 循环变量名 → 从循环对象取字段  
3. 否则若整个表达式是循环变量名 → 输出该值  
4. 否则从入参/内置变量表查找；找不到 → **空字符串**（不会报错中断模板）  

### 6.3 `rows.*.column` 细节

- 用于拼 SQL `IN (...)`  
- 每个值包在单引号中：`'ORD-1','ORD-2'`  
- **不会**做 SQL 转义；值中若含 `'` 需自行处理或改用参数化思路（当前无绑定参数 API）  
- 路径中间的 `result` / `data` 若取不到，会尝试回退到 `rows`  

### 6.4 推荐写法

```yaml
# 取单值
sql: "SELECT * FROM t WHERE id = '{{steps.s1.rows.0.id}}'"

# 取多值做 IN
sql: "SELECT * FROM t WHERE id IN ({{steps.s1.rows.*.id}})"

# 把整表塞进 AI（JSON）
prompt: |
  数据如下：
  {{steps.s1.result}}
```

---

## 7. 条件表达式

用于 `condition` 步骤的 `if` 字段。

### 7.1 处理顺序

1. 先对 `if` 字符串做 **模板替换**（`{{...}}`）  
2. 再交给条件求值器  

**推荐：比较左右两侧写步骤路径时不要包 `{{}}`**，直接写：

```yaml
if: "steps.get_orders.rows_count > 0"
```

若写成 `if: "{{steps.get_orders.rows_count}} > 0"`，模板会先变成 `3 > 0`，求值器会把左侧 `3` 当成变量名查找，通常得到空串，比较会出错。

### 7.2 支持的形式

**后缀检查**

| 写法 | 为真当 |
|------|--------|
| `steps.s1.rows_count.is_empty` | 解析值为空、`"0"`、`"null"`、`"[]"` |
| `steps.s1.rows_count.is_not_empty` | 与上相反 |

**二元比较**（运算符按此顺序匹配：`>=` `<=` `!=` `==` `>` `<`）

```yaml
if: "steps.s1.rows_count > 0"
if: "status == 'active'"
if: "steps.s1.rows_count != 0"
```

- 左侧：`resolve_expression`（可写 `steps....` 或变量名）  
- 右侧：去掉一层引号后的**字面量**；**不会**再解析 `steps.xxx`  
- 若左右都能解析为数字，则按浮点比较；否则按字符串比较  

**Truthy（无运算符）**

```yaml
if: "some_flag"
```

为真当：非空且不是 `"0"` / `"false"` / `"null"`。

### 7.3 限制

- 无 `&&` / `||` / 括号复合逻辑  
- 右侧不能写另一个步骤路径（如 `steps.a.x > steps.b.y` 无效）  

---

## 8. 步骤类型详解

所有步骤都有 `id`（字符串，同工作流内应唯一，便于 `steps.<id>...` 引用）。

---

### 8.1 `query` — SQL 查询

```yaml
- type: query
  id: get_orders
  sql: "SELECT order_id, amount FROM orders WHERE uid = '{{uid}}'"
  connection: "{{pg_conn}}"   # 可选
  database: "{{db_name}}"     # 可选，执行前 use_database
  timeout_secs: 10            # 可选，默认 30
  on_error:                   # 可选，覆盖全局
    strategy: skip
```

| 字段 | 必填 | 默认 | 说明 |
|------|------|------|------|
| `id` | 是 | — | 步骤 ID |
| `sql` | 是 | — | 支持 `{{...}}`；执行前去掉末尾 `;` |
| `connection` | 否 | 执行时传入的默认连接 | 模板解析后为 session ID 或**已保存连接的 config ID** |
| `database` | 否 | — | 非空则先 `use_database` |
| `timeout_secs` | 否 | `30` | 本步超时 |
| `on_error` | 否 | 用全局 `error_handling` | 见 [§10](#10-超时与错误处理) |

**连接解析优先级**

1. 本步 `connection`（模板解析后）  
2. 否则：`workflow_execute` / MCP 传入的 `connection_id`  
3. 都没有 → 错误：`Query step requires a database connection`  

`resolve_connection` 可接受：

- 当前已打开的会话 ID  
- 或配置库中的连接 ID（会 `get_or_connect`）  

**成功后写入上下文的结构**

```json
{
  "rows": [ { "order_id": "O1", "amount": 10 }, ... ],
  "rows_count": 2,
  "columns": [ /* 驱动返回的列元数据 */ ],
  "execution_time_ms": 12
}
```

UI 还会记录 `sql_executed`、`connection_name`（执行结果对象中，不一定进模板上下文）。

---

### 8.2 `ai` — AI 分析

```yaml
- type: ai
  id: summary
  prompt: |
    用中文总结：
    {{steps.get_orders.result}}
  timeout_secs: 60
  on_error:
    strategy: abort
```

| 字段 | 必填 | 默认 | 说明 |
|------|------|------|------|
| `id` | 是 | — | 步骤 ID |
| `prompt` | 是 | — | 支持模板；调用已配置的 AI Provider |
| `timeout_secs` | 否 | `30` | 本步超时 |
| `on_error` | 否 | 全局策略 | 同 query |

**成功结果形状**

```json
{ "result": "模型返回的文本" }
```

引用：`{{steps.summary.result}}`。  
需在设置中配置可用的 AI；温度等由执行器内部设定（实现里约 `0.3`）。

---

### 8.3 `condition` — 条件分支

```yaml
- type: condition
  id: check_orders
  if: "steps.get_orders.rows_count > 0"
  then_steps:
    - type: query
      id: get_logistics
      connection: "{{mysql_conn}}"
      sql: "SELECT * FROM logistics WHERE order_id IN ({{steps.get_orders.rows.*.order_id}})"
  else_steps:
    - type: ai
      id: no_data
      prompt: "用户 {{uid}} 没有订单，请用一句话说明。"
```

| 字段 | 必填 | 说明 |
|------|------|------|
| `id` | 是 | 步骤 ID |
| `if` | 是 | 条件表达式（YAML 键名是 `if`） |
| `then_steps` | 是 | 条件为真时执行的步骤数组 |
| `else_steps` | 否 | 为假时执行；省略则跳过 |

- 条件步本身记一条成功结果：`{ "condition": true|false }`  
- **没有**步骤级 `on_error` / `timeout_secs`（超时吃全局）  
- `then_steps` / `else_steps` 内可再嵌套任意步骤类型  

---

### 8.4 `foreach` — 循环

```yaml
- type: foreach
  id: per_order
  items: "steps.get_orders.rows"
  as_var: order
  max_iterations: 50
  steps:
    - type: query
      id: one_ship
      connection: "{{mysql_conn}}"
      sql: |
        SELECT * FROM logistics WHERE order_id = '{{order.order_id}}'
```

| 字段 | 必填 | 默认 | 说明 |
|------|------|------|------|
| `id` | 是 | — | 步骤 ID |
| `items` | 是 | — | 见下方解析规则 |
| `as_var` | 是 | — | 循环体内引用名 |
| `steps` | 是 | — | 每轮执行的子步骤 |
| `max_iterations` | 否 | `100` | 上限 |

**`items` 解析**

1. 先做模板替换  
2. 尝试把结果当 JSON 解析  
3. 失败则把字符串当作深度路径（如 `steps.get_orders.rows`）用 `resolve_deep_path` 取值  
4. 最终必须是 **JSON 数组**；否则本 foreach **跳过**（Skipped），并带错误说明  

推荐直接写路径（不必包 `{{}}`）：

```yaml
items: "steps.get_orders.rows"
```

循环体内：

- 对象元素：`{{order.field}}`  
- 标量元素：`{{order}}`（`as_var` 为 `order` 时）  

**foreach 自身结果形状**

```json
{
  "iterations_completed": 2,
  "iterations": [
    { "index": 0, "steps": [ /* 该轮子步骤执行记录 */ ] },
    { "index": 1, "steps": [ ... ] }
  ]
}
```

某轮子步骤失败且策略为 abort 时，整个工作流失败。

---

### 8.5 `merge` — 跨库/多结果并表

把多个行集**按行拼接**成一张表，常用于把多库（PG / MySQL / Redis 等）的同类结果合并后交给显示器/图表或后续 `transform`。

```yaml
- type: merge
  id: merged
  sources:
    - source: "steps.pg_orders.rows"     # 路径或 JSON 数组
      columns:                           # 可选：投影/重命名（省略=保留全部列）
        customer: customer
        amount: amount
      add: { src: "PG" }                # 注入常量列
    - source: "steps.my_orders.rows"
      add: { src: "MY" }
  columns:                              # 可选：全局输出列顺序
    - customer
    - amount
    - src
```

| 字段 | 必填 | 默认 | 说明 |
|------|------|------|------|
| `id` | 是 | — | 步骤 ID |
| `sources` | 是 | — | 有序合并分组列表 |
| `sources[].source` | 是 | — | 取值表达式（`steps.<id>.rows` 或 JSON 数组字面量） |
| `sources[].columns` | 否 | 保留原列 | 投影/重命名：`输出列名 -> 源字段路径`；写这项后仅保留所列列 |
| `sources[].add` | 否 | — | 向该组每行注入常量列 |
| `columns` | 否 | 首个出现的列顺序 | 全局输出列顺序（额外列自动附后） |
| `on_error` / `timeout_secs` | 否 | 同 query | 见 [§10](#10-超时与错误处理) |

**结果结构**：与 query 一致 `{ "rows": [{...}], "columns": [...], "rows_count": N }`，可直接被 Dashboard 的 `output` 消费。

### 8.6 `transform` — 行级计算 / 过滤 / 排序 / 截断

对单一行集做**逐行**变换：计算列、过滤、排序、offset/limit。表达式求值是**非图灵的声明式子集**（字段 / 数字 / 四则 / 比较 / `&& || !` / 括号），不嵌任何脚本引擎。

```yaml
- type: transform
  id: enriched
  from: "steps.merged.rows"            # 取值表达式
  addColumns:                          # 行级计算列
    - name: profit
      expr: "amount - cost"
    - name: ratio
      expr: "amount / total * 100"
  filter: "profit > 0 && src == 'PG'"  # 行级过滤
  sortBy: "-profit"                    # 排序；前缀 `-` 降序
  offset: 0                            # 跳过前 N 行
  limit: 100                           # 最多输出行数
```

| 字段 | 必填 | 默认 | 说明 |
|------|------|------|------|
| `id` | 是 | — | 步骤 ID |
| `from` | 是 | — | 取值表达式（`steps.<id>.rows` 或 JSON 数组字面量） |
| `addColumns` | 否 | — | 计算列：`name` 新列名 + `expr` 表达式 |
| `filter` | 否 | — | 行级过滤表达式（真值为 True） |
| `sortBy` | 否 | — | 排序列；前缀 `-` 表示降序 |
| `offset` | 否 | 0 | 跳过的前导行数 |
| `limit` | 否 | — | 最多输出的行数 |
| `on_error` / `timeout_secs` | 否 | 同 query | 见 [§10](#10-超时与错误处理) |

**表达式支持**：

| 形式 | 示例 | 结果 |
|------|------|------|
| 字段引用 | `amount`、`order.tax` | 该行对应值 |
| 字符串 | `'PG'` | 字面量 |
| 数字 / 四则 | `amount - cost`、`net / total * 100` | 数值运算（除零报错） |
| 字符串拼接 | `name + '!'` | 任一侧为字符串时按字符串拼接 |
| 比较 | `amount > 10`、`src == 'PG'` | 布尔；数值按数值、否则按字符串比较 |
| 逻辑 | `&&` `\|\|` `!` | 布尔组合 |
| 括号 | `(a + b) * 2` | 分组 |

未知字段按 `null`（数值上下文按 `0`、字符串拼接按空串）处理，不中断执行。

**与 Dashboard 的关系**：`merge` / `transform` 的输出仍为 `{ rows, columns }`，因此工作流把跨库多步结果收敛成**一张表**的推荐做法是：

```yaml
steps:
  - type: query
    id: pg_orders
    connection: "{{pg_conn}}"
    sql: "SELECT customer, amount FROM orders"
  - type: query
    id: my_orders
    connection: "{{mysql_conn}}"
    sql: "SELECT customer, amount FROM orders"
  - type: merge          # 并把 PG/MySQL 结果并成一张带 src 列的表
    id: combined
    sources:
      - source: "steps.pg_orders.rows"
        add: { src: "PG" }
      - source: "steps.my_orders.rows"
        add: { src: "MY" }
  - type: transform
    id: top
    from: "steps.combined.rows"
    sortBy: "-amount"
    limit: 10

output:
  format: json
  template: '{{steps.top.result}}'   # 一张表，可被 Dashboard 直接展示
```

`merge` / `transform` 都是**纯 Rust**（不内嵌 JS），后端引擎、`condition`/`foreach`/Dashboard 共用同一 context，因此可直接用在 GUI、MCP、定时调度等任何执行入口。

## 9. 连接与跨库

### 9.1 三层绑定

| 层级 | 来源 | 作用 |
|------|------|------|
| 执行时默认 | UI 当前连接 / IPC `connectionId` / MCP `connection_id` | 未写 `connection` 的 query 使用 |
| 步骤连接 | `connection: "{{pg_conn}}"` 或字面 ID | 覆盖默认 |
| 库名 | `database: "mydb"` | 连接后切换数据库 |

### 9.2 跨库模式

把两边连接都声明为 `type: connection` 变量，在步骤里分别引用：

```yaml
variables:
  - name: pg_conn
    type: connection
    description: 订单库 (PostgreSQL)
    required: true
  - name: mysql_conn
    type: connection
    description: 物流库 (MySQL)
    required: true
  - name: uid
    type: string
    description: 用户 ID
    required: true

steps:
  - type: query
    id: get_orders
    connection: "{{pg_conn}}"
    sql: |
      SELECT order_id, product_name, amount
      FROM test_orders
      WHERE uid = '{{uid}}'
      ORDER BY created_at DESC

  - type: condition
    id: check_orders
    if: "steps.get_orders.rows_count > 0"
    then_steps:
      - type: query
        id: get_logistics
        connection: "{{mysql_conn}}"
        sql: |
          SELECT order_id, carrier, tracking_no, status
          FROM test_logistics
          WHERE order_id IN ({{steps.get_orders.rows.*.order_id}})

output:
  format: markdown
  template: |
    ## 用户 {{uid}}

    ### 订单
    {{steps.get_orders.result}}

    ### 物流
    {{steps.get_logistics.result}}
```

仓库参考实现：`scripts/test-cross-db-workflow.yaml`。

### 9.3 独立 Workflow 窗口注意

独立窗口执行时**不一定**带默认连接；纯 query 且未写 `connection` 会失败。跨库工作流应显式声明 `connection` 变量或字面连接 ID。

---

## 10. 超时与错误处理

### 10.1 超时

| 范围 | 字段 | 默认 |
|------|------|------|
| 整条工作流 | 顶层 `timeout_secs` | `300` |
| 单个 query/ai | 步骤 `timeout_secs` | `30` |

全局超时在步骤循环中检查；超时错误类似 `Global timeout (300s) exceeded`。  
单步超时状态为 `timed_out`，并按错误策略处理。

### 10.2 策略枚举

```yaml
error_handling:
  strategy: abort          # abort | skip | fallback
  fallback_steps: []       # 仅 fallback
```

步骤级（仅 **query / ai**）：

```yaml
on_error:
  strategy: fallback
  fallback_steps:
    - type: query
      id: safe_fallback
      sql: "SELECT 'unavailable' AS error"
```

| 策略 | 行为 |
|------|------|
| `abort` | 记录失败并终止工作流（默认） |
| `skip` | 该步记为 skipped/timed_out，上下文中该步结果置 `null`，继续后续步骤 |
| `fallback` | 执行 `fallback_steps`（继承同一套默认策略与全局超时） |

`condition` / `foreach` **没有** `on_error` 字段；其内部子步骤仍各自可配。

---

## 11. 输出（output）

```yaml
output:
  format: markdown
  template: |
    摘要：{{steps.summary.result}}
```

| 情况 | 最终 `final_output` |
|------|---------------------|
| 有 `output.template` | 模板解析结果 |
| 有 `output` 但无 template | 最后一步结果：AI 优先取 `.result` 字符串，否则 pretty JSON |
| 无 `output` | 同上（最后一步） |

`format`（如 `text` / `markdown`）供展示约定；执行器以模板字符串为准。

工作流失败时仍可能带上已有 `final_output`（来自已完成步骤）和 `error` 字段。

---

## 12. 如何运行

### 12.1 连接窗口 · AI 侧栏 · 工作流

- 列表 / 新建 / 编辑 / 执行  
- 执行时通常传入**当前连接**作为默认 `connection_id`  
- 结果区可按步骤查看表格、SQL、耗时等  

### 12.2 独立 Workflow 窗口

- 运行、历史、编辑、AI 创建等标签  
- 复杂步骤建议 YAML 编辑  

### 12.3 AI 生成

对话场景使用 Prompt：`resources/prompts/{lang}/workflow_generate.txt`。  
生成结果必须是 **snake_case** YAML，并用 \`\`\`yaml 代码块包裹。

### 12.4 MCP

| Tool | 作用 |
|------|------|
| `list_workflows` | 列出可用工作流 |
| `run_workflow` | `{ workflow_id, variables?, connection_id? }` → 主要返回最终输出文本 |

另有资源：`datazen://workflows`。

### 12.5 相关 IPC（应用内）

| 命令 | 用途 |
|------|------|
| `workflow_list` / `workflow_get` | 列表与详情 |
| `workflow_save` / `workflow_delete` | 持久化 CRUD |
| `workflow_reload` | 重扫目录 |
| `workflow_execute` | 执行 |
| `workflow_history_*` | 历史 |

---

## 13. 完整示例

### 13.1 单库 + AI 摘要

见 [§3.2](#32-最小可运行示例)。

### 13.2 跨库订单物流

见 [§9.2](#92-跨库模式) 与 `scripts/test-cross-db-workflow.yaml`。

### 13.3 foreach + 条件

```yaml
id: notify-large-orders
name: 大额订单逐条说明
description: 查出大额订单，对每条用 AI 写一句话说明
timeout_secs: 120
error_handling:
  strategy: skip

variables:
  - name: min_amount
    type: number
    description: 金额阈值
    required: true
    default: 1000

steps:
  - type: query
    id: large_orders
    sql: |
      SELECT order_id, amount, customer
      FROM orders
      WHERE amount >= {{min_amount}}
      LIMIT 20

  - type: condition
    id: has_rows
    if: "steps.large_orders.rows_count.is_not_empty"
    then_steps:
      - type: foreach
        id: each_order
        items: "steps.large_orders.rows"
        as_var: order
        max_iterations: 20
        steps:
          - type: ai
            id: one_line
            prompt: |
              用一句中文描述订单 {{order.order_id}}，
              客户 {{order.customer}}，金额 {{order.amount}}。
            timeout_secs: 45
    else_steps:
      - type: ai
        id: empty_msg
        prompt: "没有金额 ≥ {{min_amount}} 的订单。"

output:
  format: text
  template: "{{steps.empty_msg.result}}{{steps.each_order.result}}"
```

说明：`else` 与 `then` 互斥，同一轮只会跑一侧；上例 `output` 模板在只有一侧存在时，另一侧引用解析为空字符串。

### 13.4 步骤级 fallback

```yaml
id: schema-or-fallback
name: 读表失败时降级
description: 主查询失败则返回提示行
error_handling:
  strategy: abort

steps:
  - type: query
    id: main
    sql: "SELECT COUNT(*) AS cnt FROM maybe_missing_table"
    on_error:
      strategy: fallback
      fallback_steps:
        - type: query
          id: fallback_row
          sql: "SELECT 0 AS cnt, 'table missing' AS note"

output:
  format: text
  template: "count={{steps.main.rows.0.cnt}}{{steps.fallback_row.rows.0.note}}"
```

注意：fallback 跑的是**另一套步骤 id**；主步骤失败时 `main` 可能没有成功结果，模板里应引用 fallback 的 id，或只依赖 fallback 步。

更稳妥的 output：

```yaml
output:
  template: "{{steps.fallback_row.result}}{{steps.main.result}}"
```

---

## 14. 排错清单

| 现象 | 可能原因 |
|------|----------|
| YAML 解析失败 / 步骤类型不对 | 用了 `thenSteps`、`timeoutSecs`、`asVar` 等 camelCase |
| `Query step requires a database connection` | 无默认连接且未写 `connection` |
| 条件总是 false | `if` 里写了 `{{steps...}} > 0`；应改为无花括号路径 |
| `IN ()` 语法错误 | `rows.*` 结果为空；先加 `condition` 判断 `rows_count` |
| foreach 被 Skipped | `items` 未解析成数组；检查路径是否为 `steps.<id>.rows` |
| Required variable missing | 未传必填变量且无 default |
| Step timed out | 增大该步或全局 `timeout_secs` |
| AI 步骤失败 | Provider 未配置或不可用 |
| 改了定义不生效 | 未刷新列表 / 未重新打开编辑器 |
| 看板刷新无历史 | 预期：`dashboardHidden` 与看板刷新不写 `workflow_history` |
| 跨库连错库 | `connection` 变量选成了错误配置 ID |

---

## 15. 附录

### 15.1 YAML snake_case ↔ 前端/IPC camelCase

| YAML（磁盘 / 执行定义） | TS / 部分 IPC 展示 |
|-------------------------|-------------------|
| `timeout_secs` | `timeoutSecs` |
| `then_steps` | `thenSteps` |
| `else_steps` | `elseSteps` |
| `as_var` | `asVar` |
| `max_iterations` | `maxIterations` |
| `error_handling` | `errorHandling` |
| `on_error` | `onError` |
| `fallback_steps` | `fallbackSteps` |
| `rows_count`（结果字段） | 同名于 JSON 结果 |

**手写与 AI 生成 YAML 时只用左列。**

### 15.2 步骤结果速查

| 步骤 | 上下文中的主要字段 |
|------|-------------------|
| query | `rows`, `rows_count`, `columns`, `execution_time_ms`；`result` 路径回退到 `rows` |
| ai | `result`（字符串） |
| condition | 执行记录含 `condition` 布尔；一般不靠模板引用 |
| foreach | `iterations_completed`, `iterations` |
| merge | `rows`, `columns`, `rows_count`（并表成一张表） |
| transform | `rows`, `columns`, `rows_count`（变换后的行集） |

### 15.3 默认值速查

| 项 | 默认 |
|----|------|
| 全局超时 | 300s |
| query/ai 超时 | 30s |
| 错误策略 | abort |
| foreach `max_iterations` | 100 |
| `variables` | `[]` |

### 15.4 相关源码与测试

| 路径 | 内容 |
|------|------|
| `src-tauri/src/workflow/workflows.rs` | 模型、执行、模板、条件 |
| `scripts/test-cross-db-workflow.yaml` | 跨库示例 |
| `src-tauri/tests/workflow_tests.rs` | 集成测试 |
| `src-tauri/resources/prompts/*/workflow_generate.txt` | AI 生成规范 |
| `e2e/specs/workflow*.ts` | UI E2E |

### 15.5 与旧版本文档的差异（纠错）

若你曾见过旧示例，请以下列为准：

- 字段必须是 **snake_case**（`then_steps` 不是 `thenSteps`）  
- 条件比较写 `steps.id.rows_count > 0`，不要把整个左侧包进 `{{ }}`  
- query 结构化结果是 `rows` / `rows_count`；`{{steps.id.result}}` 对 query 是兼容回退，不是单独字段  
- `foreach.items` 推荐 `steps.id.rows` 这种深度路径  

---

*文档版本与 DataZen Workflow 实现同步；若行为与本文冲突，以 `workflows.rs` 为准。*
