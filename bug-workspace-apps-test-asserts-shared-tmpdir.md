# 缺陷：安装测试把系统共享临时目录当成自己的沙箱

## 症状

`packages/workspace-apps` 的安装测试里，有一条断言检查「越界目标文件不存在」，而它检查的路径是
`TempDir` 的**直接父目录**。`TempDir` 由 `tempfile` 直接创建在系统临时目录下，那个父目录就是
全机共享的 `$TMPDIR`（本机实测 `TMPDIR=/var/folders/2y/5w1_2qg97r7b9bfv20b4nybh0000gn/T/`）。
于是这条断言实际断言的是：**整个机器的共享临时目录里不能有一个叫 `outside.html` 的文件**。

## 位置

- `packages/workspace-apps/src/install/tests.rs:101-106`（断言本体）
- 上下文：同文件 `:88` 创建 `wapps_root`，`:96` 的注释写的是「Nothing escaped the wapps root」，
  可见作者本意是「没有逃出 wapps 根目录」，实现却取了根目录的**上一层**。

## 证据

1. 同文件 24 处 `TempDir::new()` 全部无嵌套子目录（实测 `grep -n` 全量列出，无 `create_dir` 包裹），
   因此每个测试的 `wapps_root.path().parent()` 都等于同一个 `$TMPDIR`。
2. Wave-3 · W3-2 的独立验证方做变异探针（临时短路守卫以验证该断言真的会红）时，
   该探针真的往 `$TMPDIR/outside.html` 写入了 17 字节 `html` 内容，
   于是重跑 `cargo test -p datazen-workspace-apps` 多出 1 个 failed。验证方删掉残留后复跑 62/0，
   验证树的工作区 sha 也回到起点。**这次是探针自己写的，但它证明了这条断言真的监听着一个共享路径。**

## 影响

- **假红**：任何进程（包括并行的 `cargo test`、`cargo metadata`、别的临时文件清理任务）碰巧在
  `$TMPDIR` 下创建了同名文件，本测试就会红，而且失败信息指向的是路径穿越防护，与真实原因无关。
- **掩盖归属**：真正发生越界写时，落点同样会出现在这个共享目录里，断言通过与否都指不出一旦出错时
  是哪个测试留下的残留，排查成本被推到人工。
- **残留不自清**：写进共享目录的文件不属于任何 `TempDir`，不会被 drop 清理，会一直留着影响后续运行。

## 归因

`git blame -L 96,107` = **`d57690b29`**，即 Wave-2 · B1「下沉 Workspace App 形态中立内核到
`packages/workspace-apps`」。**本次迁移引入，不是宿主既有缺陷**，因此不适用「非本次下沉引入」
那一列的免责说明（该列只回答归谁修，不用于降级优先级）。

## 修法候选

1. **（推荐）加一层自建沙箱**：`let sandbox = TempDir::new()` 之后把 wapps 根建成 `sandbox/root`，
   越界目标写成 `sandbox/outside.html`，断言 `!sandbox/outside.html.exists()`。语义变成
   「没有逃出本次安装的沙箱」，与 `:96` 的注释一致，且 `sandbox` 顶层由本测试独占。
2. 只把 `wapps_root` 塞进 `TempDir` 的子目录、继续断言其父目录 —— **不解决问题**：父目录仍是共享 `$TMPDIR`。
3. 直接删掉这条断言，只保留 `keep.txt` 内容不变 + 无 staging 残留两条。覆盖面会缩小，
   「越界目标文件真的没被创建」这一条就没有任何守卫了。

## 关闭条件

- `packages/workspace-apps/src/install/tests.rs` 中，**针对 `TempDir` 直接父目录的断言归零**
（按文件范围检索计 0 命中；断言目标改为本次测试自建的沙箱顶层目录）。
- `cargo test -p datazen-workspace-apps` 全绿，且**连续两次运行的测试计数与逐条判定一致**。

**判据说明（重要）**：「连续两次逐字相同」这个说法**不成立** —— libtest 并行调度会改变逐条 `test …` 行的**输出顺序**，聚合的 `test result:` 行还含计时字段。可比的是**计数**（passed / failed / ignored）与**每条判定的 `ok` / `FAILED`**。
