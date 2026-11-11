# Rune 原生 UI 迁移实施方案

> 文档类型：迁移实施方案（过程文档）
> 
> 评估日期：2026-11-12
> 
> DataZen 基线：本分支创建时的 main 提交 `2c424b6238356f1b987635bfacfd502f613865a1`
> 
> Rune 评估基线：`anomalyco/rune` dev revision `2e4999f67064dd351e504bbdaaf71f0b601e2114`
> 
> 当前状态：**方案已提交；尚未开始代码迁移。先完成 SQL Editor 与 DataGrid 技术验证，再决定是否进入批量迁移。**

## 1. 目标与结论

目标是让 DataZen 的桌面端与 Web 端使用**同一套 Rust UI 业务代码**：

- 桌面端：Rune 原生渲染，最终不再把主应用 UI 放在 Tauri WebView 中。
- Web 端：同一套 Rune Rust UI 编译为 WASM，通过 WebGPU 在浏览器 Canvas 中渲染。
- 后端：继续复用现有 DataZen Rust 业务内核、数据库驱动和团队 Web 服务，不因 UI 框架迁移重写业务能力。
- 迁移过程：新旧 UI 并行，按用户旅程逐步替换；在功能和平台能力达到验收标准前，不删除 React 实现。

**建议采用“共享 Rune UI + 双平台 Host Adapter + 现有业务内核”的结构，而不是把 Tauri、Axum、数据库驱动或后端服务改造成 Rune 的一部分。**

Rune 的 Web 目标目前仍是实验性 WebGPU Canvas。它已支持 WASM 构建、浏览器文本输入和 DOM 可访问性镜像，但仍不支持 Web overlays、额外窗口、原生菜单、拖出窗口、网络层等能力。因此，Rune 迁移在技术上值得做 POC，但现在还不能据此认定它已具备完整承载 DataZen 的条件。

## 2. 当前 DataZen 架构基线

以下结构来自本分支对应的仓库代码与架构文档，迁移时应以代码为准：

| 当前模块 | 职责 | 迁移约束 |
|---|---|---|
| `packages/frontend` | React 共享 UI、Zustand Store、hooks、i18n、平台端口 | 逐步由 Rust/Rune UI 替换；不能一次性删除 |
| `packages/desktop` | 桌面 React/Tauri 壳 | 迁移期保留，最终只保留必要的兼容宿主能力或退出主 UI 路径 |
| `packages/web` | Web 壳、HTTP/SSE 传输、路由装配 | 迁移期保留为 WASM 启动与 Web Host Adapter |
| `packages/backend-client` | Web HTTP 域客户端与契约的唯一 TypeScript 来源 | 不得在 Rust UI 中再手工维护一套 108 路由/112 方法的 HTTP 契约 |
| `crates/datazen` | 唯一共享业务内核，包含领域、用例和运行时能力 | 保持形态中立；不依赖 Rune、Tauri、Axum 或 React |
| `crates/datazen-desktop` | 当前 Tauri Desktop Host：启动、命令、持久化、驱动装配与平台适配 | 提取可复用的 Tauri 无关 Host 能力，避免 Rune Host 复制一份桌面运行时 |
| `crates/datazen-server` | Axum 团队 Web 服务，提供认证、策略和 HTTP API | 继续作为 Web 的服务端入口；不依赖 UI |
| `crates/drivers/*` | 数据库驱动 | Driver API 与数据库能力保持不变 |

关键架构事实：

1. `crates/datazen` 是唯一业务内核；Desktop Host 与 Server Host 是两个组装根，二者互不依赖。
2. Tauri command 与 HTTP handler 都是同一业务用例的薄绑定。UI 不应绕过应用用例直接访问 Store、Driver 或 Runtime 内部实现。
3. Web HTTP 面以 `packages/backend-client` 的域客户端为契约来源，当前为 15 个域、112 个域方法、108 个服务端路由。
4. 当前前端 Store 已按领域拆分，包含连接、Schema、Table Data、Panel/Query、Dashboard、WAPP、Settings 等；迁移时不能把这些领域状态重新塞进单一全局 UI 状态对象。
5. 当前 DataTable 并非简单 HTML 表格：已有虚拟行、单元格/行选择、键盘导航、单元格编辑器族、剪贴板矩阵、粘贴规划、脏数据暂存、分页、排序和外键导航等行为。结果网格写回还具有独立的安全与准入逻辑。

