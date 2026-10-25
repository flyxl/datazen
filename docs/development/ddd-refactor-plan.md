# DDD 模块化重构：目标架构与实施指南

> **状态：已确认的目标设计，尚未按本方案完成实现。** 确认日期：2026-10-24；本次代码核对基线：`main`，`07bb93a261086b8aebf9c1472ca44c339bd3c55e`。
>
> **两项已确认决策**：application 是 `modules::<功能>::application`，最终不保留独立 application crate；数据库执行机制主要进入 `modules::execution::runtime`，Job 等机制分别归属自己的功能模块。
>
> 本文整体替代此前以横向 application / platform-api / runtime crate 为长期目标的方案，不是在旧方案后面追加另一套规则。旧代码仍存在不代表目标变回旧结构；目标已经确认也不代表代码已经迁移。

## 1. 如何使用本文

### 1.1 阅读路线与文档权威

第一次接触项目，按以下顺序阅读：

1. §2–§4：理解术语、最终目录和依赖方向。
2. §5–§8：理解旧包去向、执行机制、仓储、身份和宿主装配。
3. §9：用表浏览这一条真实链路理解如何实现。
4. §10–§11：选一个可独立验收的阶段，按步骤开发。
5. §12–§14：编写测试、运行检查、判断能否合并和发布。

权威范围按问题划分，不用一句“代码永远优先”掩盖目标与现状的差别：

- **未来 Rust 包组织、模块归属和 DDD 迁移顺序**：以本文为准。
- **当前实现位置**：以当前分支代码与测试为准。本文 §3.1 只记录上述基线的事实。
- **连接 DTO、错误码、会话状态和资源约束**：沿用[连接管理详细设计](../architecture/platform/connection-management.md)，本轮不重新定义协议。
- **产品形态、Web 安全与多实例交付**：沿用[平台开发计划](platform-development-plan.md)、[团队 Web 服务设计](../architecture/platform/team-server-and-auth.md)和[持久化模型](../architecture/platform/persistence-model.md)。这些文档中的旧包路径与总 application 装配方式由本文替代，功能、安全与恢复要求不因此取消。
- **已实现架构文档**：随各阶段落地逐节更新；不能先把目标目录写成已存在。

本方案是已获授权的桌面 / Web 平台演进设计的一部分，不新增另一份 PRD、进度文档或评审记录。旧方案历史通过 Git 查看，不在本文保留过期的行数指标、测试计数和互相冲突的候选方案。

### 1.2 完成与未完成的边界

本文交付的是实施方案，不是一次实际发布。`modules/`、`web/` 及本文新命名的服务和 feature 均为**目标**，不能直接宣称已经可用。

重构不同时做数据库协议升级、持久化格式重写、前端重写或微服务拆分。必要的协议/存储变更另行分期，给出兼容、数据迁移和回滚验证。

## 2. 用熟悉的概念理解分层

### 2.1 Port 不是比 interface 更特殊的东西

Java `interface` 和 Rust `trait` 是语言提供的接口机制；Port 描述一个接口在架构边界上的角色。两种语言的接口能力并不完全相同，这里只类比依赖反转的用途。

例如：业务声明 `ConnectionRepository`，桌面用加密文件实现，Web 用管理数据库实现。Repository trait 是业务需要的存储接口，也可以称为“出向 Port”；两个具体实现是 Adapter。

本方案不要求统一使用 `Port` 后缀、不建立全局 `ports/` 仓库、不为每个 Service 机械创建一个 trait：

- 聚合持久化接口叫 `XxxRepository`，放在该功能的 `domain`。
- 凭据读取、网络接入、消息发布等非仓储能力，按含义叫 `XxxProvider` / `XxxPublisher`，放在消费者的 `application` 依赖接口中。
- 应用服务自身如果只有一种业务实现，可以直接用 struct；需要替换实现或隔离跨模块依赖时，再定义窄接口。
- Fake 实现用于测试，不是未装配能力的生产回退。

### 2.2 五类职责

**Controller**：把 IPC / HTTP / MCP 输入转换为应用请求，取得可信上下文，调用应用服务，映射响应和错误。它不负责查仓储、拼 SQL、协调事务或轮询执行。

**Application**：完成一个用户动作，例如“保存连接”“读取一页表数据”。负责业务流程、授权编排、调用领域规则、调用依赖接口和确定事务边界；不是没有逻辑的转发壳。

**Domain**：拥有业务模型、不变量、领域错误和 Repository trait。例如旧 revision 不能覆盖新配置、失效会话不能重新执行。领域规则不读取桌面路径、不认识 HTTP，也不持有宿主状态。

**内部机制**：实现队列、actor、租约回收、调度等功能。主要见 `execution::runtime` 和 `jobs::engine`；不是新的全局业务层。

**宿主 infrastructure / bootstrap**：前者实现文件、管理库、凭据、网络与传输适配，后者创建对象并注入。同一份业务用例通过不同实现运行在桌面或 Web。

### 2.3 三个 ID 永不混用

- `connectionId`：持久化连接配置 ID，用于配置、归属和调度。
- `dbSessionId`：当前后端进程/执行环境中的数据库会话 ID，不作为可恢复会话落盘。
- `executionId`：一次执行的 ID。历史可以记录它，但不能把历史记录当成仍存活的执行句柄。

显式会话操作还必须验证既有 `SessionHandle` 的 epoch、owner 和 revision；不能仅凭一个字符串 ID 省略这些校验。重构只改变类型的所属模块，不改变它们的含义。

## 3. 目标代码组织

### 3.1 核对过的当前实现

以下是当前事实，不是最终设计：

- 根 `Cargo.toml` 仍列有 `packages/application`、`packages/platform-api`、`packages/runtime`、`packages/kernel`、`packages/feature-core` 和迁移领域包；尚无根目录 `modules/`、`web/` 成员。
- `packages/application/src/lib.rs` 的 `ApplicationServices` 持有多个仓储和会话接口；这不等于所有命令的业务编排已经进入应用层。
- `packages/platform-api/src/ports/` 集中存放 Profile、连接目录、设置、Job 等接口；领域仓储也有一部分位于各自领域包。
- `packages/runtime/src/application/mod.rs` 的 `RuntimeConnectionUseCases` 同时持有 Profile / Policy、registry、gateway 和资源状态，需要按职责拆开。
- `packages/kernel/src/lib.rs` 导出 Job、budget、schema cache、SQL guard 和 driver registry，不应整包复制成新公共模块。
- `src-tauri/src/commands/table_target.rs` 仍从 Store 读取 Profile、选择 Driver、提交执行、每 25ms 查询终态并读取结果块，是首批完整迁移链路。

实施者必须再次核对自己的 HEAD；不要把其他 worktree 的未合并代码、旧对话里的测试结果计入当前分支。

### 3.2 四类核心组织单元

1. **driver-api**：保留 `packages/driver-api` 的独立驱动契约地位，本轮不为目录整齐搬它。
2. **modules**：新增根目录 `modules/`，Cargo package 目标名 `datazen-modules`，Rust 路径 `datazen_modules`。它是**一个 crate**，内部按功能划分。
3. **src-tauri**：桌面宿主，保留现有 crate 名和入口，承载 IPC、桌面适配与装配。
4. **web**：目标 Rust Web 后端目录 `web/`，目标 package 名 `datazen-web`；不是 React 前端目录，也不是桌面的子模块。

