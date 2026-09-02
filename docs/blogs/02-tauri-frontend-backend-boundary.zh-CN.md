# DataZen 架构设计（二）：Tauri 桌面应用的前后端边界

> DataZen 架构设计系列第 2 篇。本文从一个具体问题出发：当用户在 React 界面点击“执行”时，哪些工作应该留在前端，哪些工作必须进入 Rust？我们会沿着一条查询调用链，拆解 Tauri IPC 的协议边界、参数映射、错误处理和流式结果传输。

![DataZen 系统架构图](diagrams/datazen-system-architecture.svg)

## 一个看似简单、很容易失控的问题

桌面数据库工具最初往往从一个很小的功能开始：输入 SQL，点击按钮，显示结果。第一版实现很自然，按钮的点击处理器里调用一个数据库 API，数据库连接对象可能就放在某个页面组件或全局变量里。

这种写法在只有一种数据库、只有一个窗口时还能工作。一旦功能开始增长，边界问题就会迅速出现：

- React 组件开始判断 PostgreSQL、MySQL 和 SQLite 的方言差异；
- 前端为了复用连接，尝试保存连接池或数据库客户端对象；
- 一个页面走 `execute_query`，另一个页面又自己实现一套查询逻辑；
- AI、Workflow 和 MCP 为了执行 SQL，重复调用不同的后端入口；
- Rust 返回一个没有结构的字符串，前端只能通过匹配错误文本决定如何提示；
- 大结果集一次性返回，WebView 需要在主线程上解析和渲染几万行数据。

这些问题的共同根源，是把“界面怎么操作”和“数据库操作怎么执行”混成了同一个问题。

DataZen 的边界设计从一个原则开始：

> 前端负责表达意图和呈现状态，Rust 负责持有资源、验证意图并执行副作用。

这里的“意图”是结构化的，例如“在这个运行时会话上执行 `query_stream`，目标数据库是 analytics，输入 SQL 是……”。前端不应该知道连接池如何创建、驱动如何选择，也不应该根据数据库类型拼接出另一套调用路径。

## 边界的物理位置：Tauri IPC

DataZen 使用 Tauri v2。React 应用运行在 WebView 中，Rust 进程运行在桌面应用的本地侧。两者之间唯一稳定、可审计的业务边界是 Tauri IPC。

在这个边界上，数据必须满足三个条件：

1. **可序列化**：只能传输 JSON 值、字符串、数组、对象和 Tauri Channel 等协议对象，不能把 Rust 的连接池或 JavaScript 的类实例直接“穿过”边界。
2. **可验证**：Rust 端必须重新校验会话、命令、输入字段和权限，不能把前端传来的值当成可信事实。
3. **可演进**：命令名和字段是协议的一部分。新增字段应尽量保持旧调用方可用，语义变化要通过类型或版本明确表达。

系统架构图中，IPC 位于 React 与 Rust 之间，但它不是一个“把函数调用搬到另一边”的透明代理。它更像一个小型 RPC 协议层：前端提交请求，后端验证并执行，结果通过一次性响应或事件流返回。

## React 前端应该负责什么

前端的职责不是越薄越好，而是要把交互相关的复杂度留在交互层。DataZen 的 React 前端主要负责以下内容。

### 1. 收集用户意图

SQL 编辑器、连接树、数据库选择器和结果面板共同决定一次操作的上下文。它们把这些上下文组装成调用参数，例如：

```typescript
await queryCommands.executeQueryStream(
  dbSessionId,
  sql,
  onEvent,
  {
    database: selectedDatabase,
    schema: selectedSchema,
    applyResultLimit: settings.limitSelectResults,
  },
);
```

这里的 `dbSessionId` 是运行时会话标识，前端可以保存并引用它，但前端不拥有对应的连接对象。`selectedDatabase` 和 `selectedSchema` 是用户当前界面的选择，不代表后端一定会无条件接受，最终语义仍由 Rust 和 Driver 决定。

### 2. 管理交互状态

