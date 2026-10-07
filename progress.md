# P5 kernel-cancel 轨道进度台账

分支 `feature/p5-kernel-cancel`，基线 `a51e5e978`。台账所在工作区：`/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-p5-kernel-cancel`。
**交付并合并后本文件必须删除**（AGENTS.md：进度台账不得存活在 `main` 上）。

## 0. 首尾 HEAD / 工作区

| 时点 | HEAD | `git status --porcelain` |
| --- | --- | --- |
| 开工 | `a51e5e978` | 空 |
| 门禁前 | `b93c23817` | 空 |
| 收工 | 携带本文件的那次提交（`git log --oneline -1`） | 空 |

提交序列：`a51e5e978`（起点）→ `ccf2175a6` → `19850bbdf` → `db5b010c0` → `dcc710286` → `e6aa5faa5` → `b93c23817`。
`packages/data-transfer/**`、`packages/runtime/**` 之外的路径**一行未动**（`git status` 全程为空，且每次提交用 `git diff --name-only <base>..<head>` 核过）。

---

## 1. 三件事的自证完成情况

| # | 事项 | 状态 | 落点 |
| --- | --- | --- | --- |
| ① | `kernel_cancel.rs` 要真驱动 round-trip | 完成（**落点判断见 §2**） | `packages/data-transfer/src/job/tests/real_sqlite.rs`（新文件，492 行） |
| ② | 阶段内 panic 的看门dog 活性 | 完成（**协调员原话有误，见 §3.2**） | `packages/runtime/tests/job_cancel_watch/in_stage_panic.rs`（新文件，115 行） |
| ③ | `CANCEL_POLL_INTERVAL` 可测 | 完成（**取舍理由见 §4**） | `packages/runtime/tests/job_cancel_watch/poll_interval.rs`（新文件，145 行）+ `runtime.rs:41` 补文档、`job/mod.rs` 补 `pub use` |

外加一项**必要的加固**：`kernel_cancel.rs` 的 `RealRun.open_rows`（同连接回读），没有它 ① 的回滚断言是假的——见 §3.1 的两次存活记录。

---

## 2. 事项 ① 的落点判断（**先判断，后动手**）

### 2.1 结论：选**真实驱动 crate 进进程 + 真 SQLite 文件**，不要真实数据库服务

- **不用真实 Postgres/MySQL 服务**。仓库里没有为此准备的环境设施：驱动 live 套件靠 `.env.test` 里的 `DATABASE_URL`，而宿主契约把「依赖外部服务的 live 套件」单列成一类（`packages/drivers/redis/` 那条已知 flaky 就是这一类）。把 ① 的验收线挂在需要凭据的套件上，结果就是没有凭据的人（包括 CI）根本跑不了这条门禁——那是把「门禁存在」和「门禁有效」混为一谈。
- **用真驱动进进程**。被测对象是「pipeline 生成的 SQL」和「pipeline 施加的事务纪律」，这两件事 100% 落在 SQL 方言层和事务层。真 SQLite 把这张面全跑到：DDL、带参 INSERT、SELECT 投影 + ORDER BY/LIMIT、`BEGIN`/`COMMIT`/`ROLLBACK`、`max_bound_parameters` 批大小、标识符引号方言。而且零外部进程、零端口、零凭据、~1.2 s。
- **证据**：M1.1 / M1.2 / M1.3 / M1.4b 四个 mutant 都在真引擎上被杀——这四条在 FakeDb 上**全部不可能被杀**，因为 FakeDb 从不解析 SQL，并把 `quote_char()` 硬编码成 `"`、把提交/回滚伪造成 `Vec` push。
- 满足「不落回 fixture 式写法」：夹具连库文件都是真的（`TempDir` 里两个 `.sqlite` 文件，6 行真数据，3 个真批次）。

### 2.2 两处**必要偏离**，已登记为缺陷、不接受为设计

`GatedSqlite` 只偏离 `SqliteDriver` 两处，其余方法逐字转发：

1. `driver_type() -> "postgresql"`——`packages/data-transfer/src/resume.rs:101` 的 `supports_chunk_driver` 只放行 `postgresql | postgres | mysql`，SQLite 进不来；而且**卡两道**：`job/pipeline.rs:98` 目标侧闸门、`resume/fingerprint.rs:11` 用 `source_driver.driver_type()` 再查一次。
2. `begin_read_snapshot`——`packages/driver-api/src/traits.rs:646-653` 默认实现是 `Err(Unsupported(..))`，`SqliteDriver` 没有覆写，`job/pipeline.rs:166` 要求它才能拿一致性快照。