“四类”不是强制全仓只剩四个 crate。具体数据库驱动、具有独立插件协议的 AI Provider 契约/实现、前端 SDK 和无业务的基础设施工具仍可独立存在；不能把它们塞进 driver-api。独立扩展的许可证、构建和协议边界保持不变。

### 3.3 最终目录示意

下面全部是目标目录；已有代码迁移按 §10 执行。

```text
packages/driver-api/                  # 保留：驱动契约
packages/drivers/<id>/                # 保留：驱动实现与专属测试
modules/
  Cargo.toml                         # datazen-modules
  src/
    lib.rs                           # 功能模块出口，不是总 Service Locator
    shared/                          # 小型身份/上下文/通用值；禁止业务仓储全集
    connection/
      mod.rs                         # 公共入口与模块装配
      application/                   # 连接 CRUD、导入、执行配置投影
      domain/                        # 配置、revision、Repository trait
    execution/
      mod.rs                         # 暴露服务/装配入口，runtime 不公开
      application/                   # 会话、执行、事务、取消、结果读取
      domain/                        # 状态机、目标、归属与执行契约
      runtime/                       # actor、lease、队列、等待、回收
    jobs/
      application/                   # 提交、取消、查询，Handler 注册契约
      domain/                        # Job 状态、claim、Repository trait
      engine/                        # 通用调度；不认识具体迁移功能
    schema/
      application/
      domain/
      cache/                         # 私有元数据缓存机制
    table_browsing/                   # 各功能内部均有 application/、domain/
    table_editing/
    schema_diff/
    data_sync/
    data_transfer/
    migration_common/                # 迁移共享纯算法，不是总 MigrationService
    settings/
    named_views/
    history/
    dashboard/
    workflow/
    ai/
    mcp/                             # 工具业务分发；传输和登录在宿主
    monitor/
    export/
    backup/
    wapps/
    tunnel/
  tests/                             # 公共契约与跨功能旅程，不放驱动方言测试
src-tauri/src/
  commands/                          # 保留现有 IPC 命名
  infrastructure/                    # 分领域 Repository 与桌面能力实现
  bootstrap/                         # 按需创建模块并接线
web/
  Cargo.toml                         # datazen-web，待实现
  src/
    controllers/
    middleware/
    infrastructure/
    bootstrap/
```

不创建 `modules/src/application/`、全局 `modules/src/runtime/` 或新的 `modules/src/platform_api/` 来重新包装旧横向分层。一个功能可以按复杂度增加私有子模块，但不为凑目录创建空 service、entity、repository。

`shared` 与 `migration_common` 是基础词汇/纯算法模块，不是独立用户功能，不强制给它们创建空 application 层。

### 3.4 每个模块的对外面

模块提供三种明确出口：

- **业务入口**：`modules::<功能>::application` 的服务、请求、响应与应用错误。
- **实现契约**：宿主需要实现的 `domain` Repository trait、模型，以及应用层外部依赖接口。
- **装配入口**：模块根公开的构造函数/Builder，连接内部实现并返回应用服务和必要的关停句柄。

“Web 和 Desktop 依赖 application”指 **Controller 的业务调用进入应用层**，不是禁止宿主 infrastructure 引用 domain；后者必须能引用 trait 才能实现仓储。bootstrap 可使用装配入口，但不能把内部 runtime 暴露给 Controller。

## 4. 依赖方向与模块边界

### 4.1 编译依赖与运行调用不是同一张图

目标编译依赖：宿主依赖 `datazen-modules`；modules 使用 driver-api 和必要的中立库；具体驱动实现 driver-api。modules、driver-api 均不反向依赖宿主。宿主只在驱动装配边界链接具体驱动，Controller 不按数据库种类分支。

模块内部约束：

1. `domain` 依赖本领域类型、少量 shared 值和明确允许的驱动契约；需要 ConnectionId、表目标等跨模块值时，可以单向引用拥有者公开的稳定值类型，但不能访问对方仓储或服务。不依赖 application、runtime/engine 实现、文件/管理库 API。
2. `application` 依赖自己的 domain、外部能力接口，以及其他模块显式公开的应用契约。
3. `execution::runtime` 实现 execution 的内部机制接口，可以引用其 application 中的**依赖接口和关联 DTO**，不能调用具体应用服务反向发起业务。
4. 应用服务不直接 new 私有 runtime。模块根装配代码同时认识二者，把实现注入应用服务，避免 application 与 runtime 的实现相互引用。
5. Repository 实现位于宿主 infrastructure，依赖拥有者模块的 domain；domain 不知道实现类。
6. `shared` 不依赖任何功能模块，不收纳所有功能的错误、DTO、配置和仓储。

一次请求运行时会从应用服务调用 trait 对象，进入宿主仓储或执行机制。这不意味着应用层在编译时依赖宿主。

### 4.2 跨功能协作的单向关系

起始允许方向如下；新增边要在实现 PR 同时更新规则与测试，不能凭 `pub(crate)` 随意互访：

- `execution` 使用 `connection` 的执行配置读取契约；不直接调用它的 Repository。
- `schema` 使用 execution；`table_browsing` 使用 schema 和 execution。
- `table_editing` 使用 schema 和 execution；读写共享的表目标类型由 schema 拥有，不能让读模块依赖写模块的内部上下文。
- `schema_diff` / `data_sync` / `data_transfer` 使用 schema、execution、jobs 和必要的 migration_common 算法；三件套不合成一个服务。
- `dashboard` / `workflow` / `ai` / `monitor` 使用被允许的基础功能应用契约，不能直接读连接仓储或执行 actor。
- `jobs` 不 import Schema Diff、Workflow 或其他业务 Handler；这些模块实现 jobs 声明的 Handler 契约，宿主在启动时注册。
- `mcp` / `wapps` 的业务桥只调用获准的应用入口，不获得任意 Service Locator 或 Repository 列表。

如果出现 A 调 B、B 又调用 A，先检查是否把跨功能编排放错位置，或是否应该通过消费者定义的窄接口注入。禁止用全局注册表、`Any`、字符串服务查找来藏起环。

### 4.3 Rust 可见性不能替代架构检查

- 私有实现尽量使用普通私有模块或 `pub(super)` / 祖先路径限制；不要默认 `pub mod runtime`。
- `pub(crate)` 对整个 modules crate 可见，不表示“只允许指定兄弟模块”。公开给宿主的 Repository trait 也无法仅靠 Rust 可见性禁止 Controller 调用。
- 不公开具体 runtime 类型、内部锁或缓存句柄；构造/关停接口只暴露必要抽象。
- 在 §10 的 D1 阶段扩展边界检查：识别模块、重导出、别名、实际调用与直接全限定路径；不能只匹配 `use ...`。
- 检查应区分生产与测试，并覆盖测试前后、独立测试文件和条件模块。不能把首个 `#[cfg(test)]` 后的所有内容一律当作非生产。
- 对宏生成边界和无法分析的声明要明确拒绝或登记可审查规则，不允许静默漏扫。

不增加文件行数上限、应用层方法数量或“Controller 行数下降百分比”门禁。验收对象是依赖方向、业务行为和测试覆盖，而不是搬了多少行。

## 5. 旧 crate 与接口的去向

### 5.1 platform-api：按拥有者拆散，最终退役

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

### 5.2 application：移入各功能，不换名保留总层

- `SettingsService` → `settings::application`。
- 连接管理用例 → `connection::application`。
- 会话/执行用例与目标处理 → `execution::application`，目标不变量进入对应 domain。
- Dashboard / 命名视图等仓储访问编排 → 对应功能 application。
- 前端事件桥与可选能力 → 消费者的依赖接口和宿主装配，不成为所有业务的共同前置依赖。
- `ApplicationServices` → 过渡兼容门面，最终退出；宿主不再必须为不使用的功能提供一整套仓储。

