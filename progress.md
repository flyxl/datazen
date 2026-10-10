# 进度台账 — 轨 `feature/cm74-handle-release`

基准 `feat/p2-p3-p4-completion` @ `043f1956c7`（已含轨 1「reset 三态」）。
工作树 `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-cm74-handle-release`。
`CARGO_TARGET_DIR=/tmp/cargo-cm74`。

## 目标（一句话）

按 CM-74（`docs/architecture/platform/connection-management.md:1323-1338`）的五条断言写测试，
修掉它们暴露的真缺陷；只动归池检查行为与 `connection/testing/**`，不碰
`PhysicalTransport::reset` 签名、不做能力分流、不动 `drivers/**`、`driver-api/**`、
`application/**`、`registry/**`、文档措辞与 CI。

## 状态

**进行中**（首条提交只含本台账；代码与门禁见下节，随进度更新）。

## 环境前提（本轨实测）

裸 worktree 已具备三个 gitignored codegen（`src-tauri/src/driver_init.rs`、
`src-tauri/capabilities/default.json`、`src-tauri/resources/builtin-ep/` 目录本体），
`cargo check -p datazen-runtime --all-targets` EXIT=0。

## 改动清单（文件:行 → 改了什么）

_待填。_

## 门禁（全部本轨实测，未采信任何他人数字）

_待填。_

## 失败证据（改动前）

_待填。_

## 取舍与待裁决项

_待填。_
