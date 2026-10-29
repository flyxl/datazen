# 前端形态拆分：目标架构与实施计划（F1–F6）

> **状态：已确认的目标设计，尚未按本方案完成实现。** 确认日期：2026-10-28；本次代码核对基线：`main`，`c85e3bd`（含全部实测数字的核对基准）。
>
> 本方案回答三个问题：桌面 / Web 两种形态的前端代码如何组织；平台能力（窗口、对话框、事件、宿主配置）以什么契约进出共享代码；从当前 `src/` 单体到目标拓扑分几步走、每步的验收与回滚是什么。它是对[共享边界与端口详细设计](../architecture/platform/shared-boundaries-and-ports.md) §7（`BackendClient` / `PlatformServices` 注入）的**前端落地路径**，不重新定义协议本身。
>
> 阶段编号使用 **F1–F6**，避免与 DDD 模块化重构的 D0–D8、平台产品计划的 P0–P10 混淆。每个 F 阶段至少一个 PR；「PR-1…PR-6」与 F1–F6 一一对应。
>
> **已确认决策（按裁决时间顺序，全部为最终裁决，后续文档不再重新讨论）**：
>
> 1. 共享前端代码不得假设 desktop / web 后端形态，平台特有功能除外。
> 2. 命名消灭 `shared` / `kernel` 特殊词：统一 `client.getSettings()`；形态以 `DesktopClient` / `WebClient` 区分；`DesktopSettings extends Settings`；Rust 命令中性化（`get_shared_settings` → `get_settings`、`platform_list_profiles` → `list_connections`）。消解顺序：前端切 facade → 删 legacy 命令 → 改名 → `commandOverrides` 表归零。
> 3. 删除 NativeTool（部署锚定的 `pg_dump` / `mysqldump --version` 探针，SHA-256 白名单默认空即拒绝一切，产品零使用）。
> 4. 不实施 Web 形态，但打好基础（编译期同形、端口完整、契约可测）。
> 5. 平台形态在**编译期**确定；消灭一切运行时形态探测（`__TAURI_INTERNALS__` 三元、`backendId === 'desktop'` 分支）。浏览器 dev 模式的便利分支用 Tauri vite 插件的编译期常量（`import.meta.env.TAURI_ENV_*`）替代 `isTauri()`，不恢复运行时探测。
> 6. `BackendClient` 按 DDD 拆为域客户端（参照 `modules/` crate 的域划分）。
> 7. 共享代码只依赖共享接口：宿主叠加能力（`platform.host.config.get()` 一类）由平台组合根在装配时注入，桌面注入实现、Web 注入空缺省——共享代码不静态 import 任何宿主实体，因此 Web 构建在**类型层面**就不存在这些名字，不存在「运行时报不报错」的问题。
> 8. 前端代码组织参照后端拓扑：`packages/frontend`（共享 lib）+ `apps/desktop`（宿主 app，未来 `apps/web`），与 `modules/`（共享 Rust）↔ `src-tauri/`（桌面宿主）镜像。
> 9. `@datazen/frontend` 必须是**完整 workspace 包**（真实 pnpm 协议消费），不是 vite alias——当前 TS 侧根本没有 workspace 协议，本方案将第一次建立它。
> 10. `@datazen/frontend` 的合法消费者只有 `apps/*`；其他任何包不得依赖它（闸门 R5 强制）。
> 11. 实施顺序：**先清场（F2 原地隔离），后分家（F6 目录拆分与包完整化）**。此项已裁决，不再复议。

## 1. 定位、权威范围与完成边界

### 1.1 阅读路线

1. 本文 §2–§4：问题、实测基线、差异分类——为什么是这套方案。
2. 本文 §3：目标拓扑与四个端口契约——方案本体。
3. 本文 §5：F1–F6 逐阶段工作项、退出标准、回滚——怎么做。
4. [共享边界与端口详细设计](../architecture/platform/shared-boundaries-and-ports.md) §7：`BackendTransport` / `PlatformServices` 契约原文（本方案 §3.3 与其逐字兼容，仅扩展不修改既有成员语义）。
5. [DDD 模块化重构总纲](ddd-refactor-plan.md)：Rust 侧对应改造；本方案 F1（NativeTool 删除）落在其既有包结构上。

### 1.2 文档权威划分

- **前端目标代码组织、端口注入机制、阶段顺序**：以本方案为准。
- **`BackendTransport` / `MethodMap` / `PlatformServices` 契约**：沿用 [shared-boundaries-and-ports.md](../architecture/platform/shared-boundaries-and-ports.md) §7.1，本方案不另立契约；§3.3 的扩展成员属于该文档「落地形态」层面的增补，合入时同步回该文档。
- **域客户端拆分（F3）**：方法清单以 [system-overview.md §6.2](../architecture/platform/system-overview.md) 的表为权威，本方案只定义拆分文件布局与组合机制。
- **当前实现位置与实测数字**：以基线 `c85e3bd` 为准；文中行号均为该基线核对值，后续漂移以符号名（函数 / 接口名）定位。

### 1.3 完成边界（目标 ≠ 现状）

本方案交付的是实施计划。`apps/`、`packages/frontend/`、`configurePlatform`、F2 的隔离目录均为**目标**，当前不存在；`pnpm-workspace.yaml` 的 `packages` 字段、`@datazen/*` 的 workspace 协议消费同样是目标。本文所有「已有」「已存在」的表述均指基线代码里真实存在并可直接复用的资产（§2.3 盘点）。旧方案历史通过 Git 查看，本文不保留过期指标与互相冲突的候选方案。

## 2. 问题陈述：双轨 IPC 与前端平台耦合

### 2.1 双轨架构回顾（摘要）

IPC 审查（272 个注册命令：266 常驻 + 6 `cfg(test)` 门控）确认的根因是**双轨**：

- **legacy 宿主命令**：前端直调 `invoke('get_settings')` 等，操作完整 `AppSettings`（= `SharedSettings` + 桌面特有字段：logPath / contextDir / monitor / mcpClientServers / 更新器等）；
- **kernel surface**：`modules/` crate 用例 + `packages/backend-client` 门面（`MethodMap` + `tauriBackendTransport.ts` 的 `commandOverrides`），操作 `SharedSettings` / `ProfileView`。

`commandOverrides` 的存在使 kernel 轨的真实用量对字面 `invoke` 搜索不可见——审查方法论规则：出现在 `commandOverrides` 中的命令一律视为 kernel 在用或预留（`platform_create/update/disable_profile` 即预留，非死命令）。死命令最终核定为 17（后随合并簇与删除项继续收敛，轨迹见 §8）。

