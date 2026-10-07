# P5 · clippy approx_constant 轨 台账（合并时删除）

分支 `feature/p5-clippy-approx`，基线 `d6423f9d1`，工作树 `.worktrees/datazen-p5-clippy-approx`，
独立 target dir `/tmp/p5ca-target`。

## 改动面

仅两个文件，与授权范围一致：

- `packages/drivers/redis/src/ops/exec.rs` — 测试 `result_type_of_values`
- `packages/drivers/redis/src/ops/mod.rs` — 测试 `test_tester_value_to_string_double_format`

`git diff --name-only d6423f9d1 HEAD` 只列出这两个文件。生产路径未触碰，未新增 `unwrap()`。

## 这个 3.14 到底是什么（两处语义不同）

- `result_type_of_values`：`result_type_of` 只按 variant 判别，`Double(_)` 的 payload 绑定到 `_`，
  任何数值都不影响返回的 category。**这里的 3.14 是纯占位**。改用 `10.5`——INCRBYFLOAT /
  ZINCRBY / HINCRBYFLOAT 这类浮点计数器命令在 RESP3 下返回的普通计数结果，并在注释里写明
  「payload 不可变结果」这一前提。
- `test_tester_value_to_string_double_format`：`value_to_string` **没有** `Double` 分支，Double 落入
  `other => format!("{other:?}")`，调用方拿到整个 `redis::Value` 的 Debug 渲染，payload 在其中可见。
  **这里的 payload 是承重的**，断言要证明数值能穿过这次转换。原写法 `s.contains("3.14")` 把同一个
  魔法数字写了两遍（构造一次、期望一次），存在漂移空间。改为 `let payload = 10.5;` +
  `assert!(s.contains(&format!("{payload:?}")))`，期望由构造时用的同一个值派生，两者不可能漂移。

## 门禁（跑在最终 HEAD 上）

| 门禁 | 退出码 | 逐字结论行 |
| --- | --- | --- |
| `cargo clippy -p datazen-driver-redis --all-targets --keep-going --message-format=json` | `0` | error 级诊断总数 = 0 |
| 同上，人类格式 | `0` | 全文 `approx_constant` 出现次数 = 0 |
| `cargo test -p datazen-driver-redis` | `0` | 见下方 5 行 `test result:` |
| `rustfmt --edition 2021 --check` × 2 | `0` / `0` | 两文件均无 diff |

`cargo test -p datazen-driver-redis` 的全部结论行：

