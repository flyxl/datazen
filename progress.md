# progress.md — 轨 A-11：`close_handle` 幂等关闭缺口

缺陷编号：A-11（独立验收发现）
状态：**READY_FOR_TEST**（D-1/D-2/D-4/D-5 均已处置，待复检）

## 验收打回项（`50c14069`，REJECT）

| 编号 | 级别 | 内容 |
| --- | --- | --- |
| D-1 | blocker | 7 条「foreign ref 在已关资源上仍被拒绝」顺序测试里，5 条把 `mark_closed()` 用在 `handle` 上、传给 `close_handle` 的却是从未关过的 `foreign` ⇒ `is_closed` 与 ownership 谁先都返回同一个 ownership 错，测试分不出顺序。受影响：mysql、sqlite、mongodb、redis、`LegacyResourceAdapter` |
| D-2 | major | 生产代码里 `mark_closed()` 只有 redis/postgres 各 1 处；mysql/sqlite/mongodb/hbase/vector 的 `close_resource` 从不标记 ⇒ 其 `is_closed()` 恒 false，短路在生产永不触发（base 上即如此，非本轨引入） |
| D-4 | minor | 7 处注释与 2 份台账引用不存在的符号 `LeaseFacade`（9 处命中、0 处定义）。真实类型是 `LeaseManager` @ `packages/runtime/src/resource/lease_manager.rs:225`；fail-closed 行为本身成立（`release` 内 `closer.close(&handle)?`，`lease_manager.rs:450`） |
| D-5 | 低 | hbase/vector/adapter 正向测试只断言 `is_closed` flag，无 budget/wire 副作用断言（mysql/sqlite 等都有） |

hbase 与 vector 的 2 条顺序测试本来就是正确形状（传给 `close_handle` 的是已关的
`handle`，foreign 作对照），作 D-1 的样板。

## 缺陷

`packages/driver-api/src/resource.rs:635-639` 的 trait 文档承诺：

> a handle the provider does not currently hold open is
> [`ResourceError::InvalidResourceState`], not `Ok`. The one legitimate
> repeat is a handle the resource handle already marks closed
> ([`ResourceHandle::is_closed`]), which is reported as success because
> the close already happened and re-doing it is not information.

即 `close_handle` 收到一个**资源句柄本身已标记 closed** 的引用时，应返回 `Ok(())`。

实测：9 个 `impl ResourceProvider` 里只有 postgres 实现了这个短路
（`packages/drivers/postgres/src/resource/provider/contract.rs:403-405`）。
postgres 把它排在 `belongs_to` 检查**之后**："closed + 外国引用" 先被
`InvalidResourceState` 拒绝。宿主 `LeaseManager::release`
（`packages/runtime/src/resource/lease_manager.rs:431-455`）对 `Err` 是 fail-closed
（保持登记不释放），所以**没有数据安全隐患**；但这是**已发布 trait 文档与实现不一致**。

## 最终选择：补实现，不改文档

- **收窄文档会把幂等关闭降级成缺陷报告**。文档那句是「已发布 trait 契约」的一部分
  （`PROTOCOL_VERSION = 5`，轨 A 合流后进入对外契约面），把它改窄等于宣布
  「资源已关后重放 close 报错」是常规形态 —— 而这恰是 A-11 要修的不一致本身。
- **阻断理由不对称**。原阻断实现给的理由是「不可重入」：担心 `Ok(())` 被误读成
  「我真的关掉了某个句柄」。这个顾虑只对**活着的资源**成立 —— 所以 postgres
  把短路放在 `belongs_to` 之后、只对已关资源生效。资源都关了，还谈「假称关闭」
  没有对象：没有任何东西可被误判。
- **fail-closed 在反方向咬人**。宿主对 `Err` 不释放登记，于是「已关资源上的重放」
  会让租约永远停在池外，而 close 其实早就成功了。这是**收益为负的拒绝**：
  不换取任何安全性，只换来永久泄漏。
- **测试面不对称**。redis 的既有测试只断言 `!handle.is_closed()`（反方向），
  没有任何测试锁正向契约；把责任推给文档会让正向契约永远无人看守。
- **与本仓库既有语义一致**。`ResourceHandle::mark_closed` 的自身文档
  （`resource.rs:162-167`）明确说 `is_closed()` 为真意味着「已经关了，重复关是
  幂等的」；`close_resource` 自己也以 `is_closed()` 为幂等短路
  （`resource_adapter.rs:612-615`「Already closed: idempotent, and no second
  budget release」）。`close_handle` 走反方向才是异类。

## 变更清单

### 实现（7 处，行为与 postgres 一致：`is_closed` 短路排在 `handle.check` 之后）