### 2.2 前端耦合实测基线（基线 `c85e3bd`）

| 指标 | 数值 | 含义 |
|---|---|---|
| `src/` 生产 ts/tsx 文件 | 930 | 共享层的真实规模 |
| 其中 import `@tauri-apps/*` | 48（5.2%） | **94.8% 的文件今天已是平台中立**——「共享层」是依赖方向属性，不是目录 |
| `@tauri-apps/*` 引用总数 | 147 处 / 112 文件（含测试与驱动包） | 拆分：81 `api/core`（invoke 数据面）+ 66 控制面 |
| 控制面构成 | event 32 / window 11 / webviewWindow 5 / webview 5 / plugin-dialog 5 / api-app 3 / updater 3 / process 3 / menu 1 / os 1 | 真正的平台差异面 |
| 生产 `listen()` 订阅点 | 15 处 / 10 文件 | 事件能力的迁移面 |
| 运行时形态探测 | ~20 处生产代码 | F2 清除对象（§4.2 逐点清单） |
| `useBackendClient` 调用 | 21 处 / 7 文件，生产仅 4 文件 | F3 拆分的爆炸半径：database×11、query×2、settings×3、panelStore×1；UI 零改动 |
| TS 侧 workspace 协议 | **不存在** | `pnpm-workspace.yaml` 无 `packages` 字段；lock 文件无内部 importers；根 `package.json` 零 `@datazen/*` 依赖；6 个既有包的 `exports` 字段是装饰性的——全部跨包解析骑在 vite / tsconfig / vitest **三张 alias 表**上（`@datazen/backend-client` → `packages/backend-client/src/index.ts` 等）。真实 workspace 只在 Rust 侧（`Cargo.toml` members） |
| 设置页 section | 12 中 7 个桌面专属 | `settingsSections.ts` `DESKTOP_SETTINGS_SECTIONS` = logging / ai / prompts / mcpServer / mcpClient / tunnels / extensions |

已核实的现存缺陷（F2/F4 修复项）：

- `src/windows/settings/SettingsContent.tsx:75`：`const desktop = backend === null || backend.backendId === 'desktop'`——浏览器 dev（backend 未绑定，`null`）被误判为桌面形态。布尔探测已经出错的活证据。
- `packages/drivers/redis/ui/observe/PubSubPanel.tsx:3`：驱动包直接 `listen('redis-pubsub-message')`——驱动包边界违规（闸门 R4 只覆盖 `packages/ui`，缺口见 §6.2）。
- `packages/drivers/redis/ui/__tests__/redisKeyWebContextMenu.test.tsx:5,9`：深路径爬回宿主 `src/`——`check-driver-import-boundaries.mjs` ALLOWLIST 中有 coordinator ruling 记录的合法豁免（driver↔host 集成夹具），注记标明 *remove together with the WebContextMenuHost exemption*。F6 以搬家注销。

### 2.3 已有基建盘点（可直接复用的资产）

本方案刻意**不发明新抽象**——以下资产构成方案的地基：

1. **`PlatformServices` 端口已存在**：`packages/backend-client/src/transport.ts:339`，与文档 §7.1 逐字一致（6 个方法：saveTextWithDialog / saveBinaryWithDialog / openTextWithDialog / openDirectoryWithDialog / writeClipboard / readClipboard）；配套 setter 注入 `setPlatformServices()` 与注册表、未注入显式报错（§7.4）语义，且 `platformServices.test.ts` 已有 rebinding 测试。桌面实现 `createDesktopPlatformServices()` 已在 `src/platform/tauriBackendTransport.ts:239`（`openDirectoryWithDialog` 为 declared-but-unimplemented，抛错而非静默）。**唯一缺口：生产代码从未调用 `setPlatformServices()`（grep 仅命中测试）——端口造好了，线没接。**
2. **三个文件已 embryonic 双轨**：
   - `src/lib/windowManager.ts`：Tauri 走 `invoke('create_sub_window')`（保证 `accept_first_mouse` 在原生层生效）vs 浏览器 `window.open` tab；singleton 去重聚合；
   - `src/lib/crossWindowBus.ts`：Tauri `emit`/`listen` 广播 vs `BroadcastChannel`；
   - `src/lib/windowKind.ts`：基于 URL 参数 `?window=`（6 现役 kind + 6 legacy alias 归一到 main）——浏览器天然兼容，无需改。
3. **编译期常量先例**：`src/lib/updater.ts` 的 `DATAZEN_VARIANT` / `DATAZEN_UPDATER_CHANNEL` 由 `scripts/resolve-drivers.mjs` 构建期生成——F5 的 capabilities 表沿用同一机制。
4. **能力 props 注入先例**：`@datazen/ui` `PathInput` 的 `onBrowse`（`src/lib/pathPicker.ts` 提供 `pickPath`），设计系统「never names a host runtime」——F2 将此模式推广为端口注入。
5. **bind 桥模式先例**：`driver-sdk` 的 `settingsStoreBridge` / `confirmDialogBridge`、`wapp-sdk` 的 theme 桥——包定义缝隙接口、宿主注入实现，是包在不依赖 frontend 的前提下获得宿主能力的既有合法通道（与 `setPlatformServices` 同构）。
6. **extension-points 注册先例**：`main.tsx:109` `extensionPoints.setTableSchemaProvider(...)`——窗口 chrome 经注册挂载的机制样板。
7. **闸门文化先例**：`scripts/check-driver-import-boundaries.mjs`（R1–R4，实测运行：2056 文件、0 blocking、1 advisory），含 ALLOWLIST 治理与「literal-complete 扫描」教训（Wave 2 有 8 个 `vi.mock` 违规从只扫 `from` 的基线漏网）。F 阶段新增规则全部挂进该脚本而非另起炉灶。

## 3. 目标架构

### 3.1 目标拓扑与后端镜像

