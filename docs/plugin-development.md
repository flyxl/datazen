# 插件开发指南

本文档介绍如何为 DataZen 开发一个新的数据库驱动插件。

## 概述

DataZen 采用**编译时插件系统**（类似 Caddy 2），插件在独立仓库中开发，构建时按需组合进最终二进制。每个插件包含：

- **Rust 后端**：实现 `DatabaseDriver` trait 的 crate
- **前端 UI**（可选）：自定义连接表单、元数据等 TypeScript/React 组件

### 命名约定

插件涉及两套命名，请勿混淆：

| 用途 | 格式 | 示例 |
|------|------|------|
| **Git 仓库** | `datazen-driver-xxx` | `datazen-driver-kiwi` |
| **Rust crate** | `datazen-plugin-xxx` | `datazen-plugin-kiwi` |

- `plugins-registry.json` 中的 `git` 字段使用 **Git 仓库名**（`datazen-driver-xxx`）
- 插件 `Cargo.toml` 的 `[package].name` 以及主项目 `src-tauri/Cargo.toml` 中的依赖名使用 **Rust crate 名**（`datazen-plugin-xxx`）

## 快速开始

### 1. 创建插件仓库

```bash
mkdir datazen-driver-mydb && cd datazen-driver-mydb
cargo init --lib
```

### 2. 添加 driver-api 依赖

```toml
# Cargo.toml
[package]
name = "datazen-plugin-mydb"
version = "0.1.0"
edition = "2021"

[dependencies]
datazen-driver-api = { git = "https://github.com/flyxl/datazen-driver-api.git" }
async-trait = "0.1"
```

### 3. 实现驱动

```rust
// src/lib.rs
use std::sync::Arc;
use datazen_driver_api::*;

pub struct MyDbDriver;

impl MyDbDriver {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl DatabaseDriver for MyDbDriver {
    fn driver_type(&self) -> DatabaseType {
        DatabaseType::Custom("mydb".to_string())
    }

    fn driver_category(&self) -> DriverCategory {
        DriverCategory::Sql
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<(), DriverError> {
        // 实现连接逻辑
        todo!()
    }

    async fn disconnect(&self, config: &ConnectionConfig) -> Result<(), DriverError> {
        todo!()
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<String, DriverError> {
        // 返回连接成功的描述信息
        todo!()
    }

    async fn query(&self, config: &ConnectionConfig, sql: &str) -> Result<QueryResult, DriverError> {
        todo!()
    }

    async fn get_databases(&self, config: &ConnectionConfig) -> Result<Vec<String>, DriverError> {
        todo!()
    }

    async fn get_tables(&self, config: &ConnectionConfig, database: Option<&str>) -> Result<Vec<TableInfo>, DriverError> {
        todo!()
    }

    // ... 实现其他必要方法，可参考内置驱动
}

// 注册驱动工厂
struct MyDbDriverFactory;

impl DatabaseDriverFactory for MyDbDriverFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(MyDbDriver::new())
    }

    fn driver_id(&self) -> &'static str {
        "mydb"
    }
}

datazen_driver_api::register_driver!(&MyDbDriverFactory);
```

### 4. 添加前端 UI（可选）

如果你的驱动需要自定义连接表单，在仓库中创建 `ui/` 目录：

```
datazen-driver-mydb/          # Git 仓库目录名
├── Cargo.toml                # [package].name = "datazen-plugin-mydb"
├── package.json              # 声明 @datazen/plugin-sdk 依赖
├── src/
│   └── lib.rs
└── ui/
    ├── plugin-meta.ts          # 数据库类型元数据
    └── MyDbConnectionFields.tsx  # 自定义连接表单（可选）
```

**`package.json`**（用于前端类型提示）：

```json
{
  "name": "datazen-plugin-mydb",
  "private": true,
  "devDependencies": {
    "@datazen/plugin-sdk": "github:flyxl/datazen-plugin-sdk"
  }
}
```

**`ui/plugin-meta.ts`**（必须）：

```typescript
import type { DatabaseTypeMeta } from '@datazen/plugin-sdk';

export const mydbMeta: DatabaseTypeMeta = {
  label: 'MyDB',
  shortLabel: 'Md',
  iconBg: 'bg-cyan-600',
  iconColor: 'text-cyan-400',
  defaultPort: 3000,
  defaultHost: '127.0.0.1',
  defaultUser: 'admin',
  quoteChar: '"',
  connectionMode: 'server',
  supportsSSH: true,
  supportsSSL: false,
  supportsBackup: false,
  supportsTables: true,
  isKeyValue: false,
  supportsSQL: true,
  category: 'sql',
  connectionView: 'sql',
  sqlDialect: 'postgresql',  // 复用已有方言，或提供自定义
  databaseFieldType: 'name',
  connectionForm: 'standard', // 使用通用表单；自定义则填写自定义名称
};
```

