# P3-FU 隧道轨台账（CM-32-FU1）

> 分支 `feature/p3fu-tunnel`。本文件是开发期台账，**验收合并时必须删除**，不得存活到 `main`。

## 一句话

「隧道引用计数只有一本账」从审计承诺变成**机械保证**：R1–R7 七条文本闸门 + 每条闸门的
kill test（植入变异当字符串喂同一套扫描器），并在原始反例上做过实证复核。

## commit 链

| commit | 内容 |
| --- | --- |
| `506f42411` | 闭合 CM-32-FU1 —— 单计数器铁律从审计承诺变成机械保证（9 文件 +2046/−110，新增 `single_counter_audit.rs`、`source_scan.rs`） |
| `68b92c99d` | 补 R7 / R2+，关掉「换名字挂镜像账」；闸门拆目录守单文件纪律（7 文件 +693/−413，`single_counter_audit.rs` 拆出 `kill_tests.rs`） |
| `5ff668be4` | 台账注释钉住「建账 vs 记账」的分界，把 R1/R2/R7 按方法名对齐（纯注释） |
| `85e5c55c1` | R6 按职责拆出 `gate_self_audit.rs`（`mod.rs` 884 → 675 行） |

## 门禁实测（本轮重跑）

首 `HEAD=68b92c99d` / 尾 `HEAD=85e5c55c1`；运行期间工作区 sha 首 `e90b53129648`、尾 `da39a3ee5e6b`
（`da39a3ee5e6b` 是空输出的 sha，即**工作区干净**——拆分期间确实只有我在写这棵树）。

```
cargo fmt -- --check                    EXIT=0
cargo test -p datazen-runtime --lib      EXIT=0   test result: ok. 461 passed; 0 failed
cargo test -p datazen-runtime \
  --test tunnel_refcount_contract \
  --test cm28_concurrent_tunnel \
  --test p4_usecase_journeys             EXIT=0   3 passed / 7 passed / 7 passed
```

基线是 `442 passed`（本轮实测，非旧台账数字），隧道轨改完 `461 passed`，净增 19。

## 反例实测（主代理亲做，非转述）

把当年「编译通过且全轨测试全绿」的原始反例植回夹具（`RecordingTunnelTransport` 加
`close_tally: Mutex<usize>` + 构造初始化 + `close()` 里 `*tally += 1` + `close_calls()` 改读该字段），
在**独立 detached 工作树**里跑 `--lib`：

```
MUT_EXIT=101
test result: FAILED. 459 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
tunnel::single_counter_audit::no_tunnel_transport_implementation_stores_its_own_tally ... FAILED
tunnel::single_counter_audit::every_port_reading_is_a_projection_of_one_pure_fold ... FAILED
```

结论：R4/R5 精确命中，不是「碰巧有别的用例红了」。临时树已 `git worktree remove --force` 清理。

## 拆分后的判据完整性自查

- 三个 R6 用例在新模块路径下逐条重跑通过：`gate_self_audit::the_gate_names_are_tied_to_the_real_symbols` ok、
  `::the_audit_registry_covers_every_tunnel_module` ok、`::the_scan_primitives_see_the_shapes_they_claim` ok。
- 新文件自己进 `AUDITED` 登记表，并加进 R6 的必备清单 ⇒ **扫描面比拆分前更宽**，不是等宽。
- R4/R5 两条 kill 用例仍绿（`--lib` 里 `... ok`）。

## 未做 / 待裁定

1. **本轨被明令不得接线**：`src/tunnel/mod.rs:52-60` 登记的两格「实现缺失」——全阶段失败矩阵下的
   隧道引用回滚、`Quarantined` 下隧道引用与物理预算归属配对——**仍开着**。根因是全仓 `TunnelLedger`
   在 `src/tunnel/` 之外零生产调用方，`runtime/src/resource/**` 里 `tunnel` 出现 0 次。
   接线是架构工作，已另开独立轨（`feature/p3fu-arch`）并行推进，本轨不得顺手接线。
2. **CM-27 的「可确认关闭的资源许可归零」不在本模块**：`mod.rs:41-51` 记录了三处闭合点
   （`platform-api/src/ports/budget/pool_ledger.rs` 的 `Ledger::release`、`runtime/src/budget/ledger.rs:397`、
   `runtime/src/budget/coordinator.rs:428`）。本轨文档已把这条误记纠正回去。
3. **§11.3:568 的排队分位数不开轨**：它归属既有队列责任方，且规格明确「不许收窄规格、不许接队列进基准」。
4. `workspace` 56 warnings 与 clippy 候选门禁留到集成阶段（已验证
   `cargo clippy -p datazen-platform-api -p datazen-driver-api -p datazen-application -p datazen-ai-api --lib -- -D warnings` 是安全的）。

## 交付状态

**编码完成，等复测。** 三轨各派全新 Tester 复测（coder 自验不可替代），复测通过后才合入 `main`，
合入时删掉本文件。