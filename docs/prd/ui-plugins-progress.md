# 插件系统开发进度管理

> 流程：编码 agent 开发 + 单测 → commit → 新测试 agent 输出 E2E 用例与结果（覆盖率 ≥80%，只报不修）→ commit → bug 循环（验证不通过→修复中→待验证→已修复）。
> 分支：`feature/ui-plugins`（worktree：`../datazen-ui-plugins`）。PRD：[ui-plugins.md](./ui-plugins.md) v0.5；技术方案：[ui-plugins-implementation.md](./ui-plugins-implementation.md)。

## 功能工作项

| # | 功能 | 范围摘要 | 状态 | 开发 commit | 测试 commit |
|---|------|---------|------|------------|------------|
| F1 | Rust 插件基座 | plugins/{mod,manifest,install,storage}.rs、IPC 命令组、AppState、单测（capabilities 走既有 ACL 豁免，见测试记录） | 已完成 | 900b9330 | d9d265b3 |
| F2 | datazen:// 协议 | register_uri_scheme_protocol：path 资产服务 + open 深链 + CSP/403/404 | 已完成 | 4c75f1b0 | ffdf64b3 | —（仅追加测试文件，未 commit） |
| F3 | 前端状态与 IPC 封装 | types/plugin.ts、pluginStore、workspaceTabsStore、commands/plugins.ts（27 单测；全量 vitest 3 个既有失败与本功能无关） | 待测试 | 149d3b2a | — |
| F4 | 主窗口集成 | WorkspaceMode 扩展、aside 两按钮、Workspace 导航栏/默认卡片/独立 Tab 条/页面壳（静态） | 未开始 | — | — |
| F5 | 插件管理页 | PluginManagementPage + InstallPluginDialog（卡片/过滤/安装/启停/卸载） | 未开始 | — | — |
| F6 | RPC 桥 | uiPluginBridge：信封路由、权限判定、限流超时、token 快照推送 | 未开始 | — | — |
| F7 | Settings 外观 | settings.appearance 菜单项 + AppearanceSection 主题切换器 | 未开始 | — | — |
| F8 | SDK 包 | packages/ui-plugin-sdk（bridge/theme/theme.css/useTheme） | 未开始 | — | — |
| F9 | 示例插件与 E2E | e2e/fixtures/sample-plugin + e2e/specs/plugins.spec.ts journeys 1-5 | 未开始 | — | — |

## Bug 跟踪

| ID | 功能 | 描述 | 重现步骤 | 状态 |
|----|------|------|---------|------|
| BUG-F2-01 | F2 | 【处置：backlog/P2 加固，不阻断】Windows 形态解析面宽于规格：`http(s)://datazen.<host>/<path>`（`datazen.` 后无 `/` 直接接 host）也被接受为合法别名 | `parse_datazen_uri("http://datazen.acme.bill-audit/index.html")` 返回 Ok（与 `http://datazen./acme.bill-audit/index.html` 等价）。规格 §2.4 字面仅定义 `http://datazen./<host>/<path>`。同一校验链（存在→enabled→路径→MIME）仍然全部生效，无安全影响，属低危加固项（可在 strip_scheme 中要求紧随分隔符） | 新建 |

Bug 状态流转：`新建 → 验证不通过(修复中) → 待验证 → 已修复`

## 测试记录

（每个功能测试完成后在此追加小节：用例清单、结果、覆盖率、bug 链接）

### F1（Rust 插件基座，commit 900b9330）

- 测试 agent 会话，2026-08-22。规格依据：ui-plugins-implementation.md §2.2/§2.3/§2.5/§2.6。
- 执行命令：
  - `cargo test -p datazen --lib plugins` → **60 passed / 0 failed**（54 既有单测 + 6 新增集成用例）
  - `cargo test -p datazen --lib commands::mcp -- --test-threads=1` → 6/6 PASS。注：mcp 组并行跑时因全局句柄竞态偶挂（`start_embedded_mcp_reports_running` 等 3 例），单线程即稳定通过；**既有 flaky，与 F1 无关**。

#### 新增测试文件

`src-tauri/src/plugins/integration_tests.rs` + `plugins/mod.rs` 内一行 `#[cfg(test)] mod integration_tests;` 接线（零发布代码影响）。落点说明：lib.rs 中 `mod plugins` 为 crate 私有，外部 `tests/` 目标无法访问 crate 内部；放宽可见性属功能代码改动被禁止，故集成套件以 lib 测试目标编译，运行入口仍为 `cargo test -p datazen --lib`。

