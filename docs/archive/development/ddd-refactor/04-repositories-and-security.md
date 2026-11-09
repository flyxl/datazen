# Repository、存储实现与安全（原 §7）

> 本文是[DDD 模块化重构总纲](../ddd-refactor-plan.md)的细节文件，承接原 §7 的全部内容；文档权威与阅读路线见总纲 §1。

## 7.1 Repository 由领域拥有

每个 Repository trait 必须明确：

- 保存什么聚合、谁拥有其 ID；
- 查询作用域、缺失与不可见语义；
- 保存是否要求 expected revision，冲突如何表达；
- 原子边界、排序/分页与只改某字段的语义；
- 返回领域模型还是明确的读投影；
- 存储失败是否可重试，以及重复调用的副作用。

不能只写一个泛型 CRUD trait 就宣称设计完成。也不机械要求“一个文件一个仓储”：多个领域仓储可以共享一个 SQLite pool 或文件写协调器，但不能共享一个暴露全部业务方法的总 Repository。

Repository 返回类型不携带 HTTP status、Tauri State、SQL transaction 对象或宿主绝对路径。领域需要跨多个存储动作的原子性时，接口表达一次业务提交，而不是让应用服务管理 SQL 连接。

仓储读投影如果直接出现在 domain trait 签名中，也由该 domain 拥有；application 再映射为公开响应，不能让 Repository 反向引用 application DTO。

## 7.2 连接目录与 Profile 的统一

目标拥有者是 `connection`，不是两个不同的“平台连接目录”：

- 目录 CRUD、分组、排序、revision 保存统一为 connection domain 的规范仓储契约。
- 建连所需的 Profile 是该配置的执行投影，供 execution 使用的读取面属于 connection application。
- AI/MCP 使用脱敏摘要，不获取全量配置、secret reference 或可解密材料。
- 现有 `ConnectionCatalog` 与 `ProfileRepository` 的方法按上述职责迁移；保留必要的读写分面，但不保留两个同义且作用域不同的 ConnectionCatalog。
- `compare_and_set`、credential revision、disabled、last-connected 更新等能力不能在“简化接口”时消失。部分更新不能变成读取旧配置后整条覆盖。

桌面初期可用 `DesktopConnectionStore` 同时适配过渡接口与新契约，但必须共享现有缓存、锁、密钥和持久化事实；Web 实现同一契约，不重新定义另一套业务语义。

## 7.3 DesktopRepository 如何拆

先把现有总对象的职责列清，再逐功能抽离，不只拆出若干 `impl DesktopRepository` 文件。

- `DesktopStorageContext`：路径策略、原子写、密钥访问与写入协调；这是宿主基础设施，不进入 modules。
- 分领域对象：Connection / Settings / Tunnel / History / NamedView / Migration Profile / AI Config 等具体实现。
- `AppDb` 可以继续共享数据库连接，但通过各领域接口提供能力；Controller 不取得裸 AppDb。
- 共享服务的内存 fake 可位于测试代码；生产的持久化 Repository 实现在 src-tauri 或 web。

首轮保持旧文件名、加密封装、SQLite schema、序列化和异常恢复行为不变。禁止为新接口创建第二份配置文件、第二套密钥或双写存储。

## 7.4 一致性与回滚

- 单聚合写入由 Repository 原子完成，CAS 的比较与写入处于同一锁/数据库事务中。
- 配置与 revision/凭据引用共同提交时，必须延续既有一致性与失败恢复承诺，不能先更新内存再忽略落盘失败。
- 跨文件没有天然数据库事务；在改造前用故障注入锁定现有行为。若需新增日志或恢复协议，作为独立持久化变更评审，不靠口头“最终一致”。
- 跨领域动作由应用层编排，定义提交点与失败后的补偿/重试；不引入全局 Repository 隐式事务。
- 需要变更存储格式时必须有旧版本读取、升级失败、降级/恢复演练。纯代码搬迁的默认回滚是 revert，不需要用户数据迁移。

## 7.5 身份、授权和错误

Web middleware 验证登录后构造 RequestContext；Desktop/MCP Adapter 从可信宿主身份构造。客户端传来的 organizationId/principalId/角色字段不能成为授权事实。

应用服务负责资源级业务授权，仓储查询始终携带可信组织作用域，执行接纳点保留 owner/epoch/revision 与策略有效性检查。跨组织资源按现有不可见契约处理，不能泄漏“存在但无权”；读取空值也不能被解释为可创建覆盖。

凭据只在实际建连所需的内部适配边界解析，不能进入公开摘要、日志、事件或测试失败文本。按职责迁移错误类型，不修改既有 IPC 错误语义；SQL 执行终态错误与请求接纳错误继续区分。

Agent 不读取 `.env` / `.env.test` 内容；测试程序正常读取环境是允许的。敏感测试的日志先留系统临时目录，只摘取脱敏后的结构性结论。