## 3. Rune 当前能力与约束

本节按 2026-11-12 可见的 Rune 上游资料评估，不把尚未验证的能力视为已具备。

### 3.1 可利用能力

- Rust UI 描述与状态管理，原生端由 GPU Renderer 绘制。
- macOS、Windows、Linux 原生运行；Web 目标可编译为 WASM。
- Web 使用一个 WebGPU Canvas，复用 Rust views、layout、glyph atlas 和 shaders。
- Web 文本输入已通过隐藏 textarea 处理浏览器输入、组合输入、预测性替换和 UTF-16 到 UTF-8 边界转换。
- 支持复制/粘贴、URL 打开、窗口可见性和安全输入等 Web 适配能力，但需要遵守浏览器用户手势与权限限制。
- 提供 AccessKit、元素树检查与 headless 渲染能力，可用于无窗口视觉回归。
- Rune 提供 `Latest<T>` 等异步结果管理原语，可辅助处理最新请求优先的 UI 场景；它不能替代 DataZen 自身的 session 身份、请求代次、取消、幂等和过期结果写入屏障。

### 3.2 已知限制

| 能力 | 当前上游状态 | 对 DataZen 的影响 |
|---|---|---|
| Web 渲染 | 实验性 WebGPU Canvas；无 WebGL fallback | 浏览器必须具备可用 WebGPU；需要明确不支持时的回退方案 |
| Web overlays | 不支持 | 补全菜单、悬浮菜单、Tooltip、Popover 等必须验证；优先实现为 Rune 场景树内的锚定面板，必要时由 DOM Host 叠加 |
| Web 多窗口 | 不支持额外窗口 | Schema Diff、Transfer、Settings 等当前独立窗口流程需改为同一 Rune App 内部路由/面板，或暂时保留外部页面 |
| Web 原生菜单 | 不支持 | 菜单栏、系统级菜单与应用内菜单需要分别设计 |
| Web 拖出窗口 | 不支持 | 不得假设桌面拖放语义在 Web 可用；需做能力探测与替代交互 |
| Web 网络层 | Rune 本身不提供 | 由 Web Host Adapter 继续复用 `packages/backend-client` 和 SSE |
| Web 字体 | 不扫描系统字体目录 | 必须为中文、日文、韩文、Emoji 和需要的字重提供字体文件；不能依赖用户机器安装字体 |
| Web 原生文件对话框 | 不支持 | 由浏览器文件输入/下载能力实现，或提供页面级 Host API |
| Web 热补丁 | 不支持 | 不作为生产能力依赖 |
| Native Shell | 当前以 winit 为主，Rune 路线图仍在演进平台输入壳 | 必须真实验证 macOS/Windows/Linux 输入法、焦点、拖放、窗口行为，不把 winit 兼容性等同于系统级体验完成 |

Rune 的 `web_document()` 能生成包含 landmarks、标题、段落、链接和图片的 DOM 镜像，适合内容型页面；DataZen 是高交互应用，应优先评估 SPA 内全视口 Canvas 的挂载方式，并单独验证屏幕阅读器与键盘可访问性。DOM 镜像的存在不等于复杂 DataGrid 和 SQL Editor 已具备完整无障碍支持。

### 3.3 SQL Editor 风险

Rune 的公开 `TextEdit` 状态模型可见文本与 anchor/head 选择位置，但目前没有确认其具备 DataZen 所需的完整多光标/矩形选择能力。不能把通用 TextEdit 或文本输入框视为 CodeMirror 的替代品。

SQL Editor POC 必须逐项验证：

