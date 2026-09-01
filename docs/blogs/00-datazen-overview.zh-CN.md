# DataZen 系列文章（一）：是什么？为什么做？做成什么样？怎么做？

## DataZen 是什么？

![DataZen](../../e2e/screenshots/uiux-review/02-query-chart.png)

DataZen 是一款主要面向开发者的开源 AI 数据库客户端，运行在 macOS、Windows 和 Linux 上。它基于 Tauri v2 构建：前端使用 React 18 与 TypeScript，后端使用 Rust。

除了常见的连接管理、Schema 浏览、SQL 编辑和数据查看，DataZen 还把几类通常分散在不同产品里的能力放进了同一个工作流：

- 自然语言生成 SQL、错误诊断和 EXPLAIN 分析；
- 查询结果图表化与运营看板；
- 使用 YAML 编排 Query、Command、AI、Condition 和 ForEach；
- Schema Diff、同族数据库的数据同步与异构数据迁移；
- MCP Server 与 MCP Client；
- 可独立扩展的数据库 Driver；
- 可安装的工作区页面和主题 Extension。

## 为什么要做一个数据库管理软件？

出发点是 Navicat 太贵了用不起，DBeaver 太重，而且在公司禁用掉 Navicat 之后，之前团队内部开发的通过 Navicat 连接 KIWI 的 MCP 服务在 DBeaver 上不可用了，一时间也没找到合适的开源替代，索性 vibe coding 自己开发一个。

### 还有两个很重要的真实的工作痛点：

#### - 工作中在遇到线上问题或者用户投诉时，经常需要在不同的数据库实例之间查询数据。比如金融消分业务中用户账单和额度占用不一致导致还款入账失败或者用户已经还款但是额度没有恢复。这个场景的一个典型数据查询路径是：
  - 先拿用户投诉的渠道（Aku, Lazada, TT等）订单号到交易系统查询金融订单号：
  
    ``` SQL
    SELECT installment_order_id FROM t_afi_loan_order WHERE channel_id=511 AND channel_order_id='xxx';
    ```

  - 拿到金融订单号之后需要查该订单的对应的账单：

    ``` SQL
    SELECT * FROM t_afi_installment_sup_bill_detail WHERE installment_id = xxx;
    ```

  - 查询还款配帐详情：
  
    ``` SQL
    SELECT * FROM t_afi_installment_sup_bill_detail_repayment_record WHERE sup_bill_detail_id = xxx;
    ```

  - 根据还款配帐详情看订单的每期还款金额，汇总后得到总的还款金额
  - 到额度中心查询用户还款流水，看还款之后用户额度是否有释放

    ``` SQL
    SELECT * FROM t_afi_installment_account_amount_record WHERE refer_id = ${installment_id};
    ```

  同一个库的 SQL 语句还可以用 JOIN 来一次性查，但不同库的数据就只能在上一条 SQL 执行完之后复制必要的字段填到下一条 SQL 的 WHERE 条件继续执行了。查询的 SQL 基本上都是固定的，SQL 语句收藏功能虽然可以快速找到流程对应的 SQL 组，但每次这么干的时候都觉得复制 ID 很繁琐。那有没有什么办法能解决只输入 channel_id 和 channel_order_id，而不用复制中间查询结果到下一条语句呢？

#### - 使用 superset 查询数据的时候，经常不知道 SQL 怎么写，想用 AI 帮忙但 AI 没有对应表的 schema 等上下文信息，无法生成准确的 SQL 语句，同时也担心直接使用 cursor 等 AI 工具会把数据直接上传给模型厂商，造成数据泄露。有没有好用又安全的工具能帮忙写 hive 查询语句，最好还能直接生成可视化报表？

在 vibe coding 时代，code is cheap，有需求自己搞！

## 做成什么样？

既然要做一个数据库管理软件，那就看看专业的数据库软件有哪些功能。于是调研了 TablePlus/Navicat/DBeaver，最后确定 DataZen 应该有以下功能：

- 跨平台，至少支持 MacOS 和 Windows
- 连接管理
- SQL 编辑和语法高亮
- SQL 语句执行以及结果显示
- SQL 最近执行历史、收藏 SQL 语句
- Explain SQL
- 查看表数据、表结构、索引、外键、DDL、导入/导出数据表数据/查询结果
- 编辑行数据
- 创建 table
- 查看 ER 图
- 查看 View/存储过程/函数等
- 备份/恢复数据库
- AIChat， NL2SQL AI 诊断，AI 解释 Explain 结果并提出优化建议
- workflow 自定义表单执行一组 SQL，要求能跨库执行
- 支持多种数据库 -- MySQL/PostgreSQL/Redis/Kiwi/Superset(hive)
- 数据迁移、数据同步、Schema diff
- 高性能，能流畅显示大数据集
  
大方向有了，功能点也有了，但是功能点太粗，很难落地，需要再细化功能。那又怎么细化呢？让 AI 帮忙，让 AI 出详细的 PRD 文档和设计稿。

## 怎么做？

