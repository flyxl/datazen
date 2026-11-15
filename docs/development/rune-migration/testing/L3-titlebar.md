# L3 自绘标题栏 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 任务：L3 自绘窗口 chrome——`App::titlebar(Titlebar::new(40))`、标题栏 `.window_drag()` 拖拽带、左侧 section 标题、右侧主题切换按钮、`min_size` 960×640、`Cx::titlebar_inset` 让位平台窗口控件；demo 与宿主两个入口同改。
- 被测提交：见 tasks.md L3 行（feat）；本报告随 docs 提交落库。
- 改动面：`crates/datazen-ui/src/lib.rs`（视图结构、`run`/`app_with_adapter` 入口常量与 builder）。不改 `packages/frontend`、`packages/web`、宿主 crate 代码与内核契约（宿主经 `datazen_ui::app_with_adapter` 复用同一入口）。

## 结论

通过。根布局由“侧栏+内嵌 header 的页面”重构为“40pt 自绘标题栏 + body(侧栏, 页面内容)”，旧 `datazen-header` 移除；标题栏整带可拖拽且前缘按 `cx.titlebar_inset()+12` 让位红绿灯/caption，主题切换复用既有 `toggle_theme()`（经 settings port）。两入口统一使用 `TITLEBAR_HEIGHT=40`、`WINDOW_MIN_SIZE=(960,640)` 并挂 `Titlebar::new(40.0)`。GR2 全绿（154 passed）。

## 实现面

- **视图**：`datazen-titlebar`（`window_drag`、`h(40)`、`pl(titlebar_inset+12).pr(12)`）内含 `datazen-titlebar-label`（当前 section）与 `datazen-theme-toggle`（on_press → `toggle_theme`）；页面内容移入 `datazen-body` 下的 sidebar+page，页面不再自带 header。
- **入口**：`run()` 与 `app_with_adapter()` 的 `AppSpec` 设 `min_size: Some((960.0, 640.0))`，`App` 链 `.titlebar(Titlebar::new(40.0))`；宿主 `datazen-rune-host` 通过 `app_with_adapter` 获得同一 chrome。

## 测试

| 测试 | 位置 | 覆盖 |
|---|---|---|
| `the_titlebar_renders_a_drag_band_label_and_theme_toggle` | `crates/datazen-ui/src/lib.rs` | titlebar/label/toggle/body key 存在、旧 header 不存在（headless Runtime elements 断言） |

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR2 | `CARGO_TARGET_DIR=$PWD/target cargo test -p datazen-ui --offline --lib -- --test-threads=1` | **154 passed / 0 failed** |
| GR4 | `CARGO_TARGET_DIR=$PWD/target cargo check -p datazen-rune-host --offline` | 通过，exit 0 |
| GR1 | 沿用 L1 基线（未改 ui-api） | 56 passed |
| GR3 | 沿用 L1 基线（未改宿主代码） | 59+3 环境失败 |

## 备注

- headless 环境 `titlebar_inset` 取 0；真实平台 inset 由 rune 在窗口化运行时提供，view 侧已按 inset 计算前缘 padding。
- 分支 rustfmt 基线不一致，按周边风格手写；CI 不跑 fmt。