#### 用例清单

既有单测（54 例，全部 PASS）：

| 编号组 | 模块 | 场景 | 数量 | 结论 |
|--------|------|------|------|------|
| U-01–U-12 | install.rs | zip/目录安装、顶层目录 zip、穿越条目拒绝、隐藏/.sh/.exe 条目拒绝、可配置大小限额、压缩比炸弹、重装备份清理、失败保留旧包+清 staging、缺源报错 | 12 | PASS |
| U-13–U-39 | manifest.rs | id 缺点号/大写/超长、apiVersion 失配、backend 非 null、semver 正反例、page id/showIn/modes、entry 缺失或文件不存在、目录名≠id、声明路径穿越/隐藏组件/非法扩展名、包扫描限额/symlink/恶意 svg/marker 跳过、未知权限字符串、顶层/page 多余字段、纯主题免 entry 免权限 | 27 | PASS |
| U-40–U-47 | storage.rs | set/get 往返、跨插件隔离、缺键 None、remove 存在性与空表清文件、超 1MB 拒绝、非法 id/key、8 线程并发原子写、无 tmp 残留 | 8 | PASS |
| U-48–U-54 | commands/plugins.rs | 全生命周期流程、目录安装+重装、无效包拒绝、storage 需已注册插件+命名空间、read_plugin_file 沙箱规则、load_from_disk 恢复 enabled 态、Summary camelCase 序列化 | 7 | PASS |

新增集成用例（6 例，全部 PASS）：

| 编号 | 场景 | 预期 | 实际 | 结论 |
|------|------|------|------|------|
| I-01 | 安装 zip→list→set_enabled(false)→list→重启模拟 reload→enable→remove→重复 remove 全流程 | summary/list/enabled 落盘正确；disabled 仍列出且读取被拒；reload 恢复 disabled；storage 数据跨启停保留；remove 删目录含 .storage.json；二次 remove 报 NotFound | 符合预期 | PASS |
| I-02 | 恶意 zip：`../` 穿越、嵌套 `assets/../../x.html`、绝对路径 `/etc/*.html`、Windows 盘符 `C:/Windows/*.html`、`.sh` 扩展名 | 安装全部拒绝、registry 保持为空、无 `.staging-*`/`*.old.bak` 残留 | 符合预期 | PASS |
| I-03 | 超 50MB 声明大小 zip（51 MiB deflated） | 基于中央目录尺寸在解压前拒绝（uncompressed size limit），无残留 | 符合预期 | PASS |
| I-04 | manifest 边界×9（经 parse→validate 与安装端到端双路径）：id 缺点号、id 双点号 `ac.me.bill`、apiVersion=1、apiVersion=3、`backend:{}`、未知权限串、顶层多余字段、page 对象多余字段、icon 文件缺失；规则 2 文案含 DataZen 版本指引 | 各项精确报错；安装路径无残留 | 符合预期 | PASS |
| I-05 | storage 跨插件隔离（双真实插件同 key）+ 未注册插件 IPC 拒绝 + 1MB 边界（MAX−512 可写、追加超限拒绝且原数据不损坏） | 命名空间互不可见；ghost 报 NotFound；超额写失败不影响已有键 | 符合预期 | PASS |
| I-06 | storage remove 幂等：底层 bool true→false、IPC 层两次 remove 均 Ok、空表删文件 | 幂等无错误、文件清理 | 符合预期 | PASS |
| I-07* | read_plugin_file 沙箱：正常/嵌套读取、`.enabled`/`.storage.json`/`assets/.secret.css` 拒绝、`../`、绝对路径、反斜杠穿越拒绝、缺失文件与未知插件 NotFound、未启用读取被拒 | 一律按约束拒绝且错误信息不含内部路径泄露 | 符合预期 | PASS |

\* I-02/I-03 合并为一个测试函数 `malicious_zips_rejected_without_side_effects`，I-07 对应 `read_plugin_file_enforces_sandbox_rules`；共 6 个 `#[tokio::test]` 函数。

#### 覆盖率结论（cargo llvm-cov 0.8.7，实测）

仅运行 `plugins` 过滤测试（`cargo llvm-cov --lib --summary-only -- plugins`）：

| 文件 | 行覆盖 | 区域覆盖 | 函数覆盖 |
|------|--------|---------|---------|
| src/plugins/mod.rs | 80.00% | 68.42% | 80.60% |
| src/plugins/manifest.rs | **92.44%** | 84.62% | 94.24% |
| src/plugins/install.rs | **87.11%** | 55.84% | 87.33% |
| src/plugins/storage.rs | **92.37%** | 84.00% | 94.61% |
| src/commands/plugins.rs | **86.33%** | 61.73% | 83.10% |

