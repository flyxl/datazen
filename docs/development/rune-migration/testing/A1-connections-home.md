# A1 连接首页/树逐页对齐（阶段一：首页）验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 任务：A1 连接首页/树逐页对齐（本提交覆盖连接首页：首页 Hero + 连接卡列表 + 最近查询区块的结构与样式对齐；连接树层级 pinned/最近/分组/值搜索后续继续）。
- 被测提交：见 tasks.md A1 行（feat）；本报告随 docs 提交落库。
- 改动面：`crates/datazen-ui/src/lib.rs`（`connection_list`/Connections 主页重排、测试）。不改 `packages/frontend`、`packages/web`、宿主与内核契约。

## 结论

通过。Connections 主页由扁平列表重构为对齐 React 首页的信息结构：`connections-home-hero`（标题与连接数/引擎类型数摘要）、`connections-card-list`（“Your connections”分组标题与卡片式连接项，保留 `connection-{id}` 交互键与既有消息/加载/表单/导入能力）、`connections-recent-queries`（“Recent queries”区块；当前无历史数据源时显示空态）。既有连接表单保存、导入、桌面适配器消息与连接打开行为保持兼容。

## 实现面

- **Hero**：`connections-home-hero`/`connections-home-title`/`connections-home-subtitle`，基于当前可见连接列表统计数量与引擎种类。
- **卡片列表**：`connections-card-list` + `connections-section-title`，每项保留 `key("connection-{id}")` 与点击打开行为，展示名称/引擎与 Connect 视觉按钮。
- **最近查询**：`connections-recent-queries` + `connections-recent-title` + `connections-recent-empty`（结构占位，待历史数据源接入后可填充）。
- **兼容**：保留 `connection-list` 根键、`desktop-adapter-message`、`connect-loading`、`connection-form*`/输入与保存键、导入相关键。

## 测试

| 测试 | 位置 | 覆盖 |
|---|---|---|
| `connections_home_shows_hero_cards_and_recent_queries_section` | `crates/datazen-ui/src/lib.rs` | hero/卡片区/最近查询区块与关键文案/键存在 |
| 既有连接相关测试（8 项） | 同上 | 连接页交互、表单、导入、适配器消息、打开连接等兼容 |

## 门禁

| 门禁 | 命令 | 结果 |
|---|---|---|
| GR2 | `CARGO_TARGET_DIR=$PWD/target cargo test -p datazen-ui --offline --lib -- --test-threads=1` | **160 passed / 0 failed** |
| GR4 | `CARGO_TARGET_DIR=$PWD/target cargo check -p datazen-rune-host --offline` | 通过，exit 0 |
| GR1 | 沿用（未改 ui-api） | 56 passed |
| GR3 | 沿用（未改宿主） | 59+3 环境失败 |

## 备注

- A1 全量目标还包含连接树层级（pinned/最近/分组/值搜索）的逐页对齐，本报告记录首页部分；后续提交继续推进树侧对齐。
- 仅改 Rune UI 与文档；React/内核契约未动。
