# DataZen 可扩展架构重构与多包迁移方案

> **分类体系：Driver / Theme / EP (Host Extension Point) / Workspace App**  

> **公共设计系统`@datazen/ui`**  

> **文档版本**：v1.0.0 (正式设计发布版)  

> **面向对象**：核心开发团队、驱动与扩展模块开发者

---

## 一、架构现状与重构动因

DataZen 目前的可扩展体系在演进过程中积累了严重的术语重叠、职责混淆和物理边界不一致的问题：

1. **术语交叉与心智负担**：

   - 早期将数据库驱动称为“Plugin”，前端暴露了 `@datazen/plugin-sdk`；

   - 随后引入运行时 iframe 沙箱，被称为“Extension”并配套了 `@datazen/extension-sdk`；

   - 紧接着主进程内的特权插槽机制被称为“Host Extension Points”，代码放在 `src/plugin-sdk/extensionPoints.ts`；

   - **痛点**：开发者无法直观分辨一个“插件”到底是连数据库的 Driver、沙箱全屏页面、纯 CSS 主题、还是编辑器内核插槽。

2. **职责倒挂与“包污染”**：

   - 驱动前端为了画配置向导，在 `plugin-sdk` 中硬性导出了 `Button`, `Input`, `Select`, `Dialog` 等宿主私有 UI 组件；

   - 这导致纯逻辑环境（如无头 `--mcp-stdio` 服务器、方言解析模块）被迫依赖 React 与 DOM 运行时；

   - 通用 UI 控件没有成为独立的设计系统（Design System），其他模块（如沙箱页面）无法以标准包形式共享复用。

3. **物理运行边界被抹平**：

   - 沙箱应用（跨进程 `window.postMessage`，毫秒级网络/IPC 开销）与特权插槽（主进程内函数直调`<5ms` 键入延迟）属于完全不同的物理运行世界；

   - 若不物理分包，外部独立仓库（如闭源的 `sql-editor-pro`）在开发编译阶段无法独立引入类型契约。

---

## 二、四维架构分类与职责矩阵

系统确立 **Driver / Theme / EP / Workspace App** 四元分类，并抽离跨维度的公共设计系统 *`@datazen/ui`**：

```

┌────────────────────────────────────────────────────────────────────────────────────────┐

│                              DataZen 核心扩展与设计体系                                 │

├───────────────────┬───────────────────┬────────────────────┬───────────────────────────┤

│ 维度 (Dimension)  │ 专属包/资产位置   │ 物理运行形态       │ 职责与典型场景            │

├───────────────────┼───────────────────┼────────────────────┼───────────────────────────┤

│ 1. Driver         │ @datazen/         │ 编译/链接时注入    │ 数据库驱动、SQL方言、     │

│    (数据库驱动)   │ driver-sdk        │ (Host 同进程)      │ DDL提取、Driver Command   │

├───────────────────┼───────────────────┼────────────────────┼───────────────────────────┤

│ 2. Theme          │ 纯静态资源目录    │ 零代码执行         │ 调色板、CSS变量、         │

│    (外观主题)     │ (无 JS 脚本)      │ (tokens.css/json)  │ 图表/CodeMirror 主题映射  │

├───────────────────┼───────────────────┼────────────────────┼───────────────────────────┤

│ 3. EP             │ @datazen/         │ 主进程内存函数调用 │ 编辑器内核增强(Pro)、     │

│    (特权扩展点)   │ extension-points  │ (微秒级响应，<5ms) │ AST深度分析、商业闭源模块 │

├───────────────────┼───────────────────┼────────────────────┼───────────────────────────┤

│ 4. Workspace App  │ @datazen/         │ 独立 iframe 沙箱   │ 独立全屏工作区应用        │

│    (工作区应用)   │ app-sdk           │ (datazen:// 协议)  │ (Table Workspace 等)      │

├───────────────────┼───────────────────┼────────────────────┼───────────────────────────┤

│ ★ Common UI       │ @datazen/ui       │ 纯 React 视图组件  │ 基础控件 (Button, Select, │

│    (通用设计系统) │ (packages/ui/)    │ (无业务 Store 绑定)│ Input, Dialog, cn)        │

└───────────────────┴───────────────────┴────────────────────┴───────────────────────────┘

```

---

## 三、包拓扑结构与物理目录划分

重构后的 monorepo 物理结构如下：

