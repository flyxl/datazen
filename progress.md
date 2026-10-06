# P3-FU 注册表轨道 · 进度台账

> 台账文件，随分支流转，**验收合并时必须删除**。缺陷结论已经写进代码、测试与 doc 注释，
> 本文件删除后结论仍可从 `src/registry/**` 与 `tests/registry_*` 读出。

- 轨道分支：`feature/p3fu-registry`，基线 `b096ffbb`（`main`）
- 文件面：`packages/runtime/src/registry/**`、`packages/runtime/tests/registry_*`
- 不碰：`packages/runtime/src/application/**`、`packages/runtime/src/gateway/**`、
  任何 `src-tauri/**` 与其它轨道的 worktree

---

## 一、逐项结论

| 项 | 提交 | 一句话结论 |
| --- | --- | --- |
| FU1 | `cbced2410` | 驱逐的五种答复不再一律吞成「什么都没发生」：只有「§9.4 已跑完」才摘表还额度，`CloseRejected` 明确不动账。 |
| FU2 | `783d261c7` | §9.4 的宿主归池前检查在生产路径上重新可达：改用 `retire` 逐批对账，`handle_count()` 不再被 `drain_handles()` 提前清零。 |
| FU3 | `f494cd28c` | CM-74 第 2 条「句柄被终结到新资源上」在当前结构下**不可达**（结论+证明写进 `context.rs` 模块文档）；真正活着的那半——屏障期间驱逐答复的记账——由 5 条新用例钉住。 |
| FU4 | `965a2ef98` + `6ae44387d` | epoch 字符串收敛到唯一事实源 `registry::epoch::epoch_string`，「只有一份格式化」改由源码扫描钉住（值比较那条缩回它真正能证明的事）。 |
| FU5 | `0d901927d` | 孤儿态（目录记着已提交、注册表已无该代）**报错** `InvariantBroken("committedWithoutHostEntry")`，不发回执、不凭空重开；无需改生产行为。 |
| 附带 | `a1b1f9652` | `registry.rs` 拆出 `registry/registry/evict.rs`，从 852 行回到 772 行（基线已是 803 行，超限是既有的）。 |

### FU1 — 驱逐的四种非成功结局
旧代码 `let Ok(Some(view)) = … else { continue }` 把 actor 的 `CloseRejected` 与「请求没送达」压成同一件事。
现按答复分五路（`src/registry/registry/evict.rs` 的 `evict_idle_at` 文档里有完整账目表）：

| actor 答复 | 摘表还额度 | 进返回值 |
| --- | --- | --- |
| `Ok(Some(view))` | 是 | 是 |
| `Ok(None)` | 否 | 否 |
| `Err(CloseRejected(_))` | 否 | 否 |
| `Err(SessionLost(_))` | 是（warn） | 否 |
| 其余 `Err(_)` | 否（warn） | 否 |

依据写在代码里：`release` 只有在 §9.4 四步全跑完后才写 `SessionLost`，所以 `SessionLost`
证明「关闭已发出」，可以摘表还额度，但不能对外声称这次驱逐成功了。

### FU2 — §9.4 归池前检查重新可达
`backend.rs:226-256` 的 `ready_to_return_to_pool(n) == (n == 0)`，此前因为 `release` 先
`drain_handles()`（`std::mem::take`）而恒真。现改为：`registered_before = handle_count()` 取样，
逐批 `retire`（要求逐批 `finalized == batch.len()` 精确对平，不看总数），随后
`finalized_total != registered_before || !ready_to_return_to_pool(handle_count())` 判不可知。
`CloseResource.registered_handles` 至此第一次带上真实数字——这就是 CM-74 里 §9.4 那条的观测面。

### FU3 — CM-74 第 2 条的可达性
先做可达性分析再动代码，结论与三条结构性事实写进 `src/registry/context.rs` 模块文档：

1. 句柄终结按**每个句柄登记时的 `resource_id`** 分组，物理资源关闭按 actor 自己的 `state.physical`——
   两者来源不同；
2. 候选是另一个 actor，`HandleRegistry::register` 只在 `exec::apply_completion` 里被调用；
3. `open_candidate` 的 `OpenRequest` 把 `idle_deadline_ms` 置 `None`，候选发布后到下一次 `SetIdleDeadline`
   之前，驱逐对它只能是空操作。

活的那半是**屏障期间的驱逐答复记账**：`CloseRejected("replacementInProgress")` 被当成「已关」
⇒ 第 ⑧ 步 `close_registered` 拿到 `UnknownSession` ⇒ 旧物理资源与旧句柄双双泄漏。
新文件 `tests/registry_evict_replacement.rs`（5 条）钉住这一半，含两条正例对照。

