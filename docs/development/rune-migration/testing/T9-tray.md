# T9 托盘 — 测试报告

- 任务：宿主启动时装 `TraySpec`（图标+菜单：显示窗口/退出），事件路由到窗口与退出；无会话总线时启动照常、托盘缺席并记日志。
- 日期：2026-11-14
- 提交：见 tasks.md T9 行。

## 结论

通过。GR1 138+55、GR3 48 全绿；wasm 目标照常编译（H6 不变量保持）。

## 实现面（如实）

- **ui 接缝**：`datazen_ui::app_with_adapter(adapter) -> App<AppState>`——宿主在构造与 `run` 之间装进程级能力；`run_with_adapter` 改为其薄壳。`AppState` 转公开（返回类型所需；字段仍私有）。
- **宿主 tray 模块**：`spec()`=稳定 id `datazen-native` + tooltip + 32×32 RGBA 程序化图标（实心圆盘，alpha 地）+ 菜单（Show DataZen / 分隔 / Quit）；`install(handle)`=`Tray::new` 装托盘（rune 自带每托盘一线程的 SNI/dbusmenu 后端，Linux 需 `dbus` 特性——宿主 crate 增该 rune 特性依赖）；`Ok` 则 `mem::forget` 保命（Drop 会摘图标），`Err` 记 stderr 照常返回（窗口界面不受影响）。事件路由：主点击或除 Quit 外的菜单项 → `handle.show()+focus()`；Quit → `handle.quit()`；`#[non_exhaustive]` 兜底臂忽略。
- **main.rs**：`app_with_adapter` 构造 → `tray::install(app.handle())` → `app.run()`。
- **环境语义（如实登记）**：rune 的托盘在无托盘宿主的 Linux 桌面（GNOME 无扩展）`Tray::new` 仍成功、图标待宿主出现；本沙箱无会话总线——`install` 的 Err 路径即此环境实测路径。

## 测试（GR3 新增 2 例）

- `the_menu_and_the_icon_shape_are_what_the_tray_carries`：菜单项清单（`TrayItem` 公开词汇）逐项断言；图标 32×32 RGBA、有实心像素有透明像素；spec 在两者之上构造。**测可观测的契约**：rune 的 `TraySpec` 字段是 `pub(crate)`（无 getter），宿主测试改测本模块的菜单/像素函数（spec 构造覆盖在内），不伪造可读性。
- `a_tray_that_cannot_start_is_an_honest_absence_not_a_crash`：本沙箱（无会话总线）真调 `Tray::new`——Err 则记探针日志，Ok 则丢弃；调用必须返回、永不 panic。断言钉住"结果被吞、界面照常"的契约形态。

## 门禁

- GR1 `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1`：138 + 55 passed。
- GR2 `cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline`：exit 0。
- GR3 `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1`：48 passed。
- GR4 `cargo check -p datazen-rune-host --offline`：exit 0。
- **wasm（H6 不变量）**：`cargo build -p datazen-ui --target wasm32-unknown-unknown --offline` → Finished（首次失败是磁盘满所致，`prune_cargo_build.py` 清理后通过；宿主 crate 不进 wasm 图，dbus 特性不污染 wasm 构建）。
- G1–G4（前端）：未改 `packages/frontend`、`packages/web`，不适用。

## 遗留（如实）

- macOS 无托盘（rune 后端未支持）——`Tray::new` 如实报 Unsupported，install 吞错继续。
- 托盘菜单位/勾选态（checked/enabled 的动态翻转）未用——当前菜单是静态两项；有状态菜单（如"当前连接"子项）属后续。
- 真机托盘宿主（KDE/waybar）渲染验证留给用户环境；沙箱只证接缝与容错路径。
