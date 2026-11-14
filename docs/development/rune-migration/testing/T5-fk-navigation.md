# T5 外键导航 — 测试报告

- 任务：出站 FK 单元格"前往引用行"（打开引用表按引用列过滤）；入站 FK"被引用"（`list_inbound_foreign_keys`）清单打开引用表带过滤。
- 日期：2026-11-14
- 提交：见 tasks.md T5 行。

## 结论

通过。GR1 135+55、GR3 38 全绿。

## 实现面（如实）

- **出站（零新服务调用）**：N10 已接的 `table_foreign_keys` 规格自带目标表/列——聚焦列在某个 FK 的 `columns` 里时，表格页过滤栏下方出现"→ 目标表 · 目标列"导航片；按下开目标表并按目标列过滤聚焦格的值（复合键按聚焦列自己的配对导航，单列答案，与过滤栏口径一致）。
- **入站**：新词汇 `DesktopAdapter::table_inbound_references`（默认固定错误→UI 诚实拒绝"the live host lists the references"，内存端口无扫描不做假）+ `InboundRef{table, column, referenced_column, constraint}`（把 `InboundForeignKeyRef` 的列配对拍平成 UI 清单行）。"Referenced by…" 开清单，每行"← 引用表 · 列 (references 本表列)"，按下开引用表并按其 FK 列过滤**当前聚焦行**的被引用列值。
- **共享定位助手**：`open_table_with_filter`（T4 的值搜索定位重构为同一实现：页签自己的打开会重置过滤（S3 生命周期），过滤在打开后落、页重读）。
- **宿主**：`LiveNavigationSource` 实现 `ForeignKeyNavigationSource`——**绑同步会话**（`sync_session()` 的自有连接，跑在适配器的 sync runtime 上）。踩坑记录：最初绑活会话（worker runtime 的池）跨 runtime 用导致 `queryFailed`——这正是 N10 开独立同步连接的原因，注释里写明因果。会话标签传 "live"（信封要求非空）；租约 guard 为 token（同步会话活在适配器里）。`list_inbound_foreign_keys` 的列配对拍平成 `InboundRef`。

## 测试（GR1 新增 2 例；GR3 新增 1 例）

- UI `a_foreign_key_cell_navigates_to_the_referenced_row`：演示 orders（复合 FK orders[id,name]→customers[id,name]）聚焦 id 列 → 导航片出现 → 按下 → customers 页签打开、过滤 `id=1`（聚焦格值）；catalog 表（无 FK）无导航片。
- UI `the_referenced_by_list_opens_the_referencing_table_filtered`：默认适配器下按 "Referenced by" → 诚实拒绝行；换宿主适配器（应答 orders.id→customers.id）→ 清单出现 → 按行 → orders 页签打开、过滤 `id=0`（customers 聚焦行的 id 值）。踩坑：直接状态调用后视图不重建，`settle_all()` 补帧后才落（记录为测试接缝习惯）。
- 宿主 `inbound_references_answer_the_scan_over_the_live_session`：夹具库 `orders.user_id REFERENCES users(id)` → `table_inbound_references("users")` 恰一条 `(orders, user_id, id)`；events 无引用 → 诚实空清单。踩坑：请求信封要求非空 `db_session_id`。

## 门禁

- GR1 `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1`：135 + 55 passed。
- GR2 `cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline`：exit 0。
- GR3 `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1`：38 passed。
- GR4 `cargo check -p datazen-rune-host --offline`：exit 0。
- G1–G4（前端）：未改 `packages/frontend`、`packages/web`，不适用。

## 遗留（如实）

- 复合出站键的整行导航（多列同时过滤）未做——桌面版由 `resolve_foreign_key_targets` 服务承担完整语义（NULL/复合/存在性探测）；Rune 侧先用 N10 规格的单列配对，服务接缝（`ForeignKeyNavigationSource` 已在宿主实现）为后续整行导航敞开。
- 入站清单在远程驱动上属同步路径（同 N10 判据），远程驱动接入时移作业通道。
- 环境记录：沙箱磁盘被 cargo 新布局的陈旧哈希目录占满（1780 个），已写 `scripts/prune_cargo_build.py`（每包保留最新）清理，后续磁盘紧张时复用。
