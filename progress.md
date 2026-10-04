# progress — p3-cm28-concurrent-return

Track: `p3-cm28-concurrent-return` · branch `feature/p3-cm28-concurrent-return` · baseline main `c048dd37c`
Worktree: `.worktrees/datazen-p3-cm28-concurrent-return`（禁止改主检出与其他 worktree）

判据（只读）：`docs/architecture/platform/connection-management.md` §16.3（「并发、预算、资源清理与取消」，标题在 `:983`；`## 16. 详细测试用例` 起 `:861`）
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
| **M5** | `tunnel/ledger.rs` 的关闭计数 `+= 1` 改 `= 1` | R2 新增隧道负控「同一台账关掉两条独立隧道 ⇒ `close_calls() == 2`」 | **RED** `EXIT=101`、`cm28_concurrent_tunnel` `2 passed; 1 failed`，panic 在 `cm28_concurrent_tunnel.rs:283` `left: 1 / right: 2`。**同一次变异下 `cm28_concurrent_release` 的 7 条仍全绿**（变异只在隧道侧），而**旧负控那条弱的 `close_calls() == 1` 仍绿** —— 这正是 F-2 的要害：它证明弱断言不可证伪。**还原后 `EXIT=0`、7 + 3 全绿**（双侧复验：红 + 绿） |
| M6 | 8 处 `#[tokio::test(flavor="multi_thread", worker_threads=4)]` 全部降级成 `#[tokio::test]` | 「真实并发」前提 | **EXIT=0，10 条仍全绿** ⇒ 断言与线程数无关（`&mut self` 串行化）。**这不是绿灯**，是**必须留档的事实**：tokio 拒绝「只留 `worker_threads` 不留 flavor」，报 ``error: The `worker_threads` option requires the `multi_thread` runtime flavor``，所以**无声降级只能靠删掉整个 attribute**（diff 可见）—— 这是 flavor 剩下的唯一机械护栏 |
| M7 | `cm28_concurrent_release.rs` 两处 `Barrier::new(N)` 塌成 `Barrier::new(1)` | 「20 路同时放行」前提 | **RED** `EXIT=101`、`4 passed; 3 failed`：`left: 0 / right: 1`、`left: 0 / right: 1`、`left: 19 / right: 20`。**还原后 `EXIT=0`、7 passed**。M6 与 M7 合起来才是完整的「并发是真的」证明 |

反向对照（写成常驻测试，绿）：`negative_control_the_driver_port_counter_reaches_more_than_one_close`、
`negative_control_the_budget_detector_reports_an_over_release`、
`negative_control_the_tunnel_ledger_counter_reaches_two_on_one_ledger`（R1 的
`negative_control_the_tunnel_port_counter_reaches_more_than_one_close` 因每轮重建台账而恒为 1，已按 F-2 重写为**同一台账两条隧道**）。

**M1/M2 的关键结论（必须进集成结论）**：「至多一次有效关闭」在 `release` 里被**三道独立守卫重复兜住**
（`state != InUse` 守卫 / `table.forget` 摘行 / `resource/transition.rs` 的 `LEASE_TRANSITIONS` 给
`Closed` 无出边），所以**任何单点变异都打不红它**。能打红的只有 M3 这种直接改 `close` 调用点的变异。
这不是测试弱，是不变量被过度确定 —— 但它意味着**不能**靠「单点变异红」来验收本条，必须同时有 M3 与反向对照。
TA 复核 M3 时还多测出一条本轨漏报的：**`repeated_tombstone_queries` 同样变红（`left: 2 / right: 1`）**。

---

## 门禁（首尾各记一次 HEAD / 工作区）

### R1

```
PRE_HEAD=e1cd7ff885ee019dc7021c9fc1db0358f72d01df
PRE_STATUS=[]            PRE_SHA=da39a3ee5e6b   # da39a3… = 空树状态
```

### R2（本轮，5 个改动文件全部为注释/测试，**行为变更零**）

