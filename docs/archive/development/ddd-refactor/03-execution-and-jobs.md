# Execution 与 Job 的详细边界（原 §6）

> 本文是[DDD 模块化重构总纲](../ddd-refactor-plan.md)的细节文件，承接原 §6 的全部内容；文档权威与阅读路线见总纲 §1。

## 6.1 一次执行由谁负责

以“对目标数据库执行一个命令”为例：

1. 宿主完成认证，构造可信 `RequestContext`，转换请求 DTO。
2. execution 应用服务通过 connection 的应用契约读取执行配置投影，不直接取连接仓储。
3. 应用服务校验目标、启用状态、策略、能力和 revision；调用领域规则形成可执行请求。
4. execution 私有机制在接纳点验证请求绑定，原子登记幂等键/执行 ID，取得预算和 lease，再交给 Driver。
5. 结果发布、失败/取消事实和资源释放由机制按领域状态机推进。
6. 应用服务提供执行状态和结果读取；宿主只转换协议，不直接访问 actor 或文件。

应用授权与机制绑定检查不能互相替代。不能把“授权移入 application”实施成删除 gateway 的 owner / epoch / revision 检查；也不能在 runtime 里复制另一份产品权限规则。

## 6.2 两种执行方式继续存在

**Session-bound**：使用调用方明确提供的有效 SessionHandle，适用于事务和连续操作。校验 connection、owner、epoch、配置 revision、上下文 revision 和目标；不匹配时明确拒绝。

**Target-bound**：按明确目标取得独立固定 lease，不借用树节点或 SQL 编辑器会话、不隐式改变别人的 database/schema。

不得在显式 handle 失效时悄悄回退 target-bound；这会丢失事务语义。也不能通过“先设置全局当前库再执行”模拟 target-bound。

## 6.3 Runtime 的实例与关停

每个执行节点/宿主运行实例只装配一套统一的会话、执行与物理资源账本，组织隔离是账本内的安全约束，不是让所有组织共享无 owner 状态。表浏览、SQL 编辑器、Workflow、Job Handler 引用同一服务实例；测试可创建隔离实例。

模块根的装配入口创建 runtime 并注入应用服务，只向宿主 bootstrap 返回服务和生命周期句柄。Controller 不得得到原始 runtime。构造依赖缺失时启动失败或明确禁用该可选模块，不得创建 DummyRepository。

关停顺序：停止接纳新任务 → 按既有策略 drain/请求取消 → 处理事务和未确定副作用 → 完成结果/历史发布 → 释放 lease 与预算 → 关闭仓储与宿主。失败不能记录成成功取消；无法证明回滚时保留 unknown 事实。

这份单进程机制不自动等于多 worker 协调。Web 的持久目录、claim/fencing 和全局预算仍须通过宿主实现并按[多 worker 设计](../../architecture/platform/multi-worker-coordination.md)验收。

## 6.4 等待执行，而不是在 Controller 轮询

目标应用能力为 `execution::application` 中的等待操作，内部机制实现通知和终态观察；具体函数签名在 D3 的契约测试中固定。

必须具备以下语义：

- 重复等待已经完成的 execution 返回同一终态，不重新执行。
- 先注册观察再复核状态，或使用等价原子方案，避免完成事件发生在订阅前而永远等待。
- 等待超时只说明等待超时，不伪造“SQL 已取消”；执行取消必须走显式取消用例，沿用现有业务的取消政策。
- 取消一个等待者或断开事件订阅不默认取消其他人共享的执行。
- 执行不存在、无权访问、结果过期、失败终态、等待超时分别映射既有契约，不吞错返回空结果。
- 成功返回的结果视图必须满足既有 artifact 发布承诺，不能看到终态就读取未发布完的数据。
- fake clock / barrier 覆盖完成与订阅、超时与完成、取消与清理的竞态，不用真实 sleep 证明顺序。

第一步可以把现有轮询封装在机制内以保持行为，之后再换通知实现；两步都必须有行为测试。新 deadline/错误码若改变现有 IPC 行为，需独立兼容变更，不夹在机械搬迁中。

## 6.5 Job 与具体功能解耦

jobs 拥有提交、claim、阶段推进、取消意图、历史和恢复规则。Schema Diff 等功能拥有自己的 plan 和业务 Handler，向 jobs 注册。jobs 不判断“这是哪种数据库迁移”。

Job Handler 只能通过 execution 的应用能力访问数据库。Job 资源借用与交互查询仍走同一资源账本；如调度器需要预算能力，由 bootstrap 注入执行侧的窄预算接口，不能生成第二份计数器。

长任务需要保存的是 plan/revision/checkpoint/效果事实，不是物理连接或可复用的旧 SessionHandle。重启恢复必须重新验证配置、授权和计划消费状态。