```text
datazen/                        # 纯 workspace 根（pnpm packages + Cargo members）
├─ apps/
│  ├─ desktop/                  # 唯一允许 import @tauri-apps/* 的 TS 领地
│  │  ├─ index.html  window.html          # 双构建入口（主窗 splash + 子窗，现 vite rollupOptions.input 两项）
│  │  ├─ vite.config.ts  tauri.conf.json  tailwind/ postcss
│  │  ├─ src/
│  │  │  ├─ main.tsx            # 装配根：configurePlatform(...) + bindDesktopBackend()
│  │  │  │                      #   + setPlatformServices(...) + chrome 注册
│  │  │  │                      #   + __DATAZEN_HOST__ 表 + e2e store 钩子（原 src/main.tsx 全量迁入）
│  │  │  ├─ platform/           # 端口实现：tauriBackendTransport / platformServices(dialogs+clipboard)
│  │  │  │                      #   / windowRouter / eventChannel / hostServices / updater / os
│  │  │  └─ chrome/             # TitleBar / WindowControls / MenuBar / UpdateSection / splash 时序
│  │  │                          #   ——经 extension-points 注册进共享窗口页
│  │  └─ src-tauri/             # Rust 宿主（移入 apps/desktop/ 或原地改 frontendDist 指向，二选一）
│  └─ web/                      # 未来。现阶段最多放一个全端口 throw 实现的 typecheck stub
│                               #   （保住编译期同形校验 + 单消费者 lib 的诚实性保险，§7.2）
├─ packages/
│  ├─ frontend/                 # ← 原 src/。@datazen/frontend
│  │  ├─ windows/ components/ stores/ commands/ lib/ hooks/   # 882 个干净文件原地不动
│  │  ├─ platform/contracts.ts  # 四个端口接口 + configurePlatform + 未注入显式错误（§7.4 语义）
│  │  └─ index.ts               # 对 app 的导出面（App / 各窗口根 / configurePlatform）
│  └─ backend-client/ ui/ driver-sdk/ extension-points/ wapp-sdk/ drivers/…  # 既有，不动
├─ modules/                     # Rust 共享（DDD 重构域），不动
└─ scripts/ e2e/ packaging/ site/
```

与后端的镜像关系（用户裁决 #8 的落地）：`modules/`（共享 Rust）↔ `packages/frontend`（共享 TS）；`src-tauri/`（桌面宿主）↔ `apps/desktop`。`modules/src/application/` 的用例编排对应 frontend 的 stores/commands；`src-tauri/platform/` 的宿主适配对应 `apps/desktop/src/platform/`。

### 3.2 依赖方向规则

| # | 规则 | 现状违规 | 闸门 |
|---|---|---|---|
| D1 | `@datazen/frontend` 的合法消费者只有 `apps/*`；`packages/**` 不得 import `@datazen/frontend`，亦不得相对路径解析进 `packages/frontend/` | 生产代码**零违规**；唯二深路径引用是 redis 集成测试的 ALLOWLIST 豁免（F6 搬家注销） | R5（§6.2） |
| D2 | TS 侧 `@tauri-apps/*` 只允许出现在 `apps/desktop/`。比「放 desktop」更强：drivers 不是 desktop 的一部分，是被两形态共享的包，碰 Tauri 即违规（走 backend-client 事件流） | 48 文件（F2/F4 清场对象）+ PubSubPanel 驱动包违规 | eslint 闸门（§6.1）+ R4 范围扩展 |
| D3 | frontend 不得反向依赖任何 app；包间能力获取只走缝隙接口 + 宿主 bind 注入（`settingsStoreBridge` 先例），不开新依赖方向 | 零违规（既有） | R5 顺带覆盖 |

D2 的包级强化：`@tauri-apps/*` 依赖声明从仓库根 `package.json` 挪入 `apps/desktop/package.json` 后，「只有 app 能碰 Tauri」同时成为**包级依赖事实**——frontend 的 package.json 没有这个依赖，谁想 import 谁自己装，装了就会在依赖审查里露出来。

### 3.3 平台端口契约（四个接口 + 注入器 + capabilities）

设计原则：**砍掉大而全 `Platform` 接口，换成窄端口**。`hostConfig`（get_settings / rebuild_menu / log path）与 updater 是纯桌面、只被设置页桌面区块和 boot 消费——它们不进共享槽位，从具体平台目录导出，共享层类型上不可见（裁决 #7 的机制实现）。共享槽位只暴露三个窄端口；`DesktopHostServices` 是第四个接口但**只从 `apps/desktop` 具体导出**。

**命名消解**：`src/hooks/usePlatform.ts:3` 已占用 `Platform`（OS 维度：`'macos' | 'windows' | 'linux' | 'unknown'`，TitleBar 用于 mac 交通灯 vs win/linux 自绘）。形态与 OS 是两个正交维度（web 构建照样区分 mac/win 浏览器），因此：能力端口沿用仓库的 `PlatformServices`（扩成员）；原方案的 `DesktopPlatform` 更名 **`DesktopHostServices`**；`usePlatform` 的 OS 检测保留并迁入平台目录作为 `os` 能力（两形态通用）。

```typescript
// packages/frontend/src/platform/contracts.ts

// ── 端口一：宿主能力（扩展 backend-client 既有 PlatformServices，既有 6 成员逐字保留）──
export interface PlatformServices {
  // 既有成员（shared-boundaries-and-ports.md §7.1 原文，不动）
  saveTextWithDialog(input: SaveTextInput): Promise<boolean>;
  saveBinaryWithDialog(input: SaveBinaryInput): Promise<boolean>;
  openTextWithDialog(input: OpenTextInput): Promise<OpenedTextFile | null>;
  openDirectoryWithDialog(input: OpenDirectoryInput): Promise<OpenedDirectory | null>;
  writeClipboard(text: string): Promise<void>;
  readClipboard(): Promise<string>;
  // 扩展成员（合入时同步回 §7.1 文档）
  /** 外部打开：URL 用系统浏览器（web 退化为 window.open）；本地目录仅桌面有语义。 */
  openExternal(input: { url?: string; path?: string }): Promise<void>;
}

// ── 端口二：窗口路由（收编 src/lib/windowManager.ts，20 个导出签名保持不变）──
export interface WindowRouter {
  openSingletonWindow(label: string, options: OpenWindowOptions): void;
  openMultiInstanceWindow(label: string, options: OpenWindowOptions): void;
  focusMainWindow(): Promise<void>;
  hasOpenChildWindows(): Promise<boolean>;
  /** 全屏查询/事件与窗口背景色（web 实现为降级 no-op 或 F11 语义）。 */
  readonly windowChrome: {
    isFullscreen(): Promise<boolean>;
    onFullscreenChanged(cb: (fs: boolean) => void): Promise<() => void>;
    setBackgroundRootColor(hex: string): Promise<void>;
  };
}

// ── 端口三：事件通道（收编 src/lib/crossWindowBus.ts）──
export interface EventChannel {
  emitCrossWindow(event: string, payload?: unknown): Promise<void>;
  listenCrossWindow(event: string, handler: (payload?: unknown) => void): Promise<() => void>;
}

// ── 端口四：桌面宿主叠加（纯桌面，只从 apps/desktop 具体导出，不进共享槽位类型）──
export interface DesktopHostServices {
  getHostSettings(): Promise<AppSettings>;        // legacy get_settings（桌面字段全量）
  saveHostSettings(s: AppSettings): Promise<void>; // legacy save_settings
  rebuildMenu(language: string): Promise<void>;
  getLogPath(): Promise<string>;
  openLogDir(): Promise<void>;
  openWorkflowsDir(): Promise<void>;
  openContextDir(): Promise<void>;
  checkUpdatesOnStartup(enabled: boolean): void;  // updater 编排入口
}

// ── 注入器（bootstrap 注入，§7.3 模式；环境注册表，不做构造器参数穿透）──
export interface PlatformBindings {
  services: PlatformServices;
  windows: WindowRouter;
  events: EventChannel;
  capabilities: PlatformCapabilities;
}
export function configurePlatform(bindings: PlatformBindings): void;

// ── 能力表（编译期由平台槽位导出；F5 落地，机制沿用 DATAZEN_UPDATER_CHANNEL 先例）──
export interface PlatformCapabilities {
  /** 设置页 section 的可用集：desktop 给全集；web 给空集或半集（server 形态可给 ai/tunnels，
   *  永不给 logging/mcpClient——本机进程概念）。取代 settingsSectionsForBackend(boolean)。 */
  readonly settingsSections: ReadonlySet<SettingsSection>;
  readonly updater: boolean;
  readonly versionSource: 'host-app' | 'build-constant' | 'http';
}

// ── 未注入显式错误（§7.4 语义照搬：调用未注入的端口成员抛结构化错误，不静默）──
```

