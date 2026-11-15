# L2 分割条组件 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 任务：L2 可拖拽分割条组件（`crates/datazen-ui/src/splitter.rs`）——垂直/水平两向、Drag 手势 → clamp → `Setting::Layout` 持久化、取消回滚、双击复位、键盘微调（arrows ±1 / Page ±10 / Home/End）、`role="separator"` + a11y 值。
- 被测提交：见 tasks.md L2 行（feat）；本报告随 docs 提交落库。
- 改动面：`crates/datazen-ui/src/splitter.rs`（新）、`crates/datazen-ui/src/lib.rs`（`mod splitter`、`AppState.splitter_preview` 字段与 Default）。不改 `packages/frontend`、`packages/web`、宿主与内核契约。

## 结论

通过。分割条核心逻辑可纯函数测试（`Drag` 的取消/窗口字段在 rune 内部不可从外部构造，因此手势决策拆为 `size_after_drag`/`drag_commit`/`finish_drag`），并提供 `splitter_handle` 元素构建器（`on_drag` 按 phase 分派、`on_double_press` 复位、`on_step`/`on_step_limit` 键盘微调、`role("separator")`+label+a11y_value、focusable）。GR2 全绿（153 passed，含新增 6 项）。

## 实现面

- **纯函数**：`size_after_drag(travel,start,min,max)->u32`（四舍五入+clamp，容忍 min/max 互换）；`size_after_step(current,delta,min,max)->u32`；`drag_commit(cancelled,travel,start,min,max)->Option<u32>`。
- **状态**：`AppState.splitter_preview: Option<(String,u32)>`——拖拽中 `splitter_size` 返回预览，Up 时 `finish_drag` 清预览并按需 `set_layout_size`；取消则丢弃预览、持久值不变。
- **元素**：`splitter_handle(state,key,axis,min,max,default)`——5pt 把手（垂直 `.w(5).h_full()` / 水平 `.w_full().h(5)`），Down 记录起始到 `drag.data`，Move 更新预览，Up 提交或取消；双击持久化默认值；水平条键盘步进取反以保持“向下/向右增大”直觉。

## 测试

| 测试 | 位置 | 覆盖 |
|---|---|---|
| `drag_clamps_and_rounds` | `crates/datazen-ui/src/splitter.rs` | travel 换算、四舍五入、上下 clamp、零 travel |
| `drag_handles_swapped_bounds` | 同上 | min/max 互换容错 |
| `step_moves_and_clamps` | 同上 | ±1/±10 增量与边界 clamp |
| `cancel_discards_the_preview_and_keeps_the_persisted_size` | 同上 | 取消回滚：预览清除、持久值不变 |
| `commit_persists_the_dragged_size` | 同上 | 提交持久化并清预览 |
| `reset_restores_the_default` | 同上 | 复位到默认值并持久化 |

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR2 | `CARGO_TARGET_DIR=$PWD/target cargo test -p datazen-ui --offline --lib -- --test-threads=1` | **153 passed / 0 failed**（含 splitter 6 项） |
| GR1 | 沿用 L1 基线（未改 ui-api） | 56 passed |
| GR3/GR4 | 未改宿主/入口，沿用 L1 基线 | 59+3 环境失败 / check 绿 |

## 备注

- `splitter_handle` 尚未接入具体布局面板（由 L4/L5 等任务挂载），当前为可复用组件与已测逻辑；dead_code 警告预期随接线消除。
- 分支 rustfmt 基线不一致，按周边风格手写；CI 不跑 fmt。