```text

datazen/

├── packages/

│   ├── driver-api/          # [Rust] DatabaseDriver + Command 统一后端抽象 (crates.io/内部)

│   ├── driver-sdk/          # [TS] 数据库驱动纯前端契约 (@datazen/driver-sdk)

│   ├── extension-points/    # [TS] 宿主特权扩展点契约 (@datazen/extension-points)

│   ├── app-sdk/             # [TS] 沙箱工作区应用客户端 (@datazen/app-sdk，原 extension-sdk)

│   ├── ui/                  # [TS/React] 通用设计系统组件库 (@datazen/ui)

│   ├── drivers/             # 各 path 驱动实现 (Postgres, MySQL, SQLite, Redis...)

│   └── apps/                # 内置 Workspace Apps (原 packages/extensions/)

├── src/                     # Host 宿主主应用源码

│   ├── components/          # 宿主专属业务复合组件 (DataTable, QueryPanel, ErDiagramView...)

│   ├── stores/              # 宿主全局状态 (connectionStore, schemaStore...)

│   └── windows/             # 宿主窗口与主工作区 Page

├── src-tauri/               # Tauri Rust 后端

│   ├── src/apps/            # Workspace Apps 资产协议、权限审计与沙箱隔离 (原 extensions/)

│   └── src/theme/           # 主题包文件读取与广播

└── LICENSE                  # 包含 DataZen Plugin, Driver & Extension Linking Exception

```

---

## 四、各包详细契约与 API 规范

### 1. 数据库驱动 SDK`@datazen/driver-sdk`

- **物理位置**`packages/driver-sdk/`

- **定位**：零 DOM、无 React 强依赖，可在浏览器主进程、Web Worker 或 Headless（无头 MCP 服务器）模式下安全运行。

```typescript

// packages/driver-sdk/src/index.ts

// 1. 数据库类型元数据

export type {

  DatabaseTypeMeta,

  ConnectionMode,

  DatabaseObjectKind,

} from './databaseMeta';

// 2. SQL 方言与生成策略（已全面落地到驱动包内）

export type {

  SqlDialectStrategy,

  SqlDialectProfile,

  SqlDialectFamily,

  TableSqlDialect,

  GeneratedSqlType,

  DdlDialect,

  IndexDialect,

} from './sqlDialectTypes';

export { BaseTableSqlGenerator } from './baseTableSql';

// 3. 驱动函数与智能粘贴

export type { FunctionEntry, FunctionParam } from './sqlFunctionTypes';

export type { ConnectionClipboardParser, ConnectionClipboardFill } from './clipboardTypes';

// 4. 连接向导模型（纯状态接口，不含 JSX 渲染）

export type { ConnectionFormState, PluginFormValidator } from './connectionFormTypes';

// 5. Driver Command IPC

export { driverCommands } from './driverCommands';

export type { ExecuteDriverCommandRequest, CommandResult } from './driverCommands';

// 6. Schema 目录树与缓存桥接（单向延迟注入解耦）

export {

  syncSchemaTables,

  syncSchemaNamespace,

  registerPathAliases,

  getCachedPathItems,

  cachePathItems,

  subscribeSchemaPathItems,

} from './schemaBridge';

```

---

### 2. 通用设计系统`@datazen/ui`

- **物理位置**`packages/ui/`

- **定位**：纯视图组件，依赖 React 18 与 Tailwind CSS，严禁引入宿主的 Zustand Store、IPC 或数据库业务逻辑。

```typescript

// packages/ui/src/index.ts

// 基础原子控件

export { Button } from './Button';

export type { ButtonProps, ButtonVariant, ButtonSize } from './Button';

export { Input } from './Input';

export type { InputProps } from './Input';

export { Select } from './Select';

export type { SelectProps, SelectOption } from './Select';

export { Label } from './Label';

export type { LabelProps } from './Label';

// 容器与反馈控件

export { Dialog } from './Dialog';

export { Tabs } from './Tabs';

export { Badge } from './Badge';

// 工具类

export { cn } from './cn';

```

**三方消费示例**：

- **Host 宿主**`import { Button } from '@datazen/ui';`

- **Driver 自定义向导**（如 Redis `ConnectionWizard.tsx`）：

  ```typescript

  import { Input, Select, Label } from '@datazen/ui';

  import type { ConnectionFormState } from '@datazen/driver-sdk';

  ```

- **Workspace App 开发者**`pnpm add @datazen/ui`，实现与宿主像素级一致的视觉体验。

---

### 3. 特权扩展点契约`@datazen/extension-points`

- **物理位置**`packages/extension-points/`

- **定位**：用于宿主主进程内特权插槽的强类型契约。受 `LICENSE` 中的 **GPL-3.0 Linking Exception** 保护，允许独立仓库使用商业闭源或独立许可发布。