- 多光标：添加/移除光标、下一处匹配、向上/向下添加光标、同时编辑、撤销/重做。
- 矩形/列选择、跨行选择、行号 gutter。
- 多行 SQL、禁用软换行、水平滚动、垂直滚动与光标自动显露。
- SQL 语法高亮、错误/警告诊断、执行位置标记。
- 补全、签名帮助、悬浮信息、跳转和快捷键；Web overlays 不可用时必须验证替代实现。
- 语句选择、选区执行、格式化、查找替换、Snippet、查询历史。
- 中文 IME、组合输入、Emoji、Surrogate Pair、粘贴大段 SQL、撤销粒度。
- 编辑器与 Query Panel、执行任务、结果网格之间的异步状态同步。

任何一个核心编辑行为无法达到当前 DataZen 的可用程度，都不得开始替换主 SQL Editor。

### 3.4 DataGrid 风险

Rune 的虚拟列表/虚拟行能力不等于数据库管理工具的完整 DataGrid。当前 DataZen DataTable 的行为必须作为迁移验收基准，包括：

- 大数据量虚拟滚动，滚动时稳定的行/列身份。
- 单元格、行、矩形区域选择及键盘导航；焦点、活动单元格和选择范围不能依赖 DOM 节点存活。
- 类型化 Cell Editor：文本、数字、布尔、枚举、时间、JSON、二进制、长文本等。
- 单元格编辑准入、只读原因、暂存脏数据、批量提交、失败回滚/重试。
- 多单元格复制/粘贴、TSV 矩阵解析、粘贴预览/确认和批量写入。
- 排序、过滤、分页、默认排序、外键跳转、列宽调整、隐藏列和列顺序。
- 查询结果网格写回的安全判定，与普通表数据编辑保持不同的准入策略。
- ARIA Grid、键盘焦点、屏幕阅读器描述和 E2E 可定位性。

在 Rune 中实现前，先确认现有 DataTable 的每种状态由谁拥有、如何持久化、如何与后端 mutation/receipt 对应；不能只复刻视觉。

## 4. 目标架构

### 4.1 结构

建议新增下列模块。名称是建议，实施时先检查是否已有可复用 crate，避免重复抽象。

| 模块 | 职责 | 允许依赖 |
|---|---|---|
| `crates/datazen-ui-api` | UI 使用的语义化应用端口、能力声明、异步事件和必要的 UI DTO | 纯 Rust；复用现有共享 ID/契约 |
| `crates/datazen-ui` | 唯一 Rune UI：App Shell、领域状态、导航、面板、组件和用户旅程 | `datazen-ui-api`、Rune；不依赖 Tauri/Axum/React/具体 Driver |
| `crates/datazen-rune-desktop` | 原生启动入口、winit/WGPU Host、桌面能力适配 | `datazen-ui`、桌面 Host Adapter |
| `crates/datazen-rune-web` | WASM 启动入口与 Rust/JS 边界 | `datazen-ui`、WASM 目标适配 |
| `packages/web/src/rune-host/` | Canvas 挂载、WASM 加载、HTTP/SSE/认证/浏览器 API 适配 | `packages/backend-client`、现有 Web Host |

桌面侧还需要从当前 `crates/datazen-desktop` 中抽取 Tauri 无关的启动、持久化、Runtime、Driver 注册与 Host 服务装配能力。抽取后，Tauri Host 和 Rune Native Host 调用同一套 Desktop Application/Infrastructure；不能在新 Rune Host 中再复制一套连接管理、Runtime、驱动注册和 Store。

建议目标数据流：

    Native Desktop
      Rune UI (datazen-ui)
        └── datazen-ui-api
              └── Desktop Adapter
                    └── Desktop Application / datazen core / Driver

    Web Browser
      packages/web (WASM bootstrap + Web Host Adapter)
        └── Rune UI (same datazen-ui)
              └── datazen-ui-api
                    └── JS bridge
                          └── packages/backend-client + SSE
                                └── datazen-server HTTP API
                                      └── datazen core / Driver

