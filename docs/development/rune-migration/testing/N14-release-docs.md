# TN N14 release 构建与文档 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`cda8ca3 chore(rune): drop a helper the release profile proves dead (issue-rune N14)`（代码清理）与 N14 的 docs 提交（文档与台账）
- 改动面：`crates/datazen-rune-host/src/dashboards.rs`（移除 release 剖面下未用的 `store_error`）、`docs/development/rune-migration/native-host.md`（新文档：用法/配置目录/驱动矩阵/剩余差距）、`README.md` 与 `README.zh-CN.md`（Build from source 各加一段指针）。

## Release 构建

- 命令：`cargo build --release -p datazen-rune-host`（workspace release 剖面：`strip` + `lto` + `codegen-units=1` + `opt-level="z"`）
- 产物：`target/release/datazen-native`，**36,076,312 字节（约 36 MB）**，ELF 64-bit x86_64 PIE（stripped），sha256 前缀 `cf8cb0e5a07b544a`。产物在最终代码（含清理）之后构建。
- 构建教训（如实记）：**2 并行在 LTO 链接段失败**（"build failed, waiting for other jobs"，2 核/4G 环境两次复现），`CARGO_BUILD_JOBS=1` 单线程一次通过（末段 4m24s）。CI 与本地宽裕机器不受影响；受限环境记此配方。
- release 剖面首启暴露一批**既有** dead-code 警告（`session()`/`history` 字段等测试专用访问器在非测试构建下未读，N4–N12 代码）与 workspace 既有的 manifest `[lints]` 警告——非本任务引入，如实记录不扩散处理。

## 文档

`native-host.md` 写明：构建与运行（含 Linux `libxkbcommon` 依赖、无显示环境行为）、配置目录八项文件清单（各自的接线任务号）、驱动矩阵（**sqlite 全功能**，其余 15 个驱动"未链接，同一接缝"——`host_registry()` 工厂清单加一行）、已接功能面与剩余差距五类（宿主能力/用例域/工作流引擎/WAPPS+Onboarding 永久 React/远程驱动作业化）。README（EN/zh）各加一段指针。

## 测试

`store_error` 移除后全量回归：

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（124 + 55 passed） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| GR3 | `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1` | 通过，exit 0（33 passed） |
| GR4 | `cargo check -p datazen-rune-host --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论

**放行，第二阶段（N1–N14）收尾。** release 产物存在且与最终代码一致；文档如实写明用法、配置、矩阵与差距。纯原生二进制具备：文件存储连接、sqlite 全功能（目录/读/写/查/结构/导出/备份/同步/对比/传输）、设置/历史/工作流/仪表盘持久化。后续扩展点已在文档与 plan-native.md 范围外清单立账。