```
PRE_HEAD =c108920fd3890b985ce7bd6110cfa5604ee7dfbb
POST_HEAD=c108920fd3890b985ce7bd6110cfa5604ee7dfbb    # 一致
PRE_SHA  =976bfa5304bf   POST_SHA=976bfa5304bf          # 一致 ⇒ 跑门禁期间无人动过这棵树
PRE_STATUS = POST_STATUS = 5 files modified:
  packages/platform-api/src/ports/budget/pool_ledger.rs
  packages/runtime/src/resource/manager.rs
  packages/runtime/tests/cm28_concurrent_release.rs
  packages/runtime/tests/cm28_concurrent_tunnel.rs
  progress.md
```

| 门禁 | EXIT | 逐字结论行 |
| --- | --- | --- |
| `cargo fmt -p datazen-runtime -- --check` | 0 | 输出 0 行 |
| `cargo fmt -p datazen-platform-api -- --check` | 0 | 输出 0 行（本轮首次跑，因为改了 platform-api 的注释） |
| `cargo test -p datazen-runtime --lib` | 0 | `test result: ok. 436 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` —— 与 R1、基线 **同数**，零净减 |
| `cargo test -p datazen-runtime` | 0 | 26 个 `test result:` 行，合计 **passed=698 failed=0**，`FAILED`/`panicked` 行 **0** |
| `cargo test -p datazen-platform-api --lib` | 0 | `test result: ok. 215 passed; 0 failed`（本轮首次跑，`pool_ledger.rs` 被改动） |
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
4. **「这个 crate 没有并发」不成立。** `registry` 的 execution/cancel/audit、directory 的
   ownership/TTL、budget 的 CM-65 生命周期、`connection/testing/barrier/**` 都已用
   `tokio::spawn` / `join!` / `Barrier`。真实缺口只在 `release` / `retire` / `return_resource` 这三处，
   判据 CM-28 限定的也正是归还路径。
5. ★ **F-6 的「爆炸半径 = `packages/runtime/tests` 全部 28 个二进制 / 698 条」是 cm28 树的口径，
   不能搬到本轨或 cm60 树上引用。** cm60 树实测是 18 个集成二进制 / 180 条（整 crate 626 条）；
   本轨 fork-point 上顶层 `tests/*.rs` 已是 **22** 个，本树 **24** 个。三个 fork-point 三个数，
   正因如此本台账的 F-6 一律标注树与计数方式（见「F-6 数字口径」）。
   —— 这是本轨第 **6** 次犯同一类**「作用域搬运」**错误（前 5 次：判据行号、函数名、`release.rs` 路径、
   「crate 无并发」、把 TA 自己 detached 验收树当成本轨工作树）。**根治办法只有一条：任何数字都必须带树名 + 计数命令**，
   不能凭上一段对话里的记忆写。

---

## 本轨自审纪律（每轮收尾对照）

1. 引用的判据条款**逐字再读一遍**再落笔，不采信任何转述（包括我自己上一轮的转述）。
2. 每个新断言都要有**变异红 + 还原绿**双侧证据；只有一侧不算证明。
3. 结论必须落进**代码 / 测试 / 冻结文档**，只落在本台账里等于没做完 —— 台账合并时会被删除。
4. 门禁首尾各记一次 `git rev-parse HEAD` 与 `git status --porcelain`，证明运行期间无人动过这棵树。
5. 数字必须注明树与计数命令（见第 5 条）。
6. **行号与符号名并写**（R3 新增）。行号只能证明「当时那行是这句」，符号名才能证明「现在这句还在」；
   两者都写，任一漂移都不至于让整条结论失效。**凭空写出的符号名必须 `grep` 一次** —— R3 的 FAIL-A
   就是把不存在的 `PoolLedger` 当证据写进了注释。

---

## 登记在案、本轨**不实现**的两格（不得写成一次性架构断言）

`src/tunnel/mod.rs` 旧残留块列了 5 条。处置如下：