**行覆盖率 80%–92%，五个文件全部 ≥80% 达标**。区域（分支）覆盖未达 80% 的两处局限如实说明：install.rs 未覆盖分支集中在防御性溢出臂（checked_add overflow）、备份为文件的罕见分支、zip 炸弹 reader 的 entry 超长臂；commands/plugins.rs 未覆盖区域主要是 `#[tauri::command]` 宏包装层与 spawn_blocking JoinError 臂——均需 Tauri 运行时或人为构造 IO 故障才能触达。

#### Bug 列表

无功能缺陷（0 FAIL）。以下为规格偏差登记：

| ID | 类型 | 描述 | 重现步骤 | 期望 vs 实际 | 状态 |
|----|------|------|---------|-------------|------|
| DEV-F1-01 | 备注 | §2.3/§2.5 的 `install_plugin_with_dialog` 未实现，仅提供 `install_plugin_from_path` | `grep -rn install_plugin_with_dialog src-tauri/src/` 无结果 | 规格：dialog 选 zip/目录入口命令；实际：dialog 流程延后至 F5 InstallPluginDialog（前端发起 dialog + from_path）。功能等价，范围决策 | 备注（F5 承接） |
| DEV-F1-02 | 备注 | §2.6 要求 default.json.host 增补 `allow-list-plugins` 等 permission identifier，实现未增补 | 查看 `src-tauri/capabilities/default.json.host` 无 plugin 条目；仓库所有 host 命令同样未列入（Tauri v2 应用自身命令不受 ACL 约束） | 规格：增补标识；实际：走应用命令 ACL 豁免（进度表 F1 已预先声明该决策），IPC 实测可达 | 备注（既定决策） |
| DEV-F1-03 | 备注 | 规则 2 错误文案为「需要更新版本的 DataZen >= x.y.z」，规格字面为「需要 DataZen ≥ x.y.z」 | apiVersion≠2 时观察错误信息 | 语义一致（告知宿主版本过旧并给出版本号），措辞差异 | 备注 |
| DEV-F1-04 | 备注 | 集成测试落点为 `src/plugins/integration_tests.rs`（非技术方案 §7 规划的 `tests/plugins_*.rs`） | 见「新增测试文件」说明 | 因 `mod plugins` 私有且禁止放宽可见性而调整；如需回归规划路径，须由开发将模块改为 pub | 备注 |

#### 规格复核结论（逐条对照 §2.2/§2.3/§2.5/§2.6）

符合项：校验规则 1–7 全部落实（含目录名==id、semver、backend 拒绝、pages→entry 必填、路径穿越/隐藏组件双重防护、扩展名白名单、50MB/2000 文件限额、纯主题免权限）；安装 staging→校验→原子改名→备份/回滚链路完整且有测试背书；storage 原子写（同目录 tmp+rename）+全局互斥+1MB 限额；read_plugin_file 四重约束（enabled 门禁/条目名校验/隐藏组件拒绝/canonicalize 包含性）；IPC 9 命令齐全且统一 CommandError+CmdExt 日志约定；`plugins:changed` 事件在 install/remove/set_enabled 后 emit（代码审查确认）；AppState.plugins 字段、finish_app_state 启动扫描、lib.rs 注册 9 命令全部就位。偏差项见 DEV-F1-01…03。

### F2（datazen:// 协议，commit 4c75f1b0）

- 测试 agent 会话，2026-08-22。规格依据：ui-plugins-implementation.md §2.4；ui-plugins.md §5（沙箱与加载）。
- 执行命令：
  - 基线：`cargo test -p datazen --lib plugins` → **78 passed / 0 failed**（F1 后基线绿）
  - 终态：同命令 → **104 passed / 0 failed**（78 既有 + 26 新增安全用例，0.9s）
  - 覆盖率：`cargo llvm-cov --lib --lcov -- plugins`（cargo llvm-cov 0.8.7）

#### 新增测试文件

`src-tauri/src/plugins/protocol_security_tests.rs` + `plugins/mod.rs` 内一行 `#[cfg(test)] mod protocol_security_tests;` 接线（零发布代码影响，未改任何功能代码）。聚焦攻击面：编码绕过 / Windows 形态 / host 混淆 / open 深链参数 / 错误契约 / MIME 白名单边界 / 符号链接置换防御。落点理由同 F1（`mod plugins` crate 私有）。