**这两条是产品缺陷，不是测试瑕疵**：`sqlite→sqlite` 的有界数据迁移在产品里根本走不通（见 §6 缺陷 2）。协调员需裁定：接受这个两处偏离作为 ① 的落点，还是先修 `resume.rs` / `begin_read_snapshot` 再让 ① 直连 SQLite。

---

## 3. 每个 mutant 的结果（**验收线 = 变异测试**）

变异在**独立 detached 验证工作树**里做（AGENTS.md：同一棵树不得同时被提交方和验证方使用），验证树路径记于 `/tmp/dz-mut-path.txt`，**收工前已 `git worktree remove`**。
判据：**撤销我新写的接线，测试必须 FAIL**；编译不过不算杀死；既有测试变红不算证伪我的新接线。

### 3.1 事项 ①

| ID | 变异点 | 变异内容 | 结果 |
| --- | --- | --- | --- |
| M1.1 | `pipeline.rs` `commit(tx)` | 换成恒 `Ok(())` | **KILLED**。只有不取消那条测试死：`EXIT=101`，`panicked at kernel_cancel.rs:428: assertion left == right failed: data stage failed on a real engine: None`，`1 passed; 1 failed` |
| M1.2 | `pipeline.rs` 取消分支 `rollback(tx)` | 换成恒 `Ok(())` | **KILLED（两次存活后修好）**，见 §3.1.1 |
| M1.3 | `execute_with_params(tgt, &sql, &params)` | 降级成 `execute(tgt, &sql)` | **KILLED**。两条都死：`0 passed; 2 failed` |
| M1.4b | `quote_ident_sql(col, source_quote)` | 去掉引号直接拼裸列名 | **KILLED**。两条都死：`0 passed; 2 failed`，`data stage failed on a real engine: None` |
| M1.4c | 同上 | 换成 `[col]` 方括号引号 | **SURVIVED**，`EXIT=0`，`2 passed` → **分类见 §5** |

#### 3.1.1 M1.2 的两次存活与定位（这条是本轨最实的一段）

- **第一次存活**（`EXIT=0`，`2 passed`）：取消路径上把 `rollback` 变成空操作，两条测试照样绿。
- **误判**：当时以为「`max_pool_size: 1` 下连接归还时未提交事务被丢弃，所以第二条连接分不出真回滚和没回滚」。据此加了 `open_transaction_rows`（**同连接**回读，因为未提交的行对**自己那条写连接**可见）。
- **第二次存活**（`EXIT=0`，`2 passed`，`finished in 1.23s`）：同连接回读**仍然分不出来**。
- **真因（实测，不是推断）**：`packages/drivers/sqlite/src/sqlite.rs:282-309` 的 `connect` **每次调用都新建一个 `SqlitePool` 并塞进 `pools` map，生成新的 `pool_id`**。我的探针为了拿句柄去 `connect` 了一次，于是读到的是**另一条连接**，本来就看不见第一条连接上没提交的事务。同连接是对的，但**我连的不是那条连接**。
- **修法**：`RealSqlite` 改为持有**交给 handler 的那个目标句柄的克隆**（`target_handle`，在 move 进 `TransferEndpoints` 之前克隆），探针直接 `query(&handle, ...)` 复用同一个 pool / 同一条连接。
- **修完再跑**：`[f12-rollbacknoop] EXIT=101`，只有取消那条死，断言消息 `on the very connection the batch was written on nothing survives; a rollback that never issued would still be holding those two rows`。**KILLED。**

**教训（已写进夹具注释）**：断言用的可观测量必须**真正连在**它声称要测的行为上。「换个连接去数」和「在同一条连接上数」都不是保证——后者还差一步：得是**那条**连接。

#### 3.1.2 夹具为什么改了列名（`id`/`name` → `id`/`order`）

M1.4 最初用 `[col]` 变异，`EXIT=0` 存活。第一反应是「SQLite 不认方括号」，**这是错的**——实测 SQLite 把 `[...]` 当 `"..."` 的等价引号形式，接受。真正的洞是：**`id` / `name` 是裸词，加不加引号 SQL 都成立**，所以「标识符引号」这条线在当时的夹具里根本没被碰到。

修法：把一列换成 SQL 保留字 `order`。于是——
- M1.4b（去引号）**被杀**（`SELECT id, order FROM ...` 是语法错）⇒ **引号接线现在是承重的**；
- M1.4c（方括号）**仍存活**，但原因换成了引擎语义（SQLite 认方括号），见 §5。

### 3.2 事项 ②

