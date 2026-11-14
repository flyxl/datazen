# TN P4 连接表单 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`35cbd1f` `feat(rune): a connection form that saves through the port (issue-rune P4)`
- `git show --stat 35cbd1f` 两个文件：`crates/datazen-ui-api/src/lib.rs`（`save_connection` 端口：名字 slug 生成 id、重名加计数、缺地址拒绝 + 1 条单测）、`crates/datazen-ui/src/lib.rs`（连接页表单三输入 + Save、`connection-form-error` 拒绝行 + 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 30 passed（净增 1：`a_connection_without_an_address_cannot_be_saved`）；`datazen-ui` 78 passed（净增 1：`a_connection_form_save_needs_an_address`），4.35s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「连接表单：名称、引擎、地址。只进端口，不进系统钥匙串。测：缺地址不能保存」核对，断言来自 GR1 实跑：

1. **缺地址不能保存**（api + 运行时）：空地址/纯空白地址 → Err、连接数不变、界面出现 `connection-form-error`；填齐名称/引擎/地址后保存 → `connection-remote-postgres` 进入列表、错误行消失、字段清空。通过
2. **只进端口**（代码核对）：`save_connection` 仅写 `self.connections`（ConnectionProfile 无凭据字段），无任何钥匙串/文件访问；地址作为必填项进端口，密码类字段未建模（H 系列宿主接钥匙串时补）

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。三字段表单、缺地址拒绝、只进端口；slug id 与重名计数。

遗留（不构成拒绝）：无地址校验（格式/连通性）；无编辑与删除已有连接；凭据字段与钥匙串随 H 系列。
