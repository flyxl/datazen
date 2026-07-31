# RFC: DataZen 插件化架构设计

> **状态**: Draft  
> **日期**: 2026-07-31  
> **作者**: DataZen Team

## 摘要

本文档描述 DataZen 的模块化插件系统设计，参考 Caddy 2 的编译时模块化方案，支持在独立仓库中开发数据库驱动插件（含前后端代码），并通过构建命令参数指定需要加载的插件，实现定制化编译。

## 动机

当前 DataZen 所有数据库驱动（PostgreSQL、MySQL、SQLite、Redis、Presto/Trino、Kiwi）硬编码在主仓库中，存在以下问题：

1. **体积膨胀** — 每个安装包包含所有驱动，即使用户只需要其中一两个
2. **耦合度高** — 添加/修改驱动必须修改主仓库代码
3. **无法外部扩展** — 第三方开发者无法在独立仓库实现新驱动
4. **特殊驱动维护负担** — 如 Kiwi 这类有独特前端 UI 的驱动，其前后端代码都绑定在主仓库

## 设计目标

1. 数据库驱动可在**独立仓库**中开发和维护
2. 添加新驱动**不需要修改主工程的任何源代码**
3. 构建时通过**命令行参数**指定需要包含的插件
4. 插件可包含**自定义前端组件**（不仅限于后端逻辑）
5. 标准 SQL 数据库可**零前端代码**接入（通过元数据驱动的通用 UI）

## 概览

```
                    构建时
                      │
    pnpm tauri:build --plugins="postgres,kiwi" --with="github:user/plugin-tidb"
                      │
                      ▼
         ┌─────────────────────────┐
         │  resolve-plugins.mjs    │ ← 预构建脚本
         │  1. 解析参数            │
         │  2. 拉取/链接插件源码   │
         │  3. 注入 Cargo.toml     │
         │  4. 生成前端 import map │
         └─────────────────────────┘
                      │
          ┌───────────┴───────────┐
          ▼                       ▼
   cargo build               vite build
   (Rust 后端编译)            (前端编译)
   含所选插件驱动             含所选插件 UI
          │                       │
          └───────────┬───────────┘
                      ▼
              最终安装包 (.dmg/.msi/.deb)
```

## 详细设计

### 1. 插件结构约定

每个插件是一个独立的 Git 仓库，同时包含 Rust 后端和可选的 TypeScript 前端：

```
datazen-plugin-kiwi/
├── Cargo.toml                 # Rust crate 定义
├── src/
│   └── lib.rs                 # DatabaseDriver trait 实现 + inventory 注册
├── ui/                        # [可选] 前端自定义组件
│   ├── index.ts               # 导出入口
│   ├── ConnectionForm.tsx     # 自定义连接表单
│   ├── Browser.tsx            # 自定义数据浏览器
│   └── package.json           # 前端依赖（如有额外依赖）
└── plugin.json                # 插件清单文件
```

#### 插件清单 `plugin.json`

```json
{
  "id": "kiwi",
  "label": "Kiwi",
  "version": "0.1.0",
  "description": "Kiwi cloud database driver for DataZen",
  "backend": {
    "crate": "datazen-plugin-kiwi"
  },
  "frontend": {
    "connectionForm": "./ui/ConnectionForm.tsx",
    "panels": [
      {
        "id": "kiwi-browser",
        "label": "Data Browser",
        "component": "./ui/Browser.tsx",
        "slot": "main-panel"
      }
    ]
  },
  "meta": {
    "defaultPort": 443,
    "connectionMode": "url",
    "sqlDialect": "mysql",
    "quoteChar": "`",
    "icon": { "label": "Ki", "color": "text-emerald-500", "bg": "bg-emerald-100" }
  },
  "capabilities": ["sql-browser", "query-editor", "custom-browser"]
}
```

对于标准 SQL 数据库（无需自定义 UI），`frontend` 字段可完全省略：

```json
{
  "id": "tidb",
  "label": "TiDB",
  "version": "0.1.0",
  "backend": { "crate": "datazen-plugin-tidb" },
  "meta": {
    "defaultPort": 4000,
    "connectionMode": "server",
    "sqlDialect": "mysql",
    "quoteChar": "`",
    "icon": { "label": "Ti", "color": "text-blue-500", "bg": "bg-blue-100" }
  },
  "capabilities": ["sql-browser", "query-editor", "explain"]
}
```

### 2. 后端：Rust 插件自注册

#### 2.1 公共 API Crate: `datazen-driver-api`

从主仓库拆分出一个独立 crate，定义所有公共 trait 和类型：

```rust
// datazen-driver-api/src/lib.rs

