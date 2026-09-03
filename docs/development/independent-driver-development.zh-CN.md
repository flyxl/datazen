# 独立驱动开发指南

本文介绍如何在独立 Git 仓库中开发 DataZen 数据库 Driver，并使用本地 DataZen 源码作为编译、运行和调试宿主。

DataZen Driver 在**编译期**集成，不通过运行时 Rust 动态库 ABI 加载。推荐：

```text
workspace/
├── datazen/
└── datazen-driver-mydb/
```

## 1. 前置条件

确保 DataZen 本身能够安装依赖并启动开发环境，然后把 Driver 仓库放在 DataZen 同级目录。

Driver 应依赖公开的 `datazen-driver-api`，不要依赖 DataZen 应用内部模块。

## 2. 本地注册

开发阶段可以在 `drivers-registry.json` 中使用：

```json
{
  "mydb": {
    "source": "path",
    "path": "../datazen-driver-mydb",
    "feature": "driver-mydb"
  }
}
```

也可以使用 gitignored 的 `.drivers-dev.json` 覆盖本地配置。

本地 registry 修改不会影响 GitHub `main`；只有提交并合并后才会成为共享配置。

## 3. 编译与运行

Driver 选择发生在构建期：

```text
--drivers=mydb
    ↓
drivers-registry.json
    ↓
resolve-drivers.mjs
    ↓
Cargo dependency + feature + generated frontend registry
    ↓
DataZen binary
```

运行：

```bash
pnpm tauri:dev --drivers=mydb
```

也可以：

```bash
pnpm tauri:dev --drivers=postgres,mysql,mydb
```

`basic`、`all` 等选择方式以当前 `drivers-registry.json` 和 resolver 实现为准。

## 4. Driver API

`packages/driver-api` 是公共 API source of truth，并发布为 MIT licensed `datazen-driver-api` crate。

Driver 可以实现：

- `DatabaseDriver`
- Driver Commands
- Schema migration renderer / capabilities
- Type normalizer
- Sync source/target adapter

Driver API 的公共接口不能暴露 sqlx、mongodb、redis 等实现类型。详见 [Driver API Dependency Boundary](driver-api-dependency-boundary.md)。

## 5. 前端 Driver

如果 Driver 提供 UI，可以把前端代码随 Driver 一起编译进 DataZen。resolver 会生成 gitignored 的 `src/plugins/generated.ts`。

插件 UI 应使用 DataZen 已定义的 Driver/Plugin contract，不直接依赖 Host 内部实现。

## 6. 测试

Driver 专属测试应放在 Driver 仓库；本仓内置 Driver 的测试放在 `packages/drivers/<id>/`，包括 Rust tests、frontend tests 和 driver-specific E2E。

Host 的 E2E 只验证 Host 与 Driver contract，不在 `src-tauri/` 或 `src/` 重复实现 Driver 方言测试。

## 7. 发布

发布后，DataZen registry 可以从：

```json
{
  "source": "path",
  "path": "../datazen-driver-mydb"
}
```

切换为固定 commit 的 Git dependency：

```json
{
  "source": "git",
  "git": "https://github.com/example/datazen-driver-mydb.git",
  "ref": "<commit-sha>"
}
```

固定 revision 可以保证构建可复现。

## 8. 开发原则

- Driver 保持独立 Git repository。
- Host 与 Driver 通过 `datazen-driver-api` 解耦。
- 方言 SQL、类型转换和 Driver-specific behavior 放在 Driver。
- Host 通过 capability / command / adapter contract 使用 Driver。
- 不把具体 Driver 的实现细节复制到 DataZen Host。