**契约要点**：

- `WindowRouter` 收编 `windowManager.ts` 后，`openDataSyncWindow()` / `openBackupWindow()` 等 20 个导出**签名不变**——`src/lib/windowManager.ts` 变为转发层（薄再导出，调用方零改动），实现住进 `apps/desktop/src/platform/windowRouter.ts`（含浏览器 tab 分支，F2 期间住 `src/platform/desktop/`）。
- `EventChannel` 与 kernel 事件流**并存不合并**：kernel 事件走 `BackendTransport.subscribe`（`EventStreamSource` 拉式抽象，断线可续订，`streams.ts` 已存在）；宿主广播事件（`menu:*`、`datazen:open-connection`、`ai:stream-*` 等非 kernel 事件）走 `EventChannel`。15 个生产 `listen()` 点按此二分归位。
- 文件对话框的 Rust 侧设计（路径永不回 JS、`begin_save_with_dialog` token 流式写）在 web 天然成立——浏览器 file input / Blob download 本来就路径无关。F4 以「导出」做双流程示范：桌面实现走 save-dialog token 流式写，web 实现走 Blob download，共享签名 `exportTables(request): Promise<ExportResult>`。

### 3.4 注入机制与装配时序

**bootstrap 注入，不走 alias、不做构造器穿透**（裁决 #7 的机制面）。app 的 `main.tsx` 在首屏渲染前：

```typescript
// apps/desktop/src/main.tsx（装配根，原 src/main.tsx 的平台相关部分）
bindDesktopBackend();                        // 既有：transport 绑定
setPlatformServices(createDesktopPlatformServices());  // 缺失的线：F2 补上（文档 §7.3 早写明）
configurePlatform({
  services: createDesktopPlatformServices(),
  windows: createDesktopWindowRouter(),      // 收编自 windowManager.ts
  events: createDesktopEventChannel(),       // 收编自 crossWindowBus.ts
  capabilities: DESKTOP_CAPABILITIES,        // F5
});
registerWindowChrome({ TitleBar, WindowControls, MenuBar });  // extension-points 注册
// 之后才进入 React 渲染（splash 时序、settings preload 逻辑原样保留）
```

三个理由，全部来自仓库自身先例：① 文档 §7.3 明文「应用入口在首屏渲染前完成 `setBackendClient` 与 `setPlatformServices`」；② 注册表 + 未注入显式报错的基建已写好且有 rebinding 测试；③ 930 个文件、store 在非 React 深处调 `openDataSyncWindow()`，纯构造器 DI 意味着参数穿透几百层——不可行。

窗口 chrome 同理：app 调 `registerWindowChrome(...)` 挂进 extension-points（`setTableSchemaProvider` 同款先例），共享窗口页渲染的是槽位不是组件——共享代码从不 import 桌面组件。

### 3.5 web 形态预留（不实施）

`apps/web/` 现阶段最多包含：一个把四端口全部实现为显式 throw 的 typecheck stub + capabilities 空集。它的作用不是跑起来，而是（a）让「共享代码只依赖共享接口」在编译期被真实消费验证；（b）做单消费者 lib 的诚实性保险（§7.2）。真实施工（HTTP transport、SSE 事件流、路由化多窗）留给平台产品计划 P 阶段，本方案不设计。

## 4. 实测基线详表（基线 `c85e3bd`）

### 4.1 `@tauri-apps` 引用分布

```text
147 处 / 112 文件（src + packages，含测试）
├─ 81 处 api/core (invoke)          ← 数据面：F4 的 facade 迁移面，不是 UI 差异
│                                      （commands/database×11、query×2、settings×3、panelStore×1
│                                       + driver-sdk 的 driverCommands / schemaClient / fileCommands）
└─ 66 处控制面 = 真正的平台差异
   ├─ api/event 32   ├─ api/window 11    ├─ webviewWindow 5   ├─ api/webview 5
   ├─ plugin-dialog 5 ├─ api/app 3       ├─ plugin-updater 3  ├─ plugin-process 3
   └─ api/menu 1     └─ plugin-os 1
```

### 4.2 运行时形态探测点清单（F2 逐点清除对象）

生产代码 ~20 处（`packages/drivers/**/e2e/**` 的 `__TAURI_INTERNALS__` 是 webdriver 测试桥，**不是**探测点，保留）：