既然需求明确了，那就开干吧。问题是怎么干？做一个只支持 MySQL 或者 Kiwi 的客户端不难，但是怎么做到同时支持多种数据库类型还能保持应用稳定和体验一致？比如 PG 和 MySQL 在语法上的差异就不小：

| 差异分类 | MySQL | PostgreSQL |
| :--- | :--- | :--- |
| **字符串引用** | 单引号 `'` 或双引号 `"` 均可 | **必须**用单引号 `'`，双引号 `"` 用于标识符（表名/列名） |
| **标识符引用** | 反引号 `` ` `` | 双引号 `"`（建议使用全小写名称避免加引号） |
| **自增主键** | `AUTO_INCREMENT` | `SERIAL` / `BIGSERIAL`，或 `GENERATED AS IDENTITY` |
| **分页查询** | `LIMIT offset, count` | `LIMIT count OFFSET offset` |
| **字符串连接** | `CONCAT(str1, str2)` 或 `||`（需开启 PIPES_AS_CONCAT） | `||` 运算符 |
| **当前时间** | `NOW()` / `CURRENT_TIMESTAMP` | `CURRENT_TIMESTAMP` / `NOW()` |
| **日期加减** | `DATE_ADD(NOW(), INTERVAL 1 DAY)` | `NOW() + INTERVAL '1 day'` |
| **NULL 比较** | 允许 `= NULL`（但结果非预期，强烈不推荐） | **只能**用 `IS NULL` / `IS NOT NULL` |
| **逻辑运算符** | 支持 `&&` / `||` 作为逻辑与/或 | **必须**使用 `AND` / `OR` 标准写法 |
| **字符串比较大小写** | 默认**不敏感**（取决于排序规则） | 严格**敏感**，忽略大小写需用 `ILIKE` 或 `LOWER()` |
| **隐式类型转换** | 较宽松，会自动转换 | 非常严格，类型不匹配直接报错 |
| **索引定义位置** | 可在 `CREATE TABLE` 内直接定义辅助索引（如 `KEY(b)`） | 表定义与索引分离，需用独立的 `CREATE INDEX` |
| **DDL 事务支持** | 大部分DDL操作**自动提交**，不可回滚 | 支持**事务性DDL**，可整体回滚 |
| **部分索引** | 不支持（或需变通实现） | 支持 `CREATE INDEX ... WHERE (条件)` |
| **JSON 支持** | 支持 JSON 类型，但函数较弱 | 支持强大的 **JSONB**（二进制JSON）类型及高效索引 |
| **行级安全策略 (RLS)** | 不支持 | 原生支持，可在表级别控制行访问权限 |
| **默认连接模型** | 线程模型，轻量级 | 进程模型（更重），生产环境需搭配连接池（如 PgBouncer） |
| **注释语法** | `--` 或 `#`（单行），`/* */`（多行） | `--`（单行），`/* */`（多行），**不支持 `#`** |

上述是驱动层面的差异，在 UI 展现层上也有差异，MySQL 的资源树是 database.table 二级结构，而 PG 是 database.schema.table 三级结构。差异更大的是 Redis，它就不是一个 SQL 数据库，DataZen 不能将其硬套到 SQL 数据库的接口和界面中。

还有，Kiwi/Superset 都是基于 HTTP 接口的驱动，而且不提供完整的 SQL 能力，比如不支持 Kiwi 只能进行查询操作，且每次查询返回的数据不能超过1000条。Superset 更狠，连 Explain 都没有，怎么让 AI 帮我写优化 SQL。

除了上面这些，做一个通用的数据库管理软件，还要考虑后续怎么支持新的数据库类型，比如 MariaDB/SQLite/MongoDB 等。

面对上述各个挑战，在架构层面要解决：

1. 怎么做到跨平台且轻量？ -- 基于 Tauri 开发，但要解决潜在的不同平台浏览器差异
2. 怎么做系统架构设计，屏蔽不同数据库驱动差异，保持前后端接口稳定？怎么做到在新增支持一种新的驱动时不用大改页面框架结构和新增一组新驱动特有的后端接口？ -- 定义通用接口 DriverTrait，让不同的数据库驱动都实现该 trait，在自己的具体实现里内部消化驱动自身的特殊逻辑
3. 新增数据库类型是在 runtime 时能直接挂载还是必须在编译期编译进主应用 -- 目前 rust 的二进制稳定性较差，不能像 c/c++ 一样稳定加载 dll 动态链接库
4. DataZen 虽然支持很多驱动，但是我作为一个用户，只使用到几种常见的数据库，不想为了用不到的数据库驱动买单怎么办？ -- 借鉴 Caddy 项目的做法，提供在线编译工具，用户选择自己需要的驱动后在线编译二进制文件。

综合上述考虑之后，最后形成了如下的系统架构：

![系统架构图](./diagrams/datazen.svg)

Driver 运行时架构：

![Driver 运行时架构](./diagrams/datazen-driver.svg)

以及 Driver 编译流程：

![Driver 编译流程](./diagrams/datazen-driver-compilation.svg)

## 最后

本文只是粗略介绍了开发 DataZen 的背景和一些决策过程，后续文章会详细说明系统架构、实现细节以及如何使用 vibe coding 在高效开发的同时保证代码质量