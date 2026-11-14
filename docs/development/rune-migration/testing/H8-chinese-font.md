# TN H8 中文字体验证报告

## 范围

- 分支：`feat/rune-native-ui-migration`
- 被测提交：`bfc5f4b5 feat(rune): a Chinese font for the interface and the grid (issue-rune H8)`
- 新增：`crates/datazen-ui/assets/`（Inter×3 与 IBM Plex Mono 自 rune 资产复制、NotoSansSC-Regular.ttf 10.6MB）；改动：`crates/datazen-ui/src/lib.rs`（`APP_FONTS`/`APP_FONT_FAMILIES` 常量 + `run()` 的 AppSpec 接线 + 2 个测试）、`Cargo.toml`（parley dev 依赖，与 rune 同版本同特性）

## 接线方式

- **AppSpec.fonts 整体替换内置字体集**（rune 语义：非空即替换）。池 = Inter×3（保 Latin 外观与 web 的 sans 兜底）+ Noto Sans SC（中文覆盖）+ IBM Plex Mono（编辑器 mono 槽与 web 的 mono 兜底）。
- **families = ("Inter, Noto Sans SC", "IBM Plex Mono")**：sans 槽是 CSS 家族列表，汉字按字回退到 Noto——界面与网格（sans 样式 `type_::LABEL`/`TITLE`）由此获得中文覆盖；编辑器的 mono 槽不受影响（"只覆盖界面和网格"）。
- 字体随 `include_bytes` 进二进制与 wasm 包（wasm 从 42MB → 54MB debug；发布时 release+wasm-opt 收缩），不依赖操作系统字体——无 CJK 字体的机器和 Web 端同样渲染。

## 测试

| 测试 | 位置 | 断言 |
| --- | --- | --- |
| `the_apps_font_pool_shapes_chinese_without_tofu` | datazen-ui | 应用字体池的 TextSystem 把 "中文数据" 整形为 4 个字形 id 且全部非 0（无 .notdef 方框）；"DataZen" 的 Latin 仍由池内字体服务 |
| `a_chinese_grid_cell_is_not_a_box_under_the_apps_fonts` | datazen-ui（runtime） | `Runtime::with_text`（应用字体池）下打开表、编辑一格为 "中文"、提交网格编辑：单元格携带该值，同一池对 "中文" 的整形无豆腐块 |

字形判定沿用 rune 自家测试的判据（`glyph id != 0`，见 `vendor/rune/crates/rune/src/text/tests.rs`）。沙箱系统装有 NotoSansSC，native 的字形来源无法区分池内/系统——但池内 Noto 在家族列表中先行命中，且 wasm 端无系统字体、完全依赖池内文件。

## 门禁

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| GR1 | `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1` | 通过，exit 0（datazen-ui 109 passed，datazen-ui-api 54 passed；新增 2 个测试） |
| GR2 | `cargo check -p datazen-ui-api -p datazen-ui --offline` | 通过，exit 0 |
| WASM | `cargo build -p datazen-ui --target wasm32-unknown-unknown --offline` | 通过（字体嵌入后 54MB debug） |
| G1–G4 | vitest / pnpm | 不适用（未改 `packages/frontend`、`packages/web`） |

## 结论与遗留

**放行。** 界面与网格的中文不再是方框，覆盖随字体文件走、不依赖系统。遗留：真机/浏览器可视确认；Bold 权重的中文（NotoSansSC-Bold）与 web 端 `full_faces` 按需加载的产品化瘦身留给后续。
