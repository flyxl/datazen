# L6 ContentStatusBar 40 验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 任务：L6 ContentStatusBar 40pt（连接状态 + 库/表/列/行计数 + 快捷键提示），替换底部 “Migration preview” 横幅。
- 被测提交：见 tasks.md L6 行（feat）；本报告随 docs 提交落库。
- 改动面：`crates/datazen-ui/src/lib.rs`（`content_status_bar`、页面骨架替换 footer、测试）。不改 `packages/frontend`、`packages/web`、宿主与内核契约。

## 结论

通过。页面底部 28pt “Migration preview…” 横幅替换为 40pt `content-status-bar`：左侧显示连接状态（Connected/Disconnected 指示点+文案），中部显示可用上下文（连接名/所选表或查询/列数/行数，缺省回退当前 section），右侧显示快捷键提示（⌘N New query · ⌘W Close）。

## 实现面

- `content_status_bar(state, theme)`：40pt 全宽行，键 `content-status-bar`；子区域 `status-connection-dot`/`status-connection-label`、`status-context`、`status-shortcuts`。
- 上下文派生：`session_open()` 决定连接态；连接名从 `desktop_connection` + `desktop_connections` 解析；表 tab 在 `grid.has_page()` 时附加 `cols`/`total_rows()`；query tab 在 `result_grid` 有页时附加计数。
- `datazen-page` 底部子元素由迁移横幅改为 `content_status_bar(...)`。

## 测试

| 测试 | 位置 | 覆盖 |
|---|---|---|
| `content_status_bar_replaces_migration_preview_with_context` | `crates/datazen-ui/src/lib.rs` | 状态栏存在且 40pt；连接态/上下文/快捷键子键存在；“Migration preview” 文案移除；快捷键提示渲染 |

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR2 | `CARGO_TARGET_DIR=$PWD/target cargo test -p datazen-ui --offline --lib -- --test-threads=1` | **158 passed / 0 failed** |
| GR4 | `CARGO_TARGET_DIR=$PWD/target cargo check -p datazen-rune-host --offline` | 通过，exit 0 |
| GR1 | 沿用（未改 ui-api） | 56 passed |
| GR3 | 沿用（未改宿主） | 59+3 环境失败 |

## 备注

- 仅改 Rune UI 与文档；React/内核契约未动。按分支约定手写格式。
