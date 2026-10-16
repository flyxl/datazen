# B14：安装测试对**系统共享临时目录**做绝对零断言（跨进程脆弱）

- **状态**：开放 · 已确认（静态核实，未做跨进程实测）
- **归属**：既有测试债，**非本次下沉引入**（同文件同类的 B10 是 `d57690b299` 引入，本条更早）。由 Wave-4 · w4-test-guards 轨在修 B10 时**顺带发现但未改**（超出「只动测试的 B10 边界」）。
- **与 B10 的区别**：B10 盯的是某个 `TempDir` 的**直接父目录**；本条盯的是 `std::env::temp_dir()` **本身**。两者的危害同类（断言落在全局共享位置），但 B10 的关闭条件（按该文件检索 `.parent()` 计 0）**不覆盖**本条。

## 实测

`packages/workspace-apps/src/install/tests.rs`：

| 位置 | 内容 | 形状 |
| --- | --- | --- |
| `:335-346` | `count_inspect_dirs()` = `fs::read_dir(std::env::temp_dir())` 里按 `INSPECT_PREFIX` 计数 | —— |
| `:354` / `:369` | `let before = ...` / `assert_eq!(count_inspect_dirs(), before)` | **差值**，正确形状 |
| `:426` | `assert_eq!(count_inspect_dirs(), 0)` | **绝对零**，脆弱 |

`INSPECT_PREFIX = ".datazen-inspect-"`（`packages/workspace-apps/src/install.rs:27`）；staging 目录由生产代码落在共享临时目录，见 `install.rs:80`：
`let staging = std::env::temp_dir().join(format!("{INSPECT_PREFIX}{}", Uuid::new_v4()));`

同文件 `:324-331` 有 `INSPECT_TMP_LOCK`（`Mutex`）并写明「`.datazen-inspect-*` entries … race on `count_inspect_dirs()`」—— 作者已意识到该风险，但只解决了**进程内**并发，没解决**跨进程 / 残留**。

## 危害

`:426` 只要共享临时目录里存在**任何一个** `.datazen-inspect-*` 目录（同机另一个进程正在跑同一条代码路径，或历史失败留下的残留），该测试就红 —— 而它与被测行为毫无关系。这类「别人搞红我」的断言会让「结论行必须连续两次逐字相同」这条纪律失效。

## 修法（订正提交方的判断：本条**不必**改生产代码）

提交方称「彻底消除需改生产代码」。**不完全对** —— 断言形状本身可以只改测试：

1. **（建议，仅测试）** 把 `:426` 的绝对零断言改成与 `:369` 相同的**差值**形态：先 `let before = count_inspect_dirs();`，执行后再断言 `count_inspect_dirs() == before`。这消除了跨进程脆弱，且不改生产行为、不动 `INSPECT_TMP_LOCK` 的既有语义。
2. **（可选后续，动生产）** 给 `inspect_wapp_package` 一个可注入的 staging 根（默认仍是 `std::env::temp_dir()`），让测试能用自建沙箱 —— 那才是真隔离，但属独立变更，不在本条关闭条件内。

## 关闭条件

`packages/workspace-apps/src/install/tests.rs` 中不再存在针对 `std::env::temp_dir()` 的**绝对**断言（`count_inspect_dirs()` 的断言全部为差值形态），且 `cargo test -p datazen-workspace-apps` 全绿、连续两次运行的**测试计数与逐条判定**一致。

**判据说明（重要）**：「连续两次逐字相同」这个说法**不成立** —— libtest 并行调度会改变逐条 `test …` 行的**输出顺序**，聚合的 `test result:` 行还含计时字段。可比的是**计数**（passed / failed / ignored）与**每条判定的 `ok` / `FAILED`**。

## 实测复现（Wave-4 · w4-test-guards 的独立验证方）

行号以被测提交 `e9c7ed9a1` 为准 —— 本文早先引的 `:335` / `:426` 是**修前**坐标，该提交因自身 +9 行位移后：

- 绝对零断言在 **`:435`**；被计数的 `fs::read_dir(std::env::temp_dir())` 在 **`:345`**。
- **确定性复现**：`mkdir "$TMPDIR/.datazen-inspect-verifyprobe"`（模拟同机另一个 DataZen 进程或陈旧残留）后跑
  `cargo test -p datazen-workspace-apps inspect_wapp_package_rejects_invalid_packages`
  ⇒ **FAILED / EXIT=101**，`panicked at install/tests.rs:435:5: assertion left == right failed  left: 1  right: 0`；`rmdir` 后同测试 EXIT=0。
- 同一诱饵下，`:378` 那处**差值**形态的断言（`assert_eq!(count_inspect_dirs(), before)`）**PASS** ⇒ 脆弱点就是 `:435` 的绝对值 0。
- `INSPECT_TMP_LOCK` 是**进程内** `static Mutex`，对跨进程零保护；而本仓 `src-tauri/src/commands/wapps/tests.rs:230` / `:246`
  （`cargo test -p datazen --lib` 就在跑）与 GUI 的 `registry.rs:68` 都会调用同一生产函数、在同一 `$TMPDIR` 造同前缀目录。
