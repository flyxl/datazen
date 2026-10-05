# progress.md — P3 `p3-cm60-pressure-drain`（CM-60 压力与 drain）

交付即销毁：验收合并后本文件必须删除。

fork 点 `7fa6630f041fa8fef127be7df3b8da2bd3420f4d`，分支 `feature/p3-cm60-pressure-drain`。

## 目标

把退出门禁 CM-60 从 PARTIAL 推到 COVERED。判据（`connection-management.md:1235-1239`）分两半：

- **A 半（行为）**：资源压力与 drain 的确定性行为。**已落地**（提交 1）。
- **B 半（性能）**：`--release` 基准、非排队网关**附加耗时** p95 ≤ 10 ms。**已落地且可运行**（提交 2–5）。
  仓库此前**一个可运行的基准都没有**，本轨新建了基准入口。

## 提交台账

| # | 提交 | 内容 |
|---|------|------|
| 1 | `2bef5bee1` | A 半：资源压力与 drain 的确定性行为测试（6 个用例） |
| 2 | `37526e084` | B 半：基准入口 `src/bin/cm60-bench`（7 个文件） |
| 3 | `a10cfa800` | 修幂等键复用与冻结时间冲突 |
| 4 | `4bd5a55c5` | 文档归因：把失效引用改回事实 |
| 5 | `04a55fa72` | **修退出码方向**，并用测试钉死 0/1/3 |

累计 15 文件 / +3981 / −8。

## B 半命令与实测

```bash
DZ_CM60_RUSTC_VERSION="$(rustc --version)" CARGO_TARGET_DIR=/tmp/dz-target-p3-cm60-pressure-drain \
  cargo run --release -p datazen-runtime --bin cm60-bench -- --vcpus 8 --mem-bytes 17179869184
```

`BENCH_EXIT=0`，全程真实墙钟 96942.849 ms。逐轮（门禁 10 ms）：

```
预热: N=1000 p50=0.007 p90=0.009 p95=0.010 p99=0.017 max=0.386 失败=0 排队=0
第 1 轮: N=10000 p50=0.007 p90=0.010 p95=0.011 p99=0.016 max=3.049 失败=0 排队=0
第 2 轮: N=10000 p50=0.008 p90=0.010 p95=0.011 p99=0.017 max=5.741 失败=0 排队=0
第 3 轮: N=10000 p50=0.008 p90=0.010 p95=0.011 p99=0.018 max=9.230 失败=0 排队=0
第 4 轮: N=10000 p50=0.008 p90=0.010 p95=0.011 p99=0.028 max=1.007 失败=0 排队=0
第 5 轮: N=10000 p50=0.008 p90=0.010 p95=0.011 p99=0.030 max=0.556 失败=0 排队=0
journal：受理 51000 / 派发 51000 / 完成 51000 / 收尾未完成 0 / 执行记录留存 51000
事件投影：applied 0 / 重复 0 / 乱序 0 / 重复块 0 / 丢失 0 / 未绑定 0 / 外来 0
driver 往返（含 fake 虚拟耗时）：样本 51000 / p95 16.802 ms / 最大 29.652 ms / 中位 15.328 ms
```

产物 `target/bench/cm60-raw-1791128233852.json` + `target/bench/cm60-summary-1791128233852.json`。
summary 的 `verdict`：`gate_passed=true`、`conforms_to_spec=true`、`rounds_passing=5`、
`failures_total=0`、`event_projection_clean=true`、`sample_counts_match=true`。
raw 的 `raw` 段 50000 条（5×10000，预热不进样本），`environment`+`build`+`raw` 三段足以复算。

**口径要紧：p95 远小于门禁不是测错了量纲。** 判据量的就是「附加耗时」，§11.2 表格写明登记段
起点是「driver 返回完成通知」，并逐字规定「fake 命令的 10 毫秒……**因此不进入测量窗口**」；
`connection-management.md:857` 同样写「**不包含 fake SQL**、预算/actor 排队、网络传输」。
窗口由网关自身的探针确定（`gateway/mod.rs:286` 打段①起点，`:360` 打段①终点且**紧贴**
`execute_in_session` 调用之前，`:369` 打段②起点，`:373` 打段②终点），fake 的 10 毫秒落在
`:363` 那个 `.await` **内部**，即两段之间的未测间隙。这是库代码的结构，不是基准的选择。

## 门禁

| 门禁 | 退出码 | 逐字结论行 |
|---|---|---|
| `cargo fmt --check -p datazen-runtime` | 0 | 无输出 |
| `cargo build -p datazen-runtime` | 0 | 0 warning |
| `cargo test -p datazen-runtime --lib` ×3 | 0/0/0 | `test result: ok. 382 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` |
| `cargo test -p datazen-runtime --bin cm60-bench` | 0 | `test result: ok. 46 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` |
| `cargo test -p datazen-runtime --test cm60_pressure_drain` | 0 | `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` |
| 其余 18 个 tests/ 二进制逐个 | 全 0 | 逐条见下 |

逐个二进制（`--test <name>`，全部 `test result: ok`）：
budget_cm65_lifecycle 8、budget_cm65_reservation 2、budget_cm65_scheduling 6、budget_cm66 7、
cm60_pressure_drain 6、directory_attachment_ttl 12、directory_no_disk 6、directory_ownership 7、
directory_replacement 10、gateway_contract 51、p3_session_port_contract 10、registry_audit 11、
registry_cancel 9、registry_execution 6、registry_lifecycle 7、registry_release 11、
resource_replacement 4、resource_return_to_pool 7、resource_rotation_and_disable 6。

`--lib` 跑了 **3 次全绿**（不是重跑到绿为止；三次都是首过）。
`cargo test --tests` 聚合会触发 `error[E0382]`，故逐个二进制跑。

## 睡眠模式增量

全树扫描 `packages/runtime/{src,tests}`：`sleep(` 共 6 处，其中基准 3 处
（`driver.rs:10` 文档、`main.rs:45` 文档、**`driver.rs:294` 唯一真实调用点**，
跑在 `tokio::time::pause()` 的虚拟时间上，消耗真实时间 0）。
`Duration::from_millis` 共 11 处，基准 6 处，生产两处是 `plan.rs:26`（`SPEC_FAKE_COMMAND`）
与 `main.rs:304`（`--fake-ms` 解析），两者都是虚拟 sleep 的参数。
`Duration::from_secs` 50 处、`yield_now` 7 处、`interval(` 0 处，**基准内均为 0**。

**净增量：真实 `sleep(` 调用点 +1（虚拟时间），生产 `Duration::from_millis` 取值点 +2（均为虚拟时间参数）。
新增真实等待 0。**

## 文档归因 old → new

