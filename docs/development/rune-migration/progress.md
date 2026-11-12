# Rune Native UI Migration Progress

## Overall Status
开发初期。Rune 骨架已在本机通过 `cargo check -p datazen-ui`。下一步是 SQL Editor POC，而不是大规模迁移现有 UI。

## Current Task
- **任务**: 检查现有 Rune crate 骨架，完成 Cargo 编译验证
- **负责人**: Coder Agent
- **状态**: 编译验证通过（见下方命令）。Tester 尚未独立复验。
- **开始时间**: 2026-11-11

## Completed Work
- [x] 创建独立开发分支
- [x] 编写并提交迁移方案
- [x] 创建 Rune 初始 crate 骨架
- [x] 扩展 shared UI shell（Connections / SQL Editor / Data Browser 等导航）
- [x] Vendor Rune 到 `vendor/rune`（submodule `2e4999f`）
- [x] 补齐 `[workspace.package]`，并把 edition 定为 2024，否则 vendored Rune 的 `edition.workspace = true` 无法解析，且 let chains 在 2021 下编译失败
- [x] 本地 `cargo check -p datazen-ui` 通过（完整 feature 集）
- [ ] SQL Editor POC
- [ ] DataGrid POC

## Verification
环境：Linux，Rust 1.99.0，约 4GB RAM、无 swap。`vendor/rune` @ `2e4999f67064dd351e504bbdaaf71f0b601e2114`。

在默认并行下，`rune` crate（约 19 万行）会在 rustc 阶段被 OOM（SIGKILL）。以下设置可通过：

```
CARGO_BUILD_JOBS=1 \
CARGO_INCREMENTAL=0 \
CARGO_PROFILE_DEV_DEBUG=0 \
RAYON_NUM_THREADS=1 \
RUSTFLAGS="-C debuginfo=0" \
cargo check -p datazen-ui
```

结果：`Finished dev profile ... datazen-ui`，`rune` 仅有 6 条既有 warning（私有类型可见性 / dead_code），无 error。

同一命令在只开 `embedded-fonts` 时也通过。产品 feature 集保持为 `accessibility`, `embedded-fonts`, `metal-backend`, `svg`, `webgl`，无需为了编译而裁剪。

`Cargo.lock` 已纳入 Rune 依赖。为与 Rune 对齐，若干传递依赖做了兼容范围内升级或并存（例如 icu 2.2 → 2.4、objc2 增加 0.6.5、zbus 增加 5.19/6.2）。DataZen 自己的 crate 仍显式 `edition = "2021"`，不受 workspace edition 2024 影响。

## Next Steps
1. Tester 用上面的命令独立复跑 `cargo check -p datazen-ui`。
2. 开始 SQL Editor POC（多光标、中文 IME、语法高亮、长行水平滚动）。
3. 在更高内存机器上补一次默认并行的 `cargo check`，确认不依赖 `RAYON_NUM_THREADS=1`。

## Coder / Tester
Coder 已完成实现侧编译验证。Tester 负责独立复验，未通过前不标记迁移任务完成。