### FU4 — epoch 字符串唯一事实源
`epoch.rs::epoch_string` 是唯一实现，`to_directory_string` 调用它；`context.rs` 的私有副本删除；
`tests/registry_fixtures/mod.rs` 的同名函数改为转调。三条测试：

- `epoch_string_keeps_the_wire_shape`：钉住 `rte-00000000 / 01 / 07 / 12345678`，并在
  `rte-123456789` 处验证不截断、`strip_prefix("rte-")` + `parse::<u64>()` 往返成立；
- `epoch_method_agrees_with_the_single_formatter`：**已改名并降级**——它只能证明「两端今天逐字一致」，
  证明不了「没有把同样的 `format!` 内联进去」（见下面 §四 的诚实负结果）；
- `epoch_formatter_has_one_implementation_in_track`：递归扫描 `src/registry`（除 `epoch.rs` 本身）
  与 `tests/registry*`，命中 `"rte-` + `{:08` 两个片段（运行时拼 needle，避开注释里的自我命中）
  即失败。这条才是判别器。

### FU5 — 孤儿态语义定案
判据在 `context.rs` 的幂等短路分支：`directory.commit_status() == Committed { handle }` 但
`registry.epoch_of(handle.db_session_id)` 返回 `Err`（注册表连这一行都没有）⇒
`InvariantBroken("committedWithoutHostEntry")`。

被否掉的三种替代（理由写在 `context.rs:332-372` 的裁定注释里）：

1. 发一份 `session_view` 兑现不了的回执；
2. 对着已被别人占用的同一行发回执并附一枚新令牌；
3. 凭空重开——幂等键会退化成随机键，每次重试漏一格额度。

被否掉的载体：`UnknownSession`（怪罪调用方的 id，而这里 id 是对的）、`SessionLost`（暗示行还活着）、
`CloseRejected`；新造一个 `OrphanedState` 变体则是纯改名、无行为变化。
任务书里写的 `RuntimeError::OrphanedState` **并不存在**，真实产物是 `InvariantBroken`。

判据的两半互不遮蔽，由 `registry_rejection.rs` 的两条用例分别钉住：
T8 = 行还在但属于他人（`Ok(epoch)` 且世代不符），T8b = 行已经没了（`Err`）。

---

## 二、门禁（HEAD `a1b1f9652`，工作区空树 `da39a3ee5e6b` 首尾一致）

```bash
export CARGO_TARGET_DIR=/tmp/p3fu-registry-target
cargo fmt -- --check                                     # FMT_EXIT=0，无 Diff in
cargo test -p datazen-runtime --lib                      # EXIT=0
cargo test -p datazen-runtime --no-fail-fast             # EXIT=101（见下方基线失败）
cargo test --workspace --lib --no-fail-fast              # 见下方
```

逐字结论行：

```
BEFORE a1b1f9652c133c742cf5619f52c4f73dd3d989a2 da39a3ee5e6b
FMT_EXIT=0
GATE2_EXIT=0
test result: ok. 447 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
AFTER  a1b1f9652c133c742cf5619f52c4f73dd3d989a2 da39a3ee5e6b
```

`--no-fail-fast`（`datazen-runtime`）逐个测试目标：

```
unittests src/lib.rs           ok. 447 passed
unittests src/bin/cm60-bench   ok.  61 passed
registry_audit.rs              ok.  11 passed
registry_cancel.rs             ok.   9 passed
registry_context.rs            ok.   7 passed
registry_evict_replacement.rs  ok.   5 passed
registry_execution.rs          ok.   6 passed
registry_lifecycle.rs          ok.   7 passed
registry_rejection.rs          ok.   4 passed
registry_release.rs            ok.  15 passed
（其余 budget_*/cm28*/cm60*/cm70*/directory_*/owner_binding/p3_*/p4_*/resource_*/tunnel_* 全绿）
tests/gateway_contract.rs      FAILED. 50 passed; 1 failed
```

`--workspace --lib`（HEAD `a1b1f9652`）：`EXIT=101`，21 条 `test result: ok.` + 1 条
`FAILED. 395 passed; 1 failed; 3 ignored`（`datazen-driver-redis`，基线同款，见 §五）。
逐 crate 通过数与基线逐位相同，只有 `datazen-runtime --lib` 442 → 447。

```
BEFORE a1b1f9652c133c742cf5619f52c4f73dd3d989a2 da39a3ee5e6b
GATE4_EXIT=101
AFTER  a1b1f9652c133c742cf5619f52c4f73dd3d989a2 da39a3ee5e6b
```

