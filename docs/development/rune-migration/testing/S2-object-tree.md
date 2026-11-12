# TN S2 对象树 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 提交：`a02dd07` `feat(rune): open a table tab from the session object tree (issue-rune S2)`（`a02dd07f6a8876dc0cf7465b1a2bc35787f2d31f`）
- 设计：`docs/development/rune-migration/plan.md`「工作区壳」任务 S2（对象树只列当前会话的表名，点击走 S1）
- 源码：`crates/datazen-ui/src/lib.rs` 的 `object_tree`（约 236–259 行），挂在侧栏（约 310 行）。容器键 `object-tree`，行键 `tree-table-{name}`。点击调用已有的 `open_table_tab` → `Tabs::open(TabKind::Table, title)`
- `git show --stat a02dd07` 只有 `crates/datazen-ui/src/lib.rs`（+47）。`packages/frontend` 与 `packages/web` 不在该提交里。React 未改
- 探针只在 `/tmp/s2_object_tree_probe.rs` 临时编译（挂进 `datazen-ui` 测试模块），跑完已删，`lib.rs` 已还原。产品源码未改。不是既有 `clicking_a_session_table_opens_a_table_tab` 的重跑

## 门禁

环境：`CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RAYON_NUM_THREADS=1 RUSTFLAGS="-C debuginfo=0"`。`--offline`。门禁在干净产品树上跑，不含探针。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 19 passed；`datazen-ui-api` 3 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web`，未启动前端测试 |

编译期有既有 `rune` `private_interfaces` 告警。`cargo check` 里 `open_query_tab` 仍是 `dead_code`（`crates/datazen-ui/src/lib.rs` 约 120 行），查询标签入口留给 S4，不记为 S2 缺陷。`open_table_tab` 已有对象树调用点，这条告警消失。另有既有 `DataGrid` / `SqlEditor` 非测试构建的 `dead_code`，与本提交无关。

## 探针

12 条新输入。`cargo test -p datazen-ui --offline --lib s2_p -- --test-threads=1`。12 passed，0 failed。

内存工作区公开接口只能列出 `customers`。带特殊名字的探针在已 `connect("local")` 之后，改的是会话里的表名列表，不改存储表，用来打键和几何，不代表产品多了一张表。

### P1 连接前的空树

- 目的：无会话时容器在、没有表行，点容器不会开标签，失败的连接也不会造出表。
- 输入：首帧不连接。点 `object-tree` 的中心。再 `connect("missing")`。
- 预期：键 `object-tree` 存在；没有任何 `tree-table-*`；树里没有文本（不会凭空造空白表名）；标签集合空；点击不改当前节。未知连接返回错误，会话仍是 `None`，树仍空。
- 实际：容器矩形 `[16, 341.25, 192, 0]`，不透明度 1，没有子标签。点击命中的是 `datazen-sidebar`，节不变，标签仍空。`connect("missing")` 失败后仍无行。
- 判定：通过。空树高度为 0，但这时没有标签可被裁切。

### P2 API 连接后只有 customers，标签没有被裁切

- 目的：会话表名与树行一一对应；父级高度不是 0，标签的可见矩形没有被父级裁成 0。
- 输入：首帧之前 `connect("local")`。不点连接行，所以不切到 Data Browser。
- 预期：节仍是 Connections。会话表名正好是 `["customers"]`。唯一行键 `tree-table-customers`。行在树盒内。树高、行高、标签可见高度都大于 0。此时还不因“列出树”而自动开标签。
- 实际：树 `[16, 341.25, 192, 28]`，行同位置、高 28。标签文本 `customers`，矩形与可见矩形都是 `[24, 347.75, 61, 15]`，没有被裁。标签集合空。
- 判定：通过。

### P3 新行在 dt=0 时点不中，正时间之后打开表标签

- 目的：新出现的行会淡入。`frame(0.0)` 可能把呈现不透明度留在 0，点击会打到后面的元素。验收路径是点连接 `local` 之后再点表名。
- 输入：首帧后点 `connection-local`，接着 `frame(0.0)`。若行不透明度为 0，再点该行中心。然后以 1/30 秒步进直到不透明度 > 0.9，再点一次。
- 预期：连接后节变为 Data Browser，行已挂上。不透明度为 0 时点击不开标签。正时间后不透明度升高，点击打开恰好一个 `TabKind::Table`，标题 `customers`，不是 Query。条带上能看见标题，标签本身不是被裁没的。
- 实际：新生行矩形高 28，不透明度正好 0。`element_at` 命中 `datazen-sidebar`，这次点击后标签数仍是 0。正时间后不透明度 0.921。再点命中 `tree-table-customers`。模型为单个表标签 `customers`。淡入后 `tab-{id}` 的文本是 `customers`，可见高度大于 8。
- 判定：通过。dt=0 点不中是 Rune 默认进入淡入（约 0.2 秒），不是树没有挂上。产品点击路径在淡入之后能打开对应表标签。

### P4 再点一次是第二个同名表标签

- 目的：S1 允许同名标题并存。看第二次点击是再开一个，还是改成只聚焦第一个。
- 输入：已连接，行处于静止（不透明度 1）。连续点 `tree-table-customers` 两次。
- 预期：至少有一个标题为 `customers` 的表标签。若去重，记录为相对 S1 的行为变化，但不单独构成 S2 失败。若不去重，应有两个不同 id，后一个被选中，种类都是 Table。
- 实际：不去重。两次之后是 `(1, Table, customers)`、`(2, Table, customers)`，选中 2。没有 Query。树行仍在。
- 判定：通过。与 S1「同名可以并存」一致，没有改成去重。

### P5 选中和关闭仍按 S1，id 不复用

- 目的：从树开出两个同名表标签后，点条带选中、关未选中、关到空，都跟 S1 一样；关标签不清树；再开的 id 不回收。
- 输入：点出行 1、行 2。等关闭键淡入后点 `tab-1`，再点 `tab-close-2`，再点 `tab-close-1`。然后再点树行。
- 预期：点 `tab-1` 后选中 1。关 2 后选中仍是 1，2 消失。关 1 后集合空、选中 `None`，`tab-strip` 还在，两个 id 的键都没有，树仍是 `tree-table-customers`。新标签 id 为 3，种类 Table，标题 `customers`。
- 实际：与预期一致。关未选中的 2 没有把选中挪走。
- 判定：通过。

### P6 切换侧栏节之后树还在，点击不开查询标签

- 目的：树在侧栏，不跟页面正文走。切节之后行还在、可点。在 Settings 上点行，节不变，新标签是表不是查询，也不改网格。
- 输入：点 `connection-local`（进入 Data Browser 并挂上 `customers` 行）。依次点 Settings、Schema、SQL Editor、Workflows、Connections、Data Browser。最后停在 Settings 再点树行。
- 预期：每一节之后行键仍是 `tree-table-customers`，不透明度 > 0.5，可见高度 > 20。点行后节仍是 Settings。恰好一个 `TabKind::Table` / `customers`。网格 `(0,1)` 不变。
- 实际：与预期一致。行可见矩形始终是 `[16, 341.25, 192, 28]`。
- 判定：通过。

### P7 别扭的表名

- 目的：键是 `tree-table-{name}` 原文。斜杠、空格、非 ASCII、`#dup`、符号和长名字不应让点击打不开，也不应让行被高度为 0 的父级裁掉。
- 输入：会话表名依次为 `a/b`、`my table`、`订单`、`foo#dup1`、`x"{}$`、80 个 `n`。每行点矩形中心。
- 预期：六行都在，标签文本等于原名。行的可见高度 > 20，底边不超出树盒。每点一次多一个 `TabKind::Table`，标题等于该表名，不是 Query。
- 实际：六行键都在（含 `tree-table-a/b` 和带空格的键）。行高都是 28，可见高度 28，标签可见高度 15。六个标签标题与表名一致，种类全是 Table。80 个 `n` 的标签可见宽度是 578，宽过 192 的行，画到了侧栏外面，但行中心的点击仍然打开了标题为这 80 个 `n` 的表标签。
- 判定：通过。长名字横向溢出只发生在注入的超长名上；内存库的 `customers` 标签宽 61，落在行内。不作为拒绝项。

