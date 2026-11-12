# TN S1 标签模型 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 提交：`52e7bab` `feat(rune): open, select and close workspace tabs (issue-rune S1)`（`52e7babe8ec3c4c412392442473fc62da148f69d`）
- 设计：`docs/development/rune-migration/plan.md`「工作区壳」任务 S1（标签只允许表和查询；打开、选中、关闭）
- 源码：`crates/datazen-ui-api/src/tabs.rs`（模型）、`crates/datazen-ui-api/src/lib.rs`（再导出）、`crates/datazen-ui/src/lib.rs`（`tab_strip`，键 `tab-{id}` / `tab-close-{id}`）
- `git show --stat 52e7bab` 只含上述 3 个 Rust 文件（+191 / −1）。`packages/frontend` 与 `packages/web` 无差异。React 未改。
- 探针只在 `/tmp/s1_ui_probe.rs` 临时编译，跑完已删。产品源码未改。

## 门禁

环境：`CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 RAYON_NUM_THREADS=1 RUSTFLAGS="-C debuginfo=0"`。`--offline`。

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 18 passed；`datazen-ui-api` 3 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web`，未启动前端测试 |

编译期有既有 `rune` `private_interfaces` 告警，以及 `datazen-ui` 非测试构建里 `open_table_tab` / `open_query_tab` 的 `dead_code` 告警。不导致门禁失败。打开入口按计划留给 S2/S4，不记为 S1 缺陷。

## 探针

12 条新输入，挂在 `datazen-ui` 的临时测试模块，`cargo test -p datazen-ui --offline --lib s1_ui_probe -- --test-threads=1`。12 passed，0 failed。不是既有单测的重跑。

### P1 关闭选中的中间标签

- 目的：三个标签里关掉选中的中间一个，选中应落到左边，而不是右边或清空。
- 输入：`Table "customers"`、`Table "orders"`、`Query "scratch"`（id 1、2、3），`select(2)`，`close(2)`。
- 预期：剩下 `(1, table, customers)`、`(3, query, scratch)`，选中 1。
- 实际：与预期一致。
- 判定：通过。

### P2 反复关闭选中的第一个

- 目的：关掉第一个时选中新的第一个；一直关到空，选中变为 `None`。
- 输入：依次打开 `a` `b` `c` `d`（id 1–4），`select(1)` 后按 1、2、3、4 关闭。
- 预期：选中依次为 2、3、4、`None`；最后集合为空。
- 实际：与预期一致。
- 判定：通过。

### P3 关闭后再打开，id 不复用

- 目的：id 从 1 起，关闭（含全部关闭）后不回收。
- 输入：打开 1、2，全部关闭，再打开；再关 3，再打开。
- 预期：关闭后选中 `None`；新 id 为 3，然后 4、5。1 和 2 不再出现。
- 实际：与预期一致。
- 判定：通过。

### P4 已有标签后再给空白标题

- 目的：`trim` 后空标题返回 `None`，且 `next_id`、标签、选中都不变；内部空白保留。
- 输入：先 `open(Table, " customers ")`。再对 Table 与 Query 各试 `""`、空格、`\t`、`\n`、`\r\n`、`" \t\n "`、U+00A0、U+3000、U+2003、`" \u{2028}\u{2029} "`、`"\u{000B}\u{000C}"`。然后 `open(Query, "  a \t b\n")`。比较整份 `Tabs`（含 `next_id`）的 `PartialEq`。
- 预期：首个标题为 `customers`、id 1。空白全部 `None`，状态与打开后的快照相等。下一成功 id 为 2，标题 `a \t b`（两端去掉，中间制表符还在）。
- 实际：与预期一致。
- 判定：通过。

### P5 未知 select，再关另一个标签，并重复关已关 id

- 目的：未知 id 的 select/close 不改状态；关未选中标签不改选中；同一已关 id 关两次仍为 false。
- 输入：两个同名 `Table "orders"`（id 1、2）加 `Query "q"`（id 3），选中 3。`select(0)`、`select(99)`、`select(11)`。再 `close(1)`，然后对 1 再 `close` 两次。
- 预期：三次 select 都是 false，快照不变。`close(1)` 为 true，剩下 id 2、3，选中仍是 3。再次 close 为 false，快照不变。
- 实际：与预期一致。
- 判定：通过。

### P6 查询夹在两个同名表之间

- 目的：同名表标签允许并存；中间查询标签种类不丢；关未选中的中间或第一个时选中不动。
- 输入：`Table "orders"`、`Query "orders"`、`Table "orders"`。先选中第一个再关中间查询；再选中最后一个再关第一个。
- 预期：打开后选中第三个，种类为 table / query / table。关中间后剩下两个 table，选中仍是第一个。关第一个后只剩最后一个，选中不变。
- 实际：与预期一致。
- 判定：通过。

### P7 超长标题

- 目的：长标题只做两端 trim，不截断、不 panic；超长空白仍拒绝且不占 id。
- 输入：`" \n\t" + "x"*80000 + " \u{00A0}"` 作为表标题；`" "*80000` 作为查询标题；再打开 `"y"*10000` 的查询（两端有空格）。
- 预期：id 1，选中标题长度 80000 且全为 `x`。空白拒绝，`Tabs` 相等。下一 id 为 2，标题长度 10000，第一条标题仍是 80000。
- 实际：与预期一致。
- 判定：通过。

### P8 从右往左关，再关已关 id

- 目的：空集合上的 close/select 为 false；关选中的最右标签时选中左邻；关光最后一个后为 `None`；已关 id 再操作不改状态。
- 输入：空模型上 `close(1)`、`select(1)`。打开四个 Query `a`–`d`，从 id 4 关到 id 1。再 `close` 1 和 4、`select` 2。
- 预期：空操作都是 false。选中依次为 4、3、2、1、`None`。之后状态与关光时的快照相等。
- 实际：与预期一致。
- 判定：通过。

### P9 UI：点选中标签的关闭键

- 目的：`tab-close-{id}` 会关掉该标签，并且不把已关标签留在选中上。关闭键与选择键是兄弟，不应让关闭失效。
- 输入：`open_table_tab("customers")`、`open_table_tab("orders")`、`open_query_tab("draft")`，frame 后点 `tab-close-{第三个}` 的中心。
- 预期：三个 `tab-{id}` 与 `tab-close-{id}` 都在。点关闭后第三个消失，选中变为第二个（左邻），`tab-{第三}` 与 `tab-close-{第三}` 都找不到。
- 实际：与预期一致。
- 判定：通过。

### P10 UI：点未选中标签的关闭键

- 目的：若关闭键误触发表选择，关中间未选中标签会把选中挪到其左邻。此探针专门卡这个。
- 输入：表 `customers`、查询 `draft`、表 `orders`，打开后选中第三个。先点 `tab-close-{中间}`，再点 `tab-close-{第一个}`。
- 预期：两次之后选中始终是第三个。中间、第一个依次消失，最后只剩第三个。
- 实际：与预期一致。关闭未选中标签不改选中。
- 判定：通过。

### P11 UI：点选中的第一个再关

- 目的：条带上先选中第一个，再关它，应选中新的第一个，已关 id 不能仍是选中。
- 输入：三个表 `customers` / `orders` / `parts`。点 `tab-{第一个}`，再点 `tab-close-{第一个}`。
- 预期：点标签后选中第一个。关闭后剩下后两个，选中原第二个。`tab-{第一个}` 与 `tab-close-{第一个}` 消失。
- 实际：与预期一致。
- 判定：通过。

### P12 UI：空白打开、同名两表、关到空

- 目的：空白标题不出现在条带上；同名两表各有自己的键；关选中的第二个回到第一个；再关最后一个，选中 `None`，键消失，`tab-strip` 仍在。
- 输入：`open_table_tab("   ")`、`open_query_tab("\t\n")`，再 `open_table_tab("  orders  ")` 与 `open_table_tab("orders")`。点第二个的关闭键，再点第一个的关闭键。
- 预期：两次空白都是 `None`。有效标题都是 `orders`，两条键都在。关第二个后选中第一个、只剩一个。再关后集合空、选中 `None`，两个 id 的键都没有，`tab-strip` 还在。
- 实际：与预期一致。
- 判定：通过。

## 结论与遗留

**放行。**

对照 S1 合同：种类只有 `Table` / `Query`；`open` 会 trim，空白返回 `None` 且不改状态、不占 id；id 从 1 起且关闭后不复用；打开即选中，同名表可以并存；未知 `select` / `close` 返回 false 且不改状态；关选中标签选左邻，关的是第一个则选新的第一个，关光则 `None`；关未选中标签不改选中；条带键为 `tab-{id}` 与 `tab-close-{id}`。P1–P12 与 GR1/GR2 均未发现违反。

无缺陷可记。没有需要按「文件 + 行 + 输入 + 期望 vs 实际」列出的 bug。

遗留（非拒绝项）：

- 对象树点击打开留给 S2。本提交的条带只做选中和关闭。非测试构建里 `open_table_tab` / `open_query_tab` 没有调用点（`crates/datazen-ui/src/lib.rs` 约 116–121 行），探针是直接调这两个方法把标签放进模型后再渲染的。
- 未把 `next_id` 打到 `u32::MAX`。溢出回绕不在本次可执行探针范围内，也不构成当前失败。
