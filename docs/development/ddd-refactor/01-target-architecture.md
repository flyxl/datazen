# 目标架构：分层概念、代码组织与依赖方向（原 §2–§4）

> 本文是[DDD 模块化重构总纲](../ddd-refactor-plan.md)的细节文件，承接原 §2、§3、§4 的全部内容；文档权威与阅读路线见总纲 §1。

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

- 根 `Cargo.toml` 仍列有 `modules`、`modules/src/platform_api`、`modules`、`packages/kernel`、`modules` 和迁移领域包；尚无根目录 `modules/`、`web/` 成员。
- `modules/src/application/lib.rs` 的 `ApplicationServices` 持有多个仓储和会话接口；这不等于所有命令的业务编排已经进入应用层。
- `modules/src/platform_api/src/ports/` 集中存放 Profile、连接目录、设置、Job 等接口；领域仓储也有一部分位于各自领域包。
- `modules/src/execution/application/mod.rs` 的 `RuntimeConnectionUseCases` 同时持有 Profile / Policy、registry、gateway 和资源状态，需要按职责拆开。
- `packages/kernel/src/lib.rs` 导出 Job、budget、schema cache、SQL guard 和 driver registry，不应整包复制成新公共模块。
- `src-tauri/src/commands/table_target.rs` 仍从 Store 读取 Profile、选择 Driver、提交执行、每 25ms 查询终态并读取结果块，是首批完整迁移链路。

实施者必须再次核对自己的 HEAD；不要把其他 worktree 的未合并代码、旧对话里的测试结果计入当前分支。

### 3.2 五类核心组织单元

1. **driver-api**：保留 `packages/driver-api` 的独立驱动契约地位，本轮不为目录整齐搬它。
2. **modules**：新增根目录 `modules/`，Cargo package 目标名 `datazen`，Rust 路径 `datazen`。它是**一个 crate**，内部按功能划分。AI Provider 契约与适配实现也并入本 crate 的 `ai` 模块（2026-10-26 决策，见[§5.4](02-legacy-crate-migration.md)）。
3. **src-tauri**：桌面宿主，保留现有 crate 名和入口，承载 IPC、桌面适配与装配。
4. **web**：目标 Rust Web 后端目录 `web/`，目标 package 名 `datazen-web`；不是 React 前端目录，也不是桌面的子模块。
5. **infra**：`packages/infra`（由 `platform-infra` 改名，crate 名 `datazen-infra`），**无业务的基础设施工具 crate**：凡被 drivers / modules / web / desktop 四类消费者中任意两者共用的工具代码（路径安全、凭据脱敏、原子写入等）放这里，提高复用率；仅单消费者使用的留在消费者处。

“五类”不是强制全仓只剩五个 crate。具体数据库驱动、前端 SDK 等具有独立许可证、构建或插件协议边界的包仍可独立存在；不能把它们塞进 driver-api。独立扩展的许可证、构建和协议边界保持不变。

### 3.3 最终目录示意

下面全部是目标目录；已有代码迁移按[阶段计划](07-stages.md)执行。

```text
packages/driver-api/                  # 保留：驱动契约
packages/drivers/<id>/                # 保留：驱动实现与专属测试
packages/infra/                       # datazen-infra：四类消费者共用的无业务工具（改名自 platform-infra）
modules/
  Cargo.toml                         # datazen
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
    migration/common/                # 迁移共享纯算法，不是总 MigrationService
    settings/
    named_views/
    history/
    dashboard/
    workflow/
    ai/
      application/                   # Provider-neutral 业务（会话、工具循环、安全策略）
      domain/                        # Provider 契约：AiProvider trait、类型、工厂注册
      providers/                     # openai/anthropic/ollama/deepseek/custom 适配与共享 HTTP 机制
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
    infrastructure/                  # web 形态的 Repository 实现（与桌面各自实现同一 domain trait）
    bootstrap/
```

不创建 `modules/src/application/`、全局 `modules/src/runtime/` 或新的 `modules/src/platform_api/` 来重新包装旧横向分层。一个功能可以按复杂度增加私有子模块，但不为凑目录创建空 service、entity、repository。

`shared` 与 `migration/common` 是基础词汇/纯算法模块，不是独立用户功能，不强制给它们创建空 application 层。

### 3.4 每个模块的对外面

模块提供三种明确出口：

- **业务入口**：`modules::<功能>::application` 的服务、请求、响应与应用错误。
- **实现契约**：宿主需要实现的 `domain` Repository trait、模型，以及应用层外部依赖接口。
- **装配入口**：模块根公开的构造函数/Builder，连接内部实现并返回应用服务和必要的关停句柄。