| # | 旧残留项 | 处置 |
| --- | --- | --- |
| 1 | permit / socket / handshake / init / register **全阶段失败矩阵**下的隧道引用回滚（CM-27 断言） | **登记，不实现**。缺的是**接线**不是断言：`TunnelLedger` 在 `src/tunnel/` 之外零生产调用方 |
| 2 | 「可确认关闭的资源许可归零」 | **从残留列表删除**。该格主语是**资源许可**，隧道不在这一格；已由三处闭合：`pool_ledger.rs` 里 `impl Ledger` 的 **`release`**（`:374` 定位并摘行、`:376` 调 `uncharge`、`:387` 出 `ReleaseDisposition::Closed`；`:385` 注释自述「本端口只有『真的 close 了』这一条释放路径」）、`budget/ledger.rs:397` permit 幂等核销 INV-10、`budget/coordinator.rs:428` 端口级核销。**符号名可重定位优先于行号**（见下） |
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
| 静默钳位（D） | `*used = Counter::new(used.get().saturating_sub(u64::from(count)))` —— 物理配额**超额核销被静默夹到 0**，既不报错也不记日志。同样的 `saturating_sub` 模式还在 `budget/ledger.rs` 的 `shared_used`。**R2 起这条结论已写进 `pool_ledger.rs` 的 `uncharge` 函数体**（注释，非行为变更）：饱和减法**保证**了 CM-28 断言 2「预算不负数」，代价是过量核销被静默钳位——若某条 release 路径的 `count` 大于实际 charge（非对称 `held.physical_count`，或同一条 charge 链被 `uncharge` 两次），台账只会少记，不会有任何错误或日志。是否加 metric/warn 属 platform-api 轨，不在 CM-28 范围内 | `packages/platform-api/src/ports/budget/pool_ledger.rs` `uncharge`（**本轨改的是注释**）；`uncharge` 的唯一调用方是**同一个 `impl Ledger` 里的 `release`**（**不是 `PoolLedger`** —— `PoolLedger` 这个类型名在 `packages/platform-api/src` 全树出现 **0** 次；对外宿主类型是 `InMemoryDriverPoolBudget`），调用点在 `locate` + `leases.remove(index)` 之后 |
| **违反判据的条款（E）** | `release` 的 `Closed` 分支用 `self.transport.close(...)?`，传输层报错时租约**卡在 `Closing`** 而非转 `Quarantined`。按 R1 反馈升级定级：它**违反 `connection-management.md` 三处**——① `:657`「**任一失败都关闭**，即使 driver 返回 Clean 也不能绕过宿主检查。**reset 不支持、失败或超时直接关闭。**」（关闭失败必须落到终态，不能停在中间态）；② `:640`「cancel/cleanup deadline \| 10 秒 \| **不能确认则隔离并核验**」（关闭未确认时的规定动作是**隔离**，`Closing` 不是隔离态）；③ `:713`「cleanup 未确认 \| 保留预算占用/隔离资源；**确认关闭**或节点隔离后才核销」（未确认时必须走隔离，否则预算核销无依据）。代码侧三处自证：`resource/transition.rs` 给 `InUse→Closing` 写的 `exits_when`「若关闭过程本身状态不明 ⇒ Quarantined」、`resource/cleanup.rs:482`「返回 `Err` 表示关闭握手没确认，调用方必须转隔离而不是放行」、以及 `force_close` 把**同一错误**映成 `Quarantined` | `resource/manager.rs` `release` 的 `CleanupDisposition::Closed` 分支（**已写进函数体**，作为「登记项 E」块，与缺陷 D 的 `pool_ledger.rs` 注释块同构：**结论 / 三处逐字事实 / 怎么复现 / 为什么本轨不改**四项俱全，且**不引用任何台账**）；`resource/transition.rs`；`resource/cleanup.rs:482` |
| 重试语义 | `force_close` 只要租约行还在就**无条件**调 `transport.close`（含 `Closing`/`Quarantined`/`Closed` 行）。**R1 时我在这里写下的「未确认关闭的重试是设计意图（§9.4），不是缺陷」已撤回** —— §9.4 全文没有这句话，`git diff <fork-point> <R1> -- packages/runtime/src/resource/manager.rs` 为**空**，证明那句假出处是**基线既有代码**，不是我这两轮引入的；但**把这句话抄进台账是我的错**，R1 验收把它判成 BLOCKER。R2 已按 `connection-management.md:657`「任一失败都关闭，即使 driver 返回 Clean 也不能绕过宿主检查」+ `:713`「确认关闭或节点隔离后才核销」重写推导：租约行还在 ⇒ 未被确认关闭 ⇒ 按 `:657` 必须**再关一次**。据此，「至多一次」只能修饰「**确认**」，不能修饰「尝试」——多次关闭尝试是 `:657` 要求的 | `resource/manager.rs` `force_close`（推导写在函数体内，不在本台账） |
| CI 缺口（**跨轨，owner = p3-cm60-pressure-drain**） | **没有任何 CI 链路跑 `datazen-runtime` 的集成测试二进制。** 链路只有一条：`.github/workflows/ci.yml:302` `pnpm test:platform-crates` → `package.json:111` → `scripts/run-platform-crate-tests.mjs:165` 硬编码 `argvList = ['test', '--lib', …]`，**只有 `--lib`**。基线与本轨 `EXTRA_TARGETS` 命中均为 **0**。`ci.yml:265` 的 `--tests` 只作用于 4 个 driver crate，**不含** `datazen-runtime`；`git grep -nE 'cargo (nextest\|test)' -- scripts .github package.json` 共 **31** 行，其中点名 `datazen-runtime` 的 **0** 行。**这是继承缺口不是本轨引入**：基线 `ci.yml:299` 的注释原文就写着「计划 :269 的 datazen-runtime CI 空缺」，基线用 `--lib` 绕过了它；本轨从 0 拉到 2 个新增二进制，而这两个**同样不被 CI 覆盖** | 计数口径见下表；**本轨禁改 `scripts/**` 与 `.github/**`** |
| 命名碰撞 | `src-tauri/src/tunnel.rs` / `ssh_tunnel` 是**另一套** SSH/HTTP-proxy 隧道，与 `datazen_runtime::tunnel` 无关。全仓 grep `tunnel::` 极易把两者混为一谈 | `src-tauri/src/**` |