Zustand Store 保存连接列表、当前 Tab、编辑器内容、查询状态、结果批次和错误提示等前端状态。它可以决定按钮是否禁用、是否显示加载指示器、是否追加一批结果行，但不应该把数据库连接池或密码放进 Store。

前端状态通常分成三类：

- **持久状态**：用户设置、连接配置的展示信息、查询历史索引等，最终通过 IPC 保存到后端 Store；
- **会话状态**：`dbSessionId`、当前查询的 `executionId`、流式结果批次等，只在应用运行期间有效；
- **纯 UI 状态**：面板展开、选中的行、弹窗可见性和编辑器光标位置，不需要进入 Rust。

### 3. 选择呈现方式

查询结果可能以表格、图表、执行计划或导出流的形式呈现。前端根据结果事件更新 DataTable 和图表，而不是为了适配不同数据库重新执行一次查询。

## Rust 后端应该负责什么

Rust 端承担“拥有资源”和“决定是否执行”的职责。所有需要本地系统能力、敏感数据或长生命周期资源的工作都在这里完成。

### 1. 资源生命周期

`ConnectionManager` 管理运行时数据库会话，连接池、驱动实例和活动数据库都属于 Rust 的资源。前端只能通过 `connect`、`disconnect`、`ping_connection` 等命令操作生命周期。

连接配置则由 `Store` 持久化。这样可以把“用户保存了什么”和“当前进程建立了什么”分开建模，也避免页面卸载时意外销毁连接池。

### 2. 业务编排与验证

Rust Commands 负责把 IPC 参数转换成领域调用：解析 `dbSessionId`、确认会话存在、根据连接找到 Driver、验证 Command Definition、处理数据库和 Schema 目标，然后才调用服务或驱动。

例如，流式查询在进入 Driver 之前会依次完成：

1. 检查 `dbSessionId` 是否为空；
2. 根据 `database` 参数切换会话当前数据库（如果需要）；
3. 找到 `query_stream` 的 Command Definition；
4. 按定义验证 `input.sql` 和可选参数；
5. 检查读权限；
6. 应用结果数量限制和参数绑定；
7. 交给 Driver 的流式实现。

### 3. 敏感边界

数据库密码、AI Key、系统钥匙串、文件路径和本地日志都由 Rust 管理。前端收到的是已经过裁剪的配置或结构化结果，而不是可直接复用的秘密。

这也是为什么“让前端直接接触数据库”并不是一个简单的性能优化。那会同时扩大凭据暴露面、破坏资源生命周期，并让每个 WebView 都成为一套新的驱动适配层。

## `invoke`：一条普通 IPC 调用如何落地

DataZen 在 `src/commands/` 中按领域封装前端命令。以连接为例，前端调用看起来像普通 TypeScript 函数：

```typescript
import { invoke } from '@tauri-apps/api/core';

export const connectionCommands = {
  connect: (connectionId: string) =>
    invoke<string>('connect', { connectionId }),

  disconnect: (dbSessionId: string) =>
    invoke<void>('disconnect', { dbSessionId }),
};
```

Rust 端则通过 `#[tauri::command]` 暴露命令，并把共享状态注入处理器：

```rust
#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<String, CommandError> {
    state
        .connection_manager
        .connect(&connection_id)
        .await
        .cmd_err("connect")
}
```

应用启动时，命令会被集中注册到 Tauri 的 `invoke_handler`：

```rust
tauri::generate_handler![
    commands::connect,
    commands::disconnect,
    commands::execute_query,
    commands::execute_query_stream,
    commands::execute_driver_command,
    commands::execute_driver_command_stream,
]
```

这层集中注册有一个容易被忽视的价值：IPC 表面是显式的。一个 Rust 函数即使存在于 crate 中，只要没有注册，就不能被前端调用。

## camelCase 与 snake_case：协议的双语边界

前端遵循 JavaScript 习惯使用 camelCase：`dbSessionId`、`applyResultLimit`、`recordHistory`。Rust 遵循 snake_case：`db_session_id`、`apply_result_limit`、`record_history`。