pub use async_trait::async_trait;
pub use inventory;

// 重导出核心类型
pub mod types {
    pub use crate::{
        ColumnInfo, ColumnSchema, ConnectionConfig, ConnectionHandle,
        DatabaseType, DriverCategory, DriverError, ExplainResult,
        ForeignKeyInfo, IndexInfo, MultiQueryResult, QueryResult,
        ServerInfo, TableDataResult, TableInfo, TableSchema, Value,
    };
}

/// 驱动工厂 trait，用于 inventory 自注册
pub trait DatabaseDriverFactory: Send + Sync + 'static {
    fn create(&self) -> Box<dyn DatabaseDriver>;
    fn driver_id(&self) -> &str;
}

inventory::collect!(Box<dyn DatabaseDriverFactory>);

/// 注册宏，简化外部驱动的注册代码
#[macro_export]
macro_rules! register_driver {
    ($factory:expr) => {
        inventory::submit! {
            Box::new($factory) as Box<dyn datazen_driver_api::DatabaseDriverFactory>
        }
    };
}
```

#### 2.2 外部驱动实现示例

```rust
// datazen-plugin-tidb/src/lib.rs

use datazen_driver_api::*;

pub struct TiDBDriver { /* ... */ }

#[async_trait]
impl DatabaseDriver for TiDBDriver {
    fn driver_type(&self) -> DatabaseType { DatabaseType::Custom("tidb".into()) }
    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        // TiDB 兼容 MySQL 协议，可复用 sqlx MySqlPool
        todo!()
    }
    // ... 其他方法实现
}

struct TiDBDriverFactory;
impl DatabaseDriverFactory for TiDBDriverFactory {
    fn create(&self) -> Box<dyn DatabaseDriver> { Box::new(TiDBDriver::new()) }
    fn driver_id(&self) -> &str { "tidb" }
}

// 一行注册
datazen_driver_api::register_driver!(TiDBDriverFactory);
```

#### 2.3 主工程驱动发现

```rust
// src-tauri/src/db/registry.rs

pub async fn init_drivers() -> DriverRegistry {
    let registry = DriverRegistry::new();

    // 自动发现所有通过 inventory 注册的驱动
    for factory in inventory::iter::<Box<dyn DatabaseDriverFactory>> {
        let driver = factory.create();
        registry.register(driver).await;
    }

    registry
}
```

#### 2.4 DatabaseType 扩展

为支持外部定义的数据库类型，`DatabaseType` 枚举需增加动态变体：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    PostgreSQL,
    MySQL,
    MariaDB,
    SQLite,
    Redis,
    Presto,
    Trino,
    /// 外部插件定义的自定义类型
    #[serde(untagged)]
    Custom(String),
}
```

### 3. 前端：动态 UI 架构

#### 3.1 插件 Registry（运行时）

前端启动时从后端获取所有可用驱动的元数据：

```typescript
// src/plugins/registry.ts

export interface PluginMeta {
  id: string;
  label: string;
  defaultPort: number;
  connectionMode: 'server' | 'file' | 'url';
  sqlDialect: string;
  quoteChar: string;
  icon: { label: string; color: string; bg: string };
  capabilities: string[];
  hasCustomForm: boolean;
  hasCustomPanels: boolean;
}

// 构建时生成的插件组件注册表
export interface PluginComponents {
  connectionForm?: React.ComponentType<ConnectionFormProps>;
  panels?: Record<string, React.ComponentType<PanelProps>>;
}

// 由 resolve-plugins.mjs 自动生成
export const pluginRegistry: Record<string, PluginComponents> = {};
```