```typescript

// packages/extension-points/src/index.ts

export interface ExtensionPoint<T> {

  readonly id: string;

  readonly name: string;

  readonly description?: string;

  getDefault(): T; // 宿主内置的基础开源 Fallback 实现

}

export interface CreateExtensionPointOptions<T> {

  id: string;

  name: string;

  description?: string;

  getDefault: () => T;

}

export function createExtensionPoint<T>(options: CreateExtensionPointOptions<T>): ExtensionPoint<T>;

export class ExtensionRegistry {

  register<T>(point: ExtensionPoint<T>, impl: T): () => void;

  unregister<T>(point: ExtensionPoint<T>): void;

  get<T>(point: ExtensionPoint<T>): T;

  isEnhanced<T>(point: ExtensionPoint<T>): boolean;

  subscribe(id: string, listener: () => void): () => void;

  reset(): void;

}

export const extensionRegistry: ExtensionRegistry;

// React 响应式订阅 Hook

export function useExtension<T>(point: ExtensionPoint<T>): T;

export function useIsExtensionEnhanced<T>(point: ExtensionPoint<T>): boolean;

```

**独立闭源扩展仓库（例如 `datazen-extension-sql-editor-pro`）的开发范式**：

```typescript

// 独立仓库只需声明 devDependencies: { "@datazen/extension-points": "^0.1.0" }

import { extensionRegistry } from '@datazen/extension-points';

import { sqlEditorProEP, type SqlEditorFeature } from './contract';

export class AdvancedSqlDiagnostics implements SqlEditorFeature {

  // 商业级深度 AST 解析与错误定位...

}

export function activate() {

  extensionRegistry.register(sqlEditorProEP, new AdvancedSqlDiagnostics());

}

```

---

### 4. 工作区应用客户端`@datazen/app-sdk`

- **物理位置**`packages/app-sdk/`（由 `packages/extension-sdk/` 平滑更名而来）

- **定位**：运行于 `<iframe>` 沙箱环境中的轻量级 RPC 客户端，通过 `window.postMessage` 与宿主 `src/lib/extensionBridge.ts` 通信。

```typescript

// packages/app-sdk/src/index.ts

export { createClient } from './bridge';

export type { AppClient, HostContext, ConnectionSummary } from './bridge';

// 主题状态监听（不依赖 React，纯 CSS 变量与事件）

export {

  subscribeTheme,

  applyThemeSnapshot,

  getThemeState,

} from './theme';

export type { ThemeSnapshot, ThemeState } from './theme';

// 可选 React 绑定：@datazen/app-sdk/react

export { useTheme } from './react';

```

---

### 5. 外观主题包：Theme Specification

- **物理位置**`{appData}/themes/{themeId}/`

- **定位**：纯静态资源描述，零代码执行，安全级别最高。

**文件布局规范**：

```text

my-theme/

├── manifest.json       # { id: "theme.slate", name: "Slate Theme", modes: ["dark", "light"] }

├── tokens.css          # --bg-surface, --text-main, --dt-string 等全局变量

├── editor.json         # CodeMirror 语法高亮配色映射表

├── charts.json         # ECharts 序列与坐标轴调色盘

└── icons/              # 可选覆盖的 SVG 目录

```

---

## 五、平滑迁移路线图（四阶段执行计划）

为避免对当前持续集成的破坏，迁移过程遵循“**新建物理包 $\rightarrow$ 设置兼容别名 $\rightarrow$ 逐层替换调用 $\rightarrow$ 清理废弃代号**”的规范。

```

                          【架构迁移四阶段甘特图】

  阶段一：物理分包与兼容别名 ──► 阶段二：UI设计系统沉淀 ──► 阶段三：调用方平滑替换 ──► 阶段四：规范与法务锁定

  (零风险，兼容别名保障CI)     (@datazen/ui 抽离)        (驱动与宿主 import 迁移)   ([AGENTS.md](http://AGENTS.md) & LICENSE)

```

### 阶段一：物理分包与兼容别名（保障存量代码 100% 编译通过）

1. **创建新包骨架**：

   - 创建 `packages/extension-points/`，将特权插槽逻辑抽离入内；

   - 创建 `packages/driver-sdk/`，承载纯净数据库驱动契约；

   - 将 `packages/extension-sdk/` 更名为 `packages/app-sdk/`。

