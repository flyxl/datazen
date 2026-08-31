# 独立驱动开发指南

本文介绍如何在独立 Git 仓库中开发 Datazen **数据库驱动插件（Driver Plugin）**，并使用本地 Datazen 源码作为完整的编译、运行和调试宿主。

最重要的一点是：**Datazen Plugin 在编译期被编译并链接进 Datazen**。插件不是通过运行时加载 `.so`、`.dylib` 或 `.dll` 的方式工作。因此，推荐使用两个同级 Git 仓库：

```text
workspace/
├── datazen/
└── datazen-driver-mydb/
```

插件仓库保持独立，Datazen 仓库负责将插件与应用一起编译，并提供真实的 Rust + 前端运行环境用于调试。

## 1. 前置条件

首先确保 Datazen 本身能够正常安装依赖并启动开发环境。

然后将插件仓库 clone 到 Datazen 仓库的同级目录：

```text
~/workspace/
├── datazen/
└── datazen-driver-mydb/
```

两个仓库不需要合并成一个 Git 仓库，插件也不需要放进 Datazen 的 `packages/` 目录。

## 2. 创建插件仓库

Driver Plugin 应该是独立的 Rust 工程。典型结构如下：

```text
datazen-driver-mydb/
├── Cargo.toml
├── Cargo.lock
├── src/
│   └── ...
├── ui/
│   └── ...
└── README.md
```

具体 Rust 和前端目录结构取决于插件提供的能力。插件可以只包含 Rust Driver，也可以同时提供数据库元数据、连接表单、连接视图、设置、Schema Tree、SQL Dialect 等前端能力。

插件应该依赖公开的 Datazen Driver API，而不是直接依赖 Datazen 应用内部实现。

## 3. 在 Datazen 中注册本地插件

本地开发时，在 Datazen 的 `drivers-registry.json` 中增加插件，并将 `source` 设置为 `path`。

例如：

```json
{
  "mydb": {
    "source": "path",
    "path": "../datazen-driver-mydb",
    "feature": "driver-mydb",
    "description": "MyDB driver"
  }
}
```

`path` 相对于 Datazen 仓库目录计算。

当前 registry 本身已经使用 `source: "path"` 表示本地/内置 Driver，同时也支持 `source: "git"` 表示独立托管的插件。可以参考 `drivers-registry.json` 中现有的 Driver 配置。

> **注意：开发阶段直接修改本地 `drivers-registry.json` 是正常的。** 这个修改只存在于你的本地 Datazen 工作区。只有你提交 commit、创建 Pull Request，并且 PR 被合并后，才会影响 GitHub 上共享的 `main` 分支。

### 可选：本地 override

Datazen 也支持 `.drivers-dev.json` 作为 gitignored 的本地 override，用于在不修改已提交的 `drivers-registry.json` 的情况下覆盖现有配置。例如：

```json
{
  "kiwi": {
    "source": "path",
    "path": "../datazen-driver-kiwi"
  }
}
```

对于新开发的插件，直接修改本地 `drivers-registry.json` 是最直观的方式，可以清楚地表达当前 Datazen checkout 要使用哪个插件源码。

## 4. 使用插件编译 Datazen

Datazen 通过 `--drivers`（或 `DATAZEN_DRIVERS`）选择 Driver。解析器读取 `drivers-registry.json`，解析指定的 Driver ID，然后生成构建配置，将选中的插件 dependency、feature 和前端注册信息注入 Datazen 构建。

运行你的插件：

```bash
cd ~/workspace/datazen
pnpm tauri:dev --drivers=mydb
```

也可以和内置 Driver 一起使用：

```bash
pnpm tauri:dev --drivers=basic,mydb
```

`basic` 会展开为四个核心 Driver：`postgres`、`mysql`、`sqlite` 和 `redis`。`all` 会展开为 registry 中所有 `source: "path"` 的 Driver。也可以直接使用逗号分隔的 Driver 列表。

例如：

```bash
pnpm tauri:dev --drivers=postgres,mysql,mydb
```

### `--drivers` 到底做什么？

`--drivers` 是**编译期 Driver 选择机制**，不是运行时动态插件加载机制。

大致流程：

```text
--drivers=mydb
        │
        ▼
drivers-registry.json
        │
        ▼
resolve-drivers.mjs
        │
        ├── Cargo dependency
        ├── Cargo feature
        └── generated frontend registry
        │
        ▼
Datazen build
        │
        ▼
包含插件的 Datazen binary
```

因此 Rust Plugin 会被编译并链接进 Datazen。运行时 Datazen 通过正常的 Driver 注册机制发现已经编译进 binary 的 Driver，而不是通过平台相关的动态库 ABI 加载插件。

## 5. 开发 Rust 部分

直接在独立插件仓库中修改代码：

```bash
cd ~/workspace/datazen-driver-mydb
```

然后从 Datazen 仓库重新启动/编译：

```bash
cd ~/workspace/datazen
pnpm tauri:dev --drivers=mydb
```

由于 registry 使用本地 `path` dependency，Cargo 会直接编译 `../datazen-driver-mydb` 当前工作区中的源码。不需要把插件代码复制到 `packages/`，也不需要把插件源码提交到 Datazen 仓库。

插件的 Rust 实现应该使用 Datazen Driver API，并通过 API 提供的注册机制注册 Driver，使编译后的插件能够参与 Datazen 的 Driver Registry。

## 6. 开发前端部分

插件也可以提供前端代码。选中的插件前端集成会随着 Datazen 前端一起构建。

因此前端开发同样应该使用本地 Datazen 作为宿主：

```text
plugin repository
      │
      ├── Rust implementation
      │
      └── frontend implementation
               │
               ▼
        Datazen frontend build
               │
               ▼
          Datazen application
```