#### 3.2 构建时生成文件

`resolve-plugins.mjs` 生成的 `src/plugins/generated.ts`：

```typescript
// AUTO-GENERATED by resolve-plugins.mjs — DO NOT EDIT

import { lazy } from 'react';

export const pluginComponents = {
  kiwi: {
    connectionForm: lazy(() => import('../../.plugins/kiwi/ui/ConnectionForm')),
    panels: {
      'kiwi-browser': lazy(() => import('../../.plugins/kiwi/ui/Browser')),
    },
  },
  // tidb 没有自定义 UI，不在此列出
};

export const pluginManifests = [
  {"id":"kiwi","label":"Kiwi","meta":{...},"capabilities":[...]},
  {"id":"tidb","label":"TiDB","meta":{...},"capabilities":[...]},
];
```

#### 3.3 前端渲染逻辑

```tsx
// src/components/ConnectionForm.tsx

function ConnectionForm({ dbType }: { dbType: string }) {
  const CustomForm = pluginComponents[dbType]?.connectionForm;

  if (CustomForm) {
    return <Suspense fallback={<Loading />}><CustomForm /></Suspense>;
  }

  // 无自定义表单 → 使用通用动态表单
  const meta = getPluginMeta(dbType);
  return <GenericConnectionForm meta={meta} />;
}
```

```tsx
// src/components/DatabasePanel.tsx

function DatabasePanel({ connection }: { connection: Connection }) {
  const panels = pluginComponents[connection.databaseType]?.panels;

  if (panels?.['main-panel']) {
    const CustomPanel = panels['main-panel'];
    return <Suspense fallback={<Loading />}><CustomPanel connection={connection} /></Suspense>;
  }

  // 默认使用通用 SQL 浏览器
  return <GenericSQLBrowser connection={connection} />;
}
```

#### 3.4 插件前端 API（暴露给插件组件的接口）

插件的 React 组件可以使用主工程暴露的 hooks 和工具：

```typescript
// datazen-plugin-api (npm 包) / src/index.ts

export interface ConnectionFormProps {
  config: ConnectionConfig;
  onChange: (config: Partial<ConnectionConfig>) => void;
  onTest: () => Promise<TestResult>;
  onSave: () => void;
}

export interface PanelProps {
  connection: ActiveConnection;
  invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
}

// 提供给插件使用的通用 UI 组件
export { Input, Button, Select, Table, CodeEditor } from './components';
```

### 4. 构建系统

#### 4.1 命令行接口

```bash
# 使用预设注册表中的插件
pnpm tauri:build --plugins="postgres,mysql,sqlite,redis"

# 使用外部 Git 仓库插件
pnpm tauri:build --with="github:user/datazen-plugin-tidb@v0.1.0"

# 混合使用
pnpm tauri:build --plugins="postgres,mysql" --with="github:corp/datazen-plugin-kiwi@v0.1.0"

# 全量构建（所有预设插件）
pnpm tauri:build --plugins="all"

# 通过环境变量（CI/CD 场景）
DATAZEN_PLUGINS="postgres,mysql,sqlite" pnpm tauri:build
```

#### 4.2 预设插件注册表

```json
// plugins-registry.json
{
  "postgres": {
    "source": "builtin",
    "path": "plugins/postgres",
    "description": "PostgreSQL driver"
  },
  "mysql": {
    "source": "builtin",
    "path": "plugins/mysql",
    "description": "MySQL / MariaDB driver"
  },
  "sqlite": {
    "source": "builtin",
    "path": "plugins/sqlite",
    "description": "SQLite driver"
  },
  "redis": {
    "source": "builtin",
    "path": "plugins/redis",
    "description": "Redis key-value store"
  },
  "kiwi": {
    "source": "git",
    "git": "https://github.com/corp/datazen-plugin-kiwi",
    "description": "Kiwi cloud database"
  },
  "tidb": {
    "source": "git",
    "git": "https://github.com/user/datazen-plugin-tidb",
    "description": "TiDB distributed SQL"
  }
}
```