2. **在 `tsconfig.json` 与 Vite 中设置向后兼容别名**：

   ```json

   {

     "compilerOptions": {

       "paths": {

         "@datazen/driver-sdk": ["./packages/driver-sdk/src/index.ts"],

         "@datazen/extension-points": ["./packages/extension-points/src/index.ts"],

         "@datazen/app-sdk": ["./packages/app-sdk/src/index.ts"],

         "@datazen/plugin-sdk": ["./packages/driver-sdk/src/index.ts"],

         "@datazen/extension-sdk": ["./packages/app-sdk/src/index.ts"]

       }

     }

   }

   ```

3. **输出 `@deprecated` 提示**：

   在旧别名导入处保留 JSDoc 警告，确保旧代码不受影响，CI 全绿。

---

### 阶段二：设计系统沉淀（创建 `@datazen/ui`）

1. **抽离通用 UI 控件**：

   - 创建 `packages/ui/`；

   - 将 `src/components/ui/` 中的原子级控件`Button.tsx`, `Input.tsx`, `Select.tsx`, `Dialog.tsx`, `Tabs.tsx`, `Badge.tsx`）和 `src/lib/cn.ts` 迁移至 `packages/ui/`；

   - `src/components/ui/` 仅保留宿主专属业务弹窗（如 `LimitationsDialogResultMessageDialog` 等）。

2. **配置打包与导出**：

   - 配置 `packages/ui/tsconfig.json` 和 `tsupvite` 构建配置，支持 ESM 输出；

   - 确保外部驱动（如 Redis 向导）与沙箱应用均可独立导入。

---

### 阶段三：调用方平滑替换（消除旧路径）

1. **驱动包替换**：

   - 检索 `packages/drivers/*/ui/`：

     - 元数据与方言导入全部替换为 `@datazen/driver-sdk`；

     - Redis 连接向导界面的 `ButtonSelect` 替换为 `@datazen/ui`。

2. **自动化代码生成脚本更新**：

   - 修改 `scripts/resolve-drivers.mjs` 和 `scripts/driver-deinject.mjs`，生成的 `src/plugins/generated.ts` 中的类型导入一律指向 `@datazen/driver-sdk`。

3. **宿主特权调用方替换**：

   - 宿主 `SqlEditor.tsx` 及测试代码中的扩展点引用改为 `@datazen/extension-points`。

---

### 阶段四：规范收口与法务条款锁定

1. **更新 `AGENTS.md`**：

   - 在关键架构原则中正式定义 **Driver / Theme / EP / Workspace App** 四维架构，禁止在代码评审中混用名词。

2. **更新根目录 `LICENSE`**：

   - 在 **DataZen Plugin and Extension Linking Exception** 中准确声明豁免范围：

     - 允许独立模块通过 `@datazen/driver-sdk` 编写私有数据库驱动；

     - 允许独立模块通过 `@datazen/extension-points` 链接闭源宿主扩展；

     - 允许通过 `@datazen/app-sdk` 编写独立的 Workspace App。

---

## 六、质量门禁与回归验证策略

每个阶段在推进时，必须通过以下三层测试门禁：

| 验证层级 | 检验范围 | 验收命令 | 预期指标 |

| :--- | :--- | :--- | :--- |

| **1. 驱动方言与UI测试** | 所有 Path 驱动 UI 单测 | `pnpm test:unit:drivers` | 18+ 文件全部通过，驱动方言生成严格正确 |

| **2. Host 单元测试** | 宿主组件、Store 与编辑器 | `npx vitest run` | 379+ 文件全量通过，无引用中断 |

| **3. Rust 后端编译** | 后端 Driver API 与 Command | `cargo test -p datazen --lib` | 0 errors, 0 warnings，命令调用链完备 |

---

## 七、实施落地状态 (Status: COMPLETED)

已全部完成并合入 `feat/sql-editor`：
1. **阶段一 (SDK 物理分包)**：`packages/driver-sdk`、`packages/extension-points`、`packages/app-sdk` 创建并生效。
2. **阶段二 (公共设计系统)**：`packages/ui` 建立并导出原子组件与 cn 工具。
3. **阶段三 (调用方平滑替换)**：驱动包已切至 `@datazen/driver-sdk` 与 `@datazen/ui`，`resolve-drivers.mjs` codegen 已对齐。
4. **阶段四 (私有增强仓与宿主净化)**：
   - 私有增强仓 `https://github.com/flyxl/datazen-extension-sql-editor-pro` 创建并推送。
   - 宿主 `datazen` 仓库完全净化，仅留纯净 open-source fallback 扩展点。
   - `AGENTS.md`、`LICENSE` linking exception 均已同步。
   - 全量回归测试（Vitest、Rust、Driver tests、tsc）均 100% 通过。