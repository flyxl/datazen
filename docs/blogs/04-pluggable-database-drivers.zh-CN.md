# DataZen 架构设计（四）：可插拔数据库驱动架构

> 支持数据库的难点不是再写一个连接表单，而是把连接、分页、Schema、EXPLAIN、类型转换和管理命令的差异隔离在正确的位置。DataZen 用 Driver API、编译时选型和链接时注册，把数据库差异压缩在驱动边界内。

## Driver 是差异边界

宿主只应知道“这里有一个实现了 `DatabaseDriver` 的对象”。Driver 负责理解具体数据库的连接协议、标识符引用、分页能力、事务、流式读取、Schema 目录和专属命令。

典型能力包括：

- `connect`、`disconnect` 与健康检查；
- `query`、`execute`、`query_stream`；
- `list_objects`、`get_object_ddl`、`get_schema`；
- `supports_offset()`、`supports_explain()` 等能力元数据；
- `command_definitions()` 与 `execute_command()`。

如果差异来自数据库本身，就优先放进 Driver，而不是让 Host 在组件中判断 `databaseType === 'postgres'`。

## 两层公共 API

`packages/driver-api` 是宿主和驱动共享的稳定契约，包含 `DatabaseDriver`、Command 类型、结果类型、错误类型、能力元数据和 `inventory` 注册宏。宿主依赖 API，不依赖每一个驱动的内部模块。

驱动 crate（例如 `datazen-driver-postgres`）实现 API，并把自己的 Rust 测试、UI 和 E2E 放在 `packages/drivers/<id>/`。这使驱动可以独立验证方言和专属行为，Host 测试只验证通用编排。

## Path Driver 与 Git Driver

DataZen 同时支持两种来源：

- **Path Driver**：位于 monorepo 的 `packages/drivers/*`，通过 Cargo workspace 和 optional feature 编译；
- **Git Driver**：由 `drivers-registry.json` 指向独立仓库，构建前克隆到 `packages/drivers/<id>/`，同样实现 Driver API 并参与注册。

Git 驱动可以固定 commit 或 tag，便于复现构建；Path 驱动则适合核心数据库和同仓库开发。两者在运行时都表现为相同的 Driver factory。

## 编译时选型

构建前运行 `scripts/resolve-drivers.mjs`：

1. 读取 `drivers-registry.json` 和 `--drivers` / `DATAZEN_DRIVERS`；
2. 校验驱动 ID、来源和 Git ref；
3. 克隆或确认驱动目录；
4. 生成前端 `DRIVER_DB_ENTRIES`；
5. 生成 Rust `driver_init.rs` 与 Cargo optional feature 注入；
6. 输出 `.driver-features.json`，供前端和测试使用。

因此 `pnpm tauri:dev --drivers=postgres,mongodb` 选择的是编译时组合，而不是启动后动态下载数据库驱动。数据库协议实现仍然在本地 Rust 进程中，权限和安装包边界更容易审计。

## inventory：链接时自动注册

每个驱动通过 `inventory` 提交自己的 factory。宿主启动时只读取已经链接进二进制的 factories，构建生成的初始化文件负责确保选择的 crate 被引用。

这比在 Host 里维护一个不断增长的 `match database_type` 更适合插件化：新增驱动主要增加自己的 crate 和注册声明，不需要让宿主知道实现细节。

## ReuseDriver 与能力转发

某些场景需要包一层 Driver，例如复用连接、增加诊断或提供兼容适配。`ReuseDriver` 必须同时转发 `command_definitions()` 和 `execute_command()`，否则 UI 可能发现了命令却无法执行，或 Workflow 与 MCP 看到的能力集合不一致。

## 新驱动的实现清单

一个可用驱动通常需要完成：

1. 连接配置解析和安全握手；
2. 基础查询与参数绑定；
3. Schema 和数据库目录；
4. 分页、排序、过滤与类型映射；
5. EXPLAIN 或“暂不支持”的能力声明；
6. Command Definition、风险级别和输入 Schema；
7. 断开、取消和异常清理；
8. crate 内 Rust/UI/E2E 测试。

“暂不支持”也应通过能力元数据表达。宿主可以隐藏按钮或给出清晰提示，而不是发出必然失败的 IPC。

## 测试为什么必须留在驱动目录

PostgreSQL 的标识符引用、Redis 的 KV 命令、MongoDB 的文档查询都是驱动知识。把这些测试放进 Host 会让宿主测试反向依赖某个方言，也会在驱动拆分时造成重复。

Host 测试验证“能否找到 Driver、能否调用统一命令、错误是否跨 IPC 返回”；驱动测试验证“这个数据库实现是否正确”。两层边界清晰，升级成本才可控。

## 结语

Driver 架构的目标不是隐藏所有差异，而是让差异只在一个地方出现：Driver。编译时选型决定安装包包含哪些能力，inventory 负责链接时注册，Driver API 和 Command Runtime 则让 GUI、Workflow、AI 和 MCP 以同一种方式使用它们。下一篇将继续讨论 Driver 如何从传统 Trait 演进为可发现、可校验的 Command API。

相关资料：[数据库驱动层](../architecture/backend/drivers.md) · [Driver API](../../packages/driver-api/README.md) · [驱动选择与编译图](diagrams/datazen-driver-build-workflow.html)