#### 4.3 resolve-plugins.mjs 核心逻辑

```javascript
// scripts/resolve-plugins.mjs (伪代码)

async function resolvePlugins() {
  const plugins = parseArgs(); // 解析 --plugins 和 --with 参数

  // 1. 获取插件源码
  for (const plugin of plugins) {
    if (plugin.source === 'builtin') {
      // 内置插件，直接使用 workspace 内路径
      symlink(`plugins/${plugin.id}`, `.plugins/${plugin.id}`);
    } else if (plugin.source === 'git') {
      // 外部插件，git clone 到 .plugins/
      await gitClone(plugin.git, `.plugins/${plugin.id}`, plugin.tag);
    }
  }

  // 2. 读取所有 plugin.json 清单
  const manifests = plugins.map(p => readJson(`.plugins/${p.id}/plugin.json`));

  // 3. 注入 Rust 依赖到 src-tauri/Cargo.toml
  injectCargoDeps(manifests);

  // 4. 生成前端 import map
  generateFrontendRegistry(manifests);

  // 5. 安装插件的前端依赖（如有）
  await installPluginDeps(manifests);
}
```

#### 4.4 Cargo.toml 注入

脚本在 `src-tauri/Cargo.toml` 中动态添加插件依赖：

```toml
# === AUTO-GENERATED PLUGIN DEPENDENCIES (DO NOT EDIT) ===
[dependencies.datazen-plugin-postgres]
path = "../.plugins/postgres"

[dependencies.datazen-plugin-kiwi]
path = "../.plugins/kiwi"

[dependencies.datazen-plugin-tidb]
git = "https://github.com/user/datazen-plugin-tidb"
tag = "v0.1.0"
# === END PLUGIN DEPENDENCIES ===
```

### 5. 插件能力系统 (Capabilities)

前端根据插件声明的 capabilities 决定渲染哪些 UI 模块：

| Capability | 含义 | 前端行为 |
|---|---|---|
| `sql-browser` | 支持表结构浏览 | 显示数据库/表树 + 表数据网格 |
| `query-editor` | 支持 SQL 查询 | 显示 SQL 编辑器 + 执行按钮 |
| `explain` | 支持执行计划 | 显示 Explain 按钮和面板 |
| `kv-browser` | Key-Value 浏览 | 显示 KV 专属浏览器 |
| `schema-selector` | 支持 Schema 切换 | 在工具栏显示 Schema 下拉框 |
| `transaction` | 支持事务 | 显示事务控制按钮 |
| `custom-browser` | 使用自定义浏览器 | 加载插件提供的 panel 组件 |

### 6. 两层插件模型

```
┌──────────────────────────────────────────────────────────────┐
│ 第一层：轻量级驱动（纯后端，无自定义 UI）                       │
│                                                              │
│   适用：TiDB, ClickHouse, CockroachDB, Oracle, MSSQL...     │
│                                                              │
│   实现内容：                                                  │
│   ├── Rust: DatabaseDriver trait                             │
│   ├── plugin.json (meta + capabilities)                      │
│   └── 无 ui/ 目录                                            │
│                                                              │
│   前端：自动使用通用 SQL 浏览器 + 动态连接表单                │
│   开发成本：~1-2 天                                           │
├──────────────────────────────────────────────────────────────┤
│ 第二层：全栈插件（有自定义 UI 组件）                           │
│                                                              │
│   适用：Kiwi, Redis, Neo4j, MongoDB, 时序数据库...            │
│                                                              │
│   实现内容：                                                  │
│   ├── Rust: DatabaseDriver trait                             │
│   ├── plugin.json (meta + capabilities + frontend 声明)      │
│   └── ui/: React 组件 (连接表单, 浏览器, 设置面板...)         │
│                                                              │
│   前端：加载插件自定义组件                                    │
│   开发成本：~3-5 天                                           │
└──────────────────────────────────────────────────────────────┘
```