- `fake-runtime-fixtures.md:71`：`| src/bench/ | … | 未实现 |` → `| src/bin/cm60-bench/ | CM-60 基准入口（§11） | 已实现：独立的 bin target，不在 testing/ 下（§11.6 要求基准不得与功能测试共用入口） |`
- `fake-runtime-fixtures.md:580`：`target/bench/cm60-raw-<ts>.json`（由 `bench.rs` 输出）→ （由 `src/bin/cm60-bench` 输出）
- `fake-runtime-fixtures.md:619`：`cargo run --release -p datazen-runtime --bin cm60-bench` 从「仍未创建、当前不可运行」表移入 §12「已落地、可立即运行」
- `connection/testing/mod.rs:11-14`：改写为「§2 表列了 `bench.rs`，但 §11.6 要求基准不得与功能测试共用二进制入口，该基准已落在 `src/bin/cm60-bench/`，故 `testing/` 下永远不建 `bench.rs`」
- `latency.rs:1-4`：不再声称 CM-60 基准尚未落地，补「跑基准的是 `src/bin/cm60-bench`」
- `connection-management.md` §15.1/§15.3：补三条真实命令（`fake-runtime-fixtures.md:627` 要求同步）

## 未触碰

`connection/port.rs`（冻结）、`hub.md`、`connection/testing/barrier/mod.rs:41`
（`BLOCK_REAL_TIME_BUDGET` = 30 s，`git diff` 对该目录为空）。

## 遗留与待裁定

1. **是否给 `cm60-bench` 加专用 CI job**（跑 `--release` 基准 + 上传 `target/bench/*.json` 作 artifact，§11.5/§15.3 要求产物留存）。
   `pnpm test:platform-crates` 已覆盖 `cargo test -p datazen-runtime`，缺的只有基准 job 与产物上传。
   **本轨不自行决定**，交协调者裁定。
   **这个缺陷反而支持加 job**：若 CI 早于本次修复采用原始退出码，每次全绿都会被读成失败。
2. **未在判据指定的 4 vCPU / 8 GiB 复测**（实测机 8 vCPU / 17179869184 字节）。故不得作「按判据达标」的结论。
3. **单线程运行时**：8 路并发共享一个线程（`pause()` 在多线程运行时上 panic），测得的 p95 是**单核下界**，
   不含跨核竞争与跨核缓存争用。已在产物 `environment.runtime_threads` 与 notes 中逐字披露。
4. **排队数为 0 是「当前实现如此」，不是「结构上永久不可能」**：`QueueFull` 只存在于 `budget/*` 与
   `connection/error.rs`，`ExecutionGateway` 当前没有预算台账字段，`SessionPort` 也没有入队参数，
   故该路径当前类型上不可达。若日后把台账接进网关，这一支会被真正触发，届时等待分位数不再是空样本。

---

# 第二轮（ROUND 2）：Tester 判 FAIL 后的修复

Tester 报告：`/tmp/p3_cm60/TA_R1_REPORT_cm60.md`（F-01 阻塞、F-05 阻塞，其余 F-02/03/04/06/07/08/09/10）。
本轮 HEAD = `48d194300`。**下面第一节的门禁表是 ROUND 1 的实测，`--lib 382` / `--bin 46`
这两个数已被本轮取代**，以本节为准。

## 第二轮提交台账（一条 F 一个提交，粒度宁细勿粗）

| # | 提交 | 对应 | 内容 |
|---|------|------|------|
| 6 | `1289231aa` | F-01 | N 取获准且未排队的请求（含随后失败/超时/取消）；打不出两段真实时长的请求计入显式 `unmeasured_failures`，不为 0 则该轮判红。根因在 `gateway/mod.rs:364`：驱动失败时 `:376` 的 `samples.push` 根本没执行，量出来的网关段被丢弃 |
| 7 | `05ca8c3a9` | F-05 | CI 门禁接入两个入口：`run-platform-crate-tests.mjs` 的 `EXTRA_TARGETS` 带 release 标记并拆成多次 cargo 调用；`ci.yml` 增 `CM-60 latency benchmark (release)` 与 `if: always()` 的产物上传 |
| 8 | `192545745` | F-04 | 分位数的**真实 harness** 守门测试；输入条数单列为 `percentile_input`，`sample_count_mismatch()` 第 6 条交叉核对 |
| 9 | `07f30c583` | F-02 | 按原因的拒绝/失败分类必须落进产物（序列化早有，测试没有；冻结产物证明旧树从没落过盘） |
| 10 | `c4b1f9a2b` | F-07 | §11 两半都已落地的状态行改回事实（三处） |
| 11 | `2143a8d95` | F-08 | 夹具单测用例数按实测改成 383 |
| 12 | `48d194300` | F-03 | §11.3 口径写明「N 的定义」与「分位数输入条数」是两回事，两条独立记账 |

`git log -1 --format=%B | grep -cE '^(APPEND_EOF|EOF|MSG)$'` 每个提交均为 **0**（零 heredoc）。

## §3.2 复算证明：冻结 raw 喂进修好后的判定逻辑

**没有重跑基准。** 冻结产物 `target/bench/cm60-raw-1791128233852.json` +
`cm60-summary-1791128233852.json`（树 `04a55fa72`，`BENCH_EXIT=0`，96942.849 ms）未被改动，
产物目录里也没多出任何文件。做法：一次性临时模块（**跑完即删，未入库**）把 raw 的 50000 条
样本按 `round` 分组 → `total_nanos` → `Percentiles::of` → `RoundOutcome` → `BenchRun::verdict()`。

```
REPLAY gate_passed=true rounds_passing=5/5
REPLAY worst_round_p95_nanos=Some(11459) failures_total=0
REPLAY admitted_total=50000 measured_total=50000 unmeasured_failures_total=0
REPLAY sample_counts_match=true event_projection_clean=true
REPLAY round=1 N=10000 实测=10000 未测出=0 分位数输入=10000 p95=Some(10500)
REPLAY round=2 N=10000 实测=10000 未测出=0 分位数输入=10000 p95=Some(10959)
REPLAY round=3 N=10000 实测=10000 未测出=0 分位数输入=10000 p95=Some(11041)
REPLAY round=4 N=10000 实测=10000 未测出=0 分位数输入=10000 p95=Some(11458)
REPLAY round=5 N=10000 实测=10000 未测出=0 分位数输入=10000 p95=Some(11459)
REPLAY_EXIT=0   (3 passed; 0 failed; 56 filtered out)
```

逐轮复算的 p95 与冻结 summary 里当时写下的 10500/10959/11041/11458/11459 **逐个相等**，
新加的第 6 条恒等式（`percentile_input == measured`）在每轮都成立，缺口计数为 0。
**结论：新口径不改变这份数据的原判定。**

反向对照（证明上面那句话不是恒真）：同一份 raw，只把每轮改成「N 里有一个获准请求打不出
两段时长」，判定立刻翻红 `NEG rounds_passing=0/5 unmeasured_failures_total=5`。

复算里除 raw 外的数分两类，都不是推断或构造，但来源不同，不能一概称「照抄」：
journal、warmup 墙钟、逐轮 wall_time 确实逐个照抄冻结 summary；而
`unmeasured_failures_total=0` 与 `percentile_input=10000` 这两列**在 commit `04a55fa72`
冻结的那份 summary 里根本不存在**——在那一版上对 `packages/runtime/src/bin/cm60-bench/`
整个目录 `git grep -c`，两个名字都是 0 命中。它们的值是从 `raw[]`
按当轮定义直接算出来的——结论相同，但「照抄」这个说法对这两列是假的，现予收窄。
（两者首次进入产物的提交已用 `git log -S` 逐个查得并记下哈希：`unmeasured_failures_total`
在 `outcome.rs` 首次出现于 `1289231aa`，`percentile_input` 首次出现于 `192545745`。
这里**只给哈希、不给「第几个提交」**——序号取决于和哪条线求 merge-base，换个基点结论就变，
把序号写进台账等于把一个随基点漂移的数当成事实。）
预热样本不进 raw，所以预热分位数复算不出来，就留空并在代码里注明预热不参与任何 p95
——这是诚实缺口，不是编数。