这样做的好处是 Plugin UI 在真实的 Datazen 应用环境中运行，而不是在单独的 Mock Host 中运行，因此可以直接调试真实的 React/Tauri 上下文。

当前 Driver resolver 会根据选择的 Driver 生成 `src/plugins/generated.ts`（gitignore）。因此前端 Plugin 和 Rust Driver 一样，都属于同一个编译期 Driver selection。文件缺失时 `pnpm install` / `pnpm build` 会 `--codegen-only` 补齐。

新增前端功能时，应参考 registry 中已有的外部 Plugin，例如 Kiwi、OLAP 和 Superset 的结构和约定。

## 7. 日常开发循环

推荐的开发循环：

```text
1. 修改 Plugin 源码
       ↓
2. 使用 --drivers=mydb 启动/重新启动 Datazen
       ↓
3. Datazen 解析本地 path plugin
       ↓
4. Rust + frontend 被编译进 Datazen
       ↓
5. 在真实 Datazen 应用中调试 Plugin
       ↓
6. 重复
```

Rust 修改后，Cargo 会重新编译受影响的 Plugin 代码。前端修改则使用 Datazen 当前前端开发工具链进行调试。

## 8. 独立测试插件

Plugin 仓库应该维护自己的测试和 CI。至少应该尽可能独立测试 Driver 实现。**不要把驱动专属测试写进 Host**（`src-tauri/`、`src/`、`e2e/specs/`）。本仓 path 驱动同样：测试落在 `packages/drivers/<id>/`（`#[cfg(test)]`、`tests/`、`ui/__tests__/`、`e2e/`）。见 [AGENTS.md](../../AGENTS.md)「驱动测试落点」。

例如：

```bash
cd ~/workspace/datazen-driver-mydb
cargo test
```

如果 Plugin 包含前端代码，则运行该 Plugin 自己定义的测试、类型检查和构建命令。

这些测试验证 Plugin 本身；使用 `pnpm tauri:dev --drivers=mydb` 运行 Datazen，则用于验证 Plugin 与当前 Datazen 源码的集成。

## 9. 发布前验证

发布前建议验证两个层面。

### Datazen 集成测试

```bash
cd ~/workspace/datazen
pnpm tauri:dev --drivers=mydb
```

确认当前 Plugin 源码能够被编译进 Datazen，并在真实应用中正常运行。

### Plugin 独立测试

```bash
cd ~/workspace/datazen-driver-mydb
cargo test
```

如果包含前端，也运行 Plugin 自己的前端检查。

## 10. 从本地 path 切换到 Git dependency

插件发布后，Datazen 可以直接从独立 Git 仓库获取插件。

开发阶段：

```json
{
  "mydb": {
    "source": "path",
    "path": "../datazen-driver-mydb",
    "feature": "driver-mydb"
  }
}
```

For a committed Datazen registry entry, the plugin can instead be pinned to a Git revision:

```json
{
  "mydb": {
    "source": "git",
    "git": "https://github.com/example/datazen-driver-mydb.git",
    "ref": "<commit-sha>",
    "feature": "driver-mydb"
  }
}
```

固定 commit 可以保证 Datazen 构建可复现，避免插件仓库内容变化导致 Datazen 构建结果发生不可控变化。

整个过程：

```text
本地开发
source = path
path = ../datazen-driver-mydb
        │
        ▼
验证 Datazen 集成
        │
        ▼
发布 Plugin 仓库
        │
        ▼
提交 Datazen Registry PR
        │
        ▼
source = git
ref = <pinned commit>
```

## 11. 提交 Datazen Registry 修改

Plugin 仓库和 Datazen 仓库保持独立。

Plugin 准备发布时：

1. Push Plugin 仓库，并发布需要使用的 revision。
2. 在 Datazen 仓库创建开发分支。
3. 将 registry 中本地 `path` 配置修改为 Plugin Git 仓库和固定 revision。
4. 使用选中的 Plugin 运行 Datazen 构建和测试。
5. 创建 Datazen Pull Request。
6. Datazen 仓库 Owner review 并 merge。

本地开发时修改 `drivers-registry.json` **不会自动影响 GitHub `main`**。只有 PR 被 merge 后，共享 registry 才会发生变化。

## 12. 推荐的 workspace 结构

完整的独立 Plugin 开发 workspace：

```text
~/workspace/
├── datazen/
│   ├── drivers-registry.json
│   ├── scripts/
│   ├── src/
│   ├── src-tauri/
│   └── ...
│
└── datazen-driver-mydb/
    ├── Cargo.toml
    ├── Cargo.lock
    ├── src/
    ├── ui/
    └── ...
```

两个仓库在 Git 层面完全独立，但通过同级目录关系，Datazen 可以使用现有的 `source: "path"` 和 `--drivers` 机制把 Plugin 编译进应用。

## 13. 总结

推荐的独立 Plugin 开发模式：

- Plugin 保持在**独立 Git 仓库**。
- Plugin checkout 与 Datazen checkout **放在同级目录**。
- 开发阶段在 Datazen 的 `drivers-registry.json` 中使用 `source: "path"`。
- 使用 `pnpm tauri:dev --drivers=<plugin-id>` 启动 Datazen。
- 由 Datazen 在**编译期**把 Plugin 编译进应用。
- 使用真实 Datazen 应用同时调试 Rust 集成和前端 UI。
- Plugin 自己维护测试和 CI。
- 发布时将 Datazen registry 切换为固定 commit 的 `source: "git"`，通过 Pull Request 提交。

这种模式既保持了 Plugin 源码的独立性，又保留了 Datazen 的编译期集成模型，避免 Rust runtime dynamic-library loading 带来的 ABI 和版本兼容问题。