Tauri 的参数反序列化会处理这层命名映射，Rust 请求结构体通常明确声明：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteDriverCommandRequest {
    pub db_session_id: Option<String>,
    pub driver_type: Option<String>,
    pub command: String,
    pub input: serde_json::Value,
    pub database: Option<String>,
    pub schema: Option<String>,
}
```

这不是单纯的格式偏好。它让 TypeScript API 和 Rust 领域代码都保持各自生态的可读性，同时把映射规则集中在协议类型上。新增字段时，应该同时检查前端封装、Rust 结构体以及相关测试，不能依赖“刚好能反序列化”的隐式行为。

需要特别注意的是，**字段命名映射与 ID 语义是两件事**。`connectionId` 映射到 `connection_id`，但它仍然表示持久化配置；`dbSessionId` 映射到 `db_session_id`，它仍然表示运行时会话。命名转换不会改变生命周期语义。

## 从专用命令到 Driver Command Runtime

DataZen 仍保留了领域清晰的 IPC 封装，例如 `get_table_data`、`get_explain` 和 `cancel_query`。但查询和驱动能力的统一执行路径是 `execute_driver_command`：

```typescript
export interface ExecuteDriverCommandRequest {
  dbSessionId?: string;
  driverType?: string;
  command: string;
  input: Record<string, unknown>;
  database?: string | null;
  schema?: string | null;
}

driverCommands.execute({
  dbSessionId,
  command: 'query',
  input: { sql },
});
```

Rust 端收到请求后，通过 `resolve_command_driver` 找到会话对应的 Driver；无连接的命令则可以使用 `driverType`。随后它根据 Driver 提供的 `command_definitions()` 找到定义，校验输入和访问级别，再调用 `execute_command()`。

这个设计把两个维度分开了：

- **入口**：SQL 编辑器、Workflow、MCP、Extension 或后台任务；
- **能力**：`query`、`query_stream`、`list_objects`、Redis KV 或某个管理 Command。

入口可以不断增加，能力也可以由 Driver 不断增加，但二者不需要互相复制一套判断逻辑。前端只负责调用协议，Driver 才负责数据库差异。

## 普通响应与流式响应

不是所有 IPC 都适合用一次性 `invoke` 返回结果。设置读取、连接信息和单次 `EXPLAIN` 通常可以返回一个 JSON 值；大查询和导出则需要事件流。

### 普通 IPC

普通调用等待一个 `Result<T, CommandError>`：

```typescript
const explain = await queryCommands.getExplain(dbSessionId, sql, database);
```

后端完成整个操作后，返回 `ExplainResult`。这种模式简单、容易组合，适合结果规模可控的命令。

### `Channel` 流式 IPC

`execute_driver_command_stream` 使用 Tauri `Channel<QueryStreamEvent>`。前端在调用前创建 Channel，并在 `onmessage` 中消费事件：

```typescript
const channel = new Channel<QueryStreamEvent>();
channel.onmessage = (event) => {
  if (event.type === 'rows') appendRows(event.index, event.rows);
  if (event.type === 'done') finish(event.totalTimeMs);
};

await invoke('execute_driver_command_stream', {
  request: {
    dbSessionId,
    command: 'query_stream',
    input: { sql },
  },
  onEvent: channel,
});
```

事件序列由协议类型约束，典型顺序是：

```text
executionStarted
statementStart
rows (0..n)
statementEnd
done
```

其中 `executionStarted` 携带 `executionId`，前端可以用它请求取消；`rows` 只携带当前批次；`statementEnd` 提供影响行数、耗时和是否截断；`done` 表示整个多语句请求完成。

流式传输的关键不只是“更快看到第一行”，还包括三个边界：

1. **内存边界**：后端和前端都以批次处理，不必把完整结果集复制到单个对象；
2. **生命周期边界**：流结束、失败或取消时，Rust 必须清理执行注册表中的 `executionId`；
3. **取消边界**：`cancel_query` 会校验 `executionId` 的所有者，防止一个会话取消另一个会话的查询。

## `CommandError`：让错误成为协议的一部分

IPC 错误不能只靠日志传递。用户需要知道是“连接不存在”“驱动不支持”“输入无效”，还是本地存储失败。

DataZen 用 `CommandError` 统一后端错误来源：

```rust
pub enum CommandError {
    Store(StoreError),
    Connection(ConnectionError),
    Driver(DriverError),
    Ai(AiError),
    Io(std::io::Error),
    Json(serde_json::Error),
    NotFound(String),
    NotConfigured(String),
    Validation(String),
    Internal(String),
}
```

它实现 `Serialize` 时仍序列化成字符串，因此兼容已有前端调用；但在 Rust 内部已经完成了分类，便于记录日志、做错误转换和编写测试。`CmdExt` 则统一负责把底层错误转换为 `CommandError` 并进行脱敏日志记录：

```rust
state.connection_manager
    .get_session(&db_session_id)
    .await
    .cmd_err("execute_query")?;
