# p3-cm74-release-paths（Coder，label H「部署无关的 runtime 契约层」）

> 交付即销毁：本文件随分支流转，合并时必须删除，不得存活在 `main`。

## 状态

| 缺口 | 状态 | 落点 |
| --- | --- | --- |
| Gap A — 替换路径缺运行时实现 | ✅ 完成并证伪（6 次变异） | `src/registry/{context.rs, candidate.rs, actor/context.rs}` + `tests/registry_context.rs`（6 用例） |
| Gap B — 归池路径无端到端断言 | ✅ 完成并证伪（2 次变异） | `tests/registry_release.rs` 新增 2 用例（11 → 13） |
| §3 顺序冲突裁定 | ✅ 基线已解决 | `connection/testing/harness/cm74_release_order.rs` + `fake_resource/ops.rs:547-591` |

## 门禁实测（逐字结论行）

| 门禁 | EXIT | 结论 |
| --- | --- | --- |
| `cargo fmt -p datazen-runtime -- --check` | 0 | 无 diff |
| `cargo test -p datazen-runtime --lib` | 0 | `running 436 tests` / `436 passed; 0 failed`（基线 436，未回退） |
| `cargo test -p datazen-runtime` | 0 | 24 个测试目标，`TOTAL_passed=692 TOTAL_failed=0` |
| `cargo test -p datazen-runtime --test registry_context` | 0 | `running 6 tests` / `6 passed; 0 failed` |
| `cargo test -p datazen-runtime --test registry_release` | 0 | `running 13 tests` / `13 passed; 0 failed` |

未触碰 TypeScript，故未跑 `pnpm typecheck`。

每次门禁首尾各记一次 `git rev-parse HEAD` 与 `git status --porcelain`，运行期间 HEAD 恒为
`8db760bfc4b9e905e83bb52051c1da725b70e312`，工作区状态逐字未变。

## 变异记录（全部 red 后按 `cmp` 校验恢复）

| 编号 | 变异点 | 结果 |
| --- | --- | --- |
| MA1 | `actor/release.rs:61` `drain_handles()` → 非破坏性 `to_vec()` | T1 red：`left: 2 / right: 0` |
| MA2 | `context.rs` 删掉 `publish_candidate` | T1/T5/T6 red：`left: [] / right: [DbSessionId("dbs_2")]` |
| MA3 | 令牌改签在旧句柄上 | 4 例 red：`CloseRejected("attachmentRejected")` |
| MA4 | 提交失败分支删掉 `abort_candidate` + `resume` | T4 red：`CloseRejected("replacementInProgress")` |
| MA5 | 幂等短路挪到定位旧会话**之后** | T5 red：`UnknownSession("dbs_1")` |
| MA6 | `actor/context.rs::gate` 改成直接放行 | T4 red：`left: ["UNEXPECTED_OK"]` |
| MB1 | `evict_idle_at` 在 `Err` 时也 `forget` | **3 例 red**，其中 2 例是既有测试（见缺陷 4） |
| MB2 | `evict_idle` 丢掉 §9.4 宿主检查结果、信任 driver Clean | 归池用例 red，对照组仍绿 |

## 待裁定 / 遗留缺陷

1. **§9.4 宿主检查的 (b) 半是不可达死代码**：`drain_handles` 是 `mem::take`，
   单次 `handle_exec` 内无交错，`!ready_to_return_to_pool(...)` 永远为假。
   要么删掉该子句，要么给它一个可达形式——不要写假装它会触发的测试。
2. **`evict_idle_at` 吞掉 `Err` 留下僵尸行**：物理资源已关，行仍在表，额度未归还。
   用例 `归池时driver报Clean但宿主仍有登记句柄不得算归池成功` **刻画**了现状而非修复。
3. **修复它不能是二分法**（MB1 实证）：`Ok(None)`（未到期/无期限）、`Err(CloseRejected)`
   （派发前拒绝）、`Err(SessionLost)`（§9.4 跑完后不可判定）三者后果不同，
   「不是 `Ok(Some)` 就 forget」会把前两类也驱逐掉，打断两个既有用例。
4. **§7.4-6 的 attachmentToken 原本不可签发**：`Prepared` 不写摘要、`Committed` 不返令牌、
   `publish()` 清期限、`open_session` 是唯一点发点。已**新增**（非伪造）
   `InMemorySessionDirectory::issue_attachment_token`。
5. **`packages/runtime/src/directory/**` 是共享面**（CM-68 已落地，不在本轨 do-not-touch 清单内）。
   本轨只做了**加法**（一个 `pub fn`），但仍需集成方知会。
6. 目录里被替换的旧条目**留在表里但不可路由**（§12 要留下替换关系）。
   「旧会话没了」只能按 `is_routable` 判，按 `!contains` 判是恒真、测不出东西。

## 设计取舍

- `ReplacementDirectory` 是 runtime 本地端口（五个方法），platform-api 一个字没动：
  它的 `SessionDirectory` 不暴露 `commit_status` / `attach_from_client` / `issue_attachment_token`，
  而 platform-api 正被多条在飞轨道共用。
- 幂等走 §12 的 `ReplacementOperationKey`，不碰 `gateway/idempotency.rs`（CM-70 在飞）。
- 重试时**重签**令牌而非复用旧串：目录只留摘要，旧串早已不在任何人手里；
  承重的是「同一个新会话句柄 + 不重建候选」，重签换掉摘要、旧令牌立刻作废，是安全的一侧。