#### 规格复核结论（逐条对照 §2.4 与 PRD §5）

| 规格条目 | 实现 | 结论 |
|----------|------|------|
| URL 双形态兼容 | `strip_scheme` 同时接受 `datazen://` 与 Windows `http(s)://datazen./`，scheme 大小写不敏感、按前缀精确匹配（`xdatazen://`、单斜杠、外域 `http://evil.com/datazen./…` 全拒） | ✅ 符合（S-07/S-13） |
| path/command 判定与保留字 | 首段 == `open` 为深链；`open` 后多段一律 404；保留字大小写敏感（`OPEN` 按资产处理）；未知 command 落资产分支→MIME 白名单拒绝→404 | ✅ 符合 |
| 安全校验链 存在→enabled→路径组件→MIME | route_datazen_request：get(404) → enabled(403) → open 校验 page ∈ contributes.pages(404) → safe_relative_path（空段/`.`开头隐藏组件/反斜杠/NUL 全拒，先解码后校验）→ content_type_for 白名单 → is_file → canonicalize 包含性 → 读盘 | ✅ 符合且顺序一致 |
| 响应头三件套 CSP/nosniff/no-cache | datazen_response 对**每个**响应固定注入三头；CSP 串与 §2.4 文本逐字符一致（含 `connect-src 'none'`，对齐 PRD §5 script-src 'self' 立场）；由 protocol.rs 既有测试 `security_headers_are_always_injected` 直接断言 PASS | ✅ 符合 |
| 403/404 无 body 不泄露 | 错误响应 body 为空 Vec；错误类型为裸 `http::StatusCode`，无任何细节载荷可泄露（S-21 全攻击面循环断言） | ✅ 符合 |
| open 深链 emit `plugins:open-page` payload `{pluginId,pageId,params}` | emit_open_page 以 camelCase 三键构造 JSON；params 排除 `page` 后原样转发字符串值；事件名常量经 S-20 断言 | ✅ 符合 |

#### 用例清单（新增安全用例 26 例，全部 PASS）

| 编号 | 场景 | 结论 |
|------|------|------|
| S-01 | 全编码穿越 `%2e%2e%2f%2e%2e%2fsettings.json`（含大写 `%2E%2E%2F`、纯 `%2e%2e`、混合形态） | PASS |
| S-02 | 编码反斜杠绕过 `%2E%2E\`、全编码 `%5c`、裸反斜杠文件名 | PASS |
| S-03 | 双重编码 `%252e%252e%252fsecret.html`：仅解码一次成字面名，永不复活为 `../`（含双编码 NUL `%2500`） | PASS |
| S-04 | UTF-8 overlong（`%c0%ae`/`%c0%af`）lossy 解码为 U+FFFD，无法伪造 `.` `/` 或复活 `.storage.json` 隐藏名；非法续字节 `%ff%fe` | PASS |
| S-05 | NUL 截断类 `index.html%00`、`icon.svg%00.exe`、`%00.html`：解码后全局 NUL 检查拒绝 | PASS |
| S-06 | 畸形转义 `a%.html`/`%zz`/尾部孤立 `%`：保持字面量不 panic | PASS |
| S-07 | Windows 形态等价性：http/https/大写 scheme 三形与 native 同解析同字节（含 query） | PASS |
| S-08 | Windows 形态反斜杠穿越（编码与裸 `\`） | PASS |
| S-09 | `http://datazen.<host>/` 无分隔符别名宽松接受 → 登记 BUG-F2-01（低危加固项，非缺陷行为恶化） | PASS（记录偏差） |
| S-10 | 盘符组件 `C:/evil.json`：Unix 下不存在即 404；Windows 下由 canonicalize 包含性兜底（join 截断语义） | PASS |
| S-11 | `http://datazen.` 空段/缺路径：host 为空串被拒 | PASS |
| S-12 | host 混淆 8 变体：大小写、尾点、`:8080` 端口、百分号编码 host（host 从不解码）、前后空白、多点号 | PASS |
| S-13 | scheme 大小写不敏感但前缀精确：单斜杠/`xdatazen://`/反斜杠形式/外域前缀全拒 | PASS |
| S-14 | open params 特殊字符原样转发：`&`/`=`/`%`/加号空格/中文（`a%26b%3Dc`→`a&b=c`、`中文`、`100%`），`page` 键排除 | PASS |
| S-15 | open 缺/空/空白 `page` 参数 → 404（含 Windows 形态） | PASS |
| S-16 | `open/sub` 多段 → 404（native + Windows 双形态、尾斜杠） | PASS |
| S-17 | page id 精确匹配：大小写不符、带 `/`、带 `../` 的值均 404 | PASS |
| S-18 | 保留字大小写敏感：`OPEN?...` 走资产分支 404 | PASS |
| S-19 | 重复 `page` 参数 last-wins（BTreeMap 语义文档化） | PASS |
| S-20 | `PLUGINS_OPEN_PAGE_EVENT` 常量 == `"plugins:open-page"`（前端契约） | PASS |
| S-21 | 攻击面错误契约：全部返回裸 StatusCode（404/403），无细节泄露；disabled 双形态 403 | PASS |
| S-22 | MIME 全表断言（10 扩展 × 小写/大写/混合大小写）+ 14 种未知扩展（exe/sh/txt/zip/gif/jpg/htm/wasm/dll/bat/cmd/ps1 等）拒绝 | PASS |
| S-23 | 全部 10 个白名单扩展经路由实测回正确 Content-Type + 字节 | PASS |
| S-24 | 最终扩展名生效策略（`logo.svg.html`→text/html 不 sniff）；尾点空扩展名 404 | PASS |
| S-25 | 安装后投放的非白名单文件（exe/sh/txt/zip）经 register 直载模拟，全部不可达 404 | PASS |
| S-26 | 符号链接置换防御（unix）：包内别名可服务；指向包外的符号链接被 canonicalize 包含性拦下 404 | PASS |