```

这里有一个重要取舍：当前 IPC 对前端保持简单的字符串错误格式，未来如果需要稳定的错误码和参数，可以在不改变领域错误分类的前提下扩展为结构化 `{ code, message, details }`。在此之前，前端不应该把完整错误文本当成稳定 API，只能把它作为展示信息。

## 一条查询的完整边界链路

把前面的组件串起来，用户点击 SQL 编辑器的“执行”按钮后，调用链大致如下：

1. **React 读取上下文**：取得当前 Tab 的 `dbSessionId`、数据库、Schema、SQL 和结果限制设置。
2. **命令封装**：`src/commands/query.ts` 调用 `driverCommands.executeStream`，统一构造 `query_stream` 请求。
3. **创建 Channel**：`src/commands/driver.ts` 创建 `Channel<QueryStreamEvent>`，把事件回调交给结果状态管理器。
4. **Tauri 反序列化**：Rust 将 camelCase 参数映射到 `ExecuteDriverCommandStreamRequest`。
5. **会话校验**：`ConnectionManager` 验证 `dbSessionId`，必要时根据 `database` 切换活动数据库。
6. **命令校验**：Driver Command Runtime 查找 `query_stream` 定义，验证 SQL 输入和读权限。
7. **驱动执行**：Driver 建立查询上下文，按批次读取行，并通过回调发出事件。
8. **前端消费**：React Store 追加行批次，DataTable 虚拟渲染可见区域。
9. **结束与清理**：收到 `done` 后，前端结束加载状态；Rust 清理执行句柄并按配置写入查询历史。

这个链路里，前端知道“我要查询”，后端知道“能否查询以及怎样查询”。前端不需要知道 PostgreSQL 如何读取批次，Driver 也不需要知道 DataTable 如何滚动。

## 为什么不让前端直接接触数据库

把数据库客户端放到 WebView 中，短期看似可以减少 IPC 调用，长期却会带来更多成本。

**安全成本**：连接密码、TLS 配置和 AI Key 更容易进入前端内存、调试工具或错误上报路径。桌面应用并不能因为是本地运行就忽略凭据边界。

**资源成本**：连接池、事务、取消句柄和重连策略需要稳定的生命周期。React 组件的挂载和卸载不适合作为这些资源的所有权模型。

**兼容成本**：每一种数据库都需要在 JavaScript 层打包驱动或协议实现，安装包、平台兼容和升级都会变复杂。DataZen 把数据库差异放进 Rust Driver，并在构建时选择需要的驱动。

**一致性成本**：GUI、Workflow、MCP 和 Extension 如果各自直接访问数据库，就会产生多套权限、错误和历史记录逻辑。统一进入 Command Runtime 后，入口可以复用同一套核心能力。

因此，IPC 并不是额外的绕路，而是把安全、资源和一致性集中管理的成本。对于数据库工具，这个边界通常值得保留。

## 设计中的几个反例

### 反例一：组件里按数据库类型分支

```typescript
if (databaseType === 'postgres') {
  // 拼接 PostgreSQL 方言
} else if (databaseType === 'mysql') {
  // 另一套分页逻辑
}
```

如果差异来自数据库，应下沉到 Driver 或 Driver 元数据。前端最多根据能力描述调整控件，不应把驱动 ID 变成 UI 业务分支。

### 反例二：先调用一个“切库”IPC，再调用查询 IPC

早期系统常见的写法是 `use_database` → `execute_query`。两个调用之间可能插入另一个 Tab 的操作，也可能在切库失败后仍然执行查询。

DataZen 的查询族命令接受可选 `database`，由 Rust 在同一个命令处理过程中完成会话绑定。这样“目标数据库”成为请求的一部分，减少了跨 IPC 的隐式状态。

### 反例三：用结果批次大小推断 SQL 限制

流式读取的批次大小只决定传输粒度，不能代表用户是否开启了 SELECT 结果上限。后端从设置中读取 `limit_select_results` 和 `query_result_limit`，并在明确允许时才注入限制。

### 反例四：把 `connectionId` 当作活动会话

配置被删除、会话重连或同一配置打开多个窗口时，二者的生命周期不同。SQL 和 Schema 操作使用 `dbSessionId`，历史、归属和调度使用 `connectionId`。混用会导致“配置存在但会话不存在”或“历史记录归属错误”。

## 测试如何守住边界

前后端边界不是写完协议就结束，还需要在测试中固定下来。

- 前端 `src/commands/__tests__/` 测试命令封装传出的命令名和参数形状；
- Rust Commands 测试会话不存在、输入校验、驱动不支持和错误转换；
- `execute_query_stream` 测试事件顺序、结果限制、失败时的历史记录和取消路径；
- Driver crate 测试自己的 Command Definition、方言和流式读取，不把驱动专属行为放进 Host；
- Host E2E 只验证用户能否从 UI 完成连接、查询和结果展示，不依赖某一个驱动的内部实现。

这种分层测试让协议变化更容易评估：如果只改了 UI 的加载状态，不需要重跑每个数据库的方言测试；如果改了 Command 输入 Schema，就应该同时检查 Host、Workflow、MCP 和对应 Driver。

## 当前限制与后续演进

当前的 IPC 协议已经能覆盖桌面端的核心场景，但仍有几个演进方向：

1. **错误结构化**：在保留旧字符串兼容的同时，引入稳定错误码，前端可以按错误码提供更精确的恢复动作。
2. **协议版本化**：当 Command Definition 或流式事件不可兼容地变化时，用协议版本而不是隐式猜测处理。
3. **更细的背压控制**：当渲染速度低于数据库读取速度时，让前端或 Channel 明确告知后端暂停或降低批次速度。
4. **共享 Core 适配器**：未来如果增加 Web 端，应该复用服务和 Driver 的核心逻辑，把 Tauri IPC 替换成受控的 HTTP 或其他边界，而不是把数据库连接搬到浏览器。

这些方向都遵循同一条原则：边界可以换一种传输方式，但资源所有权、权限验证和领域能力不应随入口复制。

## 结语

Tauri 的价值不只是把 React 和 Rust 装进一个安装包。对 DataZen 来说，它提供了一个清晰的本地系统边界：React 表达用户意图并管理交互状态，Rust 持有连接和敏感资源，IPC 把两者连接成可验证、可演进的协议。

普通命令用一次性响应，查询和导出用 Channel 事件流；camelCase 与 snake_case 在协议类型处完成映射；`CommandError` 统一错误出口；Driver Command Runtime 则让 GUI、Workflow、AI 和 MCP 共享同一套执行能力。

当这条边界稳定下来，前端可以专注于工作区体验，后端可以专注于连接和执行，驱动可以专注于数据库差异。下一篇我们继续沿着资源生命周期向下走，解释为什么 DataZen 同时需要 `connectionId` 和 `dbSessionId`，以及一个数据库会话从建立到清理究竟经历了什么。

相关资料：

- [IPC 命令层](../architecture/backend/commands.md)
- [服务层](../architecture/backend/services.md)
- [数据库驱动层](../architecture/backend/drivers.md)
- [第 1 篇：DataZen 架构全景](01-datazen-architecture-overview.zh-CN.md)