| ID | 变异点 | 结果 |
| --- | --- | --- |
| M2.1 | `CancelWatch::drop` 里删掉 `handle.abort()` | **KILLED，而且只有我这条新测试抓到**：`EXIT=101`，`11 passed; 1 failed`，唯一失败的是 `in_stage_panic::panic_inside_a_stage_still_converges_the_cancel_watchdog`。**C1–C6 那 11 条既有测试全绿**——这正是「既有测试变红不算证伪我的新接线」的反面：这条 mutant 既有测试根本抓不到 |
| M2.2 | `drop` 里删掉 `live.fetch_sub(1, SeqCst)` | **KILLED**（`5 passed; 7 failed`；其中 6 条是既有测试） |
| M2.3 | 整个 `impl Drop for CancelWatch` 删掉 | **KILLED**（`5 passed; 7 failed`；其中 6 条是既有测试） |

M2.1 的失败消息已经把「不能只看 `active_cancel_watchers()`」写进断言里：计数在 `Drop` 里**同步**归还，任务泄漏时它照样是 0，只有 `cancel_poll` 读次数会永远涨。

### 3.3 事项 ③

| ID | 变异 | 结果 |
| --- | --- | --- |
| M3.1 | 50 ms → 5 s | **KILLED**，`EXIT=101`，`poll_interval::poll_interval_keeps_the_host_cancel_contract_settle_window_honest` |
| M3.2 | 50 ms → 0 | **KILLED**，`EXIT=101`，`poll_interval::the_watchdog_actually_polls_at_cancel_poll_interval`，`finished in 0.00s` |
| M3.3 | 50 ms → 60 ms | **KILLED**，`EXIT=101`，同 M3.1 那条 |
| M3.4 | 50 ms → 40 ms | **SURVIVED**，`EXIT=0`，`2 passed` → **分类见 §5**，属**刻意单边** |

### 3.4 作废的 mutant（**列出来，且说明为何作废**）

| ID | 内容 | 作废原因 |
| --- | --- | --- |
| V1 | M1.4 最初形态（在保留字列加入之前跑 `[col]`） | **被取代**，不是通过。它在旧夹具上什么都没测到（裸词列名）；夹具改成保留字后重跑为 M1.4c，M1.4c 的存活是**引擎语义**导致，与夹具无关 |
| V2 | 「把 `open_rows` 探针整段删掉」 | 不算 mutant：那是撤掉**测试**，不是撤掉**接线**，跑它只会证明「没测试就抓不到」 |
| V3 | M1.2 在更早两个提交（`ccf2175a6` / `19850bbdf`）上的两次运行 | 不是作废，是 §3.1.1 的**诊断轨迹**：两次都 `EXIT=0`，正是它们把真因逼出来 |

**本次没有「编译不过所以不算杀死」的 mutant**：每个 mutant 都编译通过并真的跑了起来，`EXIT` 如实记录在上表。

---

## 4. 事项 ③ 的取舍与理由

**选「全派生」而不是 `assert_eq!(CANCEL_POLL_INTERVAL, Duration::from_millis(50))` 直接断言。**

- 直接断言只是把一个魔法字面量换成另一个魔法字面量：它把「50 ms」抄成第二份，任何一次合法调优都会让它变红，而它并不比原来那份**更难被误改**——两边都是硬编码，区别只是写在了不同文件里。
- 派生方案里，**所有测试依赖的时序常量都从 `CANCEL_POLL_INTERVAL` 算出来**：`QUIET_WINDOW = periods(8)`（3 个文件 10 处）、`WATCHER_SLACK = periods(24)`。interval 一改，这些自动跟着走。
- interval 本身不钉字面量，而是被**独立来源的事实**夹住：宿主契约 `src-tauri/src/commands/sync/jobs_cancel_contract.rs:60-64` 写的是「`CANCEL_POLL_INTERVAL`（50 ms）……`SETTLE = 500ms`」这一 500 ms 稳定窗口。测试断言的是「在 500 ms 落定窗口内至少能落 10 次轮询」这个**契约语义**，不是 50 这个数。

**关键机制（值得单独看）**：如果 interval 是 5 s，而 `QUIET_WINDOW` 还硬编码 400 ms，那么 400 ms 里**一次轮询都落不下**，安静窗口断言会**假绿**。派生 `QUIET_WINDOW` 把这种空洞非循环论证地消掉了。同样的派生随后也用在了 `WATCHER_SLACK` 上——M3.1 被杀就是这条派生的直接证据。

**为什么是单边的（这是 M3.4 存活的理由，不是漏洞）**：宿主契约给的是**上界**（别太慢），没给下界。比 50 ms 更快（40 ms）仍然满足契约，不算缺陷；给它钉下界等于把一个**性能目标**冒充成契约，让一次合法加速变成测试变红。0 有单独护栏（M3.2），因为 0 会退化成忙等。