Web 的 React/TypeScript 代码在迁移期不再承担主要业务 UI，但保留为浏览器 Host、HTTP/SSE 契约客户端、认证桥和 WASM 启动器。长期是否删除 React 依赖，应在所有用户旅程迁移并完成插件兼容评估后再决定。

### 4.2 UI API 与平台能力边界

`datazen-ui-api` 面向用户操作设计语义化端口，不复制 backend-client 的 HTTP MethodMap，也不建立一个新的“万能 command invoke”。

建议按领域划分窄接口，例如：

- `ConnectionApi`：连接配置、分组、连接/断开。
- `SchemaApi`：数据库、Schema、表和对象元数据。
- `TableDataApi`：数据读取、分页、排序、过滤、编辑准入和变更提交。
- `QueryApi`：查询执行、取消、状态订阅、结果读取。
- `SettingsApi`：应用设置及能力差异。
- `WorkspaceApi`：Panel、Tab、任务与工作区资源。
- `ExtensionApi`：扩展清单、贡献点和权限。

平台服务独立定义，例如文件选择/导出、剪贴板、通知、URL 打开、系统凭据、窗口和文件系统。每项能力必须声明是否支持、限制条件和失败语义，UI 不得通过猜测平台类型来决定功能是否可用。

桌面 Adapter 直接调用共享 application use case，并通过 Desktop Host 提供 Store、Driver、凭据、Runtime 等基础设施。Web Adapter 继续调用 `packages/backend-client` 的唯一 HTTP 契约，并消费现有 SSE 事件。不得让 Rust/WASM 再手工维护一份 API 路由、请求 DTO 和响应 DTO；如果未来要让 Rust 成为 HTTP 契约唯一来源，应另行完成契约生成设计，而不是在本次 Rune UI 迁移中暗中复制。

### 4.3 状态与异步模型

- UI 交互状态归 `datazen-ui`，领域资源状态由对应的 API/Store 投影提供；同一事实只能有一个权威状态源。
- 按 connection、session、panel、execution 等身份分片，不得建立一个跨所有页面的巨型 State。
- 保留 `connectionId`、`dbSessionId`、`executionId` 的语义区别；不得把连接配置 ID 当成数据库运行时 Session。
- 任何异步读写都必须校验上下文、请求代次和实体 revision；切换连接、关闭 Panel、断开 Session 后，过期响应不能写入新状态。
- 查询执行必须保留取消、终态确认、关闭阻塞和资源释放顺序。Rune 的 latest-wins 请求能力只解决一类 UI 请求竞态，不替代 DataZen 的 execution lifecycle。
- 需要跨 Host 传递的事件必须有稳定的事件类型、序列/版本、取消和错误语义；禁止向 UI 暴露 Driver 内部对象或敏感连接信息。

## 5. 迁移实施顺序

### 阶段 0：固定基线与双 Host 骨架

1. 在独立分支固定 Rune revision：`2e4999f67064dd351e504bbdaaf71f0b601e2114`。Rune 尚无稳定 Release，升级只能通过显式 revision 更新。
2. 记录 Rust toolchain、Rune Cargo features、native/WASM 构建命令和依赖体积；Web 构建禁用 `dev`、`hot`、`native-file-dialog` 等不适用于 wasm 的 features。
3. 创建最小 `datazen-ui`、Native Host 和 WASM Host，分别跑通 Rune hello、Canvas mount、窗口关闭与资源释放。
4. 在 CI 增加 native build、`wasm32-unknown-unknown` build、Web smoke test；不修改现有 React/desktop/web 默认入口。
5. 对 WebGPU 不可用的浏览器保留旧 Web UI 或清晰的兼容提示，不允许白屏。

**阶段 0 的输出是构建骨架，不代表 DataZen 已完成 Rune 迁移。**

### 阶段 1：两个技术 POC（迁移决策门）

优先完成 SQL Editor 与 DataGrid，而不是先做 Button、Dialog 或普通 CRUD 页面。

**POC A：SQL Editor**