## F-01 自证（注入失败臂，不写进产物）

`/tmp/p3_cm60/selftest.sh`，`BUILD_EXIT=0`：

- 正向 `--vcpus 4 --mem-bytes 8589934592 --inject-failure-every 100`：`POS_EXIT=1`
  （103 s）。每轮 `N=10000 测到=9900 未测出=100 失败=100 占比=0.0100 排队=0`；
  `gate_passed=false`、`rounds_passing=0`、`failures_total=510`、`admitted_total=50000`、
  `measured_total=49500`、`unmeasured_failures_total=500`、`worst_round_p95_nanos=20209`；
  `rounds[0].rejections = {runtime:invariantBroken: 100}`。**失败的请求留在样本集里，
  缺口单列，没有任何一条被用构造值补齐。**
- 反向对照（不注入）：`NEG_EXIT=0`（100 s），`gate_passed=true`、`failures_total=0`、
  `admitted_total=measured_total=50000`、`unmeasured_failures_total=0`。
  即失败注入一开一关两臂都能区分，不是「恒红」也不是「恒绿」。

## 第二轮门禁实测（HEAD `48d194300`，运行前后 HEAD 与工作区均未变）

| 门禁 | 退出码 | 逐字结论行 |
|---|---|---|
| `cargo fmt --check -p datazen-runtime` | 0 | 无输出 |
| `cargo test -p datazen-runtime`（全 crate：lib + bin + 22 个 tests/ + doc） | 0 | lib `ok. 383 passed`；bin `ok. 57 passed`；其余逐个见下 |
| `cargo test --release -p datazen-runtime --bin cm60-bench`（CI 用的那一次） | 0 | `test result: ok. 57 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` |
| `tsc -p tsconfig.scripts.json --noEmit` | 0 | 无输出 |
| `vitest run scripts/__tests__/run-platform-crate-tests.test.ts` | 0 | `Test Files 1 passed (1)` / `Tests 18 passed (18)` |
| `vitest run scripts/__tests__`（全量） | 0 | `Test Files 40 passed (40)` / `Tests 679 passed (679)` |
| `node scripts/run-platform-crate-tests.mjs --dry-run` | 0 | `cargo test --lib -p datazen-runtime -p datazen-application -p datazen-platform-api --test cm60_pressure_drain` + `cargo test --release -p datazen-runtime --bin cm60-bench`（2 次调用） |

tests/ 逐个二进制：cm60_pressure_drain 6、gateway_contract 51、directory_attachment_ttl 12、
registry_lifecycle 7、registry_release 11、registry_audit 11、registry_execution 6、registry_cancel 9、
budget_cm65_lifecycle 8、budget_cm65_reservation 2、budget_cm65_scheduling 6、budget_cm66 7、
directory_no_disk 6、directory_ownership 7、directory_replacement 10、p3_session_port_contract 10、
resource_replacement 4、resource_return_to_pool 7、resource_rotation_and_disable 6、doc-tests 0。

## 交付前最后一次确认（HEAD `3ed3ad0db`，运行前后 HEAD 与工作区均未变）