### 5.3 runtime / kernel：能力保留，位置改变

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

### 5.4 feature-core 与其他功能包

`feature-core` 按功能拆入 modules：AI、Workflow、Dashboard、命名视图、连接导入、表读写、导出、备份恢复、MCP 业务和 Wapp 内核。实际宿主 I/O 移到 infrastructure，不把混合文件原样称作 domain。

`schema-diff`、`data-sync`、`data-transfer` 的算法与计划模型分别进入对应 domain，准备/执行/恢复编排进入 application。`migration-common` 的纯共享算法进入同名内部模块，不能反向依赖三件套或 execution。

`tunnel` 的配置/known-hosts 归 domain，用例归 application，形态中立的 SSH/代理机制可作为本模块私有实现；网络出口、密钥读取、文件持久化由宿主提供。它与 execution 的接线用注入接口，避免相互 import。

`ai-api` / `ai-providers` 继续作为独立 Provider 契约和适配实现；`modules::ai` 不把 Provider 协议实现复制一份。`platform-infra` 现有无业务原语也不因减 crate 数而强迁。前端 `backend-client` / SDK / UI 不参与 Rust modules 合包。

### 5.5 防止迁移中产生 Cargo 环

实施采用**类型依赖闭包先移动、旧包单向委托新模块**：

1. 先确认要移动符号用到的全部类型、trait 与宏；一起迁移必要的定义闭包。
2. 新 modules 的生产依赖不指回准备退役的旧 application/platform-api/runtime/kernel/feature-core/迁移包。
3. 某个定义移动后，由旧包依赖 modules 并再导出；未移动内容仍留在旧包，引用已迁移定义。
4. 旧调用方与新调用方使用同一类型、同一服务/仓储实例，而不是两套状态。
5. 每批核对 `cargo metadata` 和默认/裁剪 feature 构建，再移动上层消费者。

**禁止的捷径**：先建 `modules → feature-core` 的空壳，再让 `feature-core → modules` 转发；或让 `platform-api → modules → runtime → platform-api` 形成环。若依赖闭包过大，应先拆出纯接口和模型、仍在旧包内整理职责，再成组迁移，不用复制类型绕过。

兼容路径的例外必须具名：哪个符号、哪个旧消费者、在哪个 D 阶段删除。D8 必须清零；不能以“暂时兼容”永久保留旧包。

旧 F-04 等规则曾禁止 platform-api 依赖应用/领域包；让它再导出 modules 中已迁移的契约时，需要在同一 PR 将该兼容边改成**仅允许指定契约再导出**的新断言。不能先放开整个 modules 依赖、等以后再加模块检查，也不能要求已迁移类型继续复制在旧包里以满足过期布局。组织 scope 等行为安全闸门不属于可放宽的布局规则。

## 6. Execution 与 Job 的详细边界

### 6.1 一次执行由谁负责

以“对目标数据库执行一个命令”为例：

1. 宿主完成认证，构造可信 `RequestContext`，转换请求 DTO。
2. execution 应用服务通过 connection 的应用契约读取执行配置投影，不直接取连接仓储。
3. 应用服务校验目标、启用状态、策略、能力和 revision；调用领域规则形成可执行请求。
4. execution 私有机制在接纳点验证请求绑定，原子登记幂等键/执行 ID，取得预算和 lease，再交给 Driver。
5. 结果发布、失败/取消事实和资源释放由机制按领域状态机推进。
6. 应用服务提供执行状态和结果读取；宿主只转换协议，不直接访问 actor 或文件。

应用授权与机制绑定检查不能互相替代。不能把“授权移入 application”实施成删除 gateway 的 owner / epoch / revision 检查；也不能在 runtime 里复制另一份产品权限规则。

### 6.2 两种执行方式继续存在

**Session-bound**：使用调用方明确提供的有效 SessionHandle，适用于事务和连续操作。校验 connection、owner、epoch、配置 revision、上下文 revision 和目标；不匹配时明确拒绝。

**Target-bound**：按明确目标取得独立固定 lease，不借用树节点或 SQL 编辑器会话、不隐式改变别人的 database/schema。

不得在显式 handle 失效时悄悄回退 target-bound；这会丢失事务语义。也不能通过“先设置全局当前库再执行”模拟 target-bound。

### 6.3 Runtime 的实例与关停

每个执行节点/宿主运行实例只装配一套统一的会话、执行与物理资源账本，组织隔离是账本内的安全约束，不是让所有组织共享无 owner 状态。表浏览、SQL 编辑器、Workflow、Job Handler 引用同一服务实例；测试可创建隔离实例。

模块根的装配入口创建 runtime 并注入应用服务，只向宿主 bootstrap 返回服务和生命周期句柄。Controller 不得得到原始 runtime。构造依赖缺失时启动失败或明确禁用该可选模块，不得创建 DummyRepository。

关停顺序：停止接纳新任务 → 按既有策略 drain/请求取消 → 处理事务和未确定副作用 → 完成结果/历史发布 → 释放 lease 与预算 → 关闭仓储与宿主。失败不能记录成成功取消；无法证明回滚时保留 unknown 事实。

这份单进程机制不自动等于多 worker 协调。Web 的持久目录、claim/fencing 和全局预算仍须通过宿主实现并按[多 worker 设计](../architecture/platform/multi-worker-coordination.md)验收。

### 6.4 等待执行，而不是在 Controller 轮询

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

### 6.5 Job 与具体功能解耦

jobs 拥有提交、claim、阶段推进、取消意图、历史和恢复规则。Schema Diff 等功能拥有自己的 plan 和业务 Handler，向 jobs 注册。jobs 不判断“这是哪种数据库迁移”。

Job Handler 只能通过 execution 的应用能力访问数据库。Job 资源借用与交互查询仍走同一资源账本；如调度器需要预算能力，由 bootstrap 注入执行侧的窄预算接口，不能生成第二份计数器。

长任务需要保存的是 plan/revision/checkpoint/效果事实，不是物理连接或可复用的旧 SessionHandle。重启恢复必须重新验证配置、授权和计划消费状态。

## 7. Repository、存储实现与安全

### 7.1 Repository 由领域拥有

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

### 7.2 连接目录与 Profile 的统一

目标拥有者是 `connection`，不是两个不同的“平台连接目录”：

- 目录 CRUD、分组、排序、revision 保存统一为 connection domain 的规范仓储契约。
- 建连所需的 Profile 是该配置的执行投影，供 execution 使用的读取面属于 connection application。
- AI/MCP 使用脱敏摘要，不获取全量配置、secret reference 或可解密材料。
- 现有 `ConnectionCatalog` 与 `ProfileRepository` 的方法按上述职责迁移；保留必要的读写分面，但不保留两个同义且作用域不同的 ConnectionCatalog。
- `compare_and_set`、credential revision、disabled、last-connected 更新等能力不能在“简化接口”时消失。部分更新不能变成读取旧配置后整条覆盖。

桌面初期可用 `DesktopConnectionStore` 同时适配过渡接口与新契约，但必须共享现有缓存、锁、密钥和持久化事实；Web 实现同一契约，不重新定义另一套业务语义。

### 7.3 DesktopRepository 如何拆

先把现有总对象的职责列清，再逐功能抽离，不只拆出若干 `impl DesktopRepository` 文件。