### P8 空白不是被造出来的表

- 目的：真实会话不会多出空白行。若会话里真有空白名，点击不应靠 S1 的 trim 变成一张“无名表”以外的新标签；两端有字的名字按 S1 trim。
- 输入：先看 `connect("local")` 的真实行。再把会话表名换成 `""`、`"   "`、`"\t"`、`" \n "`、`" customers "`，逐个点击。
- 预期：真实行只有文本 `customers`。注入后行数等于 5，文本等于原名，不额外多行。四个空白点击都不新增标签。`" customers "` 打开一个表标签，标题是 trim 后的 `customers`（S1 的 `open`），树上行文本仍是原文。
- 实际：与预期一致。空白行点了，标签集合不变。带边空白的那次得到 `(1, Table, customers)`。
- 判定：通过。默认内存会话没有发明空白表。trim 是 S1 既有合同，默认表名 `customers` 没有两端空白，标题与表名相同。

### P9 同名两行

- 目的：两个同名表不能并成一行，后一个键被 Rune 改名后，点击仍打开该行自己的标题。
- 输入：表名 `orders`、`orders`、`orders#dup1`。按树上的三个键各点一次。
- 预期：三行都在。两个 `orders` 各开一个 Table `orders`，`orders#dup1` 开 Table `orders#dup1`。三个 id 不同，选中最后一个点开的。
- 实际：键为 `tree-table-orders`、`tree-table-orders#dup2`（第二行 `orders`）、`tree-table-orders#dup1`（名字本身带这个后缀，所以 Rune 把重复键让到 `#dup2`）。三次点击的标题分别是 `orders`、`orders#dup1`、`orders`，都是 Table。id 1、2、3，选中最后一次。Rune 向 stderr 打了一次重复键警告，指向 `lib.rs:245`。
- 判定：通过。重复键警告是同名表的结果，内存库没有同名表；两行都能点开，处理函数没有串。

