# Desktop / Web 按需使用应用层（原 §8）

> 本文是[DDD 模块化重构总纲](../ddd-refactor-plan.md)的细节文件，承接原 §8 的全部内容；文档权威与阅读路线见总纲 §1。

## 8.1 模块级装配，不再强制全量 ApplicationServices

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

## 8.2 三种“可用”必须区分

1. **编译能力**：feature 是否包含该模块及其依赖。
2. **部署能力**：宿主是否成功装配、注册路由并具备所需资源。
3. **用户权限**：当前请求是否获准执行此操作。

功能未装配时，不返回成功空结果；Web 可不注册该路由，保留的兼容 IPC 则返回既有的明确不支持/未配置错误。前端能力显示与真实装配结果一致。即使 UI 隐藏入口，后端仍须授权。

可选 AI Provider 初始化失败不应让不依赖它的连接列表失效；执行必需依赖缺失则不能宣称执行可用。失败范围按依赖图测试，不能通过回退旧 Store 绕过。

## 8.3 Cargo feature 策略（目标，D1 起实现）

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

## 8.4 Web 范围与交付声明

本轮不启动 Web 产品实现（2026-10-26 用户决策：Web 版移出本次重构范围，延后到后续版本交付）。届时先交付同一条连接/表浏览链路的真实 Web 适配，再按平台计划扩展功能与多 worker。

只有 fake HTTP handler 或第二个内存 Repository，不足以宣称“双宿主上线”：至少要有真实身份上下文、组织隔离、管理库存储、错误映射、启用/禁用模块和一次数据库操作的集成验证。

桌面已迁移、Web 未实现时，可以明确交付“桌面模块化版本”；不能把它标为本方案的“双宿主验收完成”。Web 安全范围仍按[团队 Web 服务设计](../../architecture/platform/team-server-and-auth.md)执行，旧文中的 server 目标目录统一对应这里的 web。
