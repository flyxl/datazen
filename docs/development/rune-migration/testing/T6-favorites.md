# T6 收藏 — 测试报告

- 任务：`FavoriteQuery` 按桌面语义存 favorites 根目录下的 `.sql` 文件（`id`=文件名词干，ULID 铸造，改名不改文件）；保存当前 SQL/列表/打开/删除。
- 日期：2026-11-14
- 提交：见 tasks.md T6 行。

## 结论

通过。GR1 136+55、GR3 42 全绿。

## 实现面（如实）

- **文件格式（与桌面逐字节同规）**：`crates/datazen-rune-host/src/favorites.rs`——连续 `-- key: value` front-matter 块（title/connectionId/createdAt/updatedAt）+ 语句体 + 恰一个尾换行；值转义（`\`、换行）保证元数据永不可能溢出行；手工文件不规范化（无 front-matter 的纯 SQL 照载，标题从文件名派生）；BOM 容忍。格式出处注释指向桌面 `crates/datazen-desktop/src/store/favorites`（宿主不依赖桌面 crate——它是 Tauri 应用；格式同构是任务要求）。
- **身份**：`id` = 文件名词干 = 26 字符 Crockford base32 ULID（48 位毫秒时间戳高位 + 80 位随机——随机源用宿主已链接的 `uuid` v4 字节，零新依赖；同毫秒单调递增守卫）。文件名序 = 创建序。
- **生命周期**：保存=铸造 ULID 写新文件（原子写）；改名=只改 front-matter 的 title（updatedAt 刷新），文件名不动；删除=移入 `.trash/{时间戳}__{id}.sql`（字节永不清除）；列表=递归扫描（深度上界 16、跳过点目录、`.sql` 扩展名、id 白名单校验），按 id 降序（新者在前）。
- **词汇**：`DesktopAdapter` 四个默认方法 `favorites`/`save_favorite`/`rename_favorite`/`delete_favorite` + serde-free 的 `FavoriteSpec{id, connection, title, sql}`。内存端口无收藏——UI 诚实拒绝 "favorites need the live host"（演示进程），与 T5 入站引用同判。
- **UI**：Settings 页 Favorites 区（snippets 之后）：标题输入 + "Save current query"（语句取当前查询编辑器的文档——"保存当前 SQL"；连接取活会话的 id，无会话为空串）；清单行带 Open（开同名查询页签并载入语句）/Rename（按表单标题改）/Delete。

## 测试（GR1 新增 1 例；GR3 新增 4 例）

- UI `the_favorites_panel_saves_opens_renames_and_deletes`：演示进程拒绝（错误行）→ 宿主适配器：编辑器 SQL + 标题保存 → 行出现 → Open → Workspace 页签标题对、编辑器文档=收藏语句 → Rename 表单标题生效 → Delete 清单空。踩坑：保存后行刚挂载时按 Open 命中测试跳过未完成浮现动画的元素——补帧（T1 教训清单再添一笔）。
- 宿主 `the_file_shape_is_the_desktops`：磁盘文件形状断言（title 行在最前、元数据块连续、语句+单尾换行）；id 是 26 字符 Crockford；同毫秒两次铸造严格递增。
- 宿主 `list_rename_delete_round_trip_and_the_file_stays`：改名后文件名不变、无第二个文件出现、语句存活、新仓储重开读回；删除后清单空、`.trash` 恰一条 `{时间戳}__{id}.sql` 且内容保留。
- 宿主 `a_hand_written_file_loads_verbatim`：无 front-matter 的手工文件原样载入（正文=全文）、标题派生自词干；带转义序列的标题不溢出到语句。
- 宿主 `unsafe_ids_are_refused`：`../escape` 拒绝、未知名拒绝、空标题/空语句拒绝。

## 门禁

- GR1 `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1`：136 + 55 passed。
- GR2 `cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline`：exit 0。
- GR3 `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1`：42 passed。
- GR4 `cargo check -p datazen-rune-host --offline`：exit 0。
- G1–G4（前端）：未改 `packages/frontend`、`packages/web`，不适用。

## 遗留（如实）

- 收藏的 keyword/database front-matter 字段读取未接（ui-api 的 `FavoriteSpec` 只有四字段；补全词汇是后续任务）。
- 桌面 `favorites_root` 可配置（AppSettings），宿主固定 `data_dir/favorites`——设置面未有该项，等设置域扩展。
- folder（子目录收藏）在宿主扫描已支持（递归），UI 表单未提供目录选择。
