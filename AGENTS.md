# AGENTS.md

> 本文件面向 AI 编程助手（如 Cursor Agent, GitHub Copilot, Codex 等），帮助其快速理解项目结构和约定。

## 项目概述

DataZen 是一个跨平台桌面数据库管理工具，基于 **Tauri v2**（Rust 后端 + React 前端）构建。

- **框架**：Tauri v2 + React + TypeScript + Tailwind CSS
- **后端语言**：Rust
- **包管理**：pnpm（前端）、Cargo workspace（Rust）
- **状态管理**：Zustand
- **测试**：Vitest（单元）、WebdriverIO（E2E）

## 目录结构

```
datazen/
├── src/                         # React 前端源码
│   ├── components/              # 通用 UI 组件
│   ├── windows/                 # 各窗口页面（main, connection, settings 等）
│   ├── stores/                  # Zustand 状态
│   ├── commands/                # Tauri IPC 命令封装
│   ├── lib/                     # 工具库（databaseTypes, sqlDialects, connectionViews）
│   ├── plugins/generated.ts     # 自动生成的插件注册（勿手动编辑）
│   ├── hooks/                   # React hooks（useI18n, useTheme 等）
│   ├── locales/                 # i18n 翻译文件（zh, en）
│   └── types/                   # TypeScript 类型定义
├── src-tauri/                   # Rust 后端
│   ├── src/
│   │   ├── db/                  # 数据库驱动（postgres, mysql, sqlite, redis_driver, registry）
│   │   ├── commands/            # Tauri IPC 命令实现
│   │   └── lib.rs              # 入口
│   └── Cargo.toml
├── packages/driver-api/         # 公共插件 API crate（traits + types + inventory 宏）
├── scripts/resolve-plugins.mjs  # 插件解析构建脚本
├── plugins-registry.json        # 插件注册表
├── .plugins/                    # 构建时生成的插件目录（gitignored）
├── e2e/                         # E2E 测试
├── docs/                        # 文档（RFC、插件开发指南等）
└── Cargo.toml                   # Workspace 根配置
```

## 核心架构模式

### 插件系统

DataZen 采用**编译时插件系统**（类似 Caddy 2）：

1. `plugins-registry.json` 定义所有可用插件
2. `scripts/resolve-plugins.mjs` 在构建前执行：
   - 从 Git 克隆插件到 `.plugins/`
   - 生成 `src/plugins/generated.ts`（前端集成）
   - 生成 `.plugin-features.json`（Cargo features）
3. 插件通过 `inventory` crate 实现链接时自动注册

**控制构建包含哪些插件**：
```bash
DATAZEN_PLUGINS=all pnpm build        # 所有插件（默认）
DATAZEN_PLUGINS=kiwi pnpm build       # 只包含 kiwi
DATAZEN_PLUGINS=none pnpm build       # 仅内置驱动
```

### 数据库驱动注册

- **内置驱动**（PostgreSQL, MySQL, MariaDB, SQLite, Redis）在 `src-tauri/src/db/registry.rs` 直接注册
- **插件驱动** 通过 `datazen-driver-api` 的 `register_driver!` 宏 + `inventory` 自动发现
- 前端 `DB_REGISTRY` 由 `src/lib/databaseTypes.ts`（内置）+ `src/plugins/generated.ts`（插件）合并

### 前端约定

- **零硬编码**：行为差异通过 `DB_REGISTRY` 元数据驱动，避免 `if (type === 'xxx')` 
- **表单路由**：`ConnectionFormBody.tsx` 通过 `connectionForm` 字段选择渲染哪个表单组件
- **视图路由**：`connectionViews/index.ts` 的 `CONNECTION_VIEWS` 映射视图组件
- **SQL 方言**：`sqlDialects/` 下的策略对象处理 DDL/索引/备份差异

### IPC 通信

前端通过 `src/commands/` 调用后端，后端实现在 `src-tauri/src/commands/`。两侧按领域对齐：
- `connection.rs` / `connection.ts` — 连接管理
- `query.rs` / `query.ts` — SQL 查询
- `schema.rs` / `schema.ts` — Schema 操作
- `kv.rs` / `kv.ts` — Redis 键值操作

## 开发命令

```bash
pnpm install                    # 安装依赖
pnpm dev                        # 启动 Vite dev server
pnpm tauri dev                  # 启动完整开发模式（前端 + Rust）
pnpm build                      # 构建前端（含 resolve-plugins）
pnpm tauri build                # 构建完整应用
npx vitest run                  # 运行单元测试
pnpm e2e                        # 运行 E2E 测试
```

## 代码风格

- **Rust**：标准 `rustfmt` 格式，使用 `thiserror` 处理错误，`tracing` 记录日志
- **TypeScript**：严格模式，无 `any`（除 generated 文件），使用 absolute imports
- **CSS**：Tailwind CSS utility classes，暗色主题为默认

## 重要注意事项

- `src/plugins/generated.ts` 是自动生成的，修改后会被覆盖
- `.plugins/` 目录是 gitignored 的，运行 `pnpm build` 或 `node scripts/resolve-plugins.mjs` 会自动生成
- Cargo workspace 在项目根目录，`target/` 也在根目录
- 协议版本 `PROTOCOL_VERSION`（在 `packages/driver-api/src/lib.rs`）变更时需同步更新所有插件
