# progress — p3-cm28-concurrent-return

Track: `p3-cm28-concurrent-return` · branch `feature/p3-cm28-concurrent-return` · baseline main `c048dd37c`
Worktree: `.worktrees/datazen-p3-cm28-concurrent-return`（禁止改主检出与其他 worktree）

判据（只读）：`docs/architecture/platform/connection-management.md` §16
- CM-27 = **1027–1031**
- CM-28 = **1033–1037**，逐字：

```
**CM-28 重复 release/close（H）**

- 前置：同 Lease、多个关闭请求；cleanup 与 timeout 竞态。
- 步骤：并发释放 20 次，再重复查询 tombstone。
- 断言：driver close 至多一次有效关闭；预算不负数；隧道不多减引用；重复响应一致。
```

---

## 三态清单

### 已完成

- [x] **逐字复核判据原文**（未采信转述），确认 CM-28 四条断言。
- [x] **Gap A —「20 次并发归还下的 driver close 至多一次有效关闭」**
  新增 `packages/runtime/tests/cm28_concurrent_release.rs`。
  - 真实并发：`Barrier(20)` + 每条一个 `tokio::spawn`，`#[tokio::test(flavor="multi_thread", worker_threads=4)]`。
  - 计数对象是 **`PhysicalTransport::close`**（`resource/cleanup.rs:479` 的 trait 方法），
    不是隧道 `TunnelTransport::close`。既有 `tests/tunnel_refcount_contract.rs:352`
    数的是后者且是**顺序** for 循环，与本条判据无关。
- [x] **Gap B —「并发归还下预算不负数、重复响应一致」** 同文件：
  - 预算：`BudgetPort` 自带 `outstanding: i64` + `min_outstanding`，**允许** `outstanding` 变负，
    否则探测器与被测对象同源、断言成同义反复；另带 `underflows` / `unmatched_closes` 记录。
  - 重复响应一致：20 路里 1 个 `Ok` + 19 个**逐字段相等**的 `Err`，随后 8 次重复查询
    「已释放租约墓碑」必须稳定回答同一条。
  - 「重复查询 tombstone」在资源层钉为**已释放租约墓碑**：`manager.lease(&id)` 恒 `None`、
    `occupied_slots()` 恒 0。真正的 tombstone 代码在 `registry/**`、`gateway/**`（两处本轨冻结）。
- [x] **CM-28 断言 3「隧道不多减引用」** 从「只有顺序测试」补到**真实并发**：
  新增 `packages/runtime/tests/cm28_concurrent_tunnel.rs`。
  断言取 `TunnelRelease::refs` 的**取值全谱**必须恰为 `{0..=19}`（无重复、无缺口），
  而不是只看终值 0 —— 终值 0 抓不到丢失更新。
- [x] **`src/tunnel/mod.rs:22-32` 残留声明块重写**（现 107 行）。
- [x] **逐条变异证明 + 反向对照**，见下表。
- [x] **门禁全绿**，见下节。

### 进行中

- （无）

### 未开始

- （无）

---

## 变异证明 / 反向对照（全部已实跑，逐字结论）

| # | 变异点 | 目标断言 | 实跑结论 |
| --- | --- | --- | --- |
| M1 | `resource/manager.rs:336` 删掉 `self.table.forget(lease_id);` | 同租约 20 路并发 | **RED** `occupied_slots()` `left: 20 / right: 0`；输家错误由 `UnknownResource` 变 `IllegalTransition { from: Closed, to: Acquired }`；墓碑查询 `left: Some((true, 1, 1)) / right: Some((false, 0, 0))`。**`close_confirmed == 1` 仍绿** |
| M2 | 同时再废掉 `:291` 的 `state != InUse` 守卫 | 同上 | **仍 RED 且同因**（输家先在守卫处翻车，`assert_driver_closed_exactly_once` 未被触达） |
| M3 | `Closed` 分支里把 `transport.close` **多调一次** | 「driver close 至多一次有效关闭」 | **RED** `assertion left == right failed: driver close 至多一次有效关闭：关闭尝试数必须精确等于 1 / left: 2 / right: 1`；20 租约那条 `left: 40 / right: 20` |
| M4 | `tunnel/ledger.rs` `drain()` 里注释掉 `entry.refs.saturating_sub(1)` | 并发归还不超减引用 | **RED** `left: {20} / right: {0, 1, 2, …, 19}`（终值仍非零、全谱塌成单点） |

反向对照（写成常驻测试，绿）：`negative_control_the_driver_port_counter_reaches_more_than_one_close`、
`negative_control_the_budget_detector_reports_an_over_release`、
`negative_control_the_tunnel_port_counter_reaches_more_than_one_close`。

**M1/M2 的关键结论（必须进集成结论）**：「至多一次有效关闭」在 `release` 里被**三道独立守卫重复兜住**
（`state != InUse` 守卫 / `table.forget` 摘行 / `resource/transition.rs` 的 `LEASE_TRANSITIONS` 给
`Closed` 无出边），所以**任何单点变异都打不红它**。能打红的只有 M3 这种直接改 `close` 调用点的变异。
这不是测试弱，是不变量被过度确定 —— 但它意味着**不能**靠「单点变异红」来验收本条，必须同时有 M3 与反向对照。

---

## 门禁（首尾各记一次 HEAD / 工作区）

```
PRE_HEAD=e1cd7ff885ee019dc7021c9fc1db0358f72d01df
PRE_STATUS=[]            PRE_SHA=da39a3ee5e6b   # da39a3… = 空树状态
```