| 文件:行 | 改了什么 |
| --- | --- |
| `packages/drivers/mysql/src/resource.rs`（`close_handle`） | 加 `if handle.is_closed() { return Ok(()); }`，排在 `handle.check` 后，注释说明顺序与 postgres 一致 |
| `packages/drivers/sqlite/src/resource.rs`（同） | 同上 |
| `packages/drivers/mongodb/src/resource.rs`（同） | 同上 |
| `packages/drivers/redis/src/resource/provider/contract.rs`（同） | 同上 |
| `packages/drivers/hbase/src/resource/provider/contract.rs`（同） | 同上 |
| `packages/drivers/vector/src/resource/provider/contract.rs`（同） | 同上 |
| `packages/driver-api/src/resource_adapter.rs`（`LegacyResourceAdapter::close_handle`） | 同上 |

**未动**：postgres（原本已正确）、`packages/runtime/**`、`PROTOCOL_VERSION`、
驱动能力声明、trait 签名、CI。

### 测试（每个修过的 impl 一条正向 + 一条顺序，共 14 条）

落点遵守 `AGENTS.md` 驱动测试落点表：同文件 `#[cfg(test)]` / `<crate>/tests/`。

| 文件 | 新增 |
| --- | --- |
| `packages/drivers/mysql/src/resource_tests.rs` | `close_handle_on_an_already_closed_resource_is_idempotent_success` + `close_handle_refuses_a_foreign_ref_even_on_a_closed_resource` |
| `packages/drivers/sqlite/src/resource_tests.rs` | 同名两条 |
| `packages/drivers/mongodb/src/resource_tests.rs` | 同名两条 |
| `packages/drivers/redis/src/resource/tests_close_handle.rs` | 同名两条（该文件此前只有反方向断言） |
| `packages/drivers/hbase/src/resource/tests/unsupported.rs` | `closing_a_handle_on_an_already_closed_resource_is_success_not_a_refusal` + `a_foreign_ref_is_still_refused_on_a_closed_resource` |
| `packages/drivers/vector/src/resource/tests/unsupported.rs` | 同名两条 |
| `packages/driver-api/src/resource_adapter_close_handle_tests.rs` | `an_already_closed_resource_makes_the_handle_close_a_success` + `a_foreign_ref_is_still_refused_on_a_closed_resource` |

## 门禁实测（命令 / 退出码 / 结论行）

环境：`CARGO_TARGET_DIR=/tmp/cargo-a11-isclosed`；每轮首尾记录 HEAD
（= `e5e189a81` → `e5e189a81`，运行期间无人动过树）。

| 门禁 | 命令 | 退出码 | 结论行 |
| --- | --- | --- | --- |
| A | `cargo fmt --all -- --check` | **0** | （无 diff 输出） |
| B | `cargo check --workspace --all-targets` | **0** | `Finished dev profile [unoptimized + debuginfo] target(s) in 5m 32s`，`grep -cE "^error"` = **0** |
| C | `cargo test -p datazen-driver-api -p datazen-driver-mysql -p datazen-driver-sqlite -p datazen-driver-redis -p datazen-driver-postgres -p datazen-driver-mongodb -p datazen-driver-hbase -p datazen-driver-vector --no-fail-fast` | **101** | `passed=1585 failed=1 ignored=10`；唯一失败 `cross_database_reads_never_move_the_session`：`seed probe table dz_cross_db_probe_...: permission denied for schema public`（本地 PG role 缺 CREATE；该文件 `close_handle` 出现 **0** 次，与本改动无关，任务书已列为例外） |
| D | `cargo test -p datazen --lib` | **0** | `test result: ok. 1810 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out`（与基线逐字一致） |
| E1 | `node scripts/check-driver-protocol-compat.mjs --base 8dae098cc` | **0** | `[check-driver-protocol-compat] ok (base 8dae098cc; PROTOCOL_VERSION 5 -> 5, MIN_PROTOCOL_VERSION 1 -> 1, crate 0.0.11 -> 0.0.11)` |
| E2 | `node scripts/check-driver-protocol-release.mjs --base 8dae098cc` | **0** | `[check-driver-protocol-release] ok (base e5e189a81...; PROTOCOL_VERSION 5 -> 5, ...; driver releases: 0)` |
| F | `git status --porcelain` | — | 只含上述 14 个路径；无 `src-tauri/Cargo.toml` / `Cargo.lock` 改动，无未跟踪 codegen |

## 反向验证（证明新测试真有效，而不是恒真）

临时删掉 mysql 的 `is_closed` 短路后跑 `cargo test -p datazen-driver-mysql --lib close_handle`：
`test result: FAILED. 4 passed; 1 failed` ——
`close_handle_on_an_already_closed_resource_is_idempotent_success ... FAILED`。
即新测试确实在实现缺失时变红。变体已还原（`grep -n is_closed` 确认恢复）。
这是独立验证意义上的变异测试：**同一棵树上做，且变异前后 HEAD 与门禁结论均已记录**。

## 待裁定项

