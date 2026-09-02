# DataZen 架构设计（六）：编译时 Driver 与运行时 Extension 为什么并存

> DataZen 有两种“扩展”：编译时数据库 Driver，和运行时 UI/主题 Extension。它们都叫扩展，却解决不同问题；把两者强行合并，反而会模糊安全边界。

## 两种扩展的分工

Driver 扩展的是“DataZen 能连接和操作什么”。它需要建立真实数据库连接、持有连接池、执行协议，通常由 Rust crate 实现并在构建时链接。

Extension 扩展的是“用户如何组织和呈现能力”。它贡献工作区页面或主题，在运行时安装，以沙箱 iframe 加受控桥接运行。

| 维度 | Driver | Extension |
| --- | --- | --- |
| 载入时机 | 编译时 | 运行时 |
| 主要内容 | 数据库协议、方言、Command | 页面、主题、资源 |
| 执行位置 | Rust 进程 | 沙箱 iframe + Host Bridge |
| 数据库访问 | 直接持有连接能力 | 仅经权限桥调用 Command |
| 安装来源 | Cargo / Git Driver | plugins 目录中的包 |

## Extension Manifest v2

运行时插件以 `manifest.json` 声明身份、页面、主题和权限。Manifest 使用 camelCase、严格字段校验和 ID 规则，未知字段不会静默接受。

权限采用 deny-by-default：

- `context:connections`：读取连接摘要；
- `command:invoke`：调用受控 Driver Command；
- `storage:local`：读写插件自己的小型存储；
- `ui:notify`：向宿主发出通知。

插件默认没有数据库连接对象，也没有任意文件系统权限。

## 沙箱与桥接

页面在 opaque-origin iframe 中运行。Host 通过 `postMessage` 发送 `host.ready`，插件以 `plugin.ready` 完成握手，所有请求带 `reqId`，响应必须回显它。

桥接层会检查消息信封、目标插件、权限和并发限制。`command.invoke` 最终转发到 `execute_driver_command`，并记录命令名与连接 ID 的审计信息，但不把 SQL 参数写入日志。

这种设计让插件可以使用统一 Command 能力，却不能直接绕过权限访问连接池或本地文件。

## datazen:// 资源协议

插件静态资源通过 `datazen://{pluginId}/{path}` 提供。Rust 端先验证插件已启用，再进行路径遍历防护、MIME 白名单和 CSP 注入。Windows WebView2 的映射形式与 macOS scheme 行为不同，因此 CSP 同时兼顾 `'self'` 和自定义 scheme。

深链 `datazen://{pluginId}/open?page=...` 只转换成宿主的 `plugins:open-page` 事件，页面路由仍由 Host 控制。

## 主题是 Extension 的贡献

主题不再从旧的独立 `{appData}/themes/` 入口加载，而是作为 Extension 的 `contributes.themes`。主题可以提供 tokens CSS，以及可选的编辑器、图表和图标资源。某一资源切片加载失败时只降级该切片，不影响整个应用启动。

宿主在主题切换或明暗模式变化时，通过桥推送新的 `{dark, tokens}` 快照；插件页面不应自行猜测宿主主题。

## 安装、升级和卸载

安装包先解压到 `.datazen-staging-*` 临时目录，完成 Manifest、路径、文件数量和大小校验后再原子改名。同 ID 重装会留下备份；卸载会删除插件目录及其 `.storage.json`，因此 UI 必须明确确认这是不可恢复的用户数据删除。

## 为什么不让 Extension 承载 Driver

数据库 Driver 需要原生库、网络连接和长期资源，放进任意运行时 iframe 会同时破坏性能、权限和跨平台可控性。反过来，Driver 也不应该携带页面和主题，否则每个数据库插件都会变成一套 UI 框架。

编译时 Driver + 运行时 Extension 的组合，分别把协议稳定性和 UI 可扩展性放在合适的层。

## 结语

两种扩展机制不是重复建设，而是两个不同的信任域。Driver 负责连接能力，Extension 负责呈现能力；Command Runtime 是它们之间的受控交汇点。下一篇将回到桌面体验本身，说明 DataZen 为什么把大多数功能收进统一工作区，只保留少量真正需要原生窗口的页面。

相关资料：[Extensions 架构](../architecture/backend/extensions.md) · [驱动架构](04-pluggable-database-drivers.zh-CN.md)