### F-6 数字口径（必须注明树与计数方式，否则就是又一次「作用域搬运」）

| 口径 | 本轨工作树（`feature/p3-cm28-concurrent-return`，含本轮未提交改动） | 口径来源 |
| --- | --- | --- |
| 集成测试**二进制**数 | **24** | `cargo metadata` 的 `kind == ["test"]`（**不以 `find -name '*.rs'` 递归计数** —— 递归会把共享 `mod` 目录与顶层 shim 一并算进去） |
| 同上，fork-point `c048dd37c` | **22** | `git ls-tree c048dd37c:packages/runtime/tests` 数顶层 `.rs`；本树同一口径为 24，本轨**净增 2**（正是本轨新增的两个 `cm28_*`），两个口径在本仓恰好自洽 |
| 未被 CI 覆盖的差集 | **24 / 24，即 100%** | 由 `run-platform-crate-tests.mjs:165` 只有 `--lib` 推出 |
| cm60 树自己的实测 | 18 个二进制 / 180 条测试，整 crate 626 条，最大一组 `gateway_contract` 51 条 | **转述自 cm60 轨，本轨未复核**（不同 fork-point，本轨 fork-point 的顶层 `tests/*.rs` 已是 22，不是 18） |

★ **cm60 轨的 CI 修复是否已并入本轨（本轮实测，非推断）**：

```
$ git merge-base --is-ancestor 05ca8c3a9 HEAD            # cm60 的 CI 修复提交
EXIT=1                                                  # ⇒ 不是本轨 HEAD 的祖先 ⇒ 未并入
$ git merge-base --is-ancestor c048dd37c 05ca8c3a9       # 本轨 fork-point 是否先于它
EXIT=1                                                  # ⇒ DAG 上并列，不是「先后」
```

⇒ **登记项 F 仍未闭合**，`05ca8c3a9` 与 `c048dd37c` 在 DAG 上只相隔提交时间先后，**不构成祖先关系**，
不能拿「cm60 修过」当「本轨已覆盖」。任何一处都**不得**把 F-5 / F-6 记作「已闭合」，owner 保持
`p3-cm28-pressure-drain`，直到「本轨 HEAD 的 CI 作用域内点名 `datazen-runtime` 且含集成二进制」这条差集被证为 0。