| 文件 | 位置 | 形态 | 替换 |
|---|---|---|---|
| `src/main.tsx` | :122、:163 | `__TAURI_INTERNALS__ in globalThis` | 装配根整段进 app，探测删除（app 内无形态二义） |
| `src/components/TitleBar.tsx` | :55、:117 | 同上 | 组件迁 `shell/desktop`→`apps/desktop/chrome`，删守卫 |
| `src/stores/aiStore.ts` | :53、:80、:168、:647、:904（5 处） | 同上 | 订阅生命周期改走 `EventChannel`，注入时序保证存在性 |
| `src/stores/uiStore.ts` | :77 | 同上 | `WindowRouter.windowChrome` |
| `src/stores/wappStore.ts` | :73（listen） | 事件 | `EventChannel` |
| `src/lib/themePackApply.ts` | :95 | 同上 | `windowChrome.setBackgroundRootColor` |
| `src/lib/updater.ts` | :29-30、:62 | `isDesktopApp()` | 整文件迁 app；`getUpdateChannel()` 的 `'none'` 分支删除 |
| `src/lib/windowManager.ts` | :47-49 等 6 处 | `isTauri()` | 实现整体迁 app；浏览器分支改 `import.meta.env.TAURI_ENV_*` 编译期常量（裁决 #5） |
| `src/lib/crossWindowBus.ts` | :12-14 | `isTauri()` | 同上 |
| `src/windows/connection/ConnectionPage.tsx` | :247 | 同上 | `windowChrome`（或该 effect 迁 chrome 层） |
| `src/windows/settings/SettingsContent.tsx` | :75 | `backend === null \|\| backendId === 'desktop'` | capabilities.settingsSections（顺带修复 null 误判 bug） |
| `src/commands/settings.ts` | :9、:21、:31 | `backendId === 'desktop'` 三元 | 域客户端化后由 `DesktopSettingsClient` 内部投影（F4） |

### 4.3 48 个耦合文件的分类归宿

```text
48 个 import @tauri-apps 的生产文件
├─ ~23 个只碰 invoke（数据面）→ F4 迁 backend-client facade，与平台无关
└─ ~25 个碰控制面/探测
   ├─ 5 个「事实平台层」：windowManager / crossWindowBus / pathPicker / updater /
   │   tauriBackendTransport → 本来就该住进 platform 目录，只是没搬
   ├─ ~10 个 lib/stores/commands：themePackApply、uiStore、wappStore、aiStore、
   │   sqlFileExecution、file.ts、ai.ts、settings.ts → 触点改 import 端口
   └─ ~10 个 UI 组件：TitleBar / WindowControls / MenuBar / 4 个窗口页面 /
       PubSubPanel / BrandSidebar / SettingsContent → 迁 chrome 或改调端口
```

### 4.4 UI 渲染差异（4 类——「绝大部分相同」的精确边界）

| # | 差异 | 实测落点 | web 形态 |
|---|---|---|---|
| R1 | 窗口 chrome：mac 原生 overlay 交通灯 vs win/linux 无边框自绘拖拽（startDragging 移动阈值逻辑、fullscreen-changed 同步） | `TitleBar.tsx`（182 行）、`WindowControls.tsx`；9 个窗口页面消费 | 整条消失——浏览器自带 chrome；组件进 `apps/desktop/chrome`，经 extension-points 注册 |
| R2 | 设置页 7/12 桌面专属 section | `settingsSections.ts:65` | capabilities 驱动；ai/prompts/tunnels 理论上 server 形态可提供，不写死「隐藏 7 个」 |
| R3 | 更新区块 | `UpdateSection.tsx`、`updater.ts`（SKU manifest 体系，`getUpdateChannel()` 已有 `'none'` 分支） | 整块不存在，进 app |
| R4 | 版本号来源 | `SettingsContent.tsx:99`、`BrandSidebar.tsx:23` 的 `getVersion()` | capabilities.versionSource：`'build-constant'` 或 `'http'` |

### 4.5 行为能力差异（8 项原子能力，3 项已双轨、2 项端口已预留）

| # | 能力 | 现状 | web 替身 | 归宿 |
|---|---|---|---|---|
| B1 | 开窗/聚焦/关窗 | `windowManager.ts` **已双轨**（Tauri `create_sub_window` vs 浏览器 tab；singleton 去重） | 路由页/新 tab | `WindowRouter` |
| B2 | 跨窗口事件 | `crossWindowBus.ts` **已双轨**（Tauri 广播 vs BroadcastChannel） | BroadcastChannel | `EventChannel` |
| B3 | 窗口身份分发 | `windowKind.ts` URL 参数——浏览器天然兼容 | 同机制 | 本来就共享，不动 |
| B4 | 后端事件流 | 15 个生产 `listen()` 点；kernel 轨已有 `EventStreamSource` 拉式抽象 | SSE/WS 适配 | kernel 事件走 `BackendTransport.subscribe`；宿主广播走 `EventChannel` |
| B5 | 文件对话框 | Rust 命令两条路（`open_text_with_dialog`；`begin_save_with_dialog`+token 流式写，路径永不回 JS）+ `pathPicker.ts` plugin-dialog | file input / Blob download（路径无关设计天然成立） | `PlatformServices`（已定义，接线即可） |
| B6 | 剪贴板 | `write_clipboard` / `read_clipboard` 命令 | `navigator.clipboard` | `PlatformServices`（已覆盖） |
| B7 | 窗口外观（背景色/全屏） | `themePackApply.ts`、`uiStore.ts`、TitleBar 全屏同步 | F11 / 无 setBackgroundColor | `WindowRouter.windowChrome`（web 降级） |
| B8 | 更新器 | plugin-updater + relaunch + SKU manifest | 无替身，纯桌面 | 不进共享端口——app + capabilities.updater=false |

准第 9 项：外部打开（`openPath` / open dirs / docs URL，已有 `window.open` 兜底）→ `PlatformServices.openExternal`。

## 5. 阶段实施（F1–F6）

### 5.0 阶段总览

| 阶段 | 主题 | 依赖 | IPC 轨迹 | 风险 |
|---|---|---|---|---|
| F1 | NativeTool 删除 | — | 272 → 267 | 低（零产品使用，契约先行删除） |
| F2 | 原地清场：隔离区 + 探测清除 + 端口接线 + 闸门 | F1 | — | 中（触点改写面广但单点浅） |
| F3 | 域客户端拆分 | F2 | — | 低（UI 零改动，爆炸半径已测） |
| F4 | Settings/Connection 示范迁移 + 命名归零 | F3 | 267 → 255 → ~232 | 中（双轨消解次序敏感） |
| F5 | capabilities + 每域契约测试 + playbook | F4 | — | 低 |
| F6 | 分家：TS workspace 协议 + frontend/app 包完整化 | F2–F5 | ~215（终态） | 中（唯一真未知：worktree 软链下 pnpm install 行为，§7.1） |

每个 PR 必须构建可用、有真实消费者和测试；不提交空接口等别人接线。顺序裁决（#11）：先清场后分家，F6 之前 `src/` 始终是单 app 工程。

