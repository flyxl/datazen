# P3-FU 隧道轨台账（CM-32-FU1）

> 分支 `feature/p3fu-tunnel`。本文件是开发期台账，**验收合并时必须删除**，不得存活到 `main`。

## 一句话

「隧道引用计数只有一本账」从审计承诺变成**机械保证**：R1–R7 七条文本闸门 + 每条闸门的
kill test（植入变异当字符串喂同一套扫描器），并在原始反例上做过实证复核。

## commit 链

| commit | 内容 |
| --- | --- |
| `506f42411` | 闭合 CM-32-FU1 —— 单计数器铁律从审计承诺变成机械保证（9 文件 +2046/−110，新增 `single_counter_audit.rs`、`source_scan.rs`） |
| `68b92c99d` | 补 R7 / R2+，关掉「换名字挂镜像账」；闸门拆目录守单文件纪律（7 文件 +693/−413，`single_counter_audit.rs` 拆出 `kill_tests.rs`） |
| `5ff668be4` | 台账注释钉住「建账 vs 记账」的分界，把 R1/R2/R7 按方法名对齐（纯注释） |
| `85e5c55c1` | R6 按职责拆出 `gate_self_audit.rs`（`mod.rs` 884 → 675 行） |

## 门禁实测（本轮重跑）

首 `HEAD=874b85fb` / 尾 `HEAD=874b85fb`；运行期间工作区 sha 首 `9f374714c313`、尾 `9f374714c313`
（首尾逐字相同 ⇒ 这一轮门禁运行期间没有第二方动过这棵树）。

```
cargo fmt -- --check                    EXIT=0
cargo test -p datazen-runtime --lib      EXIT=0   test result: ok. 463 passed; 0 failed
cargo test -p datazen-runtime \
  --test cm28_concurrent_tunnel           EXIT=0   test result: ok. 3 passed; 0 failed
  --test p4_usecase_journeys              EXIT=0   test result: ok. 7 passed; 0 failed
  --test tunnel_refcount_contract         EXIT=0   test result: ok. 7 passed; 0 failed
```

基线是 `442 passed`（本轮实测，非旧台账数字），隧道轨改完 `463 passed`，净增 21。

## 反例实测（主代理亲做，非转述）

把当年「编译通过且全轨测试全绿」的原始反例植回夹具（`RecordingTunnelTransport` 加
`close_tally: Mutex<usize>` + 构造初始化 + `close()` 里 `*tally += 1` + `close_calls()` 改读该字段），
在**独立 detached 工作树**里跑 `--lib`：

```
MUT_EXIT=101
test result: FAILED. 459 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
tunnel::single_counter_audit::no_tunnel_transport_implementation_stores_its_own_tally ... FAILED
tunnel::single_counter_audit::every_port_reading_is_a_projection_of_one_pure_fold ... FAILED
```

结论：R4/R5 精确命中，不是「碰巧有别的用例红了」。临时树已 `git worktree remove --force` 清理。

## 拆分后的判据完整性自查

- 三个 R6 用例在新模块路径下逐条重跑通过：`gate_self_audit::the_gate_names_are_tied_to_the_real_symbols` ok、
  `::the_audit_registry_covers_every_tunnel_module` ok、`::the_scan_primitives_see_the_shapes_they_claim` ok。
- 新文件自己进 `AUDITED` 登记表，并加进 R6 的必备清单 ⇒ **扫描面比拆分前更宽**，不是等宽。
- R4/R5 两条 kill 用例仍绿（`--lib` 里 `... ok`）。

## 独立复测后的补正（主代理亲做）

独立 Tester 出 `PASS`，但报了三处**误伤** + 一个漏网，逐条复核后落如下修正：