★ 「全仓 `cargo test`/`nextest` 调用点 18 处，其中 2 处点名 `datazen-runtime`」这个数**不是本轨的口径**，本轨不引用它；
上面 31 行 / 0 行的数才是本轨 `scripts/** + .github/** + package.json` 范围内可复现的口径。

## 待裁定

- **登记项 E**：上面「违反判据的条款（E）」一行**不再是「真缺陷还是有意为之」的问题** —— 它违反
  `connection-management.md:657` / `:640` / `:713` 三处，事实层面已定级。剩下的是**谁来改**：
  `release` 的 `?` 与 `force_close` 的 `Quarantined` 映射必须统一，改动会外溢到 CM-73 轨的交接面，
  故本轨**只登记不改**。★ R3 起登记项 E 的**结论块完全自包含**在 `release` 的函数体里
  （结论 / 三处逐字事实 / 怎么复现 / 为什么本轨不改），**不再指向本台账，也不指向 `hub.md`** ——
  台账合并时删除、`hub.md` 本就不在本轨分支上，两者都是**死路由**（详见「R3 处置」第 2 行）。
- **登记项 D**：`pool_ledger.rs` `uncharge` 的 `saturating_sub` 静默钳位是否要改成「可观测的过量核销」，属 platform-api 轨。
- **登记项 F（CI 缺口）**：owner = `p3-cm60-pressure-drain`（该轨同时持有 `scripts/**` 与 `.github/**`）。
  ★ 该轨**当前仍在 FAIL**，所以这条依赖**未闭合** —— 不得写成「已修复」或「已闭合」。
  修法注意两条（本轨已从仓库自身交叉验证）：`ci.yml:258-265` 的注释与做法说明用 `--tests` 而非逐个 `--test`，
  但 `--tests` **不含 lib**，会把这 383 条单测挤出 `datazen-runtime` 的门禁 —— 正确做法是**并存**
  （追加一条既不带 `--lib` 也不带具体 `--test` 的调用），而不是用 `--tests` 顶替。

---

## R2（第二轮，同一 coder）验收反馈处置

R1 判定 FAIL：1 BLOCKER + 3 FAIL + 3 WARN。逐条处置如下。**行为变更：本轮为零**（5 个改动文件全是注释/测试）。

| # | 反馈 | 处置 | 落点（**必须合并后仍存活**，台账不算落点） |
| --- | --- | --- | --- |
| F-1 BLOCKER | `manager.rs` 里的注释拿 §9.4 当假出处 | **删掉任何「§9.4 说这是设计意图 / 别当笔误去修」措辞**；改为逐字引 `:657` 并写推导。落字口径只用**冻结文档已有**的词（「确认关闭」），不新造 | `resource/manager.rs` `force_close` 函数体 |
| F-2 FAIL | 隧道负控 `close_calls` 不可证伪（每轮重建台账 ⇒ 恒为 1） | 改成**同一台账上两条独立隧道**（不同路由 ⇒ 不同条目）断言 `close_calls() == 2`，并**双侧复验** | `tests/cm28_concurrent_tunnel.rs` 负控 + 下表 M5 |
| F-3 FAIL | 结论 D / E / 重试语义只活在 `progress.md` | 四条**全部落进代码或测试**：D → `pool_ledger.rs` 注释；E → `manager.rs` FIXME；重试语义 → `manager.rs` `force_close` 推导；**过度确定** → `tests/cm28_concurrent_release.rs` 文件头的 3 行变异表 | 见左列四处 |
| F-4 WARN | E 的登记沿用旧定级（「状态不自洽」） | 升级为**违反 `:657` + `:640` + `:713`**，三处逐字引用 | 本台账「遗留」表 + `manager.rs` FIXME |
| F-5 WARN | `multi_thread` flavor 钉不住 | 两个测试文件**文件头各一段可执行说明**：降级实跑数据 + barrier 塌缩反证 + 「这段注释是它唯一的存活形式」 | `tests/cm28_concurrent_release.rs` / `tests/cm28_concurrent_tunnel.rs` 文件头 |
| F-6 WARN | CI 缺口只登记在台账 | 本轨**禁改** `scripts/**` 与 `.github/**`，只能登记；已按 cm60 轨口径 + 本轨自测口径双列 | 本台账「遗留」表 + F-6 口径表 |

