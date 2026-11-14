# TN D1 同步端点选择 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`b45490f` `feat(rune): a tools page and the sync endpoint step (issue-rune D1)`
- `git show --stat b45490f` 两个文件：`crates/datazen-ui-api/src/lib.rs`（种子连接加 Read Replica（其会话不含 analytics 表）、`sync_endpoints` 端口 + 1 条单测）、`crates/datazen-ui/src/lib.rs`（侧栏 Tools 节 + 工具切换器（Sync/Compare/Transfer）、同步向导第 1 步（source/target 芯片循环 + Next 校验）+ 1 条运行时测试）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui-api` 35 passed（净增 1：`sync_endpoints_refuse_the_same_connection_twice`）；`datazen-ui` 84 passed（净增 1：`the_sync_wizard_refuses_the_same_endpoint_twice`），4.63s |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「同步：两端点选择。测：两端相同被拒绝」核对，断言来自 GR1 实跑：

1. **两端相同被拒绝**（api + 运行时）：`sync_endpoints("local","local")` Err；未知端点 Err；local→replica Ok。UI：target 芯片切到 local 后点 Next → 停在第 1 步且 `sync-error` 可见；切回 replica → Next → 进入第 2 步。通过
2. **replica 语义**（api 单测）：connect("replica") 的会话含 customers/orders、不含 catalog（analytics 不在副本上）——供 D6 结构对比制造差异

既有测试全部复跑通过（S6 侧栏测试只断言三个旧键存在、四个旧键不存在，新增 `nav-tools` 不冲突；连接列表加一行不影响按 id 查找的测试）。

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。Tools 页落地；同步向导第 1 步经端口校验端点。

遗留（不构成拒绝）：Compare/Transfer 面板为占位空容器（D6/D9 填充）；端点只有连接级（无库/schema 粒度）。