- `DesktopStorageContext`：路径策略、原子写、密钥访问与写入协调；这是宿主基础设施，不进入 modules。
- 分领域对象：Connection / Settings / Tunnel / History / NamedView / Migration Profile / AI Config 等具体实现。
- `AppDb` 可以继续共享数据库连接，但通过各领域接口提供能力；Controller 不取得裸 AppDb。
- 共享服务的内存 fake 可位于测试代码；生产的持久化 Repository 实现在 src-tauri 或 web。

首轮保持旧文件名、加密封装、SQLite schema、序列化和异常恢复行为不变。禁止为新接口创建第二份配置文件、第二套密钥或双写存储。

### 7.4 一致性与回滚

- 单聚合写入由 Repository 原子完成，CAS 的比较与写入处于同一锁/数据库事务中。
- 配置与 revision/凭据引用共同提交时，必须延续既有一致性与失败恢复承诺，不能先更新内存再忽略落盘失败。
- 跨文件没有天然数据库事务；在改造前用故障注入锁定现有行为。若需新增日志或恢复协议，作为独立持久化变更评审，不靠口头“最终一致”。
- 跨领域动作由应用层编排，定义提交点与失败后的补偿/重试；不引入全局 Repository 隐式事务。
- 需要变更存储格式时必须有旧版本读取、升级失败、降级/恢复演练。纯代码搬迁的默认回滚是 revert，不需要用户数据迁移。

### 7.5 身份、授权和错误

Web middleware 验证登录后构造 RequestContext；Desktop/MCP Adapter 从可信宿主身份构造。客户端传来的 organizationId/principalId/角色字段不能成为授权事实。

应用服务负责资源级业务授权，仓储查询始终携带可信组织作用域，执行接纳点保留 owner/epoch/revision 与策略有效性检查。跨组织资源按现有不可见契约处理，不能泄漏“存在但无权”；读取空值也不能被解释为可创建覆盖。

凭据只在实际建连所需的内部适配边界解析，不能进入公开摘要、日志、事件或测试失败文本。按职责迁移错误类型，不修改既有 IPC 错误语义；SQL 执行终态错误与请求接纳错误继续区分。

Agent 不读取 `.env` / `.env.test` 内容；测试程序正常读取环境是允许的。敏感测试的日志先留系统临时目录，只摘取脱敏后的结构性结论。

## 8. Desktop / Web 按需使用应用层

### 8.1 模块级装配，不再强制全量 ApplicationServices

每个模块提供自己的依赖集合与构造入口。启用某模块时，其必要 Repository/Provider 必须注入；未启用的模块不要求宿主提供任何假实现。

以桌面为例，装配步骤是：

1. 初始化宿主 storage、凭据与身份适配器。
2. 构造 connection 服务，注入 DesktopConnectionStore。
3. 构造一份 execution 模块，注入 connection 读取面、网络/驱动与结果存储等能力。
4. 用同一个 execution 服务装配 schema、table_browsing 及其他启用模块。
5. 如启用 jobs，创建调度服务并注册具体功能 Handler。
6. 只把所需应用服务放进相应 Controller 状态，保存关停句柄供 bootstrap 使用。

Web 做同样的业务装配，但使用 Web 管理库、凭据和认证实现。Web 不依赖 src-tauri，不通过桌面 IPC 代理完成业务。

允许宿主有一个方便组织状态的 `DesktopServices` / `WebServices`，但它们不是 modules 中强制所有形态实现的统一总构造器，也不向业务提供动态服务查找。

### 8.2 三种“可用”必须区分

1. **编译能力**：feature 是否包含该模块及其依赖。
2. **部署能力**：宿主是否成功装配、注册路由并具备所需资源。
3. **用户权限**：当前请求是否获准执行此操作。

功能未装配时，不返回成功空结果；Web 可不注册该路由，保留的兼容 IPC 则返回既有的明确不支持/未配置错误。前端能力显示与真实装配结果一致。即使 UI 隐藏入口，后端仍须授权。

可选 AI Provider 初始化失败不应让不依赖它的连接列表失效；执行必需依赖缺失则不能宣称执行可用。失败范围按依赖图测试，不能通过回退旧 Store 绕过。

### 8.3 Cargo feature 策略（目标，D1 起实现）

先支持宿主独立装配，再按依赖重量和产品需求裁剪编译。约定 `default = []`；功能 feature 是叠加能力，不创建相互排斥的 `desktop` / `web` 模式。

下面是要实现的最小 feature 依赖片段，不是当前 Cargo.toml：

```toml
[features]
default = []
settings = []
connection = []
execution = ["connection"]
schema = ["execution"]
table-browsing = ["schema"]
jobs = []
schema-diff = ["schema", "jobs"]
```

这些值表达功能依赖；实现时还需将各功能的非必要第三方依赖声明为 optional 并接入 feature。否则关掉模块仍会编译其依赖，不能声称完成裁剪。

Rust 模块使用下划线，例如 `table_browsing`；Cargo feature 使用上述短横线名称。后续功能的 feature 依赖随该功能加入，不能启用一个 feature 却漏掉它调用的模块。

Cargo 会合并同一依赖图中的 features。因此：

- 分别构建 Desktop 和 Web 的声明集合，不用一次 workspace 联合构建证明两者都能最小运行。
- 验证空 features、单功能必要闭包、两个宿主集合和全功能集合。
- 私有扩展、驱动协议和授权不依靠 feature 互斥保护。
- 新增 optional crate 的声明边也要过架构门禁，不能只检查默认 resolve 图。

### 8.4 Web 范围与交付声明

本轮文档不启动 Web 产品实现。后续先交付同一条连接/表浏览链路的真实 Web 适配，再按平台计划扩展功能与多 worker。

只有 fake HTTP handler 或第二个内存 Repository，不足以宣称“双宿主上线”：至少要有真实身份上下文、组织隔离、管理库存储、错误映射、启用/禁用模块和一次数据库操作的集成验证。

桌面已迁移、Web 未实现时，可以明确交付“桌面模块化版本”；不能把它标为本方案的“双宿主验收完成”。Web 安全范围仍按[团队 Web 服务设计](../architecture/platform/team-server-and-auth.md)执行，旧文中的 server 目标目录统一对应这里的 web。

## 9. 第一条完整示范：连接配置到表浏览

### 9.1 修改前先追踪当前链路

当前核对入口：

- `src/commands/database.ts` 的表数据请求与 `src/platform/tauriBackendTransport.ts` 的 transport 映射。
- `src-tauri/src/commands/data.rs` 的 `get_table_data_at_target` 与 `commands/table_target.rs` 的 `read` / `execute`。
- `packages/feature-core/src/services/query_executor/` 的旧查询服务及 `commands/schema.rs` 的旧 `get_table_data`。
- `packages/runtime/src/application/` 的执行、事务与结果实现。
- `src/stores/tableDataStore.ts` 的分页、结果合并和前端状态处理。

旧链路不等于可以直接删除的死代码。实施前搜索全部前端、MCP、Workflow、测试和注册入口，确认是否存在只提供 session 的调用方。

### 9.2 新链路

Controller 将既有 wire 请求映射为 `table_browsing::application` 的读页请求，后续顺序为：