> **注意**：插件通过 `@datazen/plugin-sdk` 导入组件和类型。构建时，Vite 会将此包解析为主应用中的实际实现。安装 SDK（`pnpm add @datazen/plugin-sdk@github:flyxl/datazen-plugin-sdk`）即可获得完整类型提示。

### 5. 注册到主项目

在主项目的 `plugins-registry.json` 添加：

```json
{
  "mydb": {
    "source": "git",
    "git": "https://github.com/yourname/datazen-driver-mydb.git",
    "feature": "plugin-mydb",
    "description": "MyDB driver"
  }
}
```

在 `src-tauri/Cargo.toml` 添加可选依赖和 feature（依赖名用 crate 名，Git URL 用仓库名）：

```toml
[dependencies]
datazen-plugin-mydb = { git = "https://github.com/yourname/datazen-driver-mydb.git", optional = true }

[features]
plugin-mydb = ["dep:datazen-plugin-mydb"]
```

在 `scripts/resolve-plugins.mjs` 的 `FRONTEND_PLUGIN_CONFIG` 中添加前端配置。

### 6. 构建验证

```bash
pnpm tauri:build --plugins=mydb
```

---

## 本地开发工作流

### 使用 .drivers-dev.json

在项目根目录创建 `.drivers-dev.json`（已被 gitignore）：

```json
{
  "kiwi": {
    "source": "local",
    "path": "../datazen-driver-kiwi"
  }
}
```

这会将 git clone 替换为指向本地目录的 symlink（`packages/drivers/<id>`），方便开发时实时修改和测试。该目录不进 Host git，也不加入 Cargo workspace members。

### Rust 本地路径覆盖

在 `src-tauri/Cargo.toml` 底部添加（开发时使用，勿提交）：

```toml
[patch."https://github.com/flyxl/datazen-driver-kiwi.git"]
datazen-plugin-kiwi = { path = "../datazen-driver-kiwi" }
```

或使用 `.cargo/config.toml`：

```toml
[patch."https://github.com/flyxl/datazen-driver-kiwi.git"]
datazen-plugin-kiwi = { path = "../datazen-driver-kiwi" }
```

### 热重载开发

```bash
# 终端 1：启动前端（自动 resolve plugins）
pnpm dev

# 终端 2：启动 Rust 后端
pnpm tauri dev
```

修改前端插件 UI 代码后 Vite 会自动热重载。修改 Rust 代码后 Tauri CLI 会自动重编译。

---

## 协议版本

`datazen-driver-api` 定义了 `PROTOCOL_VERSION` 常量（当前为 `2`）。

- 每个插件工厂通过 `protocol_version()` 方法声明其编译时的协议版本
- 主应用在注册插件时验证版本匹配，不匹配时拒绝加载并记录错误日志
- 协议版本在 `DatabaseDriver` / `KeyValueDriver` trait 发生 **breaking change** 时递增

**规则**：
- 新增 trait 方法（有默认实现）→ 不需要 bump
- 修改已有方法签名、删除方法、修改类型定义 → 必须 bump

---

## 目录结构约定

```
datazen-driver-xxx/         # Git 仓库目录名
├── Cargo.toml              # [package].name = "datazen-plugin-xxx"
├── src/
│   └── lib.rs              # 驱动实现 + register_driver! 宏调用
├── ui/                     # 前端组件（可选）
│   ├── plugin-meta.ts      # DatabaseTypeMeta（必须，如果有 ui/）
│   ├── *Fields.tsx         # 自定义连接表单（可选）
│   └── *Dialect.ts         # 自定义 SQL 方言（可选）
└── README.md
```

## 已有插件参考

| 插件 | 仓库 | 功能 |
|------|------|------|
| Kiwi | [datazen-driver-kiwi](https://github.com/flyxl/datazen-driver-kiwi) | 自定义连接表单 + SSO 登录 |
| OLAP | [datazen-driver-olap](https://github.com/flyxl/datazen-driver-olap) | Presto/Trino, Catalog 表单 + 自定义 SQL 方言 |
| Superset | [datazen-driver-superset](https://github.com/flyxl/datazen-driver-superset) | Superset 数据探索平台 |
