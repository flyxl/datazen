# T7 连接导入 — 测试报告

- 任务：`connection_import::parse_import_file` 六格式解析 → 预览清单 → 确认后建入文件连接存储（重名去重）。
- 日期：2026-11-14
- 提交：见 tasks.md T7 行。

## 结论

通过。GR1 137+55、GR3 44 全绿。

## 实现面（如实）

- **词汇**：`DesktopAdapter::parse_connections_import(path, password)` → `ImportPreview{handle, format, connections, skipped}` + `confirm_import(handle) -> u32`；serde-free 的 `ImportedConnectionSpec{name, engine, address}`。**handle 是关键设计**：预览与确认之间，完整 `ConnectionConfig`（凭据、端口、隧道）留在宿主的暂存里——UI 只读它展示的形状，确认按 handle 取回，无信息丢失。
- **宿主**：`parse_connections_import` 读文件（宿主是唯一有文件系统权限的一侧，字节不过适配器边界）→ `parse_import_file`（路径扩展名+字节嗅探自动路由六格式）→ 规格投影进预览、完整配置按 AtomicU64 句柄入暂存。`confirm_import` 取暂存 → `list_plain` 拿现有名单 → 重名跳过 → 其余经 `ConnectionService::save` 落文件存储（两个仓储调用都走 sync runtime——文件存储已载入，内存速）。空密码 = 共享层的 `None`。
- **宿主特性**：`datazen` 特性加 `connection_import`（桌面已启用——依赖全在锁文件，离线解析成功）。
- **UI**：连接页表单下导入区：密码输入（加密格式用；明文格式忽略）+ "Import connections…" 按钮（T2 的 `open_file`）→ 预览面板（格式标签 + 连接清单 + "Import N connections" 确认）→ 确认后通知行报落地数并刷新连接列表。取消=丢弃预览。演示进程诚实拒绝 "connection import needs the live host"。
- **踩坑（如实）**：最初词汇无密码参数——加密格式（TablePlus/DataZen 导出）被共享层如实拒绝 "Password is required"。修正是补密码通道而非绕过：词汇加 `password`，UI 加输入框，明文格式照旧。

## 测试（GR1 新增 1 例；GR3 新增 2 例）

- UI `the_connections_page_imports_a_previewed_file`：假适配器（Rc 记录器）记录解析路径与确认句柄 → 按钮开 Open 对话框 → 选 `/exports/connections.tableplusconnection` → 预览面板列出两条（格式标签 + 两条清单行）→ 确认 → 通知含 "2 connections imported"、预览离场、宿主收到的路径与句柄（42）都核对。
- 宿主 `connection_import_parses_previews_confirms_and_dedups`：`build_tableplus_export`（桌面导出同一条代码路径）造真格式文件（warehouse + 已存在的 live）→ 解析出 TablePlus 预览两条 → 确认前未知句柄拒绝 → 确认落地 1 条（live 重名跳过）→ 存储恰两条 → 同文件再导入再确认落地 0 条（全重名）。
- 宿主 `a_bad_import_file_is_an_honest_error_and_nothing_lands`：垃圾字节文件解析如实报错（RNCryptor 拒绝）、存储零新增；缺文件同样如实。

## 门禁

- GR1 `cargo test -p datazen-ui-api -p datazen-ui --offline --lib -- --test-threads=1`：137 + 55 passed。
- GR2 `cargo check -p datazen-ui-api -p datazen-ui -p datazen-rune-host --offline`：exit 0。
- GR3 `cargo test -p datazen-rune-host --offline --lib -- --test-threads=1`：44 passed。
- GR4 `cargo check -p datazen-rune-host --offline`：exit 0。
- G1–G4（前端）：未改 `packages/frontend`、`packages/web`，不适用。

## 遗留（如实）

- 其余五种格式（DataZen/DBX/DBeaver/Navicat/DataGrip）未逐一做端到端样例测试——解析路径是共享层自己的（桌面版已有测试矩阵），宿主测试钉的是"真格式文件→预览→确认→去重"的接线环；按格式补充样例属共享层的测试责任。
- 预览不含 skipped 清单的展示（词汇携带，UI 未画）——诚实省略，后续补一行即可。
- 桌面版的 `resolve_import_files`（从各 App 安装位置自动发现）未接——宿主只做显式选文件。
