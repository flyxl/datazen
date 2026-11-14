# T1 系统剪贴板 — 测试报告

- 任务：表格网格与结果网格的选中格 Ctrl+C 复制为 TSV；编辑器补齐复制/剪切/粘贴；粘贴接线从死的 sheet 挂点移到应用根。
- 日期：2026-11-14
- 提交：见 tasks.md T1 行。

## 结论

通过。三个新测试全绿，既有 124 个 datazen-ui 测试无回归。

## 实现面（如实）

- **rune 的剪贴板词汇**：`Tx::copy`（写）、`Tx::request_paste`（读请求）、根元素 `on_paste`（无平台输入框聚焦时的粘贴落点）、`Runtime::use_clipboard`（测试注入 memory 板）。
- **表格网格**：Ctrl+C 复制选中矩形为 TSV——此路径 N 系列前已存在（`data_grid::on_key`），本次补 2×2 多格的 memory 板断言（原来只有单格 + `runtime.copy_request` 读法）。
- **结果网格**：原来完全不可选中、按键不可达。现在结果格可点选（`GridSlot::Result` 的 `cell()` 挂 on_press），按下后 `result_focused` 置位，方向键/PageUp/Home/Shift 扩选/Ctrl+C 走 `data_grid::on_key`；打字或 Escape 或按编辑器表面把键盘还给编辑器；新结果落格或断开连接时复位。
- **编辑器**：自绘编辑器没有平台内建复制——原计划"沿用 rune 内建"的前提不成立，如实补齐 `Command-C/X/V`（`selected_text`/`cut_selection` 两原语，多光标复制按行拼接，纯光标不写剪贴板）。
- **粘贴接线修正**：`on_paste` 只在根（或模态根）生效；原表格 sheet 上的 `on_paste` 是死接线（元素非根，永远收不到）。移除之，改为根元素路由：表格页可编辑网格收 TSV（`paste_tsv`），查询页聚焦编辑器收文本。行内编辑器（平台 text_input）保留自己的粘贴路径。

## 测试（GR1 内新增 3 例，均过）

1. `the_result_grid_takes_the_keyboard_and_copies_tsv`：跑 `SELECT * FROM CUSTOMERS` → 点 `result-cell-0-1` → 焦点 (0,1)、`result_focused` 置位；Shift+Down/Right 扩成 2×2；Command-C 后 memory 板读回 `cell(0,1)\tcell(0,2)\ncell(1,1)\tcell(1,2)`；打字 `x` 键盘回编辑器；Command-V 后文档以 "x pasted" 结尾。
   - 踩坑两则，测试已如实绕行：结果面板浮现动画未完成时透明度为 0，命中测试跳过全透明元素——运行后补一帧再点；粘贴请求经动作队列往返，需要两帧落文本。
2. `the_table_grid_copies_a_2x2_selection_as_tsv`：连接 → 点 `cell-0-0` → Shift 扩选 2×2 → Command-C → memory 板读回四格 TSV。
3. `command_c_copies_the_editor_selection_and_command_x_cuts_it`：find 选中 "SELECT" → Command-C 板上是 "SELECT"；纯光标 Command-C 不写板（板上保留旧值）；find 选中 "FROM" → Command-X 板上 "FROM"、文档变为 `SELECT one  t`。

## 门禁

- GR1 `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1`：datazen-ui 127 passed（124 既有 + 3 新增）、datazen-ui-api 55 passed。
- GR2 `cargo check -p datazen-ui-api -p datazen-ui --offline`：exit 0。
- GR3/GR4（宿主）：本任务未改 `datazen-rune-host`，宿主不引用改签名的 `sql_editor::on_key`（已核），不适用。
- G1–G4（前端）：未改 `packages/frontend`、`packages/web`，不适用。

## 遗留（如实）

- 首次全量编译暴露 3 个预先存在的 warning（`datazen-ui-api` O5 遗留的未用绑定 `stored`、`datazen-ui` 两处多余 `mut`），不属于本任务文件，未动；门禁不因 warning 失败。
- 结果网格的行内复制菜单（右键）不存在——本产品尚无右键菜单体系，Ctrl+C 是唯一入口，与桌面版的差距如实记录。