| 门禁 | EXIT | 逐字结论行 |
| --- | --- | --- |
| `cargo fmt -p datazen-runtime -- --check` | 1 → 修后 0 | 首跑 132 行 diff（两个新测试文件未格式化）；`cargo fmt` 后复跑 `EXIT=0`，输出 0 行 |
| `cargo test -p datazen-runtime --lib` | 0 | `test result: ok. 436 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` —— 与基线 main **同数**，零净减 |
| `cargo test -p datazen-runtime` | 0 | 全部 `test result: ok`，`FAILED` 行 0，合计 698 passed |
| `cargo test -p datazen-runtime --test cm28_concurrent_release` | 0 | `running 7 tests` / `test result: ok. 7 passed; 0 failed` |
| `cargo test -p datazen-runtime --test cm28_concurrent_tunnel` | 0 | `running 3 tests` / `test result: ok. 3 passed; 0 failed` |
| `pnpm typecheck` | — | **未跑**：本轨零 TS 改动 |

`cargo test --filter NAME` 不是 cargo 的 flag；按 `cargo test -p X --lib -- NAME` 跑，并核对 `running N > 0`
（`0 passed` 是空绿）。

---

## 与用户转述不一致之处（逐条复核结论）

1. **CM-28 的行号是 1033–1037，不是「约 1027–1037」。** `1027-1037` 是 **CM-27 + CM-28** 的合并跨度，
   正是 `src/tunnel/mod.rs:23` 原先引用的口径 —— 那一行本来就对，本次只把它拆成
   `1027-1031` / `1033-1037` 两段写清楚，没有「纠正」它。
2. `tests/tunnel_refcount_contract.rs:352` 的函数名是
   `one_return_releases_exactly_once_however_many_times_it_is_repeated`，不是省略号形式的简称。
3. **`packages/runtime/tests/release.rs` 不存在。** 真实路径是
   `packages/runtime/src/registry/actor/tests/release.rs`，落在本轨**冻结**的 `registry/**` 内。

---

## 登记在案、本轨**不实现**的两格（不得写成一次性架构断言）

`src/tunnel/mod.rs` 旧残留块列了 5 条。处置如下：

| # | 旧残留项 | 处置 |
| --- | --- | --- |
| 1 | permit / socket / handshake / init / register **全阶段失败矩阵**下的隧道引用回滚（CM-27 断言） | **登记，不实现**。缺的是**接线**不是断言：`TunnelLedger` 在 `src/tunnel/` 之外零生产调用方 |
| 2 | 「可确认关闭的资源许可归零」 | **从残留列表删除**。该格主语是**资源许可**，隧道不在这一格；已由三处闭合：`pool_ledger.rs:353` `PoolLedger::release`（摘行 + `uncharge`）、`budget/ledger.rs:397` permit 幂等核销 INV-10、`budget/coordinator.rs:428` 端口级核销 |
| 3 | `CleanupDisposition::Quarantined` 下隧道引用与物理预算的**归属配对** | **登记，不实现**。同 #1：`resource/cleanup.rs:285` 的处置语义只管物理预算那一半 |
| 4 | 20 次并发归还下 driver close 至多一次有效关闭 | **本轨闭合** |
| 5 | 并发归还下预算不负数、重复响应一致 | **本轨闭合** |

已核实的接线事实（写进 `src/tunnel/mod.rs`，不留在这份台账里）：
`grep -rn TunnelLedger packages/ src-tauri/src/ --include='*.rs'` 的非测试命中**全部**在 `packages/runtime/src/tunnel/` 内；
`grep -rniE 'tunnel' packages/runtime/src/resource/*.rs` 命中 **0 次** ⇒ `ResourceManager` 不知道隧道存在。

---

## 遗留（已记录，未在本轨修）

| 类别 | 事实 | 位置 |
| --- | --- | --- |
| 静默钳位 | `*used = Counter::new(used.get().saturating_sub(u64::from(count)))` —— 物理配额**超额核销被静默夹到 0**，既不报错也不记日志 | `packages/platform-api/src/ports/budget/pool_ledger.rs:227`（`uncharge`）。跨 crate / 只读，本轨只做见证 |
| 状态不自洽 | `release` 的 `Closed` 分支用 `self.transport.close(...)?`，传输层报错时租约**卡在 `Closing`** 而非 `Quarantined`；这与 `LEASE_TRANSITIONS` 自己给 `InUse→Closing` 写的 `exits_when`（「若关闭过程本身状态不明 ⇒ Quarantined」）矛盾，也与 `force_close` 把同一错误映成 `Quarantined` 的做法不一致 | `resource/manager.rs:332` vs `resource/manager.rs:379-412`、`resource/transition.rs` |
| 重试语义 | `force_close` 只要租约行还在就**无条件**调 `transport.close`（含 `Closing`/`Quarantined`/`Closed` 行）。未确认关闭的重试是设计意图（§9.4），**不是缺陷**，但 CM-28 的「**有效**关闭」措辞正靠「只有一次*被确认*的关闭」成立，必须留档 | `resource/manager.rs:379-412` |
| 命名碰撞 | `src-tauri/src/tunnel.rs` / `ssh_tunnel` 是**另一套** SSH/HTTP-proxy 隧道，与 `datazen_runtime::tunnel` 无关。全仓 grep `tunnel::` 极易把两者混为一谈 | `src-tauri/src/**` |

## 待裁定

- 上面「状态不自洽」一行是**真缺陷还是有意为之**，需要跨轨裁定：`release` 的 `?` 与 `force_close` 的
  `Quarantined` 映射必须统一。二者不在本轨禁改清单内，但改动会外溢到 CM-73 轨的交接面，故本轨只记录不改。
- `pool_ledger.rs:227` 的 `saturating_sub` 静默钳位是否要改成「可观测的过量核销」，属 platform-api 轨。

---

本文件是分支根目录的进度台账，**合并时必须删除**，不得存活到 `main` 上。
