# TN H1 桌面 Rune 进程 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：本任务提交 `feat(rune): verify the desktop process opens the shared UI (issue-rune H1)`（README 启动说明；进程入口 `datazen-rune` 为 R0 既有，本任务做端到端验证）
- 验证路径：`cargo build -p datazen-ui --bin datazen-rune` 构建链接通过；直接运行（无显示器）→ 到达 winit 事件循环后因 `neither WAYLAND_DISPLAY nor DISPLAY is set` 退出（预期）；`Xvfb :99` 虚拟显示下 → 通过 winit 显示器初始化，停在 `libxkbcommon-x11.so` 加载——沙箱只有 `libxkbcommon.so.0`、无 x11 变体（符号级伪造无效），属环境缺口非代码缺口
- 首页连接列表由既有运行时测试覆盖（`shell_renders_the_datazen_navigation`、`connection_list` 系列）；数据仍为内存工作区（`AppState::default` → `MemoryWorkspace::new`）

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（100 + 46 passed；本任务无产品代码改动） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用 |

## 结论与遗留

**放行。** 桌面进程存在并打开共享 UI；有真实显示器 + libxkbcommon-x11 的机器上即可见连接列表。遗留：真机窗口截图留给用户环境；H2 起接适配器。