回归计数：`--lib` 基线 442 → 447，**只增不减**（FU1 三条 + FU4 三条 − 基线里已有的…= 实测净 +5，
其中 FU4 的扫描/字面量/委托三条取代了原先的一条）。任何时候 `< 442` 都意味着删了既有用例。

---

## 三、变异证据（每条都在独立 detached worktree + 独立 target 目录里跑，EXIT=101 表示红）

| 编号 | 变异 | 结果 |
| --- | --- | --- |
| FU1 / FU3-1 | 4 臂 match 换回 `let Ok(Some(view)) = … else { continue }` | 只红 `屏障期间到来的驱逐不得摘行_旧句柄仍由替换例程在原资源终结` |
| FU1-post-split | 同上，拆分**之后**复测（`a1b1f9652`） | 只红 `registry::registry::tests::不可判定的驱逐摘表还额度_不留僵尸行`，`FAILED. 446 passed; 1 failed` |
| FU2 | `retire(&batch)` → `let _ = &batch;` | 10 红 |
| FU2b | 只删 `\|\| !ready_to_return_to_pool(...)` 与随之无用的 import | 只红 `driver两批确认数正好对平但宿主账未清空时仍判不可知` |
| FU3-2 | 删掉 `actor/context.rs::gate` 里 `ExecCommand::Evict` 的拒绝臂 | 只红 1 条，`registry_evict_replacement.rs:307` `left: 1 / right: 0` |
| FU3-3 | 所有驱逐答复一律 no-op | 6 红，含红线 `空闲到期才驱逐_未到期一律不动手` |
| FU3-4 | `pending` 退回 `drain_handles()` | 3 红 |
| FU3-5 | 替身 `OpenRequest` 的 `idle_deadline_ms: None` → `Some(1)` | 只红 1 条，`registry_evict_replacement.rs:505` |
| FU3-6 | `group_by_resource` 的 key 改成 `String::new()`（单一合成桶） | 9 红（如实说明：它连每个句柄登记时的 `resource_id` 都不看，比「直接用当前物理资源」更强） |
| FU4-1 | `{:08}` → `{:08x}` | 只红 1 条，`epoch.rs:430` `left: "rte-00bc614e" / right: "rte-12345678"` |
| FU4-2 | 在 `to_directory_string` 里内联一份逐字相同的 `format!` | **绿**，`ok. 446 passed`，EXIT=0 —— 诚实负结果，见 §四 |
| FU4-3 | 在 `context.rs` 里重新塞一份私有格式化 | 只红扫描那条，`epoch.rs:499` 报出 `["src/registry/context.rs"]`，同时值比较那条仍绿 |
| FU5-1 | 给孤儿态判据加一条 `Err(_) => {}` 放行臂 | 只红 `目录记着已提交但注册表已无该行时同键重放不得签发回执也不得凭空重开`；`registry_context.rs` 仍 7 全绿、`registry_rejection.rs` 的 T8 仍绿 |

抽两条原文：

```
MUT FU5-1 APPLIED: 孤儿态（注册表连这一行都没有）被判据放过，签发回执
EXIT=101
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
--- failed ---
    目录记着已提交但注册表已无该行时同键重放不得签发回执也不得凭空重开
```

```
thread 'registry::epoch::tests::epoch_formatter_has_one_implementation_in_track' panicked at packages/runtime/src/registry/epoch.rs:499:9:
世代字符串的格式字面量在本轨道内只能出现在 epoch.rs 一处，这些文件里又各自写了一份：["src/registry/context.rs"]。
```

---

## 四、诚实负结果：FU4-2 变绿，促成 `6ae44387d`

最初的委托测试 `epoch_method_delegates_to_the_single_formatter` **是同义反复**：把一份逐字相同的
`format!` 内联进 `to_directory_string`，它照样绿（`ok. 446 passed`，EXIT=0）。原因是它比的是
「方法的输出 == 函数在同样输入下的输出」，而两份实现可以一起漂移。

处置：把它改名成 `epoch_method_agrees_with_the_single_formatter`，文档里写明它只能证明
「两端今天逐字一致」，并新增 `epoch_formatter_has_one_implementation_in_track` 做源码扫描，
由 FU4-3 证明扫描才是判别器。

扫描的已知逃逸（同样写进测试注释，不装作没有）：宽度不同的 `{:08x` 扫描抓不到但字面量形状测试抓得到；
在 `epoch.rs` 内部再写一份扫描抓不到；`tests/gateway_contract/**` 按路径前缀排除。
`src/application/convert.rs` 也是被**路径前缀**排除而不是白名单，所以将来有人修它不需要改测试。

---

## 五、两处基线失败（都不是本轨道引入，工作区未被本轨道改动）