使用真实 DataZen SQL 编辑场景，对照当前 CodeMirror 行为逐项测试。至少覆盖 §3.3 的多光标、矩形选择、无换行水平滚动、补全、诊断、IME、撤销/重做和执行状态。

**POC B：DataGrid**

使用真实 DataZen 表数据和查询结果，覆盖 §3.4 的选择、虚拟滚动、类型化编辑器、矩阵粘贴、批量写回、分页/排序/过滤、只读判定和外键跳转。

两个 POC 都要在 macOS、Windows、Linux Native 与至少 Chromium WebGPU 环境验证。Web 还需验证真实 Safari/WebKit 与 Firefox 的 WebGPU 支持状况；浏览器模拟不能代替真实设备输入法测试。

**决策规则：**

- 核心交互完整且性能不低于现有实现：继续阶段 2。
- 只有局部能力缺失且可控：评估 DataZen 自有适配组件或维护 Rune fork。
- 多光标/矩形编辑、DataGrid 粘贴写回、Web 输入法/可访问性等关键能力无法可靠完成：暂停全量迁移，不要启动大规模页面重写。

### 阶段 2：端到端纵向切片

先完成一条闭环旅程：

    启动应用
      → 建立/识别用户身份
      → 打开连接管理
      → 连接数据库
      → 展开 Schema/对象
      → 打开只读表数据
      → 加载、错误、重试、断开与重连

同一组 UI 页面必须运行在 Native 与 Web 两种 Host。该阶段重点验证 API 端口是否足够、桌面运行时能否复用、Web Adapter 是否完整，以及连接/session 身份是否保持一致。

### 阶段 3：基础 Shell 与核心工作区

按顺序迁移 App Shell、导航树、Tab/Panel、Schema Explorer、对象详情、Data Browser、SQL Editor 和查询结果。新增功能先在 Rune 端实现，已迁移旅程不得再同时维护两个版本的业务规则。

对 Web 不支持的原生窗口行为，以 Rune 单 Canvas 内部路由或分栏工作区替代；不要在 UI API 中假装 Web 有额外原生窗口。

### 阶段 4：领域页面

逐类迁移 Redis/Key-Value、Document DB、Dashboard、Workflow、ER Diagram、Schema Diff、Data Sync、Data Transfer、Backup、Settings 等页面。每一类按用户旅程迁移并复用现有后端用例，不以“页面能打开”作为完成标准。

多窗口功能应明确新的交互模型：同一窗口内多 Panel/Tab、浏览器独立路由页面或保留旧 Host 页面。不能只把原来的 Window 组件机械地改成 Rune View。

### 阶段 5：Driver、EP、WAPP 与 Theme 兼容

保留 DataZen 四类扩展边界，不把它们统统改成 Rune 插件：

| 扩展类型 | 迁移策略 |
|---|---|
| Driver | `datazen-driver-api`、Rust 驱动注册与数据库能力保持不变。当前 Driver React UI 贡献需要抽象成中立的贡献描述/能力模型，再由 Rune Host 渲染；过渡期允许旧 UI fallback |
| 静态 Theme | 保留 Theme manifest；增加 Rune token/theme JSON 映射。旧 CSS 继续服务尚未迁移的 WAPP |
| 特权 EP | 不在 Rune 中运行任意 React/ESM。迁移为受信任的 Rust trait/静态注册；SQL Editor Pro 放入独立私有 Rust crate，Community 使用默认空实现 |
| WAPP | 保留 iframe sandbox、manifest、权限、postMessage JSON-RPC 与现有 CSP。Rune Canvas 不能内嵌任意 DOM iframe；WAPP 继续作为独立 WebView/页面/窗口打开，不得为了视觉统一而取消沙箱 |

不建议引入 Rust 动态库插件 ABI；优先采用静态链接与版本化的 Rust Extension API。Driver、Theme、EP、WAPP 的权限与生命周期继续正交。

### 阶段 6：切换默认入口与清理旧 UI

