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