```
test result: ok. 397 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 7.30s
test result: ok. 4 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

首行 `397 passed; 0 failed; 3 ignored` 与基线参考逐字一致。已知 flake
（`connect::tests::live_prefer_falls_back_to_plaintext_and_require_refuses`）本次**未命中**。

## 告警集合差分（任务定义的口径）

按 `level=="warning"` 且 primary span 的 `file_name`，去重排序后 `comm`：

- baseline 路径数 = 9，修复后路径数 = 9
- `comm -23`（仅基线有）= 空
- `comm -13`（仅修复后有）= 空
- **差分 = 0 行**：本改动既未引入新告警，也未顺带消掉别的告警。

修复前后 warning 计数逐项相同（bind_instead_of_map 2、clone_on_copy 1、
field_reassign_with_default 2、inconsistent_digit_grouping 1、manual_ignore_case_cmp 2、
new_without_default 2、ptr_arg 6、redundant_closure 10、too_many_arguments 2、
unnecessary_map_or 2、unnecessary_min_or_max 2）。消失的 2 条是 error 级，从未进入 warning 集合。

## 变异测试

全部在**独立的 detached 验证工作树** `.worktrees/datazen-p5-clippy-verify`（checkout 自本分支提交）
中执行，每次 `git checkout -- .` 还原，验证树最终 `porcelain` 为空、`diff HEAD --stat` 为 0 行。
断言打在 clippy 退出码与 span 路径、测试进程的 `test result:` 上，不碰标志对象。

| # | 变异 | 期望 | 实测 | 判定 |
| --- | --- | --- | --- | --- |
| M1 | 两处还原为 `3.14` | RED | `CLIPPY_EXIT=101`，`approx_constant_errors=2`，span ∈ {exec.rs, mod.rs} | **KILL** |
| M2 | 仅 `exec.rs` 还原 `3.14` | RED | `CLIPPY_EXIT=101`，`approx_constant_errors=1`，span = exec.rs | **KILL**（逐点覆盖） |
| M3 | 仅 `mod.rs` 还原 `3.14` | RED | `CLIPPY_EXIT=101`，`approx_constant_errors=1`，span = mod.rs | **KILL**（逐点覆盖） |
| M4 | 打断 `ops::parse::value_to_string` 的 catch-all | 该测试 FAIL | `TEST_EXIT=101`，`FAILED. 0 passed; 1 failed`，panic 于 `ops/mod.rs` 的 `assert!` | **KILL** |
| M5 | 把 `Double(_)` 移出 `result_type_of` 的 `"scalar"` 臂 | 该测试 FAIL | `TEST_EXIT=101`，`FAILED. 0 passed; 1 failed` | **KILL** |
| M6 | 两处换成 `std::f64::consts::PI` | — | `CLIPPY_EXIT=0`，0 错误 | **SURVIVOR**（见下） |
| M7 | payload 改成 `0.0` | — | `TEST_EXIT=0`，`ok. 1 passed` | **SURVIVOR**（见下） |

M4 / M5 是第 6 条要求的针对性反向变异：它们动的是**生产代码**，证明两个断言仍然活着，仍然会抓住
它们本该抓住的回归——如果我把断言削弱成恒真（`assert!(!s.is_empty())` 之类），M4 就抓不到失败。

**存活变异的如实说明（存活证明的是覆盖缺口，不是说代码有错）：**

- **M6**：把两处换成 `f64::consts::PI` 同样能让 clippy 变绿。这说明 lint 本身**无法**区分「真修」
  和「消音」——所以任务要求的「不许用常量骗过 lint」靠 lint 是守不住的，只能靠语义判断。我按语义
  否决了这条路径（Redis 的浮点计数器返回值不会恰好等于 π，字面量会歪曲线上真实取值），而不是靠
  clippy 判它通过。它存活不构成本轨的覆盖缺口。
- **M7**：`0.0` 同样通过。派生式期望对任意 payload 都成立，而任意 f64 的 Debug 都非空，故
  `contains` 永不为空串——期望不可能退化成恒真。这一条存活恰恰是**期望值派生写法**的设计目标
  （payload 不再被写死第二遍）。非空性由 M4 直接证明。

## 我推翻的自己的说法

1. **「`approx_constant` 是 allow-by-default 的 style/pedantic lint，没有仓库级 deny 就只能是 warning」**
   —— 错。`clippy -- -W help` 显示 `clippy::approx-constant` 当前等级是 **deny**，且列在 `clippy::all`
   组内；clippy 版本 0.1.90。仓库侧确实没有 workspace deny / RUSTFLAGS / `.cargo/config.toml` /
   `clippy.toml` / `[lints]` / CI job，error 级完全是**上游 deny-by-default** 造成的。
   我最初的推断基于旧版本印象，未经工具核实就写进了思路，结论碰巧对但理由是错的。
2. **「`ops/mod.rs` 里的 `value_to_string` 就是 `ops/stream/parse.rs` 里那个」** —— 错。M4 第一次跑出
   SURVIVOR，我一度以为是我的修复削弱了断言；查了 import 才确认测试绑定的是
   `crate::ops::parse::value_to_string`（`ops/parse.rs`），而 `ops/stream/parse.rs` 里是另一个同名
   私有副本。该 crate 共有 **6 个** 各自独立的 `value_to_string` 定义。改对符号后 M4 转为 KILL。
   **教训：变异存活先怀疑变异本身指错了符号，再怀疑被测代码。**

## 遗留与待裁定项（不阻塞本轨，越界未修）

- 该 redis crate 里有 **6 个各自独立的 `value_to_string`**（`ops/parse.rs`、`ops/stream/parse.rs`、
  `value/preview.rs`、`driver/database.rs`、`ops/json.rs`、`ops/observe.rs`），语义相近但不一致
  （例如 `ops/parse.rs` 把 `Nil` 排在最前、显式返回空串，其余靠 catch-all）。合并成一个是独立的
  清理任务，超出本轨授权范围，仅记录。
- 修复后仍有 32 条 warning 级 clippy 诊断分布在 9 个文件。本轨只负责清 error，warning 集合差分为 0，
  未触碰任何 warning。
- 本轨未撞上已知 flake 轨 `connect::tests::live_prefer_falls_back_to_plaintext_and_require_refuses`，
  故未对其做任何判断，也未改动它。