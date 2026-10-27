# 旧 crate 与接口的去向（原 §5）

> 本文是[DDD 模块化重构总纲](../ddd-refactor-plan.md)的细节文件，承接原 §5 的全部内容；文档权威与阅读路线见总纲 §1。

## 5.1 platform-api：按拥有者拆散，最终退役

它过去承载 application 和 runtime 共同使用的接口，帮助避免跨 crate 环；这不是接口必须集中存放的理由。目标分配如下：

- `context` 中可信身份、组织/主体标识、请求关联信息 → `shared`。若既有 OwnerRef 必须引用 Job 等功能标识，只把形成归属契约所需的最小 ID/引用值放进 shared，领域再导出同一类型；shared 不 import 功能模块。不会因为一个 DTO 被两处使用就进入 shared。
- `ConnectionId`、配置 revision、初始 `NamespaceTarget`、Profile 模型、连接目录和 Profile 仓储契约 → `connection::domain`；必要的 shared 归属 ID 按上一条保持唯一定义。
- 连接列表响应、供执行使用的 Profile 只读投影 → `connection::application`。
- `DbSessionId`、`SessionHandle`、`ExecutionId`、`ExecutionTarget`、状态和生命周期不变量 → `execution::domain`；提交请求、回执视图等服务边界 DTO → `execution::application`。ExecutionTarget 组合连接侧 namespace，而不是让 connection 反向引用 execution；表对象目标由 schema 定义并在应用边界转换。
- `JobRepository`、Job 状态与持久化模型 → `jobs::domain`；Handler 注册与外部调度依赖 → `jobs::application`。
- 结果 artifact 的归属、发布状态和仓储接口 → `execution::domain`；文件/对象存储实现 → 对应宿主。按字节读取与按结果块读取继续是不同契约。
- Settings / Tunnel / KnownHosts → `settings` / `tunnel` 的 domain；语义不变，包括既有同步回调和失败处理约定。
- 授权、凭据和网络能力接口 → 实际消费者的 application；共享身份词汇保留在 shared，组织授权规则不凭空丢给存储实现。
- `MigrationProfileReader` → 迁移消费者所需的窄投影接口，签名靠已有领域信息表达；三类 Profile 仓储分别由三件套拥有，不能扩大成第四个总仓储。
- `ApiError` / `PortError` → 先保持原有序列化和错误码，仅移动定义。应用错误按功能逐步归属；公共错误信封可保留在 shared，但不能包含具体业务实体。HTTP 状态映射、IPC 脱敏仍在宿主。

每个类型只有一个定义位置；旧 import 路径可以短期再导出同一个类型。不得复制 DTO 再靠两次 JSON 转换假装兼容。DTO 的 `serde` 字段、缺省值、枚举取值和错误字符串兼容由测试决定，不能随意“顺便优化”。

## 5.2 application：移入各功能，不换名保留总层

- `SettingsService` → `settings::application`。
- 连接管理用例 → `connection::application`。
- 会话/执行用例与目标处理 → `execution::application`，目标不变量进入对应 domain。
- Dashboard / 命名视图等仓储访问编排 → 对应功能 application。
- 前端事件桥与可选能力 → 消费者的依赖接口和宿主装配，不成为所有业务的共同前置依赖。
- `ApplicationServices` → 过渡兼容门面，最终退出；宿主不再必须为不使用的功能提供一整套仓储。

## 5.3 runtime / kernel：能力保留，位置改变

- 会话 actor、执行登记、lease、队列、等待和空闲回收 → `execution::runtime`。
- 会话/事务状态机与执行安全规则 → `execution::domain`。
- `RuntimeConnectionUseCases` 的请求编排 → `execution::application`，不是整个 struct 原样搬到 runtime 目录。
- Job 状态/仓储 → `jobs::domain`；调度实现 → `jobs::engine`。
- Schema 元数据缓存 → `schema::cache`；缓存键、失效条件和读写授权保持原语义。
- SQL Guard 的纯规则 → `execution::domain`；其配置读取和业务接线 → application。
- 物理资源预算账本 → execution 的机制实现；Job 预算协调通过注入契约使用同一账本，不能每个模块各记一份资源使用量。
- Driver registry → execution 的驱动接入机制，宿主通过启动装配提供注册来源；具体驱动仍独立。
- 指纹等纯算法只在确有多个独立消费者时进入 shared；不能重建一个大 kernel。