| 门禁 | 退出码 | 逐字结论行 |
|---|---|---|
| `cargo test -p datazen-runtime`（全 crate） | 0 | 22 行 `test result: ok`，`TOTAL_PASSED=626`，0 failed |
| `cargo test --release -p datazen-runtime --bin cm60-bench`（CI 的那一次） | 0 | `test result: ok. 57 passed; 0 failed` |
| `cargo fmt --check -p datazen-runtime` | 0 | 无输出 |
| `tsc -p tsconfig.scripts.json --noEmit` | 0 | 无输出 |
| `node scripts/run-platform-crate-tests.mjs --dry-run` | 0 | `3 crate(s) selected, 2 cargo invocation(s)`，两条 argv 见上表 |
| `./node_modules/.bin/vitest run scripts/__tests__`（全量） | **1** | `Test Files 1 failed \| 39 passed (40)` / `Tests 4 failed \| 675 passed (679)` —— 4 例全在 `check-platform-crate-boundaries.test.ts`，原因已定位，见下「本轮新见」第 4 条 |
| `node scripts/check-platform-crate-boundaries.mjs`（单跑） | 0 | `PASS — 22 workspace member(s) classified, … 0 violation(s), 0 error(s), 3 advisory(ies)` |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p datazen-runtime` | 101 | 38 行既有错误，`CM60_DOC_ERRORS=0`（本轨的 bin 一条没有；该命令在本仓不是可用门禁，见「本轮新见」） |

工作区在最后一次全量运行之后仍是 `DIRTY=0`，`HEAD_AFTER=3ed3ad0db…`、`TREE_AFTER=cdf28cd55ecb…`。

同一份代码在 `f00cfb3ba` 上把全量 vitest 又跑了一遍：`VITEST_EXIT=0`、
`Test Files 40 passed (40)` / `Tests 679 passed (679)`、失败用例数 0，
`HEAD_BEFORE=HEAD_AFTER=f00cfb3ba…`、`DIRTY_AFTER=0`。与 `3ed3ad0db` 那次红相比，两次之间唯一的
提交是 `progress.md`，而没有任何门禁读它——**同一份代码一次绿一次红**。这既是「单次全绿不是门禁」的
实例，也是上面那条共享树竞态的第二个独立佐证。

## 本轮新见（不是 Tester 给的清单，是扫出来的）

1. **诚实记账的缺口不是「样本数错乱」。** 反向对照第一次写成 `assert!(sample_count_mismatch())`
   直接失败：`dispatch_failed` 与 `unmeasured_failures` 同时记 1 时六条恒等式全部成立。
   缺口的正确归宿是「这一轮判红」，不是「账本不自洽」。把两者混为一谈，反向对照就证不出东西。
2. **`sample_count_mismatch()` 与 `passes_gate()` 判的不是一回事**：前者抓说谎，后者抓真失败。
   文档与测试断言必须分清，否则会把一个正确的失败判成记账 bug。
3. **`connection/testing/barrier/tests.rs:236` 有自发的 30 s 竞态**（`await_drain_wakes_when_the_clock_crosses_the_deadline`）：
   首次全量 `--lib` 实测 `382 passed; 1 failed`（该例 30.02 s 超时后失败），紧接着第二次全量
   `383 passed; 0 failed`，单独跑 5 次全过。机制：`spawn` 与 `clock.advance` 之间没有握手，
   线程还没登记期限，advance 就过去了；失败信息自己写着「FakeClock 的单调时刻没有推进」。
   **该目录本轨禁碰，未修**（见「未触碰」），登记给它的属主。
4. **`check-platform-crate-boundaries.test.ts` 的红灯不是「外部修好了」，是脚本测试自己共享真实树造成的竞态。
   此处更正上一版的结论**：上一版写的是「本轮全量 `679 passed`，该文件 53 例全过，红灯现已不存在、
   不是本轨修的」。HEAD `3ed3ad0db` 又回到 `Test Files 1 failed | 39 passed (40)`、
   `Tests 4 failed | 675 passed (679)`，四次失败全在该文件。**一次全绿不是门禁，机制已定位且可确定性复现：**
   - `scripts/__tests__/check-module-layers.test.ts:347` 把探针种在**真实仓库树**里：
     `CLIENT_PROBE = 'packages/backend-client/src/__boundaryProbe__.ts'`，由 `clientVerdict()`（`:390`）
     经 `withTempSourceFile` 写入；`git check-ignore` 对该路径返回 0，所以它**对 `git status` 不可见**，
     事后从工作区状态看不出它存在过。`packages/ui/src/__boundaryProbe__.ts`（同文件 `:252`）同理。
   - `scripts/__tests__/check-platform-crate-boundaries.test.ts:30` 与 `:98-100` 把真实仓库根
     `REPO_ROOT` 传进 `checkPlatformCrateBoundaries({ root: REPO_ROOT, metadata, specText })`，
     而 F-07 的规则（`scripts/check-platform-crate-boundaries.mjs:172-177`）读的正是
     `<root>/packages/backend-client/src` 下的**真实源码**。于是两个 vitest worker 文件共享同一份
     可变文件系统状态：一个种探针的窗口内，另一个文件里所有「真实仓库是干净的」断言都不成立。

   确定性复现（HEAD `3ed3ad0db`；除临时探针外工作区全程未动）：

   ```bash
   printf 'import { invoke } from "@tauri-apps/api/core";\nexport const go = () => invoke("x");\n' \
     > packages/backend-client/src/__boundaryProbe__.ts
   node scripts/check-platform-crate-boundaries.mjs;   # EXIT=1
   #   VIOLATION F-07  packages/backend-client/src:1 contains `@tauri-apps/`
   #   FAIL — … 1 violation(s), 0 error(s), 3 advisory(ies)
   ./node_modules/.bin/vitest run scripts/__tests__/check-platform-crate-boundaries.test.ts
   #   EXIT=1   Tests  6 failed | 47 passed (53)
   rm packages/backend-client/src/__boundaryProbe__.ts;  # DIRTY=0
   ```

   撤掉探针后同两条命令分别 `EXIT=0`（53/53）与 `PASS — 22 workspace member(s) … 0 violation(s),
   0 error(s), 3 advisory(ies)`；两个文件同时跑（96 例）也是全过——全量并行时只红 4 例，因为种探针的
   时间窗只覆盖到该文件的一部分用例，所以现象看着像随机。**本轨未修**：`scripts/check-platform-crate-boundaries.mjs`
   与这两个测试文件都在本轨禁碰清单里；修法（探针改到临时树，或让两个文件共用一把跨文件锁 / 串行）
   属该脚本属主。
5. **同一共享树的第二处更危险**：`scripts/platform-arch-selfcheck.mjs:397-410`（M7）**覆写被跟踪的
   `packages/backend-client/src/index.ts`**，只靠 snapshot 在 revert 时写回。探针那种写法被 `.gitignore`
   兜住了（`git add` 取不到），这一处不是：进程被中断就会把一个**受版本控制的源文件**留在变异态，
   而 `git status` 只会在事后显示它「被修改过」。本轨没跑过该脚本（`pnpm` 在本 worktree 不可用），
   未修，登记给属主。

## §5 队列等待分位数：本轨未修，归属已登记

Tester F-10 要求修「排队请求的等待分位数」。协调者裁定：**本轨不做**。理由与现状：

- `ExecutionGateway` 当前**类型上不可达**排队：`QueueFull` 只存在于 `budget/*` 与
  `connection/error.rs`，网关没有预算台账字段，`SessionPort` 也没有入队参数，
  `AcceptanceDisposition` 只有 `Accepted | Replayed`，故网关路径的 queued 恒为 0。
- 现在去实现「排队等待分位数」等于为一个不可达分支造口径，属于凭空加接口。
- 归属已登记给拥有网关预算台账的那条轨（台账：见 `hub.md` 的跨轨登记）。
- 因此 `queued_wait` 这一列现在只在结构上存在（空样本），**不是已实现的功能**。

## 未触碰

`connection/port.rs`（冻结）、`hub.md`、`connection/testing/barrier/`（`mod.rs:41` 的
`BLOCK_REAL_TIME_BUDGET` = 30 s）、`scripts/check-platform-crate-boundaries.mjs`、
`scripts/__tests__/check-platform-crate-boundaries.test.ts`、`scripts/__tests__/check-module-layers.test.ts`、
`scripts/platform-arch-selfcheck.mjs`、
`target/bench/cm60-*.json`（冻结产物未被新增、修改或删除）。

## 遗留与待裁定（第二轮）

1. **§5 队列等待分位数** —— 本轨未修，归属见上。合并前需确认该轨已登记。
2. **未在判据指定的 4 vCPU / 8 GiB 复测**（实测机 8 vCPU / 17179869184 字节，单线程运行时）。
   协调者已撤回以硬件复测为本轨的交付条件；本轨因此**不得作任何性能达标结论**。
3. `barrier/tests.rs:236` 的自发竞态 —— 属主待裁定（见上「本轮新见」第 3 条）。
4. CI 的 CM-60 基准步会在每次 rust job 真跑一次 `--release` 基准（约 97 s 量级）并上传
   `target/bench/cm60-*.json`；本轨**本地没有重跑**它，那一步的首次真跑由 CI 负责。
5. **脚本测试共享真实工作树造成的门禁竞态**（见「本轮新见」第 4 条）：`pnpm test:scripts`
   在两个 worker 并行时可能因另一个文件的临时探针而红，且 `git status` 看不出原因。
   本轨未修，属 `scripts/check-platform-crate-boundaries.*` 与 `check-module-layers.test.ts` 的属主。
6. **`scripts/platform-arch-selfcheck.mjs`（M7）覆写受跟踪的 `packages/backend-client/src/index.ts`**
   （见「本轮新见」第 5 条）：中断即留变异态，且 `git add` 取得到。本轨未跑该脚本、未修。

# 第三轮（ROUND 2 修复轮）：BLOCKER F04-1 + 同批项 + 新增 F05-3 / N12

- 基线 HEAD `fdeff46f4be0437873291cfb91148f9a77ae785d`，fork point `7fa6630f041fa8fef127be7df3b8da2bd3420f4d`
  （`git rev-list --count --first-parent $(git merge-base main HEAD)..HEAD` = **16**，本轮之前 `HEAD` 计数 4982）。
- 本轮**没有重跑基准**：`target/bench/` 下 `cm60-raw-1791128233852.json` 与
  `cm60-summary-1791128233852.json`（commit `04a55fa72`）原样未动。本轮**不作任何性能结论**。
- 未触碰：`hub.md`、`docs/.../connection-management.md`、任何 `ci.yml`。

## F04-1（BLOCKER）：p95 的输入条数曾与 p95 实际吃进去的样本集脱钩

**缺陷本体。** `percentile_input`（`runner.rs:393`）与 `Percentiles::of(&totals)`（`runner.rs:394`）
是同一个变量的**两次独立求值**。MUT-B 只改后者，于是「输入条数」这一列仍在报 `totals.len()`，
而 p95 已经吃了少一个样本的集合——两列看起来是「各自算的」，其实源头是同一个 `totals`。
上一轮 implementer 把「长度单独成一列」当成了「长度已经独立」，两者不是一回事。

**为什么 `tiny()` 抓住了、N=10000 抓不住。** p95 用最近秩 `ceil(0.95·N)`（`nearest_rank_percentile`）：

- `N=4`（`tiny()`）：`ceil(0.95·4)=4`，秩恰好落在最大值上，砍掉最大值 ⇒ `p95` 变。
- `N=10000`（§11.1 的真实样本量）：`ceil(0.95·10000)=9500`，砍掉一个元素既不改秩也不改
  第 9500 名的值 ⇒ `p95_full == p95_cut`，**值断言恒绿**；`percentile_input == measured`
  也恒绿（那一列压根没被改）。两条断言同时失效。

★ **`tiny()` 能抓住是巧合，不是判据。** 巧合来自 `ceil(0.95·4)=4` 正好取到最大那条；
`N` 一换、`p95` 的秩一落进样本中段，值断言立刻变盲。本轮已把这条事实连同
「N=4 上的值断言本身还受数据影响」一起写进 `outcome/tests.rs`：
`catching_the_shortened_input_on_four_samples_is_coincidence` 把它钉成一条用例。

**实测到的第二层事实（比 F04-1 说的还糟）。** 在真实 `tiny()` 数据上做 MUT-B，探针打出
`round=1 n=4 p95_full=Some(19583) p95_cut=Some(19583)` —— **值断言在 N=4 上也没红**，
因为四条样本里有时值相等；而条数断言稳定红 `left: 3 / right: 4`。
即：**条数门与 N 无关且确定触发，值门既随 N 变、又随数据变。**
（另一组实测里值断言在 `:117` 红 `left: Some(5292) / right: Some(5666)`，两次都是真测量，
差异恰好说明值门的触发是数据依赖的。）

**修法：把独立性交给编译器，而不是交给测试。** 「靠一条测试盯着两列别脱钩」本身就是
F04-1 判 FAIL 的原因（那条测试压根测不到）。所以：

1. `Percentiles` 新增私有字段 `#[serde(skip_serializing)] input_len: usize`，
   由 `of()` 在**同一次求值**上记 `samples.len()`，对外只暴露 `Percentiles::input_len()`。
   私有 + 无 setter ⇒ 别的模块**写不进去**。
2. 删掉 `RoundOutcome` 上那个可单独赋值的 `percentile_input` **字段**，改成派生访问器
   `RoundOutcome::percentile_input()`，其值转自 `self.percentiles.input_len`。
   字段被删 ⇒ 不存在「只改其中一处」的写法。

实测两条越权路径都被编译器挡住（不是靠我加的测试）：

- 五字段全填的字面量 ⇒ `error: cannot construct Percentiles with struct literal syntax due to private fields`
- `..Percentiles::default()` 的 FRU ⇒ `error[E0451]: field input_len of struct Percentiles is private`
  （FRU 也钻不过私有字段的空子）

`assemble()` 里那行 `percentile_input: totals.len(),` 随之删除，`runner.rs:397`
的 MUT-B 位点现在只剩 `percentiles: Percentiles::of(&totals)` 一处求值。

**诚实的缺口（不掩盖）。** 没有真的搭一个 10000 请求的 harness 轮次；N=10000 这一层是用
生产 `Percentiles::of` + 生产 `RoundOutcome`/`sample_count_mismatch()` 直接证明的
（`the_input_count_gate_holds_at_the_spec_sample_size`：先证明 `full.p95_nanos == cut.p95_nanos`
即值门确实瞎，再证明 `input_len()` 是 10000 vs 9999），而 `assemble()` 的**调用点**本身
仍只由 `runner/tests.rs` 的 N=4 用例覆盖。

## F04-2 / F04-3：注释与实测相反

`runner/tests.rs` 旧注释称「砍掉最大值这条断言仍然是绿的」——实测正相反（MUT-B 下红）。
注释已按实测结论改写，**追加提交，不 amend**。

## N9：`fake-runtime-fixtures.md` §11.3 行号 off-by-one（5 处）

`outcome.rs` 模块文档 `:11/:12/:186`、`main.rs:36`、`report.rs:128` 五处对 §11.3 子句的行号引用
整体差一行，已逐一核对改正。现在引用的映射（已复核）：QueueFull 豁免 `:569`、
失败字段 `:570`、`ceil(0.95·M)`/`M=N` `:568`、独立一列 `:572`、§11.3 抬头 `:565`。

## 变异自证（MUT-A / MUT-B，本轮实测重跑）

此前 `progress.md` **通篇没有出现过 `MUT` 字样**，变异证据等于没记。本轮重跑并把
**变异定义和红点数一起记下来**——脱离定义的「红点数」没有意义。

**MUT-A（条数说谎）** — `outcome.rs:104` `input_len: samples.len(),` → `input_len: samples.len() + 1,`：

```
MUTA_EXIT=101   test result: FAILED. 48 passed; 11 failed
```

11 个红点：`outcome::tests` 4（`a_gap_that_is_not_even_counted_is_caught`、
`a_shortened_percentile_input_is_caught`、`failures_are_counted_not_deleted_and_n_does_not_shrink`、
`the_input_count_gate_holds_at_the_spec_sample_size`）、`report::tests` 2
（`percentiles_and_counts_are_reported_independently`、`rejections_by_reason_reach_the_artifact`）、
`runner::tests` 4（`every_request_produces_exactly_one_sample`、
`injected_failures_stay_inside_n_and_turn_the_gate_red`、
`the_recorded_p95_is_the_percentile_of_the_samples_this_run_measured`、
`without_injection_every_admitted_request_is_measured`）、`tests::the_exit_code_is_not_inverted` 1。

★ 口径更正：协调者转述的「MUT-A 红点数 = 6」**在本轮复算下不成立**，实测为 **11**。
台账里从未留下 MUT-A 的变异定义，那个 6 无法从本文件复现；此处以「定义 + 实测」成对给出，
以本条为准。

**MUT-B（值集被砍）** — `runner.rs:397` `Percentiles::of(&totals)` →
`Percentiles::of(&totals[..totals.len() - 1])`：

```
MUTB_EXIT=101   test result: FAILED. 52 passed; 7 failed
```

7 个红点：`report::tests::percentiles_and_counts_are_reported_independently`、
`report::tests::rejections_by_reason_reach_the_artifact`（红在产物层 `report.rs:570`
`left: Number(1) / right: Number(2)`）、`runner::tests::every_request_produces_exactly_one_sample`、
`runner::tests::injected_failures_stay_inside_n_and_turn_the_gate_red`、
`runner::tests::the_recorded_p95_is_the_percentile_of_the_samples_this_run_measured`、
`runner::tests::without_injection_every_admitted_request_is_measured`（红在 `:164`，
`assertion failed: round.passes_gate()`——**生产门本身**开始拒绝这一轮）、
`tests::the_exit_code_is_not_inverted`（红在 `main.rs:469`）。

`runner.rs:397` 的 `attempt to subtract with overflow` 只在 MUT-B 下出现（变异对空 `totals`
做 `[..len-1]`），属变异落点本身，不是缺陷。

**方法论坑（本轮真踩了）。** 用 `sed -i.bak` 变异、再 `mv` 回来回滚时，`mv` 保留 mtime，
cargo 的新鲜度检查判定「无需重建」，于是**回滚后仍跑出变异的红**（`52 passed; 7 failed`），
差点被误判成「回滚失败、缺陷还在」。回滚后必须 `touch` 一下再跑。
凡「`Finished in 0.0x s` + 结论与源码矛盾」，先怀疑构建缓存，再怀疑代码。

## F05-3：datazen-runtime 的测试覆盖差集（**F-05 至今未真正闭合**）

全仓 `git grep -inE "cargo[ '\"]*(test|nextest)" -- .` = 18 处 `cargo test`/`nextest` 调用点，
其中只有 2 处点名 `datazen-runtime`，都在 `scripts/run-platform-crate-tests.mjs:157-160`，
唯一链路 `ci.yml:302` → `package.json:111`。

以 `cargo metadata` 的 `kind==['test']` 为准（**不是 `find` 递归数**）：`datazen-runtime` 有
**19** 个集成二进制，`datazen-application` 0，`datazen-platform-api` 0；`Cargo.toml` 无 `[[test]]`。
`cargo test -p datazen-runtime` 实测 **22 条 `test result:` / 626 passed / 0 failed**
（HEAD 基线，含 21 个 `Running` 目标 + 1 个 Doc-tests）。

CI 真实跑的两条：`--lib … --test cm60_pressure_drain` ⇒ 恰好 2 个目标（lib **383** + cm60 **6**）；
`--release … --bin cm60-bench` ⇒ 恰好 1 个（**57**）。故 CI 覆盖 21 个带测目标中的 3 个、
626 个测试中的 446 个，**差集 = 18 个集成二进制 / 180 个测试**，最大一组 `gateway_contract`（51）。
（本轮 +2 用例后同一口径为 182 / 628。）

★ **性质：继承来的窟窿，不是本轨捅的。** 基线 `7fa6630f0` 的
`run-platform-crate-tests.mjs:165` 只有 `['test','--lib',…]`、根本没有 `EXTRA_TARGETS`；
基线 `tests/*.rs` 是 18 个，本轨 +1 = 19。基线 `ci.yml:299` 的注释白纸黑字写着
「计划 :269 的 datazen-runtime CI 空缺」——**基线知道这个洞，用 `--lib` 绕过去了**。

**修法：共存，不是替换。** 在 `scripts/run-platform-crate-tests.mjs` 追加一条
**不带任何 target selector** 的调用 `['test', ...names.flatMap((n) => ['-p', n])]`（`:201`），
`--lib` 那条与两条 `--test` 原样保留。理由写死在 `mjs:196-208`：

★ **明确否决 `--tests` 作为替代**：结论不变（`--lib` 一条都不能动），但**机制原文写错了，本轮订正**。
原文写「`--tests` 只选集成二进制、不选 lib——等于拿掉 383 个换回 180 个」。本轮（2026，`cargo 1.90.0`，
HEAD `83930c07d`）实测 `cargo test -p datazen-runtime --tests --no-fail-fast` 选了 **21 个目标**：
`datazen_runtime`（lib unittests）、`cm60_bench`（bin unittests）、19 个集成二进制，**628 个测试**。
即 **`--tests` 是包含 lib 的**；不含 lib 的是 `--test <name>`（必须带名字，不带名字不匹配任何目标）。
`--tests` 真正丢掉的是 **doc tests**（实测 0 个 Doc-tests block，而不带选择器的那条每个 crate 一个）。
所以换成 `--tests` 的真实代价是拿 doc-test 覆盖去换一遍已经被覆盖的目标，不是丢掉 383 个 lib 测试。
结论（不替换、不删除 `--lib`，只在旁边**追加**一条不带 selector 的调用）不受影响。

被否掉的另两条：丢掉 `--lib`（同上的洞，只是方向相反）；把每个集成二进制逐个登记成
`--test` 条目（19 条 `EXTRA_TARGETS` + 19 条 `toEqual`，测试会随每加一个 `tests/*.rs` 而腐坏，
且仍漏掉 lib 之外将来新增的目标）。

**新增调用的实测增量**：`cargo test -p datazen-runtime -p datazen-application -p datazen-platform-api`
真跑（不 dry-run）⇒ `EXIT=0`、**26 条 `test result:`、944 passed / 0 failed**。
26 = 19 个集成二进制（全部来自 runtime）+ **4** 个 lib/bin unittests
（runtime 的 lib 与 `cm60-bench`，加 application / platform-api 各自的 lib）+ 3 个 Doc-tests
（每个 crate 一个）。
（★ 本轮订正：此处原文写的是「3 个 lib/bin unittests」，19+3+3=25，对不上 26；
正确的拆法是 19+4+3=26。）
（application/platform-api 各 0 个 `kind==['test']` 目标，
所以新增价值全部来自 runtime；另两个 crate 只是把各自 lib 套件重跑一遍，
编译产物共享，代价有界）。

副作用已登记：该调用会把 `cm60-bench` 的单元测试以 **debug** 跑一遍。
要压掉它就得给「唯一一条不带 target 的调用」加 `--bin` 排除项，那会让这条调用的性质
自相矛盾；实测 debug 下这 26 个目标全绿，故保留。

## N12（WARN）：既有守卫可被两种手法绕过

`cargoTestInvocations` 的过滤条件是 `startsWith('test ')`。实测两条绕过（均已回滚）：

- **插无关参数** `['test','--offline','--lib',…]` ⇒ `EXIT=1`，3 failed / 15 passed。
  日志行仍是 `test --offline --lib …`，`startsWith` 看得见，正向 `toEqual` 因内容不符而红
  ⇒ **守卫有效**。
- **调换顺序** `['--offline','test','--lib',…]` ⇒ `EXIT=1`，3 failed / 15 passed，
  但**红的原因不同**：日志行变成 `--offline test --lib …`，`cargoTestInvocations` 的
  `startsWith('test ')` **根本看不见这次调用**（失败信息里只剩 `expected [ Array(1) ]`，
  只剩那条 release 调用还在），于是中止侧的 `toEqual([])`（`:308`/`:340`）**退化成「断言 true」**——
  与第一轮同一个失效模式，只是换了一扇门。整体拦住它的是**同一个测试里的正向精确 `toEqual`**，
  不是中止守卫本身。cargo 要求子命令在前，真实调用侧调换不了顺序，故判 **WARN 不阻塞**。

已在正向精确断言旁留注释，**禁止把它降级成 `toContain`**；并新增
`cargoTestInvocationsSeesReorderedArgv` 辅助函数与一条真实临时日志的负控用例钉住这个盲区。
★ 写这条负控时自己先踩了一次「断言空过」：最初用 `cargoTestInvocationsSeesReorderedArgv({ cargoLog: '' })`，
而 `existsSync('')` 为假 ⇒ 返回 `[]` ⇒ 断言为**错误的原因**而通过——正是本条要记录的失效模式。
改成写真实的临时日志文件（`mkdtempSync` + `writeFileSync`）后才算数。

## 本轮新见（协调者没有点名的缺陷）

### N13（已修）：`report.rs` 测试的临时目录按**毫秒**命名 ⇒ 并行下互相覆盖产物

**这是本轮门禁真红的那一条。** 症状：`cargo test --release -p datazen-runtime --bin cm60-bench`
出现 `EXIT=101`、`test result: FAILED. 58 passed; 1 failed`，
`report::tests::rejections_by_reason_reach_the_artifact` panic 在 `report.rs:567`、
`no entry found for key`。

定位过程（都是实测，不是推断）：

- 单跑该用例 3/3 绿 ⇒ 不是该用例自身的问题。
- 全量并行跑 3 次：2 绿 1 红 ⇒ 是**并发**相关。
- 加 `--test-threads=1` 跑 6 次：6/6 绿 ⇒ 确认并发依赖。
- `git stash` 掉本轮改动回到 HEAD 跑 6 次：6/6 绿（57 passed）。

**根因。** `report.rs:631` 的测试辅助
`tempdir()` 返回 `temp_dir()/cm60-bench-report-{timestamp()}`，而 `timestamp()`（`report.rs:176-182`）
是**毫秒**。同一毫秒里跑的两个测试拿到**同一个目录**；`write()` 落盘的文件名又只由时间戳决定
⇒ 两个测试把各自的 summary 写到同一对文件名上，互相覆盖，谁先读谁读到对方的产物，
于是报的是「key 不存在」这种完全指错方向的错。全仓只有这一个 `temp_dir` 辅助函数，
3 处调用全在 `report.rs` 测试里。

★ **为什么这仍算本轮的账**：缺陷本身是继承的（HEAD 不红），但**是本轮多加的 2 个用例把并发密度
推上去、把它从「测不出来」变成「3 次里红 1 次」**。按「误伤合法同类」这一维自查，
把这样的门禁交出去就是本轨的责任，不能一句「不是我的」了事。

**修法**：按「进程内唯一」改——目录名加 `std::process::id()`（挡跨进程）与一个
`AtomicU64` 序号（挡同进程内并发线程）。修后全量并行跑 **8/8 绿**，且 `/tmp/cm60-bench-report-*`
**0 个残留**。

★ **顺带挖出修法自身的一个坑**：清理侧 `the_summary_and_the_raw_artifact_are_a_matched_pair`
原本**调了两次 `tempdir()`**（先写后清），只有当它按毫秒复用名字时两次才相等。
改成每次唯一之后，第二次会指向一个**从没写过的空目录** ⇒ 清理静默失效、目录永久留在
temp 里。已改为取一次存下来（`let dir = tempdir();`），并在注释里写明原因。
这也说明：把「靠时间巧合成立的隐式契约」改成显式契约时，必须逐个回查调用点。

## F-05 的旧结论必须收窄（本轮不写「已闭合」）

第一轮写的「F-05 已闭合」**不成立**，差集非零。合并前若有人读到旧措辞会得到错误的安全感，
故本节只陈述上面实测的差集与修法，不复用「已闭合」这个说法。

### `ci.yml:299` 注释的处置（已裁定，本轮**不动** `ci.yml`）

那句「计划 :269 的 datazen-runtime CI 空缺」在 F05-3 之后**已经过时但不假**：
它描述的是修 F-05 当时的状态（那时确实只跑 `--lib` + 1 个 `--test` + 1 个 `--bin`）。
F05-3 落地后，CI 里那条**不带 selector** 的调用把 **23 个 `Running` 目标与 3 个 Doc-tests**
**全部**跑了（★ 本轮订正：原文写的是「`datazen-runtime` 的 21 个 `Running` 目标与 3 个 Doc-tests」，
混了两个口径——21 是 `cargo test -p datazen-runtime` **单 crate** 的 `Running` 数，3 个 Doc-tests
是**三 crate** 调用的数；同一个三 crate 调用实测是 23 个 `Running`，23 + 3 = 26 条 `test result:`），
所以这条注释是**低报**而非报错。属文档漂移，归 `ci.yml` 属主，本轨不擅自改 CI 文件。

★ 协调者已裁定 `ci.yml:317-320` **不是阻塞、不得动**：它是
`cargo run --release -p datazen-runtime --bin cm60-bench -- …`，不是 `cargo test`；
`--lib` 对 `cargo run` 无效；纯 `run:` 步、无 `continue-on-error`、无 `if:`
（全仓 `continue-on-error: true` 只在 `:74` 一处）。验收已确认，保持原样。

## 本轮门禁实测（HEAD `fdeff46f4be0437873291cfb91148f9a77ae785d` + 本轮工作区）

长输出全部落 `/tmp`，退出码单独打印。运行前后各记录一次 HEAD 与 `git status --porcelain`，
两者一致（8 个 modified 文件，未变）。`CARGO_TARGET_DIR=/tmp/dz-target-r2-f04`（独立目录）。

```
cargo fmt -p datazen-runtime -- --check                    FMT_EXIT=0
cargo test -p datazen-runtime                               CRATE_EXIT=0
                                                         22 条 test result: / 628 passed / 0 failed
                                                         （HEAD 基线 626 passed / 0 failed，本轮 +2）
cargo test --release -p datazen-runtime --bin cm60-bench    EXIT=0   59 passed / 0 failed
                                                         （HEAD 基线 57，本轮 +2；并行复跑 8/8 绿）
node scripts/run-platform-crate-tests.mjs --dry-run         DRY_EXIT=0   恰好 3 条 cargo 调用
  cargo test --lib -p datazen-runtime -p datazen-application -p datazen-platform-api --test cm60_pressure_drain
  cargo test -p datazen-runtime -p datazen-application -p datazen-platform-api
  cargo test --release -p datazen-runtime --bin cm60-bench
node <main>/node_modules/vitest/vitest.mjs run scripts/__tests__
                                                         EXIT=0   40 files / 684 passed
node <main>/node_modules/typescript/bin/tsc --noEmit -p tsconfig.scripts.json
                                                         TSC_EXIT=0
```

★ 脚本门禁的基线是**在本工作树现测**的，不是引别处：HEAD（stash 掉 `scripts/` 改动）
= 40 files / **679 passed** / EXIT=0，本轮 = 684 passed（+5 个 `it`，
该文件 18 → 23）。

**本轮 `vitest run scripts/__tests__` 共跑了 5 次全量并行：红 2 / 绿 3**（按时间顺序，如实记录，不追绿）：

| 次 | EXIT | Test Files | Tests |
| --- | --- | --- | --- |
| 1 | 1 | 1 failed / 39 passed | 3 failed / 681 passed |
| 2 | 0 | 40 passed | 684 passed |
| 3 | 0 | 40 passed | 684 passed |
| 4 | 1 | 1 failed / 39 passed | 4 failed / 680 passed |
| 5 | 0 | 40 passed | 684 passed |

两次红**全部**落在 `check-platform-crate-boundaries.test.ts`（一次 3 条、一次 4 条，
且每次红的用例名不同：`passes a workspace that respects every rule`、F-01、F-02、
`a declared host edge the resolve graph drops`）。该文件
`git diff 7fa6630f0..HEAD --stat -- <该文件>` **为空**（自 fork 起零改动），
单跑 **53/53 绿**，紧邻的重跑即 684 全绿 ⇒ 属**已知探针撞名 flake**，
根因见「第二轮·本轮新见」第 4 条（`.gitignore:199` 的 `__boundaryProbe__*`
让探针对 `git status` 不可见 ⇒ 探针撞名）。归该守卫属主，本轨未修。
★ 「EXIT=0 且 684 passed」才叫绿；「EXIT=0 且 0 passed」是空绿，本轮没有出现。

★ **不要用 `pnpm`**：`pnpm vitest` 会退出 1 且跑 0 个测试（`node_modules` 是符号链接造成的
假红，不是代码缺陷）；`npx` 在本环境 EPERM。判定看 `passed` 计数，`0 passed` 的
`EXIT=0` 是空绿。

## 遗留与待裁定（修复轮）

1. **§5 队列等待分位数** —— 本轨仍未修，归属见「第二轮」对应条目，合并前需确认该轨已登记。
2. **未在判据指定的 4 vCPU / 8 GiB 复测**（实测机 8 vCPU / 17179869184 字节）。
   协调者已撤回以硬件复测为本轨的交付条件；本轨**不得作任何性能达标结论**，本轮亦同。
3. **`ci.yml:329` `if-no-files-found: warn`** —— 产物缺失**不会**让 CI 红。这是 WARN，
   本轮只登记不修（属 `ci.yml` 属主）。
4. **N6：`DZ_CM60_RUSTC_VERSION` 运行时注入 vs `option_env!` 编译期取值**，撞上 rust-cache
   命中时该字段可能整个消失。WARN，本轮只登记不修。
5. **N12 的 `startsWith('test ')` 盲区** —— WARN，已加负控钉住，禁止把正向断言降级为 `toContain`。
6. **`ci.yml:299` 注释过时（低报覆盖）** —— 归 `ci.yml` 属主，本轨未动。
7. **`check-platform-crate-boundaries.test.ts` 的探针撞名 flake** —— 属该守卫属主，本轨未修。
8. **`.gitignore:199` 的 `__boundaryProbe__*`** 让探针对 `git status`/`git add` 不可见，
   污染面波及所有 boundary 守卫。属主待裁定，本轨未动。
9. **`barrier/tests.rs:236`** 的 30 s 竞态、`scripts/platform-arch-selfcheck.mjs:397-410`（M7）
   覆写受跟踪的 `packages/backend-client/src/index.ts` —— 同「第二轮」条目，本轨未动。
10. **N=10000 的证明没走 `assemble()`**（见 F04-1「诚实的缺口」）。若要求端到端覆盖，
    需要一个 10000 请求的 harness 轮次，代价远超本轮范围；请协调者裁定是否必须。

## 交付状态

**READY_FOR_TEST** —— F04-1（BLOCKER）已修且经编译器级强制 + N=10000 实证；
F04-2/F04-3、N9、MUT 证据、`:169` 归因收窄、F05-3、N12、N13 均已落；
门禁全绿（见上）。未修项均为登记在案的 WARN 或他轨归属。

## 口径返修轮（2026-10-05，HEAD `83930c07d168b67b0f2021f2b75ee9bb7ad91bc4`）

只改注释与本台账，**未改 `buildCargoArgv` 的任何行为**；门禁计数必须一格不动。

### 各数字的年份（不要拍平成一个值）

| 数值 | 哪个 commit 上测的 | 口径 |
|---|---|---|
| lib **382** | `48d194300` / `3ed3ad0db`（早期轮） | `cargo test -p datazen-runtime --lib` |
| lib **383** | `fdeff46f4b` 起，至本轮 `83930c07d` 复测一致 | 同上 |
| bench **46 / 57** | 早期轮 / `48d194300`·`3ed3ad0db` | `--bin cm60-bench`（debug 与 release 一致） |
| bench **59** | `fdeff46f4b` 起，本轮复测一致 | 同上 |
| **626** | HEAD 基线（早于本轨 +2 用例） | `cargo test -p datazen-runtime` 整 crate |
| **628** | `fdeff46f4b` 起，本轮复测一致 | 同上；22 条 `test result:` = 21 `Running` + 1 Doc-tests |
| **944 / 26** | `fdeff46f4b` 起，本轮复测一致 | 三 crate 不带 selector；26 条 = **23 `Running` + 3 Doc-tests** |
| 集成二进制 **19** | 本轨加入 `cm60_pressure_drain` 后至今 | `cargo metadata` 的 `kind==['test']` |
| 集成二进制 **18** | 基线 `7fa6630f0` | 同上 |
| 集成二进制 **22** | `ea97a94c9` | 同上（**不是 main 的 tip**） |
| 集成二进制 **24** | main tip `42bf321a1`（本轮实测当时） | 同上 |

★ 计数陷阱：数集成二进制**只能数深度 1 的 `tests/*.rs`**。本轮用 `git ls-tree -r` 递归数过一次，
得到 34，和 `cargo metadata` 的 19 对不上——递归会钻进 `tests/` 下的共享模块目录。上面 `:442`
那句「不是 `find` 递归数」就是这个坑，本轮又踩了一次，记下来。

### 本轮口径（2026-10-05 实测，`cargo metadata --no-deps --format-version 1` 按 `kind` 分类）

`datazen-runtime` 共 **21 个 cargo 目标** = `["lib"]` 1 + `["test"]` **19** + `["bin"]` 1。
「带测目标」必须点名集合：按 `["lib"]+["test"]` 是 **20**，按全部 kind 是 **21**。
`--lib` 命中集成二进制 **0** 个，`--test cm60_pressure_drain` 命中 **1** 个，**未被逐个点名的还剩 18 个**
（占 180 个测试，最大一组 `gateway_contract` 51）。
`--bin cm60-bench` 是 `["bin"]` 目标，**不在**那 19 个集成二进制里，所以既不属于 19 也不属于 18；
它留在 `EXTRA_TARGETS` 是因为 §11.3 的单元测试必须 `--release` 编，而 `--release` 作用于整条调用、
不是作用在 `--lib` 旁边，所以需要单独一条调用。

★ **`scripts/run-platform-crate-tests.mjs` 里原先那句「reaches two of the 21 test-bearing targets.
The other 19…」按 R4 意见改成 18 是错的**：它把三个集合混成一个——`EXTRA_TARGETS` 的 2 行
（1 个 `test` 目标 + 1 个 `bin` 目标）不是「两个带测目标」；21 是全部 kind 的目标数，不是带测目标数。
**在真值下「two of N」这种句式根本不成立**，所以本轮改成逐个点名集合，不再用比例句式。
同文件 `:256` 那句（19 / 0 / 1 / 383 / 18 / 180 / 51）原本就是对的，保持原样。

### 本轮另外三处（任务书没点名的）

1. `:249` 把 doctests 记在 `--lib` 那条调用名下。实测该调用 2 条 `test result:`、**0 个 Doc-tests**
   （`--lib` 只选库的单元测试目标）。doctests 是新增覆盖，不是重复。
2. `:256` 的 `26 targets` 数字对、口径错：26 是 `test result:` 行数，= 23 `Running` 目标 + 3 个 Doc-tests
   block，Doc-tests 不是 cargo 目标。已改口径。
3. 本节开头记的 `--tests` 机制错误（详见 F05-3 的订正块）。

★ `buildCargoArgv` 不带 selector 的那条**无条件**（是返回数组里的字面量，不受 `EXTRA_TARGETS` 保护）——
已读实现确认，并由 `--dry-run` 打印出的 3 条 argv 佐证，不是抄注释。