### 5.1 F1：NativeTool 删除（PR-1）

**输入**：`main` 基线；NativeTool = 部署锚定的 `pg_dump`/`mysqldump --version` 探针，SHA-256 白名单默认 `[]`（拒绝一切），产品零使用、无 UI、契约先行预留。

**工作项**（删除清单，全部核对过存在于基线）：

- 宿主命令注册：`src-tauri/src/run.rs`（NativeTool 5 个命令注册块）；
- 命令层：`src-tauri/src/commands/native_tool*.rs`；
- 平台模块：`src-tauri/src/platform/native_tool/` 目录；
- 装配接线：`src-tauri/src/adapter.rs` 4 处；
- 资源：`resources/native-tool-probes.json`（`include_str!` 编译期嵌入，随代码删）；
- modules 侧：`modules/src/jobs/domain/native_tool*`、`modules/src/jobs/application/native_tool*`、`owner.rs` 的 `verify_native_selection`、`codec.rs` 的 `nativeToolProbe` 校验分支 + 防御性启动 purge 迁移（旧持久化记录清空）；
- 文档：7 篇涉及 NativeTool 的文档段落。

**退出标准**：`cargo build` + 全量测试绿；`rg -i nativetool` 在 src-tauri/modules/ 仅命中 CHANGELOG 与历史文档；注册命令数 272 → 267（对照 `ipc_audit` 台账）。

**回滚**：单 PR revert，无数据迁移（purge 是单向清空，revert 不恢复旧记录——记录本身无产品价值）。

### 5.2 F2：原地清场（PR-2，可拆 2a/2b）

**输入**：F1 合入；三张 alias 表照常；目录不动。

**工作项**：

1. **建隔离区**（未来 app 的先遣队）：`src/platform/desktop/`（transport / platformServices / windowRouter / eventChannel / hostServices / os）与 `src/shell/desktop/`（TitleBar / WindowControls / MenuBar / UpdateSection / splash 时序）；
2. **收编不新建**：`windowManager.ts`、`crossWindowBus.ts`、`pathPicker.ts`、`updater.ts`、`tauriBackendTransport.ts` 五个文件迁入 `src/platform/desktop/`，原路径留薄再导出（调用方零改动）；`windowManager` / `crossWindowBus` 内的 `isTauri()` 浏览器分支改 `import.meta.env.TAURI_ENV_*` 编译期常量；
3. **端口契约落地**：`src/platform/contracts.ts`（§3.3 四接口 + `configurePlatform` + 未注入显式错误）；`main.tsx` 补 `setPlatformServices(createDesktopPlatformServices())` 生产接线（仓库文档 §7.3 写明而未做的那根线）；
4. **chrome 注册**：extension-points 增加 `registerWindowChrome` 槽位；TitleBar 等迁 `shell/desktop` 并注册；
5. **探测清除**：§4.2 清单逐点替换；
6. **闸门上线**：eslint 三规则（§6.1）+ `check-driver-import-boundaries.mjs` R4 范围扩展到全部非 app 包（§6.2）。

**退出标准**：`src/`（除 `platform/`、`shell/`）对 `@tauri-apps/*` 的 import 归零（闸门强制）；运行时探测归零（§4.2 清单全绿）；既有全部测试通过（tauriWindow mock 继续在 vitest alias 生效）；浏览器 dev 模式（vite 裸跑）手动冒烟通过。

**回滚**：2a/2b 各自独立 revert；薄再导出层保证调用方无感，revert 面收敛在隔离区内部。

### 5.3 F3：域客户端拆分（PR-3）

**输入**：F2 合入（触点已端口化，`useBackendClient` 是唯一消费入口）。

**工作项**：`packages/backend-client` 内部按 DDD 拆域客户端（`client.ts` 保持门面再导出，`BackendClient` 变 deprecated 别名）：

| 域 | 方法数 | 文件 | 备注 |
|---|---|---|---|
| Settings | 2 | `settingsClient.ts` | F4 示范域 |
| Connection | 4 | `connectionClient.ts` | list/create/update/disable；改名 `list_connections` 在 F4 |
| Session | 8 | `sessionClient.ts` | open/get/close/setContext/… |
| Execution | 2 | `executionClient.ts` | executeInSession/executeAtTarget |
| Event | 1 | 并入 streams.ts | subscribeEvents |
| Job | 6 | `jobClient.ts` | 有状态域，配工厂 |
| TableData | 14 | `tableDataClient.ts` | **仓库既有先例**（`createTableDataMethods` 已存在），其余域照抄此模式 |
| Artifact | 1 | `artifactClient.ts` | |
| Identity | 2 | `identityClient.ts` | |
| DriverGateway | 5 | `driverGatewayClient.ts` | 过渡逃生舱（driver-sdk 的 legacy 命令），kernel 实现后删 |

`MethodMap` = 域 map 的类型交集；`createClients(transport)` 组合根返回域客户端集合。爆炸半径已实测：生产消费仅 4 文件 21 处（database×11、query×2、settings×3、panelStore×1），UI 零改动。

**退出标准**：域客户端可独立 import；`MethodMap` 与域 map 的键一致性有编译期断言；契约测试（F5 铺垫，先以现有 `__tests__` 平移到分域文件）。

**回滚**：门面再导出保兼容，revert 不触碰消费方。

### 5.4 F4：Settings/Connection 示范迁移与命名归零（PR-4，可拆 4a/4b/4c）

**输入**：F3 合入。

**工作项**：