★ 「有效关闭」的口径（R1 未点名但必须先定，否则断言 1 无意义）：**「有效关闭」≡ 被确认的关闭**。
`grep -n '有效关闭' connection-management.md` 的复合词命中**只有 `:1037` 一处**，全文从未定义；
单字「有效」另有 10 处无关用法（`:412`、`:1286` 等）。改用 `:713`「确认关闭」与 CM-26 `:1025`
「确认关闭后只核销一次」这两个**文档已有**的词。定义写在**两处**（`manager.rs::force_close`
与 `tests/cm28_concurrent_release.rs` 文件头），任一处被删都不影响结论。

---

## R3（第三轮，同一 coder）验收反馈处置

R2 判定 FAIL：0 BLOCKER / 2 FAIL / 4 WARN。R1 的两个真缺陷 TA 已确认修复，10/10 强制检查全 PASS。
**本轮处置严格限定在「2 个 FAIL + 4 个 WARN」，不夹带任何行为变更** —— 6 个改动点**全部是注释**。
两个「有效关闭」口径定义（`manager.rs::force_close` 与 `tests/cm28_concurrent_release.rs` 文件头）、
`force_close` 的无条件重试行为、以及 D / E 两条的**行为**修复口径，TA 已实证其互相独立，本轮**一律不动**。

| # | 反馈 | 处置 | 落点（**必须合并后仍存活**） |
| --- | --- | --- | --- |
| **FAIL-A** | `src/tunnel/mod.rs` 把 `pool_ledger.rs:353` 当作 CM-27「资源许可归零」的**唯一**闭合证据。实测 `:353` 落在 **`fn reuse_idle`** 体内，与 `release` 无关 | 重写为 `impl Ledger` 的 **`release`** 的真实推导（`:374` 定位并摘行 / `:376` 调 `uncharge` / `:387` 出 `ReleaseDisposition::Closed` / `:385` 注释自述）。★ 顺带修掉一个**未被报告**的同物种缺陷：注释里的 `PoolLedger` **是个不存在的类型名**（`grep -r PoolLedger packages/platform-api/src` = **0**），真实对外类型是 `InMemoryDriverPoolBudget`，内部账本类型叫 `Ledger` —— 现已在注释里**显式声明这一点**，防止再被抄错 | `src/tunnel/mod.rs` 模块头 |
| **FAIL-B** | `manager.rs` 登记项 E 指向 `progress.md` 与 `hub.md`，**两个都在合并时消失**（台账必须删；`hub.md` 是集成分支文件，本轨分支根本没有）⇒ **死路由**，与 R1 的假出处同物种 | 按缺陷 D 的既有范式（`pool_ledger.rs` `uncharge` 函数体注释）重写成**完全自包含**的登记块：① 结论（`?` 使租约停在 `Closing`，与 `force_close` 的 `Quarantined` 互斥）② 三处**逐字**事实（`:640` §9.2 / `cleanup.rs:482` trait 文档 / `transition.rs:61` `exits_when`）③ **怎么复现**（注入 `Err` 走归还路径）④ **为什么本轨不改**（外溢 CM-73，跨轨改动不夹带）。**删掉全部台账路由**；`self.transport.close(&record.resource_id)?;` 一字未动 | `src/resource/manager.rs` `release` 的 `CleanupDisposition::Closed` 分支 |
| WARN-1 | 文件头变异表的 `:480` / `:594` / `:632` **三个行号统一 +7 失效** | 改为 `:487` / `:601` / `:639`，并**当场用 `awk` 逐行核对**（`:487`/`:639` 是多行 `assert_eq!(` 的首行，`:601` 是单行式 —— 已把「行号指向断言行本身」这点写进注释，避免下轮再漂） | `tests/cm28_concurrent_release.rs:92` |
| WARN-2 | `:713` 被标成「§10.1」，实际在 **§10.1.1**（`:694`）下 | 两处（`manager.rs`、`cm28_concurrent_release.rs:52`）改标 §10.1.1。★ 顺带核实：基线里另两处 `§10.1`（`dimension.rs:45`、`pool.rs:701`）指的是 `:690`，**确在 §10.1 内，是对的** —— 同一个「§10.1」字样同时存在对错两种，不逐一核对就会连坐 | `manager.rs::force_close` + `cm28_concurrent_release.rs:52` |
| WARN-3 | 判据被标成「§16」，实际是 **§16.3**（`:983`） | 三个位置改标 §16.3（两个测试文件头 + 本台账 `:6`） | 两个 `cm28_*.rs` 文件头 + 本台账 |
| WARN-4 | 登记项 F（CI 缺口）只活在台账里，**台账一删就没了** | 把整段事实登记**下沉进 `src/tunnel/mod.rs` 模块头**：`:165` 的 `['test','--lib',…]` 硬编码、`EXTRA_TARGETS` 全仓 0 调用方、CI 作用域 31 行 / 点名 0 行、24 个 `tests/*.rs` 全部不进 CI、owner `p3-cm28-pressure-drain`，并明写「该修复未并入本树之前，此处不得记作已覆盖」 | `src/tunnel/mod.rs` 模块头 |