独立 `runtime`、`kernel` crate 在调用者迁完后退役，不永久保留第二套实现。

## 5.4 modules 与其他功能包

`modules` 按功能拆入 modules：AI、Workflow、Dashboard、命名视图、连接导入、表读写、导出、备份恢复、MCP 业务和 Wapp 内核。实际宿主 I/O 移到 infrastructure，不把混合文件原样称作 domain。

`schema-diff`、`data-sync`、`data-transfer` 的算法与计划模型分别进入对应 domain，准备/执行/恢复编排进入 application。`migration-common` 的纯共享算法进入同名内部模块，不能反向依赖三件套或 execution。

`tunnel` 的配置/known-hosts 归 domain，用例归 application，形态中立的 SSH/代理机制可作为本模块私有实现；网络出口、密钥读取、文件持久化由宿主提供。它与 execution 的接线用注入接口，避免相互 import。

**AI crate 收拢（2026-10-26 决策，替代此前"继续独立"的表述）**：`ai-api` 先并入
`ai-providers`，合并后的整体再并入 `modules/src/ai/`——Provider 契约（trait、类型、
工厂注册）归 `modules::ai` 的契约层，具体 Provider 适配（openai / anthropic / ollama /
deepseek / custom 及共享 HTTP 机制）作为 `modules::ai` 的内部实现，不再保留独立
AI crate。`modules::ai` 因此持有唯一一份 Provider 协议与实现；外部插件经
`datazen` 的 ai feature 实现契约。

**platform-infra 改名 infra（2026-10-26 决策）**：`packages/platform-infra` 改名为
`packages/infra`（crate 名 `datazen-infra`），定位为 **drivers / modules / web /
desktop 四类消费者中任意两者共用**的无业务基础设施工具的家：凡是被其中两方及以上
使用的工具代码（路径安全、凭据脱敏、原子写入等）收拢进 infra，提高复用率、防止
各消费者各抄一份。仅单消费者使用的基础设施代码留在该消费者处，不预先进 infra。
前端 `backend-client` / SDK / UI 不参与 Rust modules 合包。

## 5.5 防止迁移中产生 Cargo 环

实施采用**类型依赖闭包先移动、旧包单向委托新模块**：

1. 先确认要移动符号用到的全部类型、trait 与宏；一起迁移必要的定义闭包。
2. 新 modules 的生产依赖不指回准备退役的旧 application/platform-api/runtime/kernel/modules/迁移包。
3. 某个定义移动后，由旧包依赖 modules 并再导出；未移动内容仍留在旧包，引用已迁移定义。
4. 旧调用方与新调用方使用同一类型、同一服务/仓储实例，而不是两套状态。
5. 每批核对 `cargo metadata` 和默认/裁剪 feature 构建，再移动上层消费者。

**禁止的捷径**：先建 `modules → modules` 的空壳，再让 `modules → modules` 转发；或让 `platform-api → modules → runtime → platform-api` 形成环。若依赖闭包过大，应先拆出纯接口和模型、仍在旧包内整理职责，再成组迁移，不用复制类型绕过。

兼容路径的例外必须具名：哪个符号、哪个旧消费者、在哪个 D 阶段删除。D8 必须清零；不能以“暂时兼容”永久保留旧包。

旧 F-04 等规则曾禁止 platform-api 依赖应用/领域包；让它再导出 modules 中已迁移的契约时，需要在同一 PR 将该兼容边改成**仅允许指定契约再导出**的新断言。不能先放开整个 modules 依赖、等以后再加模块检查，也不能要求已迁移类型继续复制在旧包里以满足过期布局。组织 scope 等行为安全闸门不属于可放宽的布局规则。