### 7. 目录结构（改造后）

```
datazen/
├── packages/
│   └── driver-api/            # 公共 API crate (独立发布)
│       ├── Cargo.toml
│       └── src/lib.rs         # DatabaseDriver trait, types, macros
│
├── plugins/                   # 内置插件（各自可独立为仓库）
│   ├── postgres/
│   │   ├── Cargo.toml
│   │   ├── src/lib.rs
│   │   └── plugin.json
│   ├── mysql/
│   ├── sqlite/
│   ├── redis/
│   │   ├── Cargo.toml
│   │   ├── src/lib.rs
│   │   ├── plugin.json
│   │   └── ui/               # Redis KV 浏览器
│   └── olap/
│
├── .plugins/                  # [gitignore] 外部插件的临时目录
│
├── plugins-registry.json      # 预设插件注册表
├── scripts/
│   └── resolve-plugins.mjs   # 预构建脚本
│
├── src-tauri/                 # 主 Rust binary
│   ├── Cargo.toml             # 核心依赖 + 自动注入的插件依赖
│   └── src/
│       ├── db/
│       │   ├── mod.rs         # trait 定义 (re-export from driver-api)
│       │   └── registry.rs   # inventory 自动发现
│       └── ...
│
└── src/                       # React 前端
    ├── plugins/
    │   ├── generated.ts       # [auto-generated] 插件组件 import map
    │   ├── registry.ts        # 插件元数据注册表
    │   └── slots.tsx          # 插件插槽渲染逻辑
    └── ...
```

### 8. 插件开发者工作流

#### 开发新的标准 SQL 驱动（第一层）

```bash
# 1. 创建仓库
mkdir datazen-plugin-tidb && cd datazen-plugin-tidb
cargo init --lib

# 2. 添加依赖
# Cargo.toml: datazen-driver-api = { git = "https://github.com/datazen/driver-api" }

# 3. 实现 trait
# src/lib.rs: impl DatabaseDriver for TiDBDriver { ... }

# 4. 写 plugin.json
# { "id": "tidb", "label": "TiDB", "meta": {...} }

# 5. 测试
cargo test

# 6. 发布
git tag v0.1.0 && git push --tags
```

#### 在 DataZen 中使用

```bash
pnpm tauri:build --with="github:user/datazen-plugin-tidb@v0.1.0"
```

### 9. 实施路线

| 阶段 | 内容 | 工期估算 |
|------|------|---------|
| **Phase 1** | 拆分 `datazen-driver-api` 公共 crate | 2-3 天 |
| **Phase 2** | 实现 inventory 自注册机制 + 改造 DriverRegistry | 2 天 |
| **Phase 3** | 将现有驱动改为插件结构（先 postgres 作为 PoC） | 3 天 |
| **Phase 4** | 实现 `resolve-plugins.mjs` 构建脚本 | 3-4 天 |
| **Phase 5** | 前端动态化改造（通用表单 + 插件插槽） | 5-7 天 |
| **Phase 6** | 将所有内置驱动迁移为插件 | 3-4 天 |
| **Phase 7** | 文档 + 插件开发模板仓库 | 2 天 |

总计约 **3-4 周**（一人全职）。

### 10. 风险与缓解

| 风险 | 缓解措施 |
|------|---------|
| `inventory` crate 平台兼容性 | 桌面三平台 (macOS/Windows/Linux) 均已验证支持 |
| 插件 Rust ABI 兼容 | 使用 trait object + 同一编译器版本（构建时链接，非动态加载） |
| 前端 lazy import 性能 | 桌面应用启动时一次性加载，影响可忽略 |
| 构建脚本复杂度 | 分阶段实施，先支持 builtin 路径，再支持 git URL |
| 插件版本与主工程不兼容 | driver-api 遵循 semver，主工程声明最低兼容版本 |

### 11. 与 Caddy 2 方案的对照

