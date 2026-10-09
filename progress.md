# p3-live-port 进度台账（交付时销毁）

任务：`p3-live-port`。基线 `b1d7b32506ea42b6b37f528d8759562240e2dd6a`。

## 自证

### 已完成

- **任务 A：活连接出口端口**
  - `application/live.rs`（新增，532 行）：`LiveConnectionSource` / `LiveConnection` /
    `LiveConnectionLease` / `LiveConnectionLedger` / `LiveConnections` / `LivePurpose` /
    `LiveConnectionError` / `LiveRelease`。
  - 所有权契约写在模块头的四行表里（谁取得 / 取得的是什么 / 持有多久 / 谁归还）+ 四节正文。
  - `application/mod.rs:229` 固有方法 `borrow_live_connection`；`with_live_connections` 接线。
  - `application/operations.rs` 的 `close_session` 在 `directory.release` **之前**作废在外借用。
  - `application/convert.rs:live` 错误映射，消息恒为固定文案（不外传驱动报错文本）。
- **任务 B：空闲回收接线**
  - `application/idle.rs`（新增，221 行）：`IdlePolicy` / `IdleReclaimer` / `IdleReclamation`。
  - `application/mod.rs` 的 `OpenRequest.idle_deadline_ms` 由 `idle_deadline_ms()` 实算
    （原为硬编码 `None`）；`reclaim_idle()` 手动扫一次；`start_idle_reclaimer()` 起周期任务。
- **测试**：`packages/runtime/tests/p3_live_port.rs`（757 行，13 个集成测试）。

### 未开始 / 不做

- 不动 `src-tauri/**`、`registry/**`、`resource/**`、`lib.rs`、`docs/**`。
- 不给 `src/` 任何新出口。

## 门禁实测

- `cargo test -p datazen-runtime` → **EXIT=0**
  - `test result: ok. 496 passed; 0 failed; ...`（lib 单测）
  - `test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s`（`p3_live_port`）
  - 其余 37 个测试二进制全 `ok`，0 failed。
  - 唯一 3 条 `unused import` 警告来自 `packages/runtime/tests/job_kernel.rs`（既有文件，不在本轨范围）。

## 变异验证（硬门禁）

独立 detached worktree `.worktrees/p3lp-mutate`（`git worktree add --detach … 1673e0c93`），
独立 `CARGO_TARGET_DIR=/tmp/cargo-p3lp-mutate`；跑完已 `git checkout --` + `git worktree remove --force`。

| 变异 | 位置 | 前 | 后 |
| --- | --- | --- | --- |
| 基线（未变异） | — | `test result: ok. 13 passed; 0 failed; … finished in 0.68s`（EXIT=0） | 同 |
| M1 断掉「期限写进 `OpenRequest`」（等价回到硬编码 `None`） | `application/mod.rs::idle_deadline_ms` | ok 13/0 | `test result: FAILED. 10 passed; 3 failed; …`（EXIT=101）<br>FAILED：`期限开着时到期会话被回收_且四本账都干净`、`周期扫描器自己会驱动回收_宿主不必再挂定时器`、`回收会作废在外的借用_持有者拿到的是已作废引用` |
| M2 归还时不回台账（异常/取消/超时路径上账本只进不出） | `application/live.rs::LiveConnectionLease::return_inner` | ok 13/0 | `test result: FAILED. 7 passed; 6 failed; …`（EXIT=101）<br>含 `凭证被丢弃时兜底归还`、`超时打断持有者时兜底归还`、`正常归还后台账归零` |
| M3 关闭路径不作废在外借用 | `application/operations.rs::close_session` | ok 13/0 | `test result: FAILED. 11 passed; 2 failed; …`（EXIT=101）<br>FAILED：`关闭会话会作废在外的借用_且关闭后不留登记行`、`取消与未知效果路径关闭后不留登记行` |

三个变异都被测试咬住，且都咬在契约本身上（不是碰巧编译失败）。

## 遗留与待裁定项

1. `registry/actor.rs:696-706` 的 `set_idle_deadline` **没有** `SessionRegistry` 公有包装 ⇒
   空闲期限只能在登记时一次写死，逐会话的暂停/续期不可达。需要 registry 轨补一个包装方法。
2. §6.4「执行中暂停空闲检查」目前**只靠 FIFO 邮箱顺序**成立（`evict_idle` 走同一条队列，
   在飞的 `Execute` 不会被并发驱逐），`evict_idle` 本身没有显式的 executing 判定。
3. `evict_idle_at` 是全表扫描，回收不了子集。
4. `close_session` 在 `sessions` 表留**墓碑行**（`entry.closed`）以便重复关闭回放同一张回执，
   该行不会回收；`tracked_session_rows()` 因此只数 `closed == None` 的行。
   空闲回收路径则是直接 `remove()`、不留墓碑。
5. 生产代码不存在回收驱动器，原先只有测试在调 `evict_idle_at`；现在驱动器归应用层
   （`start_idle_reclaimer`），宿主无需改动，但「宿主是否要在应用启动时调用一次
   `start_idle_reclaimer`」是宿主侧的接线决定。