既有回归：protocol.rs 18 例功能单测 + F1 60 例全数通过，无回归。

#### 覆盖率结论（cargo llvm-cov 0.8.7，过滤 `-- plugins` 实测）

| 文件 | 行覆盖 | 区域覆盖 | 函数覆盖 |
|------|--------|---------|---------|
| **src/plugins/protocol.rs** | **90.21%** | **94.87%** | **89.91%** |

**行覆盖 ≥80% 目标达标（90.21%）**。未覆盖主体集中在 L301–355：`handle_datazen_request` / `emit_open_page` / `datazen_response` 的 Tauri 运行时接线层——需要真实 `AppHandle`/`UriSchemeContext`（tauri "test" feature 未启用，引入需动 Cargo.toml，超出本次"只加测试文件"的授权范围）。该层风险已由三重旁证覆盖：① `datazen_response` 被 protocol.rs 既有测试直接调用并逐字符断言三头值与空 body（PASS）；② lib.rs:723 注册点经代码审查确认；③ 路由层全部判定逻辑（parse/route/path/MIME）已 90%+ 直测。其余零星未覆盖行为既有测试中 `assert!` 的 panic 分支（不可达即正确）。

#### Bug 列表

无功能缺陷（0 FAIL）。登记 1 个低危加固项 + 2 条备注：

| ID | 类型 | 描述 | 重现步骤 | 期望 vs 实际 | 状态 |
|----|------|------|---------|-------------|------|
| BUG-F2-01 | 低危加固 | Windows 形态解析面宽于规格字面：`datazen.` 后无 `/` 直接接 host 也被接受 | `parse_datazen_uri("http://datazen.acme.bill-audit/index.html")` → Ok 且与规范形等价 | 规格：仅 `http://datazen./<host>/<path>`；实际：`http(s)://datazen.<host>/<path>` 别名同样可达。同一校验链仍全程生效，无安全影响；如需收紧可在 strip_scheme 要求紧随 `/` | 新建 |
| DEV-F2-01 | 备注 | MIME 表为 themePackApply.ts 映射的**超集**而非"同表复制"：新增 html/js/mjs/css/json（插件 UI 页面必需）；共享 5 项（svg/webp/png/woff2/woff）取值一致无冲突 | 对照 src/lib/themePackApply.ts `MIME_BY_EXT`（5 项）与 content_type_for（10 项） | 规格措辞"同表复制"；实际为合理超集，主题包场景取值完全兼容 | 备注 |
| DEV-F2-02 | 备注 | Tauri 接线层（handle_datazen_request 及 emit_open_page）无自动化直测 | 见覆盖率结论 L301–355 说明 | 单测需 tauri mock runtime；当前以 datazen_response 直测 + 注册点审查旁证。若 F9 E2E 需要端到端协议验证，建议届时补 WebdriverIO 层用例 | 备注（F9 承接评估） |

## 回归测试

- [ ] 全量回归（cargo test -p datazen --lib + npx vitest run）
- [ ] 文档更新（架构文档 docs/architecture/backend/plugins.md、AGENTS.md 精简增补）
- [ ] 合并 main
