# 测试树裁剪探针（test-tree pruning probe）

## 问题

本机完整链接 `datazen-ui` 测试二进制是确定性 OOM：GNU ld 与 lld 均已验证，
9 次独立证据，链接时 RSS 约 5.9 GB，cgroup 上限 6.1 GiB，rustc 被 SIGKILL。
链接失败不是证据：它既不能证明代码对，也不能证明代码错。`gate --fast` 和
`cargo check` 只能证明能编译，不能证明测试跑过。

## 做法

`scripts/rune-test-probe.sh <模块>...`：

1. 备份当前 `crates/datazen-ui/src/tests/mod.rs`（含 tester 未提交的注册行）；
2. 只保留 `support` / `tokens` 和指定的模块，把其余 `mod X;` 行临时裁掉
   （121 个测试模块 → 一片通常只剩 2–4 个，编译体量缩小约 30 倍），
   其余内容（imports、`runtime()`、注释）原样保留；
3. 以 `CARGO_BUILD_JOBS=1`、`RUSTFLAGS='-C debuginfo=0'` 跑
   `cargo test -p datazen-ui --lib --offline`，拿真实运行时红/绿；
4. 无论通过、失败还是被中断，都把 `mod.rs` 还原成备份的逐字节一致，
   还原失败时大声报错、退出码非零，绝不把裁剪后的树留在工作区。

并发保护：`target/.rune-test-probe.lock` 上 `flock`，第二个探针直接拒绝，
不互相覆盖。模块名写错也在动文件之前拒绝。

## 用法

```bash
scripts/rune-test-probe.sh column_mapping_editor column_mapping_editor_audit
scripts/rune-test-probe.sh ai_chat_panel_input_area ai_chat_panel_input_area_audit -- --exact
```

完整 cargo 日志留在 `$TMPDIR`，脚本每次都打印日志路径；结论只认日志里的
`test result:` 行和退出码。**绝不拿文件里 `#[test]` 的个数当通过数**——
那不是跑过的证据。

## 采用规则（tester / coder 必读）

- 每个 slice 的 coder 和 tester 都走探针，不再手改 `mod.rs` 裁剪
  （手改没有锁、没有还原校验，两个 agent 一撞就丢注册行）。
- 一次探针只覆盖本 slice 的模块（coder 模块 + 审计模块）；跨 slice 的
  回归仍需在 CI 全量跑，探针不代替全量 gate。
- 探针脚本本身跑失败（非测试失败）时，退回手改裁剪并如实上报，
  不得把失败说成通过。
- 跑完核对 `git diff -- crates/datazen-ui/src/tests/mod.rs`：除 tester
  自己的注册行外必须为空。

## 已知边界

- 全量 gate 仍会在本机 OOM（链接整棵测试树），探针不解决全量链接，
  只解决“每个 slice 拿真实运行时证据”的问题。
- 被测模块若引用了被裁掉的兄弟测试模块的项，编译会失败——
  此时把缺的模块名也传给探针。
