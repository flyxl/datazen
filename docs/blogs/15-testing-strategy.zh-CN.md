# DataZen 架构设计（十五）：可插拔系统的测试策略

> 可插拔架构最怕“Host 测试全绿，但某个 Driver 已经坏了”。DataZen 将测试按所有权分层：Host 验证编排和协议，Driver 验证方言和专属能力，E2E 验证用户旅程。

## 测试落点

| 层级 | 位置 | 关注点 |
| --- | --- | --- |
| Host Rust | `src-tauri` | Commands、Workflow、MCP、Store、窗口编排 |
| Host 前端 | `src/**/__tests__` | Store、组件、IPC 封装 |
| Driver Rust | `packages/drivers/<id>` | 方言、连接、Command、类型映射 |
| Driver UI/E2E | 驱动自己的 `ui/__tests__`、`e2e/` | 专属 UI 和真实数据库路径 |
| Host E2E | `e2e/specs/` | 通用连接、查询、导航和 IPC journey |
| 手工黑盒 | `test/` | 平台和复杂交互补充 |

规则很简单：只验证某个 Driver 的 SQL/KV 方言、专属 Command 或 UI，就写在该 Driver crate 内，不能把它塞进 Host。

## IPC 契约测试

前端测试验证命令名和参数形状，例如 `dbSessionId`、`command`、`input`、`database` 和 `schema` 的映射。Rust 测试验证会话不存在、输入无效、驱动不支持和 `CommandError` 转换。

命名测试尤其重要：camelCase 是 WebView 协议，snake_case 是 Rust 结构体字段，任何一侧改名都可能造成运行时而非编译期错误。

## 流式查询测试

`execute_query_stream` 需要测试：

- `executionStarted → statementStart → rows → statementEnd → done` 顺序；
- 多语句 index 不串线；
- 设置开启和关闭时的结果限制；
- 失败时仍写入正确的历史状态；
- 取消只允许拥有该 `executionId` 的会话；
- 所有终止路径都清理执行注册表。

这些测试应使用 Mock Driver 验证 Host 编排，不把 PostgreSQL 或 Redis 方言细节复制进来。

## Driver Contract Matrix

Host 可以定义通用 journey：连接、列出表、运行查询、查看结构、断开。`contract matrix` 再让多个 Driver 执行同一套契约，检查统一 API 的最低能力。

契约不是要求所有数据库行为完全相同。Driver 可以声明不支持 EXPLAIN、Offset 或取消；测试应验证能力声明和用户可见降级，而不是强迫每个数据库伪造实现。

## E2E 构建约束

Host E2E 必须使用 `pnpm tauri build --debug --features webdriver` 生成合格二进制，不能拿普通 Cargo build 代替。驱动 E2E 保留在驱动目录，默认 Host E2E 不应隐式启动所有数据库。

无法自动化的路径要登记到覆盖矩阵，说明原因和手工步骤，而不是把缺口当成“以后再测”。

## 安全回归

Manifest 未知字段、路径遍历、SVG 内容扫描、插件存储隔离、MCP token、连接白名单和日志脱敏都应有回归测试。安全测试的目标是拒绝越权输入，而不仅仅是验证 happy path。

## 结语

分层测试让架构边界成为可执行的约定：Host 测协议和编排，Driver 测数据库知识，E2E 测用户旅程。这样新增 Driver 不会迫使 Host 复制方言测试，也能在统一 Command Runtime 发生变化时快速发现影响面。最后一篇将讨论这套桌面架构如何逐步演进为可共享 Core 与 Web 平台。

相关资料：[测试策略](../architecture/testing.md) · [E2E 测试](../development/e2e-testing.md) · [驱动测试落点](../architecture/testing.md#11-驱动测试必须写在驱动 crate 内)