### P10 树的点击不开查询，也不改网格

- 目的：先有一张查询标签时，点表行只追加表标签。不把查询改掉，不改会话表名，不改网格，不离开当前节。
- 输入：API 连接后 `open_query_tab("draft")`，再 `open_table("customers")` 记下网格。点 `tree-table-customers`。
- 预期：标签为 `(query, draft)` 然后 `(table, customers)`。选中新的表标签。Query 仍是 1 个。会话表名仍是 `["customers"]`。网格 `(0,1)` 不变。节仍是 Connections。
- 实际：与预期一致。`[(1, Query, draft), (2, Table, customers)]`，选中 2。
- 判定：通过。点击只走 `open_table_tab`。没有调用 `workspace.open_table`（网格内容是探针自己事先打开的）。把表放进标签里的网格是 S3，不在本任务。

### P11 多行时父级高度包得住，靠后的行能点

- 目的：专门看「父级高度 0 把标签裁没」。六行时高度应为 `6*28 + 5*4 = 188`，最后一行仍可点，并且点开的是那一行的名字。
- 输入：表名 `t0`…`t5`。检查每一行的矩形、可见矩形、标签可见高度。点 `t5`，再点 `t0`。
- 预期：树高 > 0 且不小于 188。每行高约 28，可见高度 > 20，标签可见高度 > 8，行与行不重叠，底边不超出树。点 `t5` 得到 `(1, Table, t5)`，再点 `t0` 得到 `(2, Table, t0)`。
- 实际：树矩形 `[16, 341.25, 192, 188]`，不透明度 1。六行可见矩形都是完整的 192×28，标签可见高度 15。两次点击与预期一致。
- 判定：通过。有行时父级不是高度 0，最后一行没有被裁掉。

### P12 已经静止的行，frame(0) 仍能点；关标签不清树

- 目的：只有“这一帧新出现”的行会停在不透明度 0。已经显示过的行在 `frame(0.0)` 后仍可点。关掉它打开的标签后，会话和树都还在。
- 输入：连接并完成首帧。再 `frame(0.0)`。点行。淡入后点 `tab-close-1`。
- 预期：不透明度在 0 帧前后都 > 0.9。点开 `(1, Table, customers)`。关闭后标签空、选中 `None`，会话表名仍是 `customers`，行仍在且可见高度 > 20，`object-tree` 还在。
- 实际：不透明度前后都是 1。关闭后树行可见矩形仍是完整的 28 高。
- 判定：通过。

断开连接不在本次范围。`WorkspaceApi` 和这版 UI 都没有断开入口；会话为 `None` 时树是空的（P1）。S5 才要求断开后清树和标签。

## 结论与遗留

**放行。**

对照 S2 合同：无会话时 `object-tree` 在且没有表行；连接 `local` 之后内存会话只有 `customers`，对应一行 `tree-table-customers`；点该行（淡入完成之后）打开 `TabKind::Table`，标题是表名；再点一次再开一个同名表标签，没有改成 S1 所不允许的去重；切侧栏节之后树还在；点击不开查询标签；空白表名不是 UI 造出来的；有行时父级高度按行数和间距算，`customers` 和六行探针的标签都没有被裁成高度 0。选中、关闭、id 不复用仍符合 S1。GR1、GR2 通过。P1–P12 未发现要拒绝的缺陷。

无缺陷可记。没有需要按「文件 + 行 + 输入 + 期望 vs 实际」列出的 bug。

遗留（非拒绝项）：

- 新行的进入淡入是 Rune 默认行为。`runtime.frame(0.0)` 时 `tree-table-customers` 的呈现不透明度是 0，点击命中后面的侧栏，标签不会开。约 0.2 秒之后可以点。既有单测用的是 `frame(1.0)`，与这条一致。
- 对象树点击只调用 `open_table_tab`（`lib.rs` 约 253–255 行），不调用 `workspace.open_table`。标签标题对，网格不跟着这下点击换表。按计划留给 S3。
- `open_query_tab` 仍没有产品调用点，留给 S4。
- 同名表会在 `lib.rs:245` 的键上触发一次 Rune 重复键警告，后一行被改成 `tree-table-{name}#dupN`，两行仍都能点开。内存库没有同名表。
- 超长表名的文本可以画出 192 宽的行。`customers` 不会。侧栏也没有在很多张表时滚动；六行（高 188）仍在 900 高的视口里。断开并清空树是 S5。
