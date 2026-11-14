# TN P2 外观明暗 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`0221bf0` `feat(rune): the theme setting drives light and dark (issue-rune P2)`
- `git show --stat 0221bf0` 一个文件：`crates/datazen-ui/src/lib.rs`（view 从端口读 Theme（缺省回退系统暗色）、设置页 `settings-page` 替换空态、Appearance 区 `settings-theme` 切换按钮只写 P1、1 条新测试 + 1 条旧空态测试更新）。没有 `packages/frontend`、`packages/web`。React 未改

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0。`datazen-ui` 76 passed（净增 1：`the_theme_toggles_through_the_port_and_survives_reopening`；`the_settings_page_is_an_empty_state_not_the_old_preview_card` 更名为 `the_settings_page_is_its_own_page_not_the_old_preview_card`——设置页自本任务起是真实页面，占位卡断言保留），4.21s；`datazen-ui-api` 29 passed |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| G1–G4 | vitest / pnpm | 不适用。本提交未改 `packages/frontend` 或 `packages/web` |

环境同前（rustc 1.99.1，`--offline`）。

## 验证

按 tasks.md 验收标准「外观：明暗。只写 P1。切换后重新打开仍在」核对，断言来自 GR1 实跑：

1. **切换与保持**（`the_theme_toggles_through_the_port_and_survives_reopening`）：设置页点 `settings-theme` → 端口 `Theme { dark: false }`；离开设置页再回来 → 仍是 false（端口保存）；再点 → 回 true。view 的暗色来源改为端口设置（代码核对：`view` 读 `read_settings()` 的 Theme，缺省回退 `cx.system_dark()`）。通过

## 缺陷

无。

## 结论与遗留

**放行。**

GR1、GR2 通过。G1–G4 不适用。明暗只经设置端口写入与读取；设置页从空态变为真实页面（Appearance 区）。

遗留（不构成拒绝）：明暗即时生效但无过渡动画；跟随系统选项未做（当前缺省回退即系统暗色）。