1. **Settings 域示范**：`SettingsClient.getSettings(): Promise<Settings>` 两形态同签名；桌面实现内部做投影（`toSettings`）；`DesktopSettings extends Settings`；**字段归属复核**——行为字段（monitor / onboarding / rowCountMode）迁入共享 `Settings`，路径/进程/更新器字段留在 `DesktopSettings`；共享 store 只持纯 `Settings`，无合并无分支。`src/commands/settings.ts` 的 `backendId === 'desktop'` 三元（:9/:21/:31）由 `DesktopSettingsClient` 内部投影消解；`get_settings` / `save_settings` legacy 命令随之退役（裁决 #2 消解顺序第 2 步）；
2. **命名归零**：Rust 命令中性化 `get_shared_settings` → `get_settings`、`platform_list_profiles` → `list_connections`（消解顺序第 3 步：前端已在 facade 上，改名只动 `commandOverrides` 映射值）；改完后 `tauriBackendTransport.ts` 的 `commandOverrides` 表**归零删除**（消解顺序第 4 步，kernel 轨与命令名 1:1）；
3. **Connection 域示范**：`platform_create/update/disable_profile` 预留命令激活——前端从 legacy `save_connection` 族切到 `connectionClient`；
4. **双流程示范**：导出——桌面 save-dialog token 流式写 vs web Blob download，共享签名（§3.3 要点 3）；
5. **缺陷清理**：修复 `SettingsContent.tsx:75` null 误判（切 capabilities 预备形态：`settingsSectionsForBackend` 的入参从 boolean 改为 section 集合，F5 定型）；修复 `PubSubPanel.tsx:3` 驱动包违规（改走 `EventChannel` 或 backend-client 事件流）；
6. **死命令删除**：剩余死命令 12（A6+B2+D4 类）删除，267 → 255；合并簇（schema plan 族 6→1、job 读侧 13→5、文件对话框 8→6、dir-open 4→2、clipboard 3→2、file pickers 3→1、AI 一次性 8→2）推进，255 → ~232。

**退出标准**：`src/commands/settings.ts` 无形态分支；`commandOverrides` 为空或删除；override 表归零后 `rg "backendId ===" src/` 归零；设置页在「桌面全量 / 缩减集」两种 section 集合下渲染正确（手工或快照）。

**回滚**：4a（域迁移）/4b（改名）/4c（删除合并）独立 revert；改名 PR 在 `commandOverrides` 映射值层面 revert 即恢复旧命令名，无持久化影响。

### 5.5 F5：capabilities 与契约测试（PR-5）

**输入**：F4 合入。

**工作项**：

1. **静态 capabilities 表**：`DESKTOP_CAPABILITIES` 常量（§3.3），机制沿用 `DATAZEN_UPDATER_CHANNEL` 编译期生成先例（`scripts/resolve-drivers.mjs` 同族脚本生成或手写常量）；`settingsSectionsForBackend` 定型为 capabilities 驱动；`UpdateSection` / `getVersion` 消费 `capabilities.updater` / `capabilities.versionSource`；
2. **每域契约测试**：每个域客户端一份「方法 × 请求形状 × 响应形状」表驱动测试，fake transport（`__tests__` 已有 spy 模式可复用）；`PlatformServices` / `WindowRouter` / `EventChannel` 各一份「未注入显式错误 + 注入后行为」契约测试；
3. **web stub**：`apps/web/`（若 F6 未先行）或 `src/platform/web/` 全 throw 实现 + 空 capabilities，纳入 typecheck——编译期同形的持续验证；
4. **playbook**：新增域客户端 / 新增平台能力的操作手册（含「何时该扩 PlatformServices、何时该进 DesktopHostServices、何时只是普通 invoke 迁移」的判别树）。

**退出标准**：capabilities 驱动全部形态分支（§4.2 表中 capabilities 列全绿）；每域至少一份契约测试；web stub 通过 typecheck。

**回滚**：纯新增，revert 无连带。

### 5.6 F6：分家——TS workspace 协议与包完整化（PR-6）

**输入**：F2–F5 全部合入（`src/` 内 `@tauri-apps` import 已归零，分家变为纯机械动作）。

**工作项**：

1. **workspace 基建**：`pnpm-workspace.yaml` 增补 `packages: [apps/*, packages/*, packages/drivers/*]`。⚠ 先在 worktree checkout 上实测 `pnpm install`（§7.1 风险项），确认 node_modules 软链行为后才可合入；
2. **`packages/frontend/package.json`**：源码包模式（`main`/`types`/`exports` 三件套指 `src/index.ts`，与既有包形态一致——但这次被协议真实消费）；`dependencies` 声明 `@datazen/backend-client` 等 `workspace:*`；`peerDependencies`：react / react-dom / zustand / @codemirror（跟随 ui / extension-points 的 peer 惯例，实版本由 app 提供）；
3. **`apps/desktop/package.json`**：`@datazen/frontend: workspace:*` + `@tauri-apps/*` 全家（**从仓库根 dependencies 挪入——`@tauri-apps` 依赖声明从此只存在于 app 包**，D2 的包级强化）+ react 实版本；
4. **根 package.json 退化**为工具链持有（vite / vitest / tailwind / typescript / husky / wdio）；
5. **目录迁移**：`src/main.tsx`、双 html 入口、vite/tailwind/postcss/tauri 配置、`src/platform/desktop/`、`src/shell/desktop/`（连同测试）→ `apps/desktop/`；`src/` 其余整体改名 `packages/frontend/`（930 文件相对路径 import，零代码改动）；`src-tauri/` 移入 `apps/desktop/`（Cargo.toml members 一行）或原地改 `frontendDist` 指向（二选一，倾向前者）；
6. **alias 表退役**：vite / tsconfig / vitest 三处 `@datazen/*` 条目删除（exports 接管）；app 的 vitest 保留 `@tauri-apps/api/window → tauriWindow.ts` mock 别名（测试基建归 app）；vitest 配置拆 lib / app 两半；
7. **tsconfig 变体核对**（实测清单）：`tsconfig.json`（include 改 `apps/*` + `packages/*`）、`tsconfig.pack-ep.json`（pack-ep 的 HOST_SHARED_MODULES 与 include 空表同步）、`tsconfig.scripts.json` / `tsconfig.scripts-checkjs.json`（不受影响，核对即可）；
8. **脚本路径**：`scripts/typecheck.mjs`、`pack-ep.mjs`（HOST_SHARED_MODULES 与 `__DATAZEN_HOST__` 表两侧同步，`pack-ep.test.ts` 会拦漂移）、`e2e-tauri-build.mjs` 等的入口指向更新；
9. **R5 闸门上线**（§6.2）；redis 集成测试迁 `packages/frontend/__tests__/integration/`，注销 ALLOWLIST 两项豁免（兑现其注记 *remove together with the WebContextMenuHost exemption*）。

**退出标准**：`pnpm install` 在主 checkout 与 worktree checkout 均成功；`pnpm typecheck` / 全量测试 / `tauri build` 三绿；`rg "@tauri-apps" packages/frontend` 归零；`check-driver-import-boundaries`（含 R5）0 blocking；e2e 冒烟（webdriver build）通过。

**回滚**：单 PR revert（目录与配置整体回退）；workspace 协议若与 worktree 软链冲突则回退 `pnpm-workspace.yaml` 的 packages 字段即可，其余迁移不受影响（alias 表此时已删，需连 alias 恢复一起 revert——这正是把分家做成单个大 PR 的原因）。