1. **`gateway_contract::invariants::gateway_sources_obey_the_locked_invariants`**
   `packages/runtime/tests/gateway_contract/invariants.rs:227`：`mod.rs 超过 800 行：801`。
   `packages/runtime/src/gateway/mod.rs` 在基线 `b096ffbb` 上就是 801 行，本轨道对
   `src/gateway/**` 零改动。

2. **`datazen-driver-redis --lib` 的 `connect::tests::live_prefer_falls_back_to_plaintext_and_require_refuses`**
   `rustls-0.23.43/src/crypto/mod.rs:249`：`Could not automatically determine the process-level
   CryptoProvider from Rustls crate features.`
   **已在基线工作树上复现，与本分支无关**，三条证据：

   - 本分支对 `packages/drivers/redis` 的 diff 为空；
   - 基线 `b096ffbb` 的 detached 工作树（`/tmp/p3fu-baseline`，`cargo test --workspace --lib
     --no-fail-fast`）→ `EXIT=101`，**同名测试、同一个 CryptoProvider 报错**，
     `test result: FAILED. 395 passed; 1 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.08s`，
     `Could not automatically determine the process-level CryptoProvider` 命中 1 次；
   - 同一个提交上单独跑这条是绿的：`cargo test -p datazen-driver-redis --lib
     connect::tests::live_prefer_falls_back_to_plaintext_and_require_refuses` → `EXIT=0`，
     `test result: ok. 1 passed; 0 failed; ...`。只有 `--workspace`（全工作区一起构建、rustls
     feature 被统一）时才复现——这是命令形态决定的，不是代码差异决定的。

   两边 `--workspace --lib` 的逐 crate 通过数逐位相同，只有 `datazen-runtime --lib` 从 442 变 447：

   ```
   基线 b096ffbb：2287 0 101 329 44 33 21 56 1 21 43 177 200 13 89 120 13 48 18 215 442
   本分支 a1b1f9652：2287 0 101 329 44 33 21 56 1 21 43 177 200 13 89 120 13 48 18 215 447
   两边 FAILED 行：均为 datazen-driver-redis 的 `FAILED. 395 passed; 1 failed; 3 ignored`
   ```

   注：基线工作树要跑 `--workspace --lib` 必须先补齐 gitignored 的 codegen 产物
   （`src-tauri/src/driver_init.rs`、`src-tauri/capabilities/default.json`、
   `src-tauri/resources/builtin-ep/`），否则 `datazen` crate 直接 `error[E0583]: file not found for
   module driver_init` / `resource path resources/builtin-ep doesn't exist`，连编译都过不去。
   上表是在把这三样从本工作树复制过去之后测的。

---

## 六、红线仍然绿（逐字摘自 `/tmp/p3fu-gate-final.log`）

```
1617:test 回执里的令牌在新会话上作数且旧令牌立即失效 ... ok
1619:test 重试同一旧句柄返回同一份回执且不重建候选 ... ok
1621:test 替换成功_旧句柄在原资源上终结后旧资源才关闭 ... ok
1622:test 重试不换令牌_第一次那枚在重试之后仍然作数 ... ok
1686:test 未设空闲期限的会话永不被驱逐 ... ok
1689:test 空闲到期才驱逐_未到期一律不动手 ... ok
```

以上六条自基线起**逐字未改**（只被 FU3-3 这类变异点红过，从未被本轨道的改动削弱或删除）。

---

## 七、待裁定 / 遗留

1. **`packages/runtime/src/application/convert.rs:62-69` 还有第三份 epoch 格式化**
   `public_handle` 里写着 `format!("rte-{:08}", value.runtime_epoch.get())`，而同一文件
   `handle`（:50-61）是用 `strip_prefix("rte-")` 解析的。文件不在本轨道面内，**没有改**。
   建议改法：调 `registry::epoch::epoch_string`。FU4 的扫描按路径前缀排除它，所以修的时候不需要改测试。
2. **一个已关闭（而非已作废）的候选可能被重放重新 `publish()`**：`registry_rejection.rs`
   的台账里记着这个口径缺口。本轮未修——它需要先定「重放是否应当重新登记一个已关闭的 id」，
   属设计裁定，不是缺陷修复的范畴。
3. **FU5 的代价**：调用方必须换一枚新的幂等键才能重来；目录里那条 `Committed` 记录宿主撤不掉
   （§13.1 的结构前提：落盘记录只有请求摘要与回执投影）。
4. `registry.rs` 的单文件规模：`a1b1f9652` 之后是 772 行，已在 800 以内；基线 803 行说明
   这一条此前没人守，建议把 800 行检查从 `gateway/mod.rs` 推广到全仓 `packages/runtime/src`。
