# TN H2 适配器类型 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`69b82c82 feat(rune): the desktop adapter types answering not-implemented (issue-rune H2)`
- 改动面：`crates/datazen-ui-api/src/lib.rs`（`DesktopAdapter` trait、`DESKTOP_NOT_IMPLEMENTED` 固定错误、`UnimplementedAdapter`）、`crates/datazen-ui/src/lib.rs`（`AppState.desktop` 适配器字段、进入连接页/进程启动时询问适配器、固定错误在连接页的展示、端口列表兜底）

## 实现

- **适配器类型**（ui-api）：`DesktopAdapter` 只含 Host 系列要接的三个用例——`list_connections`（H3）、`read_table_page`（H4，签名对齐 `open_table`：表名、页号、页大小、排序、过滤）、`commit_cells`（H5，一批 `(行, 列名, 新值)`，返回落库条数）。`UnimplementedAdapter` 每个方法返回同一句固定错误 `DESKTOP_NOT_IMPLEMENTED`（"not implemented in the desktop adapter yet"）。
- **UI 能显示这句**（ui）：`AppState` 持有适配器；进入连接页（`select`）和进程启动（`run` 的 `init_state`）都会询问一次 `list_connections`。成功（H3 之后）时适配器列表替换端口演示列表；失败（现在）时端口的演示列表保留，页面顶部以弱色提示行显示适配器的原句。演示不回归，H3 只改适配器。

## 测试

| 测试 | 位置 | 断言 |
| --- | --- | --- |
| `the_unwired_adapter_answers_every_method_with_the_fixed_error` | datazen-ui-api | 三个方法都得到同一句固定错误 |
| `the_unwired_desktop_adapter_shows_its_fixed_error_and_keeps_the_demo_list` | datazen-ui（runtime） | 点击导航进入连接页后，页面上渲染出该句原文（经 `Runtime::elements` 读文本），且 `connection-local` 演示列表仍在、适配器没有替换列表 |
| `the_desktop_adapter_message_leaves_the_connections_page_with_the_section` | datazen-ui（runtime） | 离开连接页后提示行消失（消息属于连接页） |

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（datazen-ui 102 passed，datazen-ui-api 47 passed；新增 3 个测试） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0（仅存量警告，无 H2 引入） |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 适配器类型就位，未接通的方法统一答固定错误且 UI 能显示这句。遗留：H3 起逐个接真用例。