只有在以下条件全部满足后，才允许把 Rune 设为默认 UI 并逐步删除 React UI：

- 核心用户旅程在 Native/Web 均完成并有 E2E 覆盖。
- SQL Editor 与 DataGrid 达到现有产品交互基线。
- macOS、Windows、Linux 的 IME、焦点、快捷键、剪贴板、拖放和高 DPI 通过验证。
- WebGPU 不可用时有明确回退；Web 可访问性、键盘操作和浏览器输入通过验证。
- Driver UI、Pro EP、Theme 与 WAPP 有可执行的兼容方案。
- Community/Pro 两种构建均通过；旧 UI 不再是唯一可用入口。
- 新旧 UI 之间没有重复执行查询、重复提交写入或双重持有 session 的情况。

删除 React/CodeMirror/Tauri 主 UI 依赖是最后一步，不是迁移起点。

## 6. 平台专项要求

### Desktop

- Rune Native Host 负责窗口、事件循环、GPU 渲染与 Rune App 生命周期。
- 桌面业务装配与持久化从 Tauri UI 宿主中抽取，不能由新 Host 再造一份。
- 更新器、系统钥匙串、文件选择器、通知、协议注册、多窗口、拖放等平台服务通过明确的 Desktop Adapter 暴露。
- Tauri 在迁移期可以继续承载旧 UI/WAPP 或兼容窗口；Rune 主 UI 完成之前不删除其能力。
- 验证窗口关闭、最小化、全屏切换、多显示器、高 DPI、窗口恢复、系统休眠恢复、IME、快捷键和剪贴板。

### Web

- 由 `packages/web` 保留 SPA 路由、认证、HTTP、SSE、浏览器能力与 WASM 加载。
- Rune 使用 `App::canvas_id(...)` 挂载在现有页面提供的 Canvas；路由离开时明确调用退出流程并释放 GPU 资源、监听器与页面样式。
- WebGPU 需要安全上下文（本地 localhost 或 HTTPS）；服务器正确返回 `application/wasm`，并提供 gzip/Brotli 压缩。
- 通过 `AppSpec::fonts` / `font_url` 显式提供中文字体、Emoji、必要字重；字体必须与浏览器 DOM 字体设置保持一致。
- 浏览器剪贴板操作必须从用户交互事件发起；敏感内容复制不承诺绕过浏览器权限或系统剪贴板历史。
- Web 不支持的文件对话框、原生菜单、额外窗口和拖拽外部窗口必须展示能力差异或提供页面级替代交互。
- 保留旧 React Web UI 作为不支持 WebGPU 环境的 fallback，至少到 WebGPU 浏览器支持策略和用户升级策略明确为止。

## 7. 验证与质量门禁

| 类别 | 必须验证 |
|---|---|
| 编译 | Native macOS/Windows/Linux；WASM release build；Community/Pro 构建 |
| UI 行为 | App Shell、Tab/Panel、快捷键、命令路由、弹层/菜单、焦点恢复 |
| SQL Editor | 多光标、矩形选择、语法高亮、补全、诊断、格式化、查找、IME、粘贴、撤销/重做 |
| DataGrid | 虚拟滚动、选择、键盘、编辑器、矩阵粘贴、排序/过滤/分页、暂存/提交/失败恢复 |
| 后端边界 | 用例复用、API 契约唯一、身份/权限、幂等、取消、过期响应屏障 |
| 平台 | WebGPU、剪贴板、文件、字体、通知、系统主题、窗口生命周期 |
| 无障碍 | AccessKit/DOM mirror、键盘可操作性、屏幕阅读器、焦点顺序、可访问名称 |
| 稳定性 | 长时间编辑、频繁切换连接、关闭运行中 Query、断网重连、窗口关闭和资源释放 |
| 性能 | 与当前 React/CodeMirror/DataTable 做同数据集对照；采集启动时间、交互延迟、滚动帧、CPU/GPU/内存和 WASM 包体积 |