### R3 新增的失败模式（独立于已有的「行号漂移」）

本轨前两轮的失败模式是**行号漂移**（`awk` 一查即穿）。R3 的两个 FAIL 里出现了**第二个同物种**：

> **凭空写出的符号名** —— 行号可以机械核对，**符号名不能**。`PoolLedger::release` 里的
> `PoolLedger` 在全树不存在，而它是被写进注释当证据的。任何以符号名背书的引用都必须
> `grep` 一次，**代价极低**。

**根治办法（本轮起写进纪律第 6 条）：行号与符号名并写**，任一漂移都不至于让整条结论失效。

### 门禁（R3）

**本轮行为变更 = 零**，且不是自述，是机械证明：对 4 个改动文件各跑
`grep -vE '^\s*(//|//!)' <file> | shasum` 并与 `git show HEAD:<file>` 的同样管道比对，**4/4 完全相同**。

```
PRE_HEAD =3e2075f9807c3a11e670d8b1445cf25b132c2745
POST_HEAD=3e2075f9807c3a11e670d8b1445cf25b132c2745    # 一致
PRE_SHA  =37a243269142   POST_SHA=37a243269142          # 一致 ⇒ 跑门禁期间无人动过这棵树
PRE_STATUS = POST_STATUS = 5 files modified:
  packages/runtime/src/resource/manager.rs
  packages/runtime/src/tunnel/mod.rs
  packages/runtime/tests/cm28_concurrent_release.rs
  packages/runtime/tests/cm28_concurrent_tunnel.rs
  progress.md
```

| 门禁 | EXIT | 逐字结论行 |
| --- | --- | --- |
| `cargo fmt -p datazen-runtime -- --check` | 0 | 输出 **0 行**（无 `rustfmt.toml` ⇒ 默认配置不重排注释，本轮全是注释改写，这条不是白跑） |
| `cargo fmt -p datazen-platform-api -- --check` | 0 | 输出 **0 行** |
| `cargo test -p datazen-runtime --lib` | 0 | `test result: ok. 436 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` —— 与 R1、R2、基线**同数**，零净减 |
| `cargo test -p datazen-runtime` | 0 | **26** 个 `test result:` 行，合计 **passed=698 / failed=0**，`FAILED`/`panicked` 行 **0** |
| `cargo test -p datazen-platform-api --lib` | 0 | `test result: ok. 215 passed; 0 failed` |
| `cargo test -p datazen-runtime --test cm28_concurrent_release` | 0 | `running 7 tests` / `test result: ok. 7 passed; 0 failed` |
| `cargo test -p datazen-runtime --test cm28_concurrent_tunnel` | 0 | `running 3 tests` / `test result: ok. 3 passed; 0 failed` |
| `pnpm typecheck` | — | **未跑**：本轨零 TS 改动 |

R3 的 698 / 436 / 215 / 7 / 3 与 R2 的 698 / 436 / 215 / 7 / 3 **逐项同数**。
TA 独立测得的门禁值（fmt 0 / lib 436 / total 698 / release 7 / tunnel 3）与本表一致。

---

本文件是分支根目录的进度台账，**合并时必须删除**，不得存活到 `main` 上。