1. **mongodb 的「外国引用」用例用了 epoch 0**，断言 `ResourceOwnershipMismatch`
   （expected == actual == "mongodb"）或 `StaleRuntimeEpoch`。用 epoch 0 租借号是
   为了不与真实租约撞键；postgres/sqlite/mysql/redis 的同类用例用的是
   `ResourceHandle::issue("some-other-provider", ...)`（provider_id 不同 ⇒
   `ResourceOwnershipMismatch`）。四库更贴文档原意（「belongs to another resource」），
   mongodb 这条是退化形式。已确认它能守住顺序语义，但若裁决者希望严格一致，
   应改成 provider_id 不同的写法 —— 那会需要给 `test_support` 加一个 helper。
2. **vector / hbase 的外国引用用例**用的是 `unacquired_handle` + 第二个 provider
   实例（epoch 不同 ⇒ `StaleRuntimeEpoch`）。与 postgres 的
   `belongs_to` 拒绝（`InvalidResourceState`）不等价，但两者都在 `handle.check`
   这一层、且都在 `is_closed` 之前，守住的是同一条顺序。

## 修复轮 1计划（D-1 / D-2 / D-4）

- **D-1**：把 5 条顺序测试里传给 `close_handle` 的那个 handle 也 `mark_closed()`，
  让「已关」前提真的作用在被测引用上。hbase 是样板
  （`packages/drivers/hbase/src/resource/tests/unsupported.rs:326-348`）：
  传给 `close_handle` 的是已关的 `mine`/`handle`，foreign 只作对照组不进调用。
- **D-4**：9 处 `LeaseFacade` 全部改为 `LeaseManager`，并核对 fail-closed 的真实
  位置与行为后如实引用。

## D-2 实测结论：选 (a)，代价低于预估，且 `LiveResource` 根本不用改

先独立确认事实：`grep -rn "\.mark_closed()" packages/ src-tauri/src/ --include=*.rs | grep -v tests`
⇒ 生产命中只有 redis（`provider/contract.rs:358`）与 postgres（同构 `:347`）各 1 处。
MySQL/sqlite/mongodb/hbase/vector 的 `close_resource` 从不标记，`close_resource` 的**已关**判断
靠各自的 registry key 集合／`live` map，与 flag 无关 —— 所以它们的 `is_closed()`
恒 false，A-11 补的短路在生产永不触发（base 上即如此）。

**预估代价被高估了。** 上面写「要改 4 个 struct + 4 处构造 + 4 处调用」是错的：
`close_resource(&self, handle: &ResourceHandle)` **本来就把 handle 拿到手**，
mark 只需要在已确认关闭的分支里加一行，`LiveResource` 一行不动：

| 位置 | 落点 |
| --- | --- |
| `packages/drivers/mysql/src/resource.rs` | `close_resource` 的 `Ok(())` + 无残留事务分支 |
| `packages/drivers/sqlite/src/resource.rs` | 同 |
| `packages/drivers/mongodb/src/resource.rs` | `close_resource` 的 `Ok(())` 分支 |
| `packages/drivers/hbase/src/resource/provider.rs` | `close()` 的 `Closed` 分支（`closed` key set 之旁） |
| `packages/drivers/vector/src/resource/provider.rs` | 同 |
| `packages/driver-api/src/resource_adapter.rs` | `close_resource` 的 `Ok(Closed)` 分支 |

**放在哪条臂是有讲究的**（不是无脑 success 就 mark）：与 redis/postgres 完全一致，
只在**已确认关闭**时 mark —— `CloseUnconfirmed`（mysql/sqlite 有残留事务、hbase/vector/
adapter disconnect 失败）与 `Err` 臂一律**不**mark。否则未确认的关闭会被读成「已经关了」，
重试直接 `Ok`，而 permit 还挂着 —— 正是 `mark_closed` 文档警告的那个 bug。

顺带补了 3 条**生产可达性**测试把新 true 的行为焊死：mysql
（`close_releases_the_budget_exactly_once`）、hbase
（`closing_releases_the_charge_exactly_once_and_is_idempotent`）、adapter
（`an_already_closed_resource_makes_the_handle_close_a_success` 改为真 acquire +
真 `close_resource` + 真 `close_handle`，并断言 `disconnect_calls` 不增）。
前两条若无此改动必红。

## D-5 判定：做了一半，改了 3 处

mysql/sqlite/mongodb/redis 本来就有 `budget.released()` / wire 断言，缺的是
hbase/vector/adapter 三个。**已补**：

- hbase / vector 正向测试：加 `budget.released().is_empty()`（把测试从
  `opened()` 改成显式 acquire，才拿得到 ledger 引用）。
- adapter 正向测试：加 `disconnect_calls` 前后不变断言。

**没做 wire 侧**：hbase/vector 的 stub（stargate / qdrant）`close_resource` 之外
没有可观测的 wire 计数器，adapter 的 `LegacyFake::disconnect_calls` 是现成的所以补了。
budget 侧才是 A-11 这条 short-circuit 真正可能骗过人的地方（二次放行 permit）。

## D-1 放行条件的复验方式

把任一 impl 的 `is_closed` 短路提到 `handle.check` 之前，该 impl 的顺序测试必须
变红，7/7 全红才算修好。变异在独立 detached worktree 做
（`git worktree add --detach`），不碰本工作树。