1. 校验 page size、filter/sort 结构及必须提供的目标信息。
2. execution/connection 应用契约读取可信配置并校验权限、revision 和驱动能力；不信任客户端自报 driver type。
3. schema 应用服务取得表结构，按当前能力形成列、主键和排序信息。
4. table_browsing domain 计算分页、keyset 可用性、count 计划和拒绝原因。
5. 使用 Driver 契约生成目标命令，交给 execution 应用服务执行；表浏览不直接调用具体 Driver 的物理 query。
6. 通过 execution 等待与结果读取能力取得数据，转换为现有 TableDataResult。
7. Controller 映射错误；前端继续消费同一 DTO。

模块内部的 `TableReadRequest` 与写入上下文解耦，共享目标由 schema 的公开契约提供。现有 IPC 请求字段不能因此改名或删除，适配器负责转换。

### 9.3 两种旧入口如何收敛

- `get_table_data_at_target` 和旧 `get_table_data` 最终进入同一套读页业务实现。
- target 调用保留独立 lease；session 调用保留已验证 handle 与事务语义。
- 旧入口若只有 dbSessionId，必须通过受授权的会话解析能力取得目标和有效 handle，不能猜测 connectionId 或填默认 database。
- 可以保留不同的入口适配 DTO，不能保留两份分页/count 算法。
- 旧入口没有生产调用方后才删除注册；仍在兼容期的 IPC 可以保留，但只负责映射并委托。

### 9.4 本轮不能悄悄改变的语义

- 无主键、复合主键、NULL 排序、keyset 拒绝与 offset fallback。
- `require_keyset`、`after_key`、cursor depth、默认排序和翻页参数。
- count 的复用、不可用、精确、估算不是同一状态；失败不能伪装成 0。
- 手动事务会话目标变化必须拒绝，不在后台换连接继续执行。
- schema / rows / count 目前分次执行，收进同一个 Service **不自动提供同一 snapshot**。增强一致性需要 Driver 能力与单独行为设计。
- 前端 pending changes、count drift、缓存失效和错误后的下一步操作仍可用。

验收必须包含连续旅程：打开表 → 筛选 → 排序 → 翻页 → 修改配置/会话上下文 → 再读被拒绝 → 刷新后恢复，不能只测一条最终合法查询。

### 9.5 这条切片的完成标准

宿主表浏览命令不再访问 Store / AppDb / driver registry / runtime internals；两个入口共享同一应用流程；配置与资源状态只有一份；领域规则独立可测；桌面运行旅程通过。D7 再对同一公共用例执行 Web 集成验证，不能重新写一份 Web 表读取算法。

## 10. 分阶段实施与验收

使用 **D0–D8** 编号，避免与平台产品计划的 P0–P10 混淆。每阶段可以有多个小 PR，但每个 PR 必须构建可用、有真实消费者和测试；不先提交一堆空接口等待别人接线。

旧源码注释中引用的 P0/P2/R2/D2 等仍指旧版断环与端口决策，不自动映射成本节新阶段；迁移对应代码时同步更新这些注释和守卫依据。

共同要求：行为兼容、依赖无环、安全检查不减少、目标路径与测试一起移动。改守卫时先增加等价新规则和失败自证，再退役只适用于旧布局的规则。

### D0：建立可信基线

**输入**：当前 HEAD、已安装驱动集合、未提交差异、CI 配置。

**工作**：

1. 核对所有 Cargo 成员和依赖闭包；记录现有边界门禁、测试收集范围及 feature 集合。
2. 为连接/表浏览/执行链路记录入口、协议、持久化副作用与错误行为。
3. 跑现有定向测试；缺少的兼容行为先增加测试，再移动代码。
4. 基线耗时、内存/资源计数、已有失败记录在 PR 验证说明中，不另建长期进度文件。

**退出**：每条已有失败有真实命令/退出码/原因分类；无凭据进入说明；不能把历史绿灯当本轮基线。

**回滚**：基线测试无业务副作用。新增测试保留，不因后续移动删除断言。

### D1：新 modules 与第一条可运行小切片

**输入**：D0；优先从依赖闭包小的共享 Settings 用例开始，而非一次移动所有功能。

**工作**：

1. 建立 `modules/` Cargo 成员、模块级 application/domain、最小 shared。
2. 按 §5.5 移动必要类型，旧包只再导出；接入现有桌面 Settings 适配和命令，不只交目录。
3. 实现空 features 与该功能 feature 构建；新模块不反向依赖待退役包。
4. 扩展 `scripts/lib/cargoWorkspace.mjs`、`scripts/check-ddd-layers.mjs`、`scripts/check-platform-crate-boundaries.mjs` 与对应自证，使新成员/内部模块真正进入扫描。
5. 更新 `scripts/run-platform-crate-tests.mjs` 的收集与 feature 矩阵；不能让 datazen-modules 只有空 feature 测试而漏掉业务。
6. 对已迁移模块新增 §4 的内部边界检测。旧 crate 规则仍覆盖未迁移部分。

**退出**：能实际读写设置；旧/新 import 指向同一类型；禁止依赖的合成样例会失败；扫描目标或测试模块为空时不能报告全绿。

**回滚**：revert 本切片恢复旧入口，设置文件格式不变。

### D2：连接领域与宿主仓储

**输入**：D1；Profile / ConnectionCatalog 现有契约和调用方清单。

**工作**：

1. connection domain 统一配置、ID、revision 与仓储责任；application 提供 CRUD、导入、摘要及执行配置投影。
2. 把无 scope 的旧连接目录消费者改为可信上下文调用，移除重复 ConnectionCatalog 定义。
3. 抽取 DesktopConnectionStore，复用同一缓存、锁和密钥；把目录与 Profile 适配接到同一存储事实。
4. 兼容既有连接表单、分组、排序、部分更新与 CAS，不把原有凭据语义改成空值覆盖。
5. 收口前端连接命令、导入命令以及 AI/MCP 的目录读取；后续 AI/MCP 整体迁移在 D6。

**退出**：同一契约测试通过真实桌面适配与 fake；跨组织不可见；并发保存不覆盖新 revision；普通摘要不带凭据；连接 UI 旅程保持可用。

**回滚**：恢复旧适配调用，不需要转换配置文件。不得在旧新实现之间双写。

### D3：Execution 拆分与资源机制归位

**输入**：D2；既有连接管理和 fake-runtime 测试契约。

**工作**：

1. 拆开 RuntimeConnectionUseCases 的应用编排、领域状态和私有机制。
2. 迁移所需 DTO/机制类型的完整依赖闭包。若包含 Job 契约、缓存或预算基础类型，先迁其拥有者的必要部分，不能让新 execution 反向依赖旧 kernel。
3. runtime 私有化，经模块装配入口创建；旧 facade 委托同一个新服务。
4. 保留目标执行、会话执行、事务、取消、幂等、资源预算、idle 与 artifact 行为。
5. 封装等待能力，按 §6.4 验证竞态；生命周期装配接入桌面关停。

**退出**：§12 的执行用例及现有资源测试通过；同一 operation 只执行一次；失败/取消/关停均无资源泄漏；owner/epoch/revision 校验完整。不能仅以 cargo check 代替生命周期验证。

**回滚**：以干净进程切回旧代码。会话不跨版本持久化迁移；不能运行中把旧句柄交给新资源表。

### D4：Schema、表读写与 SQL 消费者

**输入**：D3；现有表浏览 transport、分页与写回行为测试。

**工作**：