| 维度 | Caddy 2 | DataZen (本方案) |
|------|---------|-----------------|
| 语言 | Go | Rust + TypeScript |
| 插件定义 | Go package + Caddy module interface | Rust crate + DatabaseDriver trait |
| 自注册 | `init()` + `caddy.RegisterModule()` | `inventory::submit!` |
| 构建工具 | xcaddy | resolve-plugins.mjs |
| 命令行 | `xcaddy build --with ...` | `pnpm tauri:build --with ...` |
| 前端 | 无 | 插件可携带 React 组件 |
| 产物 | 单文件 binary | 安装包 (.dmg/.msi/.deb) |
| 插件粒度 | 纯后端 | 全栈（前后端一体） |

## 附录

### A. 支持的前端插槽 (Slots)

| Slot ID | 位置 | 用途 |
|---------|------|------|
| `connection-form` | 新建连接弹窗 | 自定义连接表单 |
| `main-panel` | 主内容区 | 自定义数据浏览器 |
| `toolbar-actions` | 工具栏右侧 | 自定义操作按钮 |
| `settings-panel` | 设置页 | 插件特有的设置选项 |
| `sidebar-section` | 左侧边栏 | 自定义导航节点 |

### B. 通用连接表单字段定义

```typescript
interface FormField {
  name: string;
  label: string;
  type: 'text' | 'number' | 'password' | 'select' | 'toggle' | 'file';
  required: boolean;
  default?: string | number | boolean;
  placeholder?: string;
  options?: { value: string; label: string }[];
  group?: 'basic' | 'auth' | 'ssl' | 'advanced';
  visibleWhen?: { field: string; value: unknown };
}
```

### C. Plugin SDK — 前端一致性保障

插件前端必须通过 `@datazen/plugin-sdk` 与宿主应用交互，确保主题、语言、字体等切换时所有插件保持一致。

#### C.1 设计原则

- **插件不持有任何全局状态** — 所有环境信息通过 SDK hooks 获取
- **CSS 变量优先** — 颜色、字体、圆角等视觉属性通过 CSS 变量传递，主题/字体切换时浏览器自动级联更新
- **组件复用** — 插件使用 SDK 提供的 UI 组件库（与主应用同款），确保风格一致
- **构建时检查** — resolve-plugins 脚本验证插件合规性

#### C.2 主题一致性

主应用定义一套 CSS Design Tokens，插件必须使用这些变量：

```css
:root {
  --dz-bg-primary: #ffffff;
  --dz-bg-secondary: #f8f9fa;
  --dz-bg-tertiary: #f1f3f5;
  --dz-fg-primary: #1a1a1a;
  --dz-fg-secondary: #4b5563;
  --dz-fg-muted: #9ca3af;
  --dz-border: #e5e7eb;
  --dz-border-focus: #3b82f6;
  --dz-accent: #3b82f6;
  --dz-accent-hover: #2563eb;
  --dz-danger: #ef4444;
  --dz-success: #10b981;
  --dz-warning: #f59e0b;
  --dz-radius-sm: 4px;
  --dz-radius-md: 6px;
  --dz-radius-lg: 8px;
  --dz-shadow-sm: 0 1px 2px rgba(0,0,0,0.05);
  --dz-font-sans: var(--user-font-sans, 'Inter', system-ui, sans-serif);
  --dz-font-mono: var(--user-font-mono, 'JetBrains Mono', monospace);
  --dz-font-size-xs: 12px;
  --dz-font-size-sm: 13px;
  --dz-font-size-base: 14px;
  --dz-font-size-lg: 16px;
}

[data-theme="dark"] {
  --dz-bg-primary: #1a1a2e;
  --dz-bg-secondary: #16213e;
  --dz-bg-tertiary: #0f3460;
  --dz-fg-primary: #e0e0e0;
  --dz-fg-secondary: #a0a0a0;
  --dz-fg-muted: #6b7280;
  --dz-border: #2d3748;
  /* ... */
}
```

**插件无需监听主题切换事件 — CSS 变量由浏览器自动级联更新，React 组件无需重渲染。**