## 6. 闸门体系

### 6.1 eslint 路径规则（F2 上线）

```jsonc
// .eslintrc 补充（no-restricted-imports，三条闸门）
[
  {
    "files": ["src/**/*.{ts,tsx}"],          // F6 后：packages/frontend/src/**
    "excludedFiles": ["src/platform/desktop/**", "src/shell/desktop/**"],
    "rules": { "no-restricted-imports": ["error", {
      "patterns": [{ "group": ["@tauri-apps/*", "*@tauri-apps/*"],
        "message": "共享层禁止直接依赖宿主运行时——通过 platform/contracts.ts 的端口（裁决 D2）。" }]
    }]}
  },
  {
    "files": ["src/**/*.{ts,tsx}"],
    "excludedFiles": ["src/shell/desktop/**"],
    "rules": { "no-restricted-imports": ["error", {
      "patterns": [{ "group": ["**/platform/desktop/**"],
        "message": "共享代码只能经 contracts 接口与 configurePlatform 拿到窄类型，不得 import 具体平台实体（裁决 #7）。" }]
    }]}
  },
  {
    "files": ["packages/drivers/**/*.{ts,tsx}"],
    "rules": { "no-restricted-imports": ["error", {
      "patterns": [{ "group": ["@tauri-apps/*"],
        "message": "驱动包是两形态共享层——事件走 backend-client 事件流（裁决 D2）。" }]
    }]}
  }
]
```

### 6.2 `check-driver-import-boundaries.mjs` 扩展

既有 R1–R4 保持不动（实测基线：2056 文件、0 blocking、1 advisory）。两处变更：

- **R4 范围扩展**：`packages/ui` 的 host-free / runtime-free 断言扩展到全部非 app 包（`packages/drivers/**` 的 `@tauri-apps` 禁令从 eslint 补强为构建闸门——eslint 拦编辑期，脚本拦 CI）。已知违例 `PubSubPanel.tsx:3` 在 F4 修复后才可置 blocking（脚本自身注释记录的同一治理模式：DocumentConnectionView 的 R3 advisory 就是这么挂账的）；
- **R5 新增**：`packages/**` 不得 import `@datazen/frontend`，亦不得相对路径解析进 `packages/frontend/`。扫描口径照抄 R1 的教训——**所有字符串字面量**（plain import / `export … from` / 动态 `import()` / `vi.mock()` / `vi.doMock()` / `require()`），因为 Wave 2 有 8 个 `vi.mock` 违规从只扫 `from` 的基线漏网。豁免治理沿用 ALLOWLIST 三元组（rule, file, specifier）+ 注记义务。

### 6.3 契约测试模板（F5）

```typescript
// 每域一份；fake transport 沿用 backend-client/__tests__ 既有 spy 模式
describe('SettingsClient 契约', () => {
  it.each([
    ['getSettings', undefined, 'get_settings', { preferences: {}, execution: {} }],
    ['saveSettings', { settings: sharedSettings }, 'save_settings', sharedSettings],
  ])('%s → %s', async (method, payload, expectedCommand, response) => { /* … */ });
});

// 端口契约：未注入显式错误（§7.4 语义）
it('configurePlatform 前调用 EventChannel.emitCrossWindow → 抛结构化错误而非静默', async () => { /* … */ });
it('注入后 emitCrossWindow → 转发到注入实现', async () => { /* … */ });
```

## 7. 风险登记与回滚

### 7.1 worktree 软链与 pnpm install（F6 唯一真未知）

`pnpm-workspace.yaml` 既有注释记录：本仓工作树把 `node_modules` 软链到主 checkout，pnpm 判定依赖过期并先装一次，而安装必然失败（`workspace hoist directory is not a real directory`）——`verifyDepsBeforeRun` 就是为这个坑关的。**引入 `packages` 列表后 `pnpm install` 的行为必须先在 worktree checkout 实测**，这是 F6 唯一无法靠静态分析预判的项；其余全部是机械动作。缓解：F6 开工前先做一个只加 packages 字段的探针分支，在主 checkout 与 worktree 各跑一次 install，记录行为再定迁移细节；CI 侧已由 `DATAZEN_VERIFY_DEPS_BEFORE_RUN=install` 显式把关，不受本地关闭影响。

### 7.2 单消费者 lib 的诚实性

lib 今天只有一个消费者，会悄悄积累 app 形状假设（多窗口 URL 分发、localStorage 键、splash 时序）。保险 = ① web stub 纳入 typecheck（编译期同形持续验证）；② 端口契约测试（未注入显式错误）；③ R5 闸门（依赖方向不腐化）。三者都是 F5/F6 的一次性成本，远低于未来真做 web 形态时的考古成本。

### 7.3 各阶段回滚策略

见各阶段「回滚」小节。总原则：F1–F5 每步可独立 revert 且不触碰调用方（薄再导出 / 门面兼容层承压）；F6 是唯一整体性动作，做成单 PR 以保证 alias 恢复与目录回退的原子性。

### 7.4 与 DDD 重构（D 阶段）的交叉

F1 落在 `modules/src/jobs/` 既有结构上（NativeTool 的 domain/application 子模块）；若 D 阶段同期推进 jobs 域，F1 先行合入可减少 D 的迁移面。F3 的域客户端划分以 `system-overview.md §6.2` 为权威，与 D 阶段的 modules 域划分保持同名映射（`modules/src/connection` ↔ `connectionClient.ts` 等），两侧演进各自独立、命名对齐即可，不设硬同步门槛。

## 8. IPC 命令轨迹与验收对照

| 节点 | 命令数 | 动作 | 验收 |
|---|---|---|---|
| 审查基线 | 272 | 266 常驻 + 6 cfg(test) | 台账核对 |
| F1 后 | 267 | NativeTool 5 命令删除 | `rg -i nativetool` 仅历史残留 |
| F4c 后 | 255 | 死命令 12 删除（A6+B2+D4） | 台账逐条对照 |
| F4c 后（续） | ~232 | 合并簇：schema plan 6→1、job 读侧 13→5、dialogs 8→6、dir-open 4→2、clipboard 3→2、pickers 3→1、AI 一次性 8→2 | 每簇前后行为等价测试 |
| 双轨收敛终态 | ~215 | legacy settings/connection 族退役 + 命名归零（override 表归零） | `commandOverrides` 删除；`rg "backendId ===" src/` 归零 |

> 轨迹数字为方案估算，合入时以命令注册处的机械计数为准；台账（审查产物）随各 PR 更新，不另立进度文档。