1. schema 应用服务和私有缓存归位；缓存 key 包含现有组织/连接/目标/revision 等隔离维度，不只按表名缓存。
2. 按 §9 完成表浏览完整链路，删除重复业务算法，兼容旧 IPC 映射。
3. table_editing 迁移 ChangeSet、预览、提交、行身份与 fingerprint，复用 execution 事务边界。
4. SQL 编辑器/结果写回接入应用入口，保留风险确认、只读与取消规则。
5. 真实方言测试留在对应 driver crate；公共 UI 行为仍在 Host E2E。

**退出**：读页、筛选/排序、keyset/count、连续编辑、提交/回滚与目标漂移旅程通过；不能因读接口改造破坏下一步写入。

**回滚**：旧新前端 wire 兼容；已产生的数据修改按原事务语义处理，不能把代码 revert 当数据回滚。

### D5：Jobs 与迁移三件套

**输入**：D3/D4；既有计划消费、checkpoint、fencing 与恢复测试。

**工作**：

1. 完成 jobs domain/application/engine 和宿主 JobRepository 接线。
2. 迁移 Schema Diff，再迁 Data Sync、Data Transfer；每个功能单独交付 Profile → plan → review → execute → history 的完整路径。
3. Handler 由各功能实现，宿主注册；Job 引擎不 import 具体功能。
4. 三件套各自拥有仓储 trait；跨三类 Profile 的读取用已定义窄投影，不能恢复总 Store。
5. 计划 revision、唯一消费、预算、取消、部分成功和恢复事实保持不变；迁移共享算法落 migration_common。

**退出**：重复提交不会重复应用；旧 revision/权限撤销被拒绝；中断后恢复不复用过期会话；每个三件套有真实驱动和 UI 旅程证据。

**回滚**：持久化计划和历史不变；已经产生的外部 DB 副作用不能靠 revert 撤销，必须按既有恢复/补偿流程处理。

### D6：其余功能与旧宿主业务移除

**输入**：前述基础功能稳定；每批只迁一个功能族。

迁移顺序及边界：

1. Settings 剩余路径、Named Views、History：共享业务进各模块；窗口位置、桌面路径等仍是宿主专属配置。
2. Dashboard：保存/组件创建/运行历史/导入导出规则进入 application/domain，管理库实现留宿主。文件对话框留宿主交互层，应用服务接收已选择的内容/受控文件能力，不能把 GUI 对话框作为 Web 必填依赖。
3. Workflow：保留 step/session/transaction block 的资源边界，调用 execution 与 jobs。
4. AI / MCP：Provider-neutral 业务进入模块；认证/stdio/HTTP/事件传输在宿主，授权数据库工具走同一执行入口。
5. Monitor、Export、Backup/Restore、Wapps、Tunnel：按各自模型迁移，保留 grant、文件安全、恢复锁、沙箱桥与网络限制。

每个功能的验收都包含取消/失败、宿主依赖缺失、当前协议兼容和一次用户旅程。最后移除 DesktopRepository 的领域业务方法；它若只剩装配与基础设施，改为用途明确的宿主对象或删除。

**回滚**：每个功能单独 revert；不随重构变更用户数据布局。

### D7：真实 Web 装配与功能选择验收

**输入**：最小 connection/execution/schema/table-browsing 模块集合可独立构建。

**工作**：

1. 新建 web 宿主，注册 `datazen-web`，同步包分类、CI 和平台文档；不再同时创建 server/ 作为第二宿主。
2. 按既有 Web 安全设计接认证、RequestContext、真实 Repository 与执行依赖。
3. 复用相同应用服务，完成连接与表浏览端到端；选择不启用至少一个可选模块，验证无需提供它的仓储。
4. 分开构建两个宿主的 feature 集合，校验无 Tauri 泄入 Web/共享层。
5. 对 Web 实际提供的功能运行真实存储契约、租户/角色、错误映射、断连/取消与关停测试。

**退出**：最小双宿主链路成立，未提供的功能显式声明；完整团队产品仍须满足平台 P7/P8 等相应交付门槛。没有真实 Web 集成结果，不能把 D7 标完成。

**回滚**：回退 Web 服务版本和配置；如首次引入管理库迁移，使用独立评审的 expand/contract 与恢复方案，不由 DDD 目录调整代替。

### D8：旧包退役与发布收尾

**输入**：所有计划迁移的生产消费者和测试都使用新模块，D7 范围明确。

**工作**：

1. 删除旧 application/platform-api/runtime/kernel/feature-core 及已并入的迁移/tunnel 包的重复实现、兼容再导出和 workspace 成员。
2. 更新 Cargo.lock、驱动测试的 dev-dependency、CI 发现、构建脚本、Rustdoc 链接与正式架构说明。
3. 删除临时例外；旧包名仅允许出现在明确的历史说明中，不能出现在生产依赖或可执行测试路径里。
4. 旧 IPC 是否删除单独按外部兼容政策决定；保留委托命令不等于保留旧 crate。
5. 对全部受支持 feature 集合和发布形态执行 §13–§14，不以一个全功能构建掩盖最小集合缺依赖。

**退出**：只剩目标模块和明确保留的独立插件/工具包；全部测试真实被收集；文档无两套现行目标；发布证据完整。

**回滚**：按合并顺序 revert；不使用 reset/restore 旧 stash 覆盖其他分支工作。运行期间的用户数据库副作用按功能自身的恢复约束处理。

## 11. 实习生每个 PR 的操作步骤

### 11.1 开工前

1. 阅读根 AGENTS.md 与任务涉及目录的说明；核对 branch、worktree、HEAD 和已有差异，不覆盖他人修改。
2. 仓库有 CodeGraph 时优先定位调用链；不可用再搜索符号和调用方。搜索包括 Rust、前端、测试、脚本和驱动，不能只看正在编辑的文件。
3. 在 PR 描述中写清一句用户动作、当前链路、目标链路、保持不变的协议/存储格式、目标 D 阶段。
4. 跑最窄行为测试并核对非零测试数；缺环境与代码失败分开记录。
5. 找到预计移动的符号、它依赖的类型和所有调用方，确定先迁的闭包。

### 11.2 实现顺序

1. 先给原行为补回归测试，不为迁移降低断言。
2. 把模型/规则/Repository 放到目标 domain，迁入应用请求/流程和必要依赖接口。
3. 迁入内部机制或复用已有机制，经模块根装配；编写 fake/真实机制测试。
4. 实现并接线宿主 Repository，保持原加密、缓存和锁。新增接口必须同时有真实消费者。
5. Controller 改为取上下文、映射、调用服务、映射错误；保留合法的协议与 UI 适配分支。
6. 更新调用方、注册、Cargo/feature、CI 分类、测试入口与文档；临时旧路径只再导出/委托。
7. 查剩余生产引用；未删除的兼容壳写明消费者和删除阶段。

不要先写一份含 `todo!()`、空返回值和注释步骤的 Service 当作交付。本文中的目标命名不是现成 API；实现时以真实请求类型、错误和完整流程落地。

代码质量沿用项目规则：生产路径不新增裸 unwrap/expect；Rust 使用明确的 Result/错误类型和现有异步接口约定；TypeScript 严格类型、不用 any 消错。保留全局错误脱敏，不能把数据库失败原文直接向外传播。改名/搬迁时同步 exports、调用方和测试，不用宽泛 allow 或放宽权限白名单让门禁变绿。

### 11.3 自查三个方向

- **本问题**：业务编排是否真的离开 Controller，而不是把 AppState 传进新文件？
- **合法同类**：是否保留其他驱动、显式事务、空结果、取消和离线恢复路径？
- **下一步操作**：用户翻页后编辑、取消后重试、改配置后刷新是否仍顺畅？

同时排除常见假重构：