---

## 5. 存活 mutant 的分类（**三选一，不许含糊**）

### M1.4c：引号改成 `[col]` —— **① 覆盖缺口（引擎特异，且我已把可关的部分关掉了）**

- 已证明的部分：M1.4b（完全去引号）**被杀**，所以引号接线本身是承重的、不是空断言。
- 存活的精确原因：**SQLite 把 `[...]` 视为 `"..."` 的等价标识符引号形式**，所以在 SQLite 上把引号换成方括号**不是缺陷**，自然不该被测试判死。
- 这不是「② 绑错符号」，也不是「③ 门禁构造性地区分不出」——换一个不接受方括号的引擎（PG/MySQL），同一条变异会立刻变成真错。要覆盖它需要第二个真实方言，不在 ① 的落点判断内。

### M3.4：interval 50 ms → 40 ms —— **① 覆盖缺口，且是刻意单边**

- 见 §4：契约只给上界。更快的轮询不违反契约，钉下界会把合法调优判成缺陷。
- **不打算关**。这是取舍的可见后果，不是遗漏。

### ③ 类（构造性区分不出）—— **本轨没有**

两项存活都归 ①。**没有**任何一条存活属于「门禁只能拿「真修好」和「换个绿灯」换」这一类：M1.2 的两次存活是真覆盖缺口（探针没连到正确的连接），已通过改正可观测量杀掉；M1.4c 与 M3.4 的存活是外部事实（引擎接受方括号 / 契约只有上界）。

---

## 6. 门禁命令与逐字结论行

全部在 **`b93c23817`**、工作区干净的状态下跑，长输出落系统临时目录，退出码单独打印。

```
$ cargo test -p datazen-data-transfer --all-targets
TRANSFER_EXIT=0
test result: ok. 218 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.01s

$ cargo test -p datazen-runtime --all-targets
RUNTIME_EXIT=0
     Running tests/job_cancel_watch.rs (target/debug/deps/job_cancel_watch-1db46542791100af)
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.06s
（`grep -c '^test result: FAILED'` → 0）
```

```
$ cargo clippy -p datazen-data-transfer -p datazen-runtime --all-targets --message-format=json
CLIPPY_EXIT=0
REASON_HISTOGRAM = {'compiler-artifact': 401, 'build-script-executed': 29, 'compiler-message': 256, 'build-finished': 1}
```
先出 `reason` 直方图再筛诊断：`compiler-message` 共 256 条（**是实测到的 0/非 0，不是没看见**），其中落在这两个包的 204 条**绝大多数是仓库既有噪声**。按文件收窄到本轨新写/改的两个文件：
```
DIAGNOSTICS_IN_MY_TWO_FILES = 2
  ('warning', 'clippy::needless_borrow', 'real_sqlite.rs', 444, '...')
  ('warning', 'dead_code', 'kernel_cancel.rs', 331, 'field `source_path` is never read')
```
两条都已修（提交 `b93c23817`），复测：
```
DIAGNOSTICS_IN_MY_TWO_FILES = 0
```

**格式**：全仓 `rustfmt --check` 在 **HEAD 就已经不干净**（实测 10 处、分布在三个我没碰的文件里），所以只按文件判、并归因到 HEAD：
```
$ rustfmt --edition 2021 --check packages/data-transfer/src/job/tests/real_sqlite.rs
RS_FMT=0
$ rustfmt --edition 2021 --check packages/data-transfer/src/job/tests/kernel_cancel.rs
KC_FMT=1，仅 :45 :53 :243 :261 四处 —— 与 HEAD 自带的 :45 :53 :222 :240 一一对应，新增偏离 0
```
**未跑** `cargo fmt --all`（禁），**未跑** `scripts/resolve-drivers.mjs`（禁）。验证树缺 gitignored 的 `src-tauri/src/driver_init.rs`，全仓格式扫描在那边会被**跳过**（扫描被截断看起来像更干净），所以根本没做全仓扫描。

**`.env` / `.env.test` 内容全程未进入上下文**；本次新增测试是纯进程内的 SQLite 文件，**不读任何 env**。

**文件规模**（限 800 行）：`real_sqlite.rs` 492、`kernel_cancel.rs` 481、`poll_interval.rs` 145、`in_stage_panic.rs` 115、`runtime.rs` 415。

**并发轨干扰**：`packages/drivers/redis/` 的 `connect::tests::live_prefer_falls_back_to_plaintext_and_require_refuses` 是已知 flaky，与本轨无关；本轨未改动 `packages/drivers/**`。本次全部测试跑在 `packages/data-transfer` 与 `packages/runtime`，`EXIT=0`，无 flaky 迹象。