| # | 现象 | 根因 | 修正 |
|---|------|------|------|
| M13 | 全新第三个测试文件自带 `close_tally` ⇒ **无一转红** | R6 的登记表是硬编的 `AUDITED`，新增文件不在表里就等于没扫 | 新增 `every_transport_implementing_file_on_disk_is_registered`：运行时递归枚举 `src/`、`tests/` 两棵子树的 `.rs`，逐个扫 `impl TunnelTransport`，断言「含实现的文件集合全部在 `AUDITED` 里」；并自证枚举本身没空转 |
| 非 ASCII | 扫描器 panic 于 `byte index 16669 … inside 'ï'` | `blank()` 把 UTF-8 字节当 Latin-1 字符回推，输出仍是多字节字符，下游按字节切片全在字符边界上炸 | `blank()` 入口把 `>= 0x80` 的字节落成空格（一字节→一空格，长度与行号守恒） |
| **新发现** | `blank()` **不幂等**、且每个字符串字面量**整体短一字节** | 左引号被吃掉时不补位（只给 `b` 补了一格），右引号却留着 ⇒ 输出里留下**孤立闭引号**，第二趟被当成新字符串开头一路吃到下一个引号，把中间真代码整片抹掉 | 字符串/裸字符串的左右定界符一律抹成等长空白，与 `char_literal_len` 分支一致。实测 `impl_blocks(&blank(&blank(x)))` 的 impl 块曾从 **1 变 0** —— 不报错、不 panic、直接**什么都没扫到**，闸门变成永远绿的空转 |
| L2 | `&Vec<TunnelEvent>` 折法被打成「实得 0 个：[]」，随后 R5 反过来报「端口没调用唯一折函数」——**诊断与事实相反** | `fold_functions` 只认 `&[` 一个子串 | 抽 `is_fold_param`，认 `&[`/`&mut [`/`&Vec<`/`&mut Vec<` |
| L4 | `refs()` 返回新值类型 `Refs(pub u32)`（比裸 `u32` **更安全**）被判成「返回字段引用等于把账外泄」 | R1 判据写成「返回类型必须是某个整数类型」 | 抽 `leaks_by_return_type`：判据回到「是不是引用类型」，`u32` / `Refs` 都放行，`&u32` 仍拦 |

L1（端口新增 `revision_of(&self) -> u64` 这样的整数观测方法被 R5 抓）**判定为已披露的策略选择**，
`transport.rs:49-52` 白纸黑字写着「所有返回整数的 `&self` 观测方法都必须是这个折法的投影」，本轮不动。

顺带纠正 Tester 的一处误报：`transport.rs:49` 引用的 `every_port_reading_is_the_projection_of_one_journal_fold`
**确实存在**（`journey_single_counter.rs:329`），上下文讲的是**值层面**沿整条旅程对账，与该 journey 用例吻合。

**误伤比漏抓更危险**：三个误伤全落在同一个缺口上 —— R5/R7 从来只有「植入变异应当转红」，
没有一条「合法形状应当放行」。误伤会把人逼向更隐蔽的写法，而受压的人改的往往不是闸门。
已补 `legal_shapes_are_not_misjudged` 负控钉死：四种合法折法签名 / 按值返回放行而引用返回拦截 /
非 ASCII 不 panic 且不漏扫 / `blank()` 等长且幂等。

## M13 反例复验（主代理亲做，提交后在 detached 树实测）

在 `194ff75c7` 的 detached 工作树里新建 `packages/runtime/tests/tunnel_new_probe.rs`：
一个此前不存在的第三个测试文件，`SneakyProbeTransport` 实现 `TunnelTransport`
且自带 `close_tally: Mutex<usize>`，构造并 `drop` 以保证不触发 dead_code。

```
M13_EXIT=101
test result: FAILED. 462 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
---- …::gate_self_audit::every_transport_implementing_file_on_disk_is_registered stdout ----
panicked at packages/runtime/src/tunnel/single_counter_audit/mod.rs:202:5:
磁盘上实现了 TunnelTransport 却没进审计登记表 —— 这份端口不会被 R4 / R5 扫到，
它完全可以私藏第二本账（CM-32-FU1）
tests/tunnel_new_probe.rs
```

旧口径（硬编 `AUDITED`）下这是 `461 passed` 全绿漏网；新规则转红，且**诊断点名了具体文件**。
临时树与 target 目录已 `git worktree remove --force` 清理。

## 未做 / 待裁定

1. **本轨被明令不得接线**：`src/tunnel/mod.rs:52-60` 登记的两格「实现缺失」——全阶段失败矩阵下的
   隧道引用回滚、`Quarantined` 下隧道引用与物理预算归属配对——**仍开着**。根因是全仓 `TunnelLedger`
   在 `src/tunnel/` 之外零生产调用方，`runtime/src/resource/**` 里 `tunnel` 出现 0 次。
   接线是架构工作，已另开独立轨（`feature/p3fu-arch`）并行推进，本轨不得顺手接线。
2. **CM-27 的「可确认关闭的资源许可归零」不在本模块**：`mod.rs:41-51` 记录了三处闭合点
   （`platform-api/src/ports/budget/pool_ledger.rs` 的 `Ledger::release`、`runtime/src/budget/ledger.rs:397`、
   `runtime/src/budget/coordinator.rs:428`）。本轨文档已把这条误记纠正回去。
3. **§11.3:568 的排队分位数不开轨**：它归属既有队列责任方，且规格明确「不许收窄规格、不许接队列进基准」。
4. `workspace` 56 warnings 与 clippy 候选门禁留到集成阶段（已验证
   `cargo clippy -p datazen-platform-api -p datazen-driver-api -p datazen-application -p datazen-ai-api --lib -- -D warnings` 是安全的）。

## 交付状态

**编码完成，等复测。** 三轨各派全新 Tester 复测（coder 自验不可替代），复测通过后才合入 `main`，
合入时删掉本文件。