- 全局 `platform-api` 换名 `shared`，接口归属没有改变。
- 把当前所有文件合进 modules，但内部仍可任意访问 runtime 和别人的仓储。
- 每个功能都 new 一份执行服务，导致多个会话表或预算账。
- 为了让 Web 编译，给未启用功能注入永远成功的 DummyRepository。
- feature 关掉了模块，却没有关掉它独有的第三方依赖。
- 用“请求成功返回”代替“副作用已提交且资源已释放”的断言。

### 11.4 提交材料

PR 描述必须包含目标/当前与目标链路/变更边界/兼容说明/新增测试/实际命令与退出码/未完成项/回滚方式。记录事实，不写“理论上应该通过”。

长期结论进入代码、测试和正式架构文档。并行开发若使用仓库允许的 progress.md / hub.md，验收合并时必须删除；不以它们作为交付。验证变异与提交使用不同 worktree。

## 12. 验收测试设计

以下编号是**待实现或需映射到既有测试的验收要求，不是已通过的测试统计**。优先复用现有 CM 和功能旅程；移动测试保持断言强度，记录旧测试到新测试的对应关系。

### 12.1 测试分层与落点

- domain 单测：模型/规则/状态转换，不启动宿主、不访问用户文件或真实 DB。
- application 单测：Fake Repository、Policy、执行能力，断言输入/输出、副作用次数和关键先后顺序；不要要求与业务无关的日志顺序。
- 机制测试：fake clock、barrier、失败注入验证并发、租约、幂等和资源清理。
- 宿主契约测试：同一 Repository 契约分别跑桌面临时存储与 Web 测试管理库，验证 scope、CAS、失败与恢复。不能只有两个 fake。
- transport 测试：真实经过 DTO/上下文/错误映射，不能 mock 掉需要验证的服务接缝。
- 用户旅程：Host 通用 UI 在 e2e；Driver 方言、专属 Command 和 live 契约留在 `packages/drivers/<id>/`。不把方言测试搬进 modules。

### 12.2 边界与装配：B01–B06

- **B01**：domain 引用 application/runtime/宿主应失败；合法纯规则可以编译。
- **B02**：Controller 直接取得 Repository 或 runtime 应被门禁拒绝；bootstrap 合法装配不被误报。
- **B03**：模块循环、shared 反向依赖功能、经别名/重导出绕过边界必须被发现。
- **B04**：空 features、各宿主最小集合、完整集合均正确收集与构建；关闭 AI 等可选功能不要求其仓储。
- **B05**：未装配、初始化失败、无权使用是不同状态；能力输出和实际可调用入口一致。
- **B06**：新成员/新目录缺失、扫描目标为空、测试数意外为零不能静默通过；门禁用合成违规与合法样例自证。

### 12.3 仓储与安全：R01–R07

- **R01**：缺失与跨组织不可见保持既有契约；不能覆盖其他组织同名 ID。
- **R02**：并发保存同一 revision 最多一次成功；timestamp 更新不覆盖编辑内容。
- **R03**：旧配置/加密文件/SQLite 数据在新实现可读，旧字段不丢失。
- **R04**：写入失败、磁盘故障/事务回滚后，不返回成功，不使缓存与持久化悄悄分叉。
- **R05**：公开连接摘要、AI/MCP 摘要和错误/日志不包含 secret；内部建连/授权配置编辑的凭据路径单独验收，不把摘要规则误施于必须取凭据的适配器。测试用合成凭据，失败输出不泄密。
- **R06**：凭据/profile/network revision 变化使过期请求按契约拒绝，disabled 不是“空配置继续执行”。
- **R07**：同一存储事实服务目录与执行读取；更新后两个消费者观察一致，不存在第二份缓存。

### 12.4 执行与 Job：E01–E09

- **E01**：错误 owner、组织、epoch、revision 或目标的 handle 被拒绝；拒绝前不产生 DB 副作用。
- **E02**：相同幂等键和相同请求回放同一回执；相同键不同载荷拒绝；不执行两次。
- **E03**：target-bound 使用独立 lease；不改变编辑器上下文。session-bound 保留事务会话，失败不切换执行模式。
- **E04**：成功、异常、取消与关停达到应清理边界后资源账归位；等待超时/订阅断连不提前释放仍在执行的资源，最终终态仍正确清理；取消无法确认时不伪造成功。
- **E05**：Active/Aborted/Unknown 事务和关闭/重连规则保持；过期句柄不能透明绑定新连接。
- **E06**：等待前已完成、订阅竞态、重复等待、多等待者取消均有确定性测试；等待超时不等于执行取消。
- **E07**：结果发布未完成、artifact 过期、分块错误、权限变化分别处理，不返回静默空表。
- **E08**：Job plan 只消费一次；checkpoint/claim/fencing/重启恢复不重复提交已生效副作用。
- **E09**：交互执行与 Job 共用预算，越限排队/拒绝与释放行为正确，不能借拆模块突破预算。

### 12.5 表浏览与宿主：T01–T06

- **T01**：旧/新入口的请求响应与错误语义兼容，旧入口只剩映射和委托。
- **T02**：无主键/复合主键/NULL/keyset/offset/默认排序和 count 策略按现有契约工作。
- **T03**：连续筛选、排序、翻页、缓存失效、pending changes 和下一次编辑完整可用。
- **T04**：读取途中配置或手动会话目标变化被拒绝；刷新后可以恢复，不借用错误连接。
- **T05**：桌面与真实 Web Adapter 对相同授权输入得到等价业务结果；认证/传输差异不复制业务算法。
- **T06**：每个宿主选择不同功能集合；未启用功能不能调用；启用功能具备完整依赖和关停路径。

### 12.6 验证质量要求

上述用例必须映射到真实测试文件与符号，不要求为了编号再写一份重复测试。断言观察行为，不以 `source.contains("Service")` 代替业务测试。

架构门禁与安全守卫要验证“故意破坏后会红”。涉及真实树变异时用独立 worktree；按[验证方法论](verification-methodology.md)区分变异证据、管线实测与机制推断。

性能对比使用相同数据、feature、构建模式和并发条件，至少关注查询延迟、执行次数、连接/lease 峰值、取消清理和编译耗时。预算在 D0 记录并经评审确定，不在本文虚构已测百分比。

## 13. 检查命令与证据记录

### 13.1 先确认环境与运行对象

在对应 worktree 根目录运行。安装/生成流程遵循 package.json 和[worktree 隔离规则](worktree-isolation.md)，不要为了修缺失 codegen 去手写生成文件；driver stash 陈旧时按错误给出的恢复流程处理，不运行旧 stash restore 覆盖合并结果。

每轮检查首尾记录 HEAD、工作区内容指纹和 Git 状态，证明测试的是同一份代码。`git diff --binary HEAD | git hash-object --stdin` 只能覆盖 tracked 差异；新增文件、影响构建的生成输入需另记校验值，不把它谎称为完整工作区指纹，不读取受保护 env 内容。

测试/门禁输出放系统临时目录。下面是可在 Bash 定义的通用包装器，避免 Linux/macOS 的 mktemp 模板差异：

```bash
run_check() {
  local log rc
  log="$(mktemp "${TMPDIR:-/tmp}/datazen-ddd.XXXXXX")" || return 1
  if "$@" >"$log" 2>&1; then
    rc=0
  else
    rc=$?
  fi
  printf 'EXIT=%s LOG=%s\n' "$rc" "$log"
  return "$rc"
}
```