---

## 7. 协调员描述与代码事实冲突之处（**代码为准**）

1. **事项 ② 的原话「watchdog still converges the terminal state」是错的。**
   `packages/runtime/src/job/` 里**没有任何叫 watchdog 的符号**，实际是 `watch_cancel_request` + `CancelWatch`。
   更要紧的是：阶段 panic 会从 `dispatch`（`runtime.rs:208-210`）**展开出去**，而终态 CAS 块在 `runtime.rs:248-272`，**在它后面**。所以 **没有任何东西把这个 job 收敛到终态，记录会卡在 `Running`**。代码里唯一被保证并写进文档的性质（`runtime.rs:280-283`）只是**「不会一直跑下去」**。
   所以我的测试断言的是**最终收敛（读次数不再增长）**，**不是**钉死某个终态；再加一条 `assert_ne!(record.view.state, JobState::Succeeded)` 堵住「一路冲成成功」。
2. **`kernel_cancel.rs` 不是 fixtures-only**，协调员此前的说法已纠正并确认。
3. **事项 ③ 的描述属实**（`runtime.rs:41` 定义、只有一处使用、既有测试隐式依赖它）；协调员补充的 `WATCHER_SLACK=1200ms` / `QUIET_WINDOW=400ms` 两处硬编码属实，本轨已按 §4 全部改为派生。

---

## 8. 遗留缺陷（**不在本轨修**，登记为下游输入）

1. `SqliteDriver` 继承 `max_bound_parameters() == 60_000`，而 SQLite 真实上限 `SQLITE_MAX_VARIABLE_NUMBER` 是 32766。归 `packages/drivers/**` 的权。（本轨已按 `job_for_pipeline(2)` 小批跑：6 行 = 3 批。）
2. **`resume::supports_chunk_driver` 硬编码 `postgresql|postgres|mysql`**，导致 `sqlite→sqlite` 的有界数据迁移在产品里不可达；且源侧闸门读的是 `source_driver.driver_type()` 而非端点标签，再叠上 `begin_read_snapshot` 默认 `Unsupported` 且 `SqliteDriver` 未覆写。归 `packages/data-transfer/src/resume.rs` + `packages/driver-api`。
3. `SqliteDriver::connect` 无法创建缺失的库文件（sqlx 默认 `create_if_missing: false`，报 code 14 `SQLITE_CANTOPEN`）。归 `packages/drivers/sqlite`。
4. （越界，仅登记）`JobResult.error` 在 `runtime.rs:271` 被**硬编码成 `None`**。M1.1 的失败文本 `data stage failed on a real engine: None` 就是它——**失败原因不可观测**。归 `packages/runtime`（不在本轨改动范围外的文件里，但不在本轨职责内）。
5. ~~`sqlx` 池归还语义可能让 no-op ROLLBACK 不可观测~~ —— **已证伪并撤回**。真因见 §3.1.1（是 `connect` 每次新建 pool，不是池归还语义）。

---

## 9. 待协调员裁定

- **a. 事项 ② 的产品决定**：panic 的 job 目前会**卡死在 `Running`**。要不要加 `catch_unwind` + 收敛成 `Failed`，加在哪一层？（跨 `packages/runtime` 与宿主边界，不在本轨授权内。）
- **b. 事项 ① 的两处偏离**（§2.2）：接受 `GatedSqlite` 装饰器作为落点，还是先修 `resume.rs` / `begin_read_snapshot` 让 ① 直连 SQLite？两处偏离**必须**被登记成缺陷而不是默默接受。
- **c. 事项 ③ 的上界钉在宿主契约上**：接受「`CANCEL_POLL_INTERVAL × 10 ≤ 500 ms`」这个前提（`jobs_cancel_contract.rs:60-64`），还是要重新谈这个契约？
- **d. 缺陷 1**（`SqliteDriver::max_bound_parameters`）已另行派轨，请确认路由。
- **e. 本轨未合并**：`feature/p5-kernel-cancel` 原样留在原地待验收，**没有**合进集成分支，**没有**删 `progress.md`。请勿让本文件存活到 `main`。

---

## 10. 收工记录

- 变异验证用的 detached 工作树（系统 temp 目录）**已 `git worktree remove`**，`git worktree list` 里已无残留；验证用完即删，不入库。
- 所有临时日志都在系统 temp 目录，**仓库内无任何未跟踪文件**（`git status --porcelain` 为空）。
- 本分支**未合入集成分支**，feature 分支与本工作树**原样保留待验收**。
- `hub.md` 全程只读，未修改。

