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

复算里除 raw 外的每一个数都是冻结 summary 的**照抄**（journal、warmup 墙钟、逐轮 wall_time），
无一处推断或构造；预热样本不进 raw，所以预热分位数复算不出来，就留空并在代码里注明
预热不参与任何 p95——这是诚实缺口，不是编数。

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