每次运行后检查退出码与日志的实际汇总。确认日志无敏感信息后，可用 `tail -n 40 "$LOG"` 查看结论，其中 LOG 设置为上一步打印的路径。不要把全部 stdout、原始连接串或 panic 中的凭据贴进 PR/对话。

失败后停止该验收结论、先读错误再处理；不能忽略包装器的非零返回。超时、OOM、测试断言失败分别记录。Rust 测试过滤器不是正则 OR，不能用 `a|b` 后看到 0 tests 就报通过。

### 13.2 当前仓库已有的检查

以下命令已从本次基线的 package.json / 脚本核对存在；列出不代表本次文档修改已全部运行。按改动范围先定向再扩大：

```bash
run_check git diff --check
run_check cargo fmt --all -- --check
run_check node scripts/check-ddd-layers.mjs
run_check node scripts/check-platform-crate-boundaries.mjs
run_check node scripts/check-module-layers.mjs
run_check node scripts/check-id-terminology.mjs
run_check node scripts/check-shared-crate-panic-policy.mjs
run_check node scripts/run-platform-crate-tests.mjs --dry-run
run_check cargo test -p datazen-application -p datazen-platform-api --lib
run_check cargo test -p datazen-runtime
run_check cargo test -p datazen --lib
run_check pnpm typecheck
run_check pnpm exec vitest run src/platform/__tests__/tauriBackendTransport.tableData.test.ts
run_check pnpm exec vitest run src/stores/__tests__/tableDataStore.keyset.test.ts
run_check pnpm exec vitest run src/stores/__tests__/tableDataStore.countStrategyContract.test.tsx
```

当前 `check-module-layers.mjs` 主要检查前端层，不是未来 Rust 功能模块守卫；D1 必须实际扩展 Rust 检查，不能借同一个名字假称已覆盖。当前 DDD 脚本里的固定成员下限与旧包断言也要随合包替换成非空/成员完整性规则，不把合法合包当违规，也不直接删除防漏扫能力。

Rust 链接确属内存不足时，可以用 `CARGO_BUILD_JOBS=1 RUSTFLAGS='-C debuginfo=0'` 重跑原测试并记录；这不替代最终发布配置检查，也不能掩盖测试失败。

### 13.3 modules / web 建立后的新增验证

**以下是 D1/D7 必须接入 CI 的目标命令；当前没有对应 crate，不应现在运行后把“不存在”当成代码回归。** feature 片段与 §8.3 一致：

```bash
run_check cargo check -p datazen-modules --no-default-features
run_check cargo test -p datazen-modules --no-default-features --features settings
run_check cargo test -p datazen-modules --no-default-features --features connection
run_check cargo test -p datazen-modules --no-default-features --features table-browsing
run_check cargo test -p datazen-modules --all-features
run_check cargo test -p datazen-web
run_check cargo build -p datazen-web --release
```

另外单独执行 Desktop 和 Web 的实际声明 feature 集合并检查依赖闭包。新模块测试包含 lib、集成、文档测试和必要的 release-profile 套件；不能把旧 runtime 集成测试搬丢后只跑 `--lib`。

新旧兼容期同时跑已迁移模块与仍存在的旧包测试。旧包删除的同一个 PR 替换对应 CI 命令，不保留永远报 package-not-found 的步骤，也不无声跳过。

### 13.4 合并与发布检查

驱动注入/生成状态正确后，受影响 Rust 包与宿主检查完整运行；全量基础检查包括：

```bash
run_check cargo check --workspace --all-targets
run_check pnpm test:platform-crates
run_check pnpm typecheck
run_check pnpm exec vitest run
run_check pnpm test:unit:drivers
run_check pnpm build
```

构建/Driver API 或布局变化需要真实驱动 Rust 测试；驱动 live 环境缺失只能记录未完成，不能用 fake 替代发布验收。独立 Pro 扩展的门禁在其自己的仓库运行，宿主绿灯不代表 Pro 通过。

E2E 必须使用项目构建器和匹配的驱动集合：

```bash
run_check pnpm tauri:build:webdriver
run_check pnpm e2e:skip-build
run_check pnpm e2e:contract:matrix
```

不使用裸 cargo build 代替 Tauri Webdriver 构建。迁移三件套若仅 DMG 阶段失败但已生成可运行 `.app`，按项目规则继续验证应用与 WDIO，分别报告功能结果和打包失败。

正式发布还须通过实际 release workflow 的门禁、目标 OS/架构/驱动/Community-Pro 变体构建及安装冒烟。至少包含翻译硬门禁，不因“只是重构”跳过：

```bash
run_check node scripts/i18n-sync-check.mjs
run_check pnpm tauri:build:community
```

Pro 发行仅在对应独立扩展已装配并授权的环境运行其构建和验收。签名、上传、部署、版本/tag/发布动作另需明确授权，完成本文实现不自动授权这些外部动作。

## 14. 发布标准与最终交付清单

### 14.1 三种状态不要混写

- **文档完成**：目标设计、迁移步骤、检查和链接一致；不等于代码实现。
- **阶段可合并**：本阶段代码与相关检查完成，其余兼容路径有明确下一阶段；不等于整个重构结束。
- **版本可发布**：该版本声明的功能、宿主、驱动与发行变体全部满足真实发布门槛。关键测试未跑、超时或环境缺失时仍有缺口，不能标发布完成。

### 14.2 全部重构完成的必要条件

- [ ] 业务按功能进入一个 modules crate；每个功能拥有应用层与领域层。
- [ ] 独立 application/platform-api/runtime/kernel/feature-core 与已合入的旧功能 crate 退役；明确保留的插件契约和基础设施包不被误删。
- [ ] Repository trait 由各领域拥有，生产持久化实现在 src-tauri / web；没有全局仓储全集或另一个总 application 层。
- [ ] runtime 私有地归属 execution；Job、Schema cache 等归属对应功能；不存在每个消费者一份资源账本。
- [ ] Controller 只进入应用服务；宿主基础设施与装配可以引用必要领域契约，不被错误的层门禁阻挡。
- [ ] Desktop 与 Web 能独立选择、装配和编译自己的功能集合；未启用功能无需假依赖。
- [ ] IPC / wire DTO / error / Driver 协议与存储兼容有测试证据；任何行为变化已单独说明和验收。
- [ ] owner/scope/epoch/revision、事务、取消、幂等、资源清理及计划恢复语义没有因移动而削弱。
- [ ] §12 验收要求都有真实测试映射，最小 feature、全功能、真实宿主和驱动测试没有漏收集。
- [ ] 依赖与模块门禁在合成违规/合法样例上有鉴别力；临时兼容例外清零。
- [ ] D7 的真实 Web 接缝完成；如仅交付桌面阶段，明确不宣称双宿主方案全部完成。
- [ ] 正式架构文档逐节改成实现事实，索引与代码路径可达；阶段台账在合并收尾删除。
- [ ] 发布矩阵、安装/升级与回滚演练符合所声明版本；没有未解释的失败和未运行检查。

### 14.3 回滚演练和发布说明

至少用合成用户数据完成：旧版读写 → 新版启动并执行代表性操作 → 退出/重启 → 兼容允许范围内回退读取。确认配置、凭据引用、历史和计划不丢失，运行时句柄没有被误恢复。

发布说明写用户可感知结果与真实兼容边界，不把“减少 crate 数”“新增几个 trait”当成功标准。无法验证的 OS、驱动、Web 能力和恢复场景明确列出，不能以文档中的计划替代实际证据。
