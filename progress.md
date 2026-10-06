# P5 修复轨 `p5-endpoint-overlap` 进度台账

> 分支 `feature/p5-endpoint-overlap`，基线 `codex/p5-integration @ 4b782750ddd1c51f6f75f8275f63dccf4902e8b0`。
> 本文件是交付前临时台账，**验收合并时必须删除**。

## 缺陷

Data Transfer Job 路径相对 legacy 是**功能回退**：源 `public.users` → 目标 `public.users`
跨两个不同物理库时，会被判定为「同一端点既读又写」而拒绝，同名跨库复制在 Job 路径上不可用。

根因两处：

1. `packages/runtime/src/job/budget.rs` `detect_endpoint_overlap` 只按 `(service_key, object)` 建键。
2. `job_api/runtime.rs` 的 `TRANSFER_SERVICE_KEY` 常量对 reader / writer **取同一个值**，
   且 `ConnectionId` 硬编码为 `local-source` / `local-target`。

`EndpointRef` 早就带 `connection_id`，但检测器从没用过它。

## 已完成

| 步骤 | 提交 | 内容 |
| --- | --- | --- |
| 1 | `653f2347c` | `detect_endpoint_overlap` 改用「service_key + connectionId」**身份键并集** |
| 2 | `24f5a8955` | Job 端点身份改由**真实连接配置**导出（`job_api/endpoint_identity.rs`） |
| 3 | `b00a1b0c2` | 端点身份用例拆为独立测试二进制 + 消除格式偏差（单文件规模纪律） |

## 已实现的事实（随代码与测试落库，台账删除后仍可读出）

- 端点身份来自 `ConnectionConfig`：`connection_id = config.id`（持久化连接配置 id），
  `service_key = "data-transfer:" + sha256(物理位置摘要)`。**不是**拆成两个固定字符串。
- 物理位置摘要字段集与 `datazen_schema_diff::reviewed::same_endpoint` 对齐
  （driver/host/port/database/schema/options/tunnel，剔除 `user`），`id` 刻意不参与摘要。
- 两个端点「共享任意一个身份键」即按同一物理服务处理（`Service` ∪ `Connection`）。
- `detect_endpoint_overlap` 签名未变（`&[EndpointRef] -> Result<(), JobError>`），无调用方需要改。
- §6.2 一次性全有或全无预算预留保持不变：`ensure_service` 幂等、本地端点在首个 Job 前注册、
  每个端点按物理端点独立预留、整体全有或全无。
- `sql_file == true` 的目标侧没有写端点（`target_identity: Option::None`），该路径行为不变。
- `any_unprovable` 判定保持 `keys.is_empty()`：把「没有 Service 键」当作不可证明会比 legacy 更严，
  违反主规则。代价是该分支从宿主路径不可达（持久化 `connectionId` 不会为空），
  作为纵深防御保留。

## 门禁（HEAD `b00a1b0c245c7c58a4587a6609f59d3bdbb3cf3d`）

起止 `HEAD` 一致，起止 `git status --porcelain` 均为空。驱动集 `--drivers=all`
（`shasum -a256 drivers-registry.json` = `8531e125fa9bc0c9fe4249c6403b479b1aae61fdd7932c5eaadb0971b96f7cb8`，
`cargo metadata --no-deps` 计得 **17** 个 `datazen-driver-*` 包）。

| 门禁 | 退出码 | 结论行 |
| --- | --- | --- |
| `cargo test -p datazen-runtime` | 0 | 847 passed / 0 failed |
| `cargo test -p datazen-data-transfer` | 0 | 213 passed / 0 failed |
| `cargo test -p datazen-data-sync` | 0 | 176 passed / 0 failed |
| `cargo test -p datazen --lib` | 0 | `test result: ok. 1757 passed; 0 failed; 6 ignored; ...` |
| `cargo check -p datazen` | 0 | `error_lines=0`，`warning_lines=44` |
| `pnpm typecheck` | 0 | `error TS` 计数 0 |

## 变异验证（对照变异）

全部在**交付 HEAD `b00a1b0c2`** 上复跑，每个变异一个全新空 `CARGO_TARGET_DIR`，用后即删，
日志均显示从零 `Compiling`。对照组在**修复前** `4b782750d` 上跑，结果为绿（如实记录，未修改基线树）。

| 变异 | 改动 | 探针 | 结果 |
| --- | --- | --- | --- |
| A | `identify` 回退为常量 `service_key` | `cargo test -p datazen --lib` | KILLED，EXIT=101，`1745 passed; 12 failed` |
| B-host | `identify` 硬编码 `ConnectionId::new("local-source")` | `cargo test -p datazen --lib` | KILLED，EXIT=101，`1748 passed; 9 failed` |
| B-runtime | `identity_keys` 丢弃 `Connection(...)` 键 | `cargo test -p datazen-runtime` | KILLED，EXIT=101，`job_endpoint_identity.rs:83` |
| C | 检测器 `.find` 丢掉 `read_key == key` 维度 | `cargo test -p datazen-runtime` | KILLED，EXIT=101，`job_endpoint_identity.rs:31` |
| 对照 | 无改动（缺陷树 `4b782750d`） | 两个探针 | SURVIVED，EXIT=0（runtime 843 / host lib 1733 全绿） |

## 遗留与待裁定

- **已加强 legacy 的窄面**：两个连接配置指向同一台物理服务器时，现在会判为同一端点并拒绝。
  这是 req 3 明确要求的安全性质（灾难性自覆盖），legacy 不具备。如裁定认为必须严格不严于 legacy，
  请指出要退回哪一半。
- **已知漏检**：同一台服务器的两个配置，其中一个省略 `host`（走驱动默认值）、另一个显式填写，
  摘要与连接 id 均不同 ⇒ 漏检。同 `connectionId` 的情形由 `Connection` 键兜住；
  `assembly.rs` 的 `validate_no_self_table_overwrite` 是第二层。
- **`schema_diff` Job 路径未动**：`commands/schema_diff/job.rs` 的 `endpoints_from_session_pair`
  同样使用 `EndpointRef`，但它给两端不同的 connection id，因此行为未变。本次**超范围，未改**。