测试必须使用真实用户旅程，不以单组件截图或 hello 示例通过代替功能验收。对 Rune 视觉回归可利用其 headless PNG 与元素树能力；浏览器端仍需 Playwright/WebGPU smoke test，并补充真实设备检查。

## 8. 风险与应对

| 风险 | 影响 | 应对 |
|---|---|---|
| Rune 仍是 pre-release，API 变化快 | 迁移期间可能发生破坏性变化 | 固定 commit SHA；升级单独 PR 并跑完整验证；必要时维护 DataZen fork |
| Rune 上游不接受 PR | 无法依赖上游及时合入 DataZen 专属修复 | 先向上游提交 Issue；无法等待时维护小型 fork，定期 rebase |
| WebGPU 覆盖率与浏览器差异 | Web 端可能无法启动或出现 GPU 问题 | 默认保留旧 Web UI fallback；明确最低浏览器矩阵 |
| SQL Editor 能力差异 | 影响 DBA/开发者高频工作流 | 先做 SQL Editor POC；不过关不迁移 |
| DataGrid 交互复杂 | 影响数据浏览和安全写回 | 以现有 DataTable 行为文档及结果写回规则为验收基线 |
| Native/Web Host 行为差异 | 相同 UI 在两端语义不一致 | 所有差异收敛到能力声明与 Platform Adapter |
| Driver/Pro UI 插件依赖 React | 新 UI 无法直接渲染既有组件 | 定义中立贡献协议，提供兼容期；不要把任意 React 插件塞进 Rune |
| WAPP 需要 DOM 沙箱 | Rune Canvas 无法原样容纳 iframe | 保留独立 WebView/页面，不削弱 sandbox/CSP |
| Web 字体体积与 CJK 字形 | 首屏体积、缺字与渲染差异 | 字体子集化、按需加载；验证中文/Emoji 与 fallback |

## 9. 完成定义

“迁移完成”不是指一个 Rune 窗口能运行，也不是说桌面和 Web 能显示同一张页面。完成定义是：

1. Native 与 Web 由同一份 `crates/datazen-ui` 业务 UI 源码构建。
2. 两个 Host 只处理平台差异、能力注入和后端适配，不复制 UI 业务流程。
3. 所有核心数据库操作仍通过现有 DataZen application use cases 和权限边界完成。
4. SQL Editor、DataGrid、连接管理与核心工作区达到当前功能基线。
5. Driver、Pro EP、Theme、WAPP 均有明确兼容/迁移路径。
6. 原 React 主 UI 可以安全退役，且不影响 Web fallback、扩展安全或 Community/Pro 构建。

## 10. 参考资料

### DataZen 仓库

- [系统架构总览](../../architecture/README.md)
- [前端状态管理](../../architecture/frontend/state.md)
- [DataTable 实现事实](../../architecture/frontend/data-table.md)
- [前端扩展性](../../architecture/frontend/extensibility.md)
- [WAPP 运行时架构](../../architecture/backend/wapps.md)
- [Web HTTP API 契约](../../architecture/platform/web-http-api.md)
- [结果网格写回](../../architecture/backend/result-write.md)
- [仓库开发约定](../../../AGENTS.md)

### Rune 上游（评估基线）

- [Rune Repository](https://github.com/anomalyco/rune)
- [Rune Plan](https://github.com/anomalyco/rune/blob/2e4999f67064dd351e504bbdaaf71f0b601e2114/PLAN.md)
- [Rune Web Target](https://github.com/anomalyco/rune/blob/2e4999f67064dd351e504bbdaaf71f0b601e2114/WEB.md)
- [Rune Changelog](https://github.com/anomalyco/rune/blob/2e4999f67064dd351e504bbdaaf71f0b601e2114/CHANGELOG.md)
- [TextEdit implementation](https://github.com/anomalyco/rune/blob/2e4999f67064dd351e504bbdaaf71f0b601e2114/crates/rune/src/edit.rs)

上游功能状态以固定 revision 的源码和官方文档为准。升级 Rune revision 时，必须重新验证第 3、5、7 节涉及的能力和限制。