插件使用方式：
```tsx
// 方式1: 直接 CSS 变量
<div style={{ background: 'var(--dz-bg-secondary)', borderRadius: 'var(--dz-radius-md)' }}>

// 方式2: Tailwind（通过 SDK preset 映射）
<div className="bg-dz-secondary rounded-dz-md text-dz-fg-primary">

// 方式3: SDK 组件（已内置正确样式）
import { Card, Text } from '@datazen/plugin-sdk';
<Card><Text>内容</Text></Card>
```

#### C.3 语言/国际化一致性

**方案：主应用注入 locale context + 插件自带翻译文件**

```typescript
// 插件使用 SDK 提供的 i18n hook
import { useI18n } from '@datazen/plugin-sdk';

function KiwiBrowser() {
  const { t, locale } = useI18n();
  // t() 自动查找当前 locale 对应的翻译
  return <h1>{t('browser.title')}</h1>;
}
```

插件结构：
```
ui/
├── locales/
│   ├── en.json      { "browser.title": "Data Browser", "actions.refresh": "Refresh" }
│   └── zh-CN.json   { "browser.title": "数据浏览器", "actions.refresh": "刷新" }
├── index.ts
└── Browser.tsx
```

`plugin.json` 声明翻译文件位置：
```json
{
  "frontend": {
    "locales": "./ui/locales"
  }
}
```

构建时，resolve-plugins 将所有插件翻译合并进主应用的 i18n 资源中（按 namespace 隔离）。

主应用切换语言时，React Context 触发所有使用 `useI18n()` 的插件组件重渲染。

#### C.4 字体一致性

**方案：字体通过 CSS 变量统一管理，插件禁止自定义字体**

```css
/* 主应用字体变量（用户切换字体时更新） */
:root {
  --dz-font-sans: 'Inter', system-ui, sans-serif;   /* 用户可切换 */
  --dz-font-mono: 'JetBrains Mono', monospace;      /* 用户可切换 */
  --dz-font-size-base: 14px;                        /* 用户可调整 */
}
```

插件必须使用：
```css
.plugin-text { font-family: var(--dz-font-sans); font-size: var(--dz-font-size-base); }
.plugin-code { font-family: var(--dz-font-mono); }
```

SDK 组件已内置正确字体引用，插件使用 SDK 组件即自动一致。

#### C.5 通知/Toast 系统

插件不应自己实现 toast，而是调用宿主提供的统一接口：

```typescript
import { toast } from '@datazen/plugin-sdk';

// 插件中使用
toast.success('数据已保存');
toast.error('连接失败');
toast.info('正在同步...');
```

#### C.6 Dialog/Modal 系统

```typescript
import { dialog } from '@datazen/plugin-sdk';

// 确认弹窗
const confirmed = await dialog.confirm({
  title: '删除确认',
  message: '确定要删除这条记录吗？',
  confirmLabel: '删除',
  variant: 'danger',
});

// 自定义内容弹窗
dialog.open({
  title: '导入设置',
  content: <ImportSettings />,
  size: 'lg',
});
```

#### C.7 Tauri IPC 通信

插件通过 SDK 安全调用后端命令：

```typescript
import { invoke } from '@datazen/plugin-sdk';

// SDK 内部自动附加 pluginId 前缀，防止命名冲突
const result = await invoke<QueryResult>('kiwi_custom_action', { param: 'value' });
```

后端对应：
```rust
#[tauri::command]
async fn kiwi_custom_action(param: String) -> Result<QueryResult, String> { ... }
```

#### C.8 CSS 隔离策略

防止插件 CSS 污染主应用：

```tsx
// 主应用的 PluginHost 包装器
function PluginHost({ pluginId, children }: PluginHostProps) {
  return (
    <div
      className="plugin-boundary"
      data-plugin={pluginId}
      style={{ contain: 'layout style' }}  // CSS containment
    >
      {children}
    </div>
  );
}
```