“Web 和 Desktop 依赖 application”指 **Controller 的业务调用进入应用层**，不是禁止宿主 infrastructure 引用 domain；后者必须能引用 trait 才能实现仓储。bootstrap 可使用装配入口，但不能把内部 runtime 暴露给 Controller。

## 4. 依赖方向与模块边界

### 4.1 编译依赖与运行调用不是同一张图

目标编译依赖：宿主依赖 `datazen`；modules 使用 driver-api、infra（共用工具）和必要的中立库；具体驱动实现 driver-api，可用 infra。modules、driver-api、infra 均不反向依赖宿主。宿主只在驱动装配边界链接具体驱动，Controller 不按数据库种类分支。infra 不依赖 modules / driver-api / 宿主——它只提供无业务工具，保持在依赖图最底层。

模块内部约束：

1. `domain` 依赖本领域类型、少量 shared 值和明确允许的驱动契约；需要 ConnectionId、表目标等跨模块值时，可以单向引用拥有者公开的稳定值类型，但不能访问对方仓储或服务。不依赖 application、runtime/engine 实现、文件/管理库 API。
2. `application` 依赖自己的 domain、外部能力接口，以及其他模块显式公开的应用契约。
3. `execution::runtime` 实现 execution 的内部机制接口，可以引用其 application 中的**依赖接口和关联 DTO**，不能调用具体应用服务反向发起业务。
4. 应用服务不直接 new 私有 runtime。模块根装配代码同时认识二者，把实现注入应用服务，避免 application 与 runtime 的实现相互引用。
5. Repository 实现位于宿主 infrastructure（web / desktop 各自实现），依赖拥有者模块的 domain；domain 不知道实现类。Repository **trait 定义永远在 domain 层**（`modules::<功能>::domain`），不落在 platform-api、application 或宿主里。
6. `shared` 不依赖任何功能模块，不收纳所有功能的错误、DTO、配置和仓储。

一次请求运行时会从应用服务调用 trait 对象，进入宿主仓储或执行机制。这不意味着应用层在编译时依赖宿主。

### 4.2 跨功能协作的单向关系

起始允许方向如下；新增边要在实现 PR 同时更新规则与测试，不能凭 `pub(crate)` 随意互访：

- `execution` 使用 `connection` 的执行配置读取契约；不直接调用它的 Repository。
- `schema` 使用 execution；`table_browsing` 使用 schema 和 execution。
- `table_editing` 使用 schema 和 execution；读写共享的表目标类型由 schema 拥有，不能让读模块依赖写模块的内部上下文。
- `schema_diff` / `data_sync` / `data_transfer` 使用 schema、execution、jobs 和必要的 migration/common 算法；三件套不合成一个服务。
- `dashboard` / `workflow` / `ai` / `monitor` 使用被允许的基础功能应用契约，不能直接读连接仓储或执行 actor。
- `jobs` 不 import Schema Diff、Workflow 或其他业务 Handler；这些模块实现 jobs 声明的 Handler 契约，宿主在启动时注册。
- `mcp` / `wapps` 的业务桥只调用获准的应用入口，不获得任意 Service Locator 或 Repository 列表。

如果出现 A 调 B、B 又调用 A，先检查是否把跨功能编排放错位置，或是否应该通过消费者定义的窄接口注入。禁止用全局注册表、`Any`、字符串服务查找来藏起环。

### 4.3 Rust 可见性不能替代架构检查

- 私有实现尽量使用普通私有模块或 `pub(super)` / 祖先路径限制；不要默认 `pub mod runtime`。
- `pub(crate)` 对整个 modules crate 可见，不表示“只允许指定兄弟模块”。公开给宿主的 Repository trait 也无法仅靠 Rust 可见性禁止 Controller 调用。
- 不公开具体 runtime 类型、内部锁或缓存句柄；构造/关停接口只暴露必要抽象。
- 在[阶段计划](07-stages.md)的 D1 阶段扩展边界检查：识别模块、重导出、别名、实际调用与直接全限定路径；不能只匹配 `use ...`。
- 检查应区分生产与测试，并覆盖测试前后、独立测试文件和条件模块。不能把首个 `#[cfg(test)]` 后的所有内容一律当作非生产。
- 对宏生成边界和无法分析的声明要明确拒绝或登记可审查规则，不允许静默漏扫。

不增加文件行数上限、应用层方法数量或“Controller 行数下降百分比”门禁。验收对象是依赖方向、业务行为和测试覆盖，而不是搬了多少行。
