# 平台扩展 Playbook（form-split F5 交付物）

> 新增域客户端 / 新增平台能力时的操作手册。决策树回答三个问题：这个能力属于哪一层？改动半径有多大？测试怎么铺？

## 0. 前置地图

```
UI / stores / hooks          ← 只认域客户端与端口，永不 import platform/desktop
  │
  ├─ packages/backend-client  域客户端（F3）+ BackendTransport + PlatformServices(§7.1)
  │     └─ transport 适配器    桌面: tauriBackendTransport / web: 未来的 httpBackendTransport
  │
  ├─ src/platform/contracts   WindowRouter / EventChannel / DesktopHostServices
  │     / HostSettingsPort / TablesExporterPort / PlatformCapabilities + configurePlatform
  │
  ├─ src/platform/desktop     桌面区（R7 禁区，只有装配与桥可进）
  └─ src/platform/web         web 区（F5 stub；未来 web 装配根）
```

## 1. 决策树：一个新能力落在哪一层

按顺序自问：

1. **它是 kernel 数据面的方法吗？**（连接/会话/执行/任务/工件/表数据——`system-overview.md` §6.2 表内的领域）
   → 是：在对应域客户端的 `*MethodMap` 切片加键 + `*Client` 接口加方法 + 工厂加委托；
     `MethodMap` 是域 map 的交集，键自动入表（F3 断言）。
     Rust 侧命令名与 kernel 键 1:1（`commandOverrides` 已归零，不再新增 override）。

2. **它是宿主能力（对话框/剪贴板/外链）吗？** 且**两种形态都有语义**（web 有降级形态）
   → 扩 `PlatformServices`（backend-client §7.1）：加接口方法 + 桌面 impl + web stub impl
     + `platformServices.test.ts`。`openExternal` 是先例（F2 第 9 个原子能力）。
     ⚠️ 修改 §7.1 须同步 `shared-boundaries-and-ports.md`（扩展不修改既有成员语义）。

3. **它是纯桌面宿主能力吗？**（原生菜单/托盘/更新器/宿主目录）
   → `DesktopHostServices`（port 4）加方法，桌面区实现；共享代码经
     `HostSettingsPort`（port 5）或 capabilities 消费，永不直名引用（ruling #7）。

4. **它是窗口/跨窗口拓扑吗？** → `WindowRouter` / `EventChannel`（port 2/3）。

5. **它是「按形态给出的静态事实」吗？**（设置分区/有没有更新器/版本来源）
   → `PlatformCapabilities` 表（`src/platform/capabilities.ts`）加字段 +
     `DESKTOP_CAPABILITIES` / `WEB_CAPABILITIES` 各填一行 + 消费点从布尔/探针切到表。

6. **只是既有 legacy invoke 的搬家吗？**（命令已在、只是散落在 src）
   → 优先挂到 `HostSettingsPort` / 对应端口；命令本身属于 kernel 面的走 1。
     R6 台账（advisory）随迁移逐条烧毁。

## 2. 新增域客户端（F3 模式复刻）

1. `packages/backend-client/src/<domain>Client.ts`：`<Domain>MethodMap` 切片 +
   `<Domain>Client` 接口 + `create<Domain>Methods(transport, guard)` 工厂 +
   文件尾 `export type <Domain>ClientCoversMap = Assert<...>`（编译期覆盖断言，
   仿 `InsertMarkedStaysOptional` 先例）。
2. `clients.ts`：`DomainClients` 加字段 + `createClients` 加构造 + 若入 facade，
   `client.ts` 的 `BackendClient` extends 与展开同步。
3. `index.ts` 导出；`domainContractTables.test.ts` 加一行契约表
   （方法名 × 请求封套 × 响应直通）。
4. 有状态域（收据表/轮询）：工厂内闭包持态，参考 `jobClient.ts` 的
   idempotency receipt map。
5. 过渡逃生舱（driver-sdk legacy 命令）：进 `driverGatewayClient.ts`，
   snake_case 键名即命令名，kernel 实现后整域删除。

## 3. 契约测试怎么铺（F5 标准）

- **每域至少一份**：`domainContractTables.test.ts` 的表驱动行
  （fake transport 记录 `call`，断言 method/payload/response 三事实）。
- **端口各一份**「未注入显式错误 + 注入后行为」：`platformContracts.test.ts`
  （§7.4 语义：未绑定的 throw 必须指名装配根；inert 端口例外见 HostSettings）。
- **web stub 一份**：`webStub.test.ts`（throw 语义 + capabilities 表解析）。
- 能力缺口两分类都要测：`capability`（形态永远无）vs `notImplemented`
  （该后端未实现）——错误码同、`unsupportedKind` 异、UI 反应不同。

## 4. 改动半径速查

| 改动 | 必动文件 | 禁动 |
|---|---|---|
| 新 kernel 方法 | 域切片 + Rust 命令（1:1 命名） | commandOverrides（已删） |
| 新 §7.1 能力 | transport.ts 接口 + 两 impl + §7.1 文档 | 在 src 里 import @tauri-apps |
| 新 capabilities 字段 | capabilities.ts 两表 + 消费点 | 任何 backendId/形态探针 |
| 新桌面宿主命令 | hostServices.ts + HostSettingsPort | 共享代码直名 DesktopHostServices |

## 5. 红线（gate 复述）

- R6：src 非桌面区禁 `@tauri-apps/*`（台账 advisory，随迁移烧毁）。
- R7：`platform/desktop` 实体只在装配根与六座薄桥出现。
- R8：驱动层事件走 driver-sdk 事件桥（`bindHostEventListener`）。
- `backendId ===` 探针：归零（capabilities 时代用表，不用探针）。