规则：
- 插件只能使用 CSS Modules 或 Tailwind（通过 SDK preset）
- 禁止全局 CSS（构建时检查）
- SDK preset 自动为 Tailwind 类添加 `dz-` 前缀

#### C.9 Plugin SDK 完整 API

```typescript
// @datazen/plugin-sdk

// === 环境感知 ===
export function useTheme(): { mode: 'light' | 'dark' };
export function useLocale(): { locale: string; direction: 'ltr' | 'rtl' };
export function useI18n(): { t: (key: string, params?: object) => string; locale: string };
export function useFont(): { sans: string; mono: string; size: number };

// === UI 组件库（与主应用同款样式） ===
export { Button, IconButton } from './components/button';
export { Input, Textarea, Select, Toggle, Checkbox } from './components/form';
export { Card, Panel, Divider } from './components/layout';
export { Table, DataGrid } from './components/data';
export { Tabs, TabList, Tab, TabPanel } from './components/tabs';
export { Dialog, AlertDialog, Sheet, Popover, Tooltip } from './components/overlays';
export { Spinner, Skeleton, Progress } from './components/loading';
export { Badge, Tag } from './components/display';
export { CodeEditor } from './components/editor';
export { Icon } from './components/icon';

// === 反馈系统 ===
export { toast } from './toast';
export { dialog } from './dialog';

// === 数据通信 ===
export function invoke<T>(command: string, args?: object): Promise<T>;
export function useInvoke<T>(command: string, args?: object): {
  data: T | undefined; loading: boolean; error: Error | undefined; refetch: () => void;
};

// === 应用状态（只读） ===
export function useConnection(): ActiveConnection;
export function useAppSettings(): AppSettings;
export function usePanel(): { width: number; height: number };

// === 快捷键注册（自动避免冲突） ===
export function useHotkey(key: string, handler: () => void, opts?: { when?: boolean }): void;
```

#### C.10 构建时合规检查

`resolve-plugins.mjs` 自动验证插件合规性：

| 检查项 | 规则 |
|--------|------|
| SDK 依赖 | 必须依赖 `@datazen/plugin-sdk` |
| Tailwind preset | 如使用 Tailwind，必须引用 SDK preset |
| 翻译文件 | 必须提供 `en.json` + `zh-CN.json` |
| 无全局 CSS | 不允许 `.css` 文件中出现无作用域选择器 |
| 无字体声明 | CSS 中不允许出现 `font-family` 硬编码 |
| SDK 版本兼容 | 检查 plugin-sdk 版本与主应用兼容 |

不通过检查的插件，构建时报警告或错误。

#### C.11 插件开发体验

提供 `create-datazen-plugin` 脚手架：

```bash
npx create-datazen-plugin my-driver
# → 生成标准结构 + Cargo.toml + plugin.json + SDK 配置
# → 包含示例组件 + 翻译文件
# → 自带 dev 模式（独立预览插件 UI）
```

开发模式下，插件可独立运行预览：
```bash
cd datazen-plugin-kiwi
pnpm dev  # 启动 Storybook-like 环境，注入 mock theme/locale context
```

### D. 决策记录

- **为什么不用动态加载 (dlopen)**: Rust 没有稳定 ABI，动态插件需要 C-ABI 中间层，复杂度极高且易出错。桌面应用场景下，构建时静态链接是更可靠的选择。
- **为什么不用 WASM 插件**: 虽然可跨平台，但 WASM 中无法直接使用 sqlx 等网络库，性能也有损失。
- **为什么选择 inventory 而非 linkme**: inventory 在 Windows/macOS/Linux 均稳定工作，API 更简洁，社区使用更广泛。
- **为什么用 CSS 变量而非 JS theme context**: CSS 变量更新时无需 React 重渲染，性能更好，且天然支持子组件级联。主题切换是高频操作，避免全树重渲染很重要。
- **为什么强制使用 SDK 组件库**: 保证插件在任何主题/字体/语言下都与主应用完全一致，降低插件开发者的心智负担。
- **为什么构建时检查而非运行时**: 问题越早发现成本越低，构建时即可拦截不合规插件。
