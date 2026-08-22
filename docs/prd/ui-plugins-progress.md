# 插件系统开发进度管理

> 流程：编码 agent 开发 + 单测 → commit → 新测试 agent 输出 E2E 用例与结果（覆盖率 ≥80%，只报不修）→ commit → bug 循环（验证不通过→修复中→待验证→已修复）。
> 分支：`feature/ui-plugins`（worktree：`../datazen-ui-plugins`）。PRD：[ui-plugins.md](./ui-plugins.md) v0.5；技术方案：[ui-plugins-implementation.md](./ui-plugins-implementation.md)。

## 功能工作项

| # | 功能 | 范围摘要 | 状态 | 开发 commit | 测试 commit |
|---|------|---------|------|------------|------------|
| F1 | Rust 插件基座 | plugins/{mod,manifest,install,storage}.rs、IPC 命令组、AppState、单测（capabilities 走既有 ACL 豁免，见测试记录） | 已完成 | 900b9330 | d9d265b3 |
| F2 | datazen:// 协议 | register_uri_scheme_protocol：path 资产服务 + open 深链 + CSP/403/404 | 已完成 | 4c75f1b0 | ffdf64b3 | —（仅追加测试文件，未 commit） |
| F3 | 前端状态与 IPC 封装 | types/plugin.ts、pluginStore、workspaceTabsStore、commands/plugins.ts（42 单测全绿；覆盖率 Lines 100%/Branch 96.87%；全量 vitest 4 个失败文件均为分支既有，见测试记录） | 已完成 | 149d3b2a | 7c36c65e |
| F4 | 主窗口集成 | WorkspaceMode 扩展、aside 两按钮、Workspace 导航栏/默认卡片/独立 Tab 条/页面壳 + 管理页/安装对话框提前落地（59 组件测试全绿：37 开发 + 22 测试补充；覆盖率 Lines 97.42%/Branch 89.01%；登记 BUG-F4-01…04 低危缺陷/偏差；BUG-F4-01/02/03/04 已修复并经验证 agent 复核，见「F4 修复验证」小节） | 已完成 | 62141434 | ca2218bc+46c195fb |
| F5 | 插件管理页 | ~~独立功能项~~ 已并入 F4 交付（管理页+两步安装对话框） | 已完成（并入F4） | 62141434 | ca2218bc |
| F6 | RPC 桥 | uiPluginBridge：信封路由、权限判定、限流超时、token 快照推送（开发 31 + 测试补充 33 = 64 单测全绿；覆盖率 Lines 99.27%/100%；安全专项复核通过；登记 BUG-F6-01 低危协议偏差，见测试记录） | 测试完成（BUG-F6-01 低危新建，不阻断） | c77085c8 | —（仅追加测试文件，未 commit） |
| F7 | Settings 外观 | settings.appearance 菜单项 + AppearanceSection 主题切换器 | 未开始 | — | — |
| F8 | SDK 包 | packages/ui-plugin-sdk（bridge/theme/theme.css/useTheme） | 未开始 | — | — |
| F9 | 示例插件与 E2E | e2e/fixtures/sample-plugin + e2e/specs/plugins.spec.ts journeys 1-5 | 未开始 | — | — |

## Bug 跟踪

| ID | 功能 | 描述 | 重现步骤 | 状态 |
|----|------|------|---------|------|
| BUG-F2-01 | F2 | 【处置：backlog/P2 加固，不阻断】Windows 形态解析面宽于规格：`http(s)://datazen.<host>/<path>`（`datazen.` 后无 `/` 直接接 host）也被接受为合法别名 | `parse_datazen_uri("http://datazen.acme.bill-audit/index.html")` 返回 Ok（与 `http://datazen./acme.bill-audit/index.html` 等价）。规格 §2.4 字面仅定义 `http://datazen./<host>/<path>`。同一校验链（存在→enabled→路径→MIME）仍然全部生效，无安全影响，属低危加固项（可在 strip_scheme 中要求紧随分隔符） | 新建 |
| BUG-F4-01 | F4 | 插件停用联动不完整：跨窗口/外部触发的 `plugins:changed` 只刷新 pluginStore，无人调 closeByPlugin，残留可激活的僵尸 Tab | 开插件 Tab → 另一窗口 set_plugin_enabled(false) → 原窗口导航项消失但 Tab/iframe 保留。规格 §4.3/§4.4 要求停用即关 Tab；实际仅管理页内操作联动（PluginManagementPage.tsx:93,110）。建议 F6 统一订阅处理 | 已修复（ca2218bc） |
| BUG-F4-02 | F4 | 「同一插件页多开」不可实现：key=`{pluginId}:{pageId}` + open 幂等，同页重复点击仅聚焦 | Workspace 点击同一导航项两次 → 仅一个 Tab。PRD §4.2/§4.4 允许多开，但 §4.4 表格自身定义该唯一 key，自相矛盾——需产品拍板 | 已修复（产品决议：单实例） |
| BUG-F4-03 | F4 | 安装流程缺「名称/版本/权限清单确认」中间步骤，确认即直接写入 | 管理页[安装插件…] → 输入合法 zip 路径 → Install：无任何预览确认直接安装成功。规格 §4.3 要求写入前展示确认 | 已修复（ca2218bc） |
| BUG-F4-04 | F4 | 管理页默认过滤器为「全部」（规格为默认 Workspace），且「全部」视图平铺不分组 | 打开管理页未点 chip 即显示全部插件平铺列表（PluginManagementPage.tsx:57 初值 'all'）。规格 §4.3：默认 Workspace、「全部」分组 | 已修复（ca2218bc） |
| BUG-F6-01 | F6 | 【处置：backlog/P2 加固，不阻断】【低危/协议卫生，无安全影响】原型链键名作为 API type 时回 `E_PERMISSION` 而非设计文档声明的 `E_NOT_FOUND`：`API_ROUTES` 为普通对象字面量，`__proto__`/`constructor`/`hasOwnProperty`/`toString`/`valueOf` 经 Object.prototype 原型链解析为非 undefined 值，绕过「unknown api → E_NOT_FOUND」门（uiPluginBridge.ts:374-380），落入权限判定后被拒。**无法到达任何 handler**（granted Set 仅含 manifest 字符串），不消耗并发配额 | attachBridge 后从 iframe window 投递 `{ch:'ui-plugin',type:'__proto__',target:'host',reqId:'r1'}` → 收到 `__proto__.err{code:'E_PERMISSION'}`；同型 `constructor`/`hasOwnProperty`/`toString`/`valueOf` 一致。按 uiPluginBridge.ts:126 自述契约与 §3.2 路由语义应为 `E_NOT_FOUND('unknown api')`。修复建议：`Object.prototype.hasOwnProperty.call(API_ROUTES, type)` 或 `Map`/null-prototype 路由表。回归锚点：security.test「denies prototype-chain api type …」（5 例） | 新建 |

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

### F3（前端状态与 IPC 封装，commit 149d3b2a）

- 测试 agent 会话，2026-08-22。规格依据：ui-plugins-implementation.md §4.1；后端契约以 `src-tauri/src/commands/plugins.rs` 与 `plugins/manifest.rs` 为准。
- 执行命令：
  - 目标套件：`npx vitest run src/stores/__tests__/{pluginStore,workspaceTabsStore}.test.ts src/commands/__tests__/plugins.test.ts src/types/__tests__/plugin.test.ts` → **42 passed / 0 failed**（27 既有 + 15 新增，0.7s）
  - 全量回归：`npx vitest run` → **1572 passed / 3 failed tests（4 failed files）/ 207 文件**；`git stash -u` 干净树复跑 → **完全相同的 4 个失败文件**（206 文件 / 1560 用例），确认与本功能无关
  - 覆盖率：`npx vitest run --coverage`（v8 provider，include 仅限四个被测文件）

#### 契约复核结论（逐条对照）

| 契约项 | 前端 | 后端 | 结论 |
|--------|------|------|------|
| `PluginSummary` 字段 | types/plugin.ts:74-85（apiVersion/enabled/permissions/pages/themes） | commands/plugins.rs:49-64 `rename_all="camelCase"`；author/description/icon 走 `skip_serializing_if` 省略 | ✅ 一一对应；TS 侧声明为可选与省略语义兼容 |
| `Permission` 四字符串 | `'context:connections'/'command:invoke'/'storage:local'/'ui:notify'` | manifest.rs:97-105 serde rename + `as_str()`（commands/plugins.rs:77-81 序列化路径） | ✅ 完全一致（fixture 编译期 `satisfies` 锁定） |
| `PluginManifest` | 含 showIn/tokensCss/modes/previewImage/backend?:unknown\|null | manifest.rs:39-96 camelCase + deny_unknown_fields | ✅ 一致；backend v1 恒 null 有 fixture 断言 |
| API 版本 | `UI_PLUGIN_API_VERSION = 2` | `PLUGIN_API_VERSION = 2`（plugins/mod.rs） | ✅ 一致 |
| 命令名 ×9 | invoke 名 list_plugins / get_plugin_manifest / install_plugin_from_path / remove_plugin / set_plugin_enabled / plugin_storage_{get,set,remove} / read_plugin_file | 同名 `#[tauri::command]` 且 lib.rs:977-985 全部注册 | ✅ 一致 |
| 参数键名 | `{pluginId,key}`、`{id,relativePath}`、`{path}`、`{id,enabled}` 等 camelCase | Rust snake_case 形参（plugin_id/relative_path），Tauri v2 自动映射 | ✅ 正确 |
| 事件名 | `PLUGINS_CHANGED_EVENT='plugins:changed'`（commands/plugins.ts:5） | 同值常量并在 install/remove/set_enabled 成功后 emit（commands/plugins.rs:19,340,351,363） | ✅ 一致 |

#### 新增测试文件

既有 `__tests__` 内追加用例（零功能代码改动）：`workspaceTabsStore.test.ts` +5、`pluginStore.test.ts` +4、`commands/plugins.test.ts` +2；新增 `src/types/__tests__/plugin.test.ts`（serde 形态契约 fixture，编译期 `satisfies` 防漂移）。

#### 用例清单（42 例全部 PASS）

| 分组 | 场景 | 数量 | 结论 |
|------|------|------|------|
| pluginStore（12） | fetch 成功填充+清 error、失败置 error（Error 与非 Error 字符串两分支）、loaded 标记、setEnabled 乐观翻转→refetch 对账、仅翻转目标插件不误伤他插件、setEnabled 失败经 refetch 回滚并重抛、remove 成功刷新/失败置 error 重抛、byId 命中与不存在返回 undefined、`plugins:changed` 订阅恰一次+事件触发 refetch 换新数据、listen 失败后守卫复位可重试 | 12 | PASS |
| workspaceTabsStore（17） | open 追加激活、open 幂等（key 冲突原位刷新元数据）、open 重聚焦非激活重复 key、activate 仅对存在 tab 生效、close 矩阵全量：关中间→右邻/关首→右邻/关尾→左邻/关唯一→null/关非激活保持选中/未知 key 无操作、closeByPlugin 批量关+锚点落位（首移除槽位锚定、尾移除左邻回退、他插件激活保持、清空置 null、未知插件 no-op）、`workspaceTabKey` 冒号拼接格式 | 17 | PASS |
| commands/plugins（9） | 全部 9 个 invoke 的命令名+参数键断言、install/manifest/storage 值/readPluginFile 载荷透传、事件名与 API 版本契约常量 | 9 | PASS |
| types/plugin（4） | UI_PLUGIN_API_VERSION==Rust PLUGIN_API_VERSION(2)、Permission 四串精确集合、serde 形态 PluginSummary（可选字段省略）与 PluginManifest（showIn/tokensCss/backend=null）fixture | 4 | PASS |

#### 覆盖率结论（vitest 4.1.10 + @vitest/coverage-v8，include 仅四个被测文件实测）

| 文件 | %Stmts | %Branch | %Funcs | %Lines |
|------|--------|---------|--------|--------|
| src/commands/plugins.ts | 100 | 100 | 100 | 100 |
| src/stores/pluginStore.ts | 96.77 | 83.33 | 100 | 100 |
| src/stores/workspaceTabsStore.ts | 100 | 100 | 100 | 100 |
| src/types/plugin.ts | 100 | 100 | 100 | 100 |
| **合计** | **98.82** | **96.87** | **100** | **100** |

**四文件 Lines/Stmts/Funcs 均 ≥96%，Branch 合计 96.87%，≥80% 目标达标**（且高于 vitest.config 既有 stores 门槛 lines/statements 80、functions 75、branches 55）。残余未覆盖：pluginStore.ts L75 一带（`setEnabled` catch 内 refetch 自身再失败的极端分支，v8 行级 remap 显示为部分命中）；四个文件当前无其他生产调用方（F4+ 未开始），上述数字即完整口径。

#### Bug 列表

无功能缺陷（目标套件 0 FAIL）。登记备注 2 条：

| ID | 类型 | 描述 | 重现步骤 | 期望 vs 实际 | 状态 |
|----|------|------|---------|-------------|------|
| NOTE-F3-01 | 备注 | 进度表旧文案「全量 vitest 3 个既有失败」不准确：实为 **4 个失败文件**（ConnectionNavigatorTree 文件级收集失败 + RunHistoryDrawer/WidgetEditorDrawer/ObjectBrowser 各 1 失败用例 = 3 个失败用例分布在 4 个文件） | 干净树（git stash -u）复跑 `npx vitest run` 对比 | 表述偏差，失败本身与本功能无关已复核 | 已在本表 F3 行修正 |
| NOTE-F3-02 | 备注 | pluginStore 在模块 import 时即执行 `ensurePluginsChangedListener()`（模块级副作用）；非 Tauri 环境 listen reject 后靠 catch 复位守卫支持后续重试 | 测试新增 retry 用例覆盖该路径 | PRD §4.1 只要求"监听 plugins:changed 刷新"，实现为超集且行为正确 | 备注 |

### F4（主窗口集成，commit 62141434）

- 测试 agent 会话，2026-08-22。规格依据：ui-plugins.md §4.1–§4.4；ui-plugins-implementation.md §4.2/§4.3。
- 执行命令：
  - 开发自带套件：`npx vitest run src/windows/workspace src/windows/plugins` → **37 passed / 0 failed**
  - 补充后同口径 → **59 passed / 0 failed**（37 开发 + 22 新增）
  - 全量回归：`npx vitest run` → **1631 passed / 3 failed tests（4 failed files）/ 217 文件**；失败文件与基线（stash 前对照）完全一致——ConnectionNavigatorTree（文件级收集失败）+ RunHistoryDrawer/WidgetEditorDrawer/ObjectBrowser 各 1 例，均为分支既有，不计回归
  - 类型检查：`npx tsc --noEmit` stash 前后 diff 为空 → 16 条错误全为既有，无新增
  - 覆盖率：`npx vitest run --coverage.enabled --coverage.reporter=text --coverage.include='src/windows/workspace/**' --coverage.include='src/windows/plugins/**'`（运行触及被测模块的全部 11 个测试文件；注：全量 suite 下 v8 覆盖率报告被 ConnectionNavigatorTree 收集崩溃静默吞掉，见 NOTE-F4-03）

#### 规格复核结论（逐条对照 PRD §4.1–§4.4 / 实现方案 §4.2–§4.3）

| 规格条目 | 结论 |
|----------|------|
| §4.1 aside 顺序 = 连接/工作流/数据看板/**Workspace**/插件 + 底部设置 | ✅ DOM 先后顺序断言（新测试 T-01）；ConnectionPage.tsx:907-956 |
| §4.1 图标隐喻 LayoutGrid(网格)/Puzzle(拼图) + i18n `nav.workspacePages`/`nav.plugins` + testId `workspace-nav-workspace-pages`/`workspace-nav-plugins` | ✅ iconIds.ts/hostLucideMap.ts 同步增量两枚；ThemedIcon fallback 链完整 |
| §4.1 更新/异常角标提示 | P1 范围未实现，符合排期（不记缺陷） |
| §4.2/§4.4 无 Tab → 默认卡片视图；开 Tab 后 Tab 条出现；二者互斥 | ✅ WorkspaceView.tsx:62-75；集成测试以**真实 workspaceTabsStore**驱动开/关切换验证互斥（T-08…T-11） |
| §4.2/§4.4 两套 Tab 体系独立、模式往返状态各自保持 | ✅ workspaceMode 与连接 tabs 为分离 state；round-trip 测试断言切回连接模式无重连（T-05）；settings 往返恢复 workspace 模式（T-06） |
| §4.2 左侧导航栏 180px、已启用插件页列表（图标+名称+描述）、hover 高亮、点击开 Tab | ✅ WorkspaceNavigator `w-[180px]`；禁用插件不入列 |
| §4.2/§4.4 停用/卸载时自动关闭对应 Tab | ⚠️ **仅管理页内操作符合**；跨窗口/外部路径触发的 `plugins:changed` 只刷新 pluginStore，无人调用 `closeByPlugin` → **BUG-F4-01** |
| §4.2/§4.4 同一插件页多开（每 Tab 一个独立 iframe） | ❌ tab key=`{pluginId}:{pageId}` 且 open 幂等 → 同页仅单实例，重复点击只聚焦 → **BUG-F4-02**（PRD §4.4 表格自身定义该 key，与多开条款自相矛盾，实现取 key 设计，需产品拍板） |
| §4.3 页面壳 `sandbox="allow-scripts"` | ✅ 属性逐字符断言（开发用例 + T-09） |
| §4.3 懒挂载（首次激活才建 iframe）/非激活 CSS 隐藏保留实例/关闭即卸载/10s 超时重载 | ✅ PluginPageShell 7 例（hidden+aria-hidden+iframe 存活、10_000ms watchdog→reload 按钮重建 frame）+ T-11/T-12 |
| §4.3 深链 `plugins:open-page` 校验链 payload→插件存在→enabled→page∈pages | ✅ WorkspaceView.tsx:40-47 + 正反用例（未知/停用/缺 pageId/未知 page/空 payload 全忽略） |
| §4.3 管理 chips 三类（全部/Workspace/主题）+搜索 | ✅ 计数徽标齐全 |
| §4.3 启停调 setEnabled（停用联动关 Tab）/卸载二次确认/取消保留 | ✅ useConfirmDialog 确认/取消两分支均有测试 |
| §4.3 主题卡不提供应用动作 + 「在 设置→外观 中切换」提示 | ✅ themeBadge/themeHint，无 [打开] 按钮 |
| §4.3 apiVersion 不匹配置灰 + 版本提示 + 禁操作 | ✅ opacity-60/warning badge/toggle disabled/[打开]隐藏 |
| §4.3 安装校验失败错误可复制 | ✅ CopyableError：selectable 文本 + role=alert + copy 按钮 → clipboard.writeText（T-17/T-18） |
| §4.3 安装流程含「名称/版本/权限清单确认」步骤 | ❌ 选路径确认后直接写入，无预览确认步 → **BUG-F4-03** |
| §4.3 内容主体默认过滤器 = Workspace；「全部」混合展示并分组 | ❌ 默认 'all' 平铺、无分组 → **BUG-F4-04** |
| §4.4 Tab 标题 = 插件名 | ⚠️ 实现为 `page.title \|\| plugin.name`（页面标题优先），语义可辩护 → NOTE-F4-01 |
| i18n 仅 en.ts + zh-CN.ts 变更且键集一致 | ✅ git numstat 仅两文件（41/38 行）；键集 parity 1503==1503 |

#### 新增测试文件（4 个，22 例全 PASS，零功能代码改动）

`src/windows/connection/__tests__/ConnectionPage.pluginsNav.test.tsx`（7）、`src/windows/workspace/__tests__/WorkspaceIntegration.test.tsx`（6，真实子组件+真实 store，仅 mock pluginStore/i18n/tauri event/manifest IPC）、`src/windows/plugins/__tests__/InstallPluginDialog.test.tsx`（6）、`src/windows/workspace/__tests__/PluginIcon.test.tsx`（3）。

#### 用例清单（新增 22 例）

| 编号 | 场景 | 结论 |
|------|------|------|
| T-01 | ConnectionPage 渲染两个新 aside 按钮，DOM 顺序位于 dashboard 之后、settings 之前 | PASS |
| T-02 | 点击 Workspace 按钮 → WorkspaceView 渲染 + 标题栏 nav.workspacePages + 连接树卸载 | PASS |
| T-03 | 点击 插件 按钮 → PluginManagementPage 渲染 + 标题栏 nav.plugins | PASS |
| T-04 | 跨模式快捷回调：workspace 空态→管理页→[返回 workspace] 双向切换 | PASS |
| T-05 | 连接 Tab 在 workspace/plugins 模式往返后保持：无二次 connect | PASS |
| T-06 | 从 workspace 模式进 Settings 再返回，workspace 模式恢复（settingsReturnModeRef 兼容性） | PASS |
| T-07 | 两按钮高亮互斥（active 态 bg-accent/20 单一持有） | PASS |
| T-08 | 无 Tab 时仅默认卡片视图，TabBar 不渲染 | PASS |
| T-09 | 导航项点击 → TabBar 出现 + 卡片视图消失 + sandbox="allow-scripts" iframe 挂载（src=datazen://…?v=） | PASS |
| T-10 | 默认卡片点击同样开 Tab（真实 openPluginPage 链路） | PASS |
| T-11 | 关闭最后一个 Tab：TabBar 消失、卡片视图恢复、iframe 卸载、导航项可重开 | PASS |
| T-12 | 双 Tab 场景非激活壳 hidden+aria-hidden 保留实例（双 iframe 同挂载） | PASS |
| T-13 | 畸形深链 payload（null/undefined/空对象/空串/缺 pageId）全部忽略不开 Tab | PASS |
| T-14 | 安装对话框确认键随路径空白禁用/启用 | PASS |
| T-15 | 空白路径永不触发后端 installPluginFromPath | PASS |
| T-16 | trim 后路径安装 + store 刷新 + 关窗 + onInstalled 回传 | PASS |
| T-17 | 失败错误可复制契约：selectable 类名 + role=alert + copy 按钮 + 对话框保持打开 + 可重试 | PASS |
| T-18 | copy 按钮将原始错误文本写入剪贴板 | PASS |
| T-19 | 再次修改路径清除旧错误 | PASS |
| T-20 | PluginIcon 无图标回退 puzzle glyph | PASS |
| T-21 | datazen:// 图标 URL 规范化（剥 `./` 前缀） | PASS |
| T-22 | 图标加载失败→puzzle 回退；换 icon 复原；再败再回退 | PASS |

#### 覆盖率结论（vitest 4.1.10 + @vitest/coverage-v8，include 限 F4 九个源文件，实测）

| 文件 | %Stmts | %Branch | %Funcs | %Lines | 主要未覆盖 |
|------|--------|---------|--------|--------|------------|
| windows/plugins/InstallPluginDialog.tsx | 95.23 | 83.33 | 100 | 100 | （语句缺口为 v8 行映射伪影，行覆盖 100%） |
| windows/plugins/PluginManagementPage.tsx | 96.72 | 86.36 | 100 | 98.11 | L112 handleRemove 的 catch 错误臂 |
| windows/workspace/PluginIcon.tsx | 100 | 100 | 100 | 100 | — |
| windows/workspace/PluginPageShell.tsx | 96.66 | 97.82 | 89.47 | 96.07 | L176-177 iframe onError 臂（jsdom 不产生真实加载错误） |
| windows/workspace/WorkspaceDefaultCards.tsx | 100 | 87.5 | 100 | 100 | L29 未传 onOpenPlugins 的空态臂 |
| windows/workspace/WorkspaceNavigator.tsx | 100 | 90 | 100 | 100 | L30（同上） |
| windows/workspace/WorkspaceTabBar.tsx | 84.21 | 75 | 88.88 | 84.61 | L29-31 onWheel 横向滚轮滚动 |
| windows/workspace/WorkspaceView.tsx | 96.66 | 93.75 | 100 | 100 | L49 卸载竞态 disposed 臂 |
| windows/workspace/workspacePages.ts | 90.47 | 75 | 100 | 100 | L38/78/80 title 回退与 openPluginPage 失败臂 |
| **合计** | **95.27** | **89.01** | **96.62** | **97.42** | |

**行覆盖合计 97.42%，最低单文件 WorkspaceTabBar 84.61%，九文件全部 ≥80% 达标**。残余缺口集中在滚轮滚动、onError 竞态等 jsdom 无法自然触发或纯 UI 分支。

#### Bug 列表

无阻断级缺陷（目标套件 0 FAIL）；登记低危缺陷/偏差 4 项：

| ID | 类型 | 描述 | 重现步骤 | 期望 vs 实际 | 状态 |
|----|------|------|---------|-------------|------|
| BUG-F4-01 | 低危缺陷 | 插件停用联动不完整：跨窗口/外部触发的 `plugins:changed` 只刷新 pluginStore，不关闭对应 Tab | ① 打开已启用插件的页面 Tab；② 经另一窗口（或不经管理页的任意路径）执行 set_plugin_enabled(false) 使后端 emit plugins:changed；③ 观察原窗口 | 规格 §4.3 禁用联动/§4.4：停用即 closeByPlugin 关 Tab；实际仅管理页内 toggle/uninstall 走 closeByPlugin（PluginManagementPage.tsx:93,110），pluginStore 订阅只 refetch（pluginStore.ts:77-79），残留可激活的僵尸 Tab（后续资源请求将被协议层 403）。建议 F6 桥接阶段在 store/shell 层统一订阅处理 | 已修复（ca2218bc，WorkspaceView effect diff + 门闸单测）|
| BUG-F4-02 | 规格偏差 | 「同一插件页多开」不可用：同页重复点击仅聚焦既有 Tab | ① Workspace 点击同一导航项两次 | 规格 §4.2/§4.4 字面允许每 Tab 一个独立 iframe 多开；实际 key=`{pluginId}:{pageId}` + open 幂等（workspaceTabsStore.ts:36-47）。注意 PRD §4.4 表格自身规定 key={pluginId}:{pageId}，与多开条款矛盾——需产品拍板取哪一条 | 已修复（产品决议：单实例）|
| BUG-F4-03 | 规格偏差（低） | 安装流程缺「名称/版本/权限清单确认」中间步骤 | ① 管理页[安装插件…]；② 输入合法 zip 路径；③ 点 Install | 规格 §4.3：校验→展示名称/版本/权限清单确认→写入；实际确认即直接调 install_plugin_from_path 写入并刷新（InstallPluginDialog.handleInstall），权限信息用户安装前不可见 | 已修复（ca2218bc，inspect_plugin_package 预检 + 两步流转单测）|
| BUG-F4-04 | 规格偏差（低） | 管理页默认过滤器为「全部」而非「Workspace」，且「全部」平铺不分组 | ① 装有 workspace 插件 + 主题插件时打开管理页 | 规格 §4.3：内容主体默认过滤 Workspace、「全部」混合展示并分组；实际 useState 初值 'all'（PluginManagementPage.tsx:57）且无分组逻辑 | 已修复（ca2218bc，默认 'workspace' + allGroups 分组单测）|
| NOTE-F4-01 | 备注 | Tab 标题为 `page.title || plugin.name`（页面贡献标题优先于插件名） | 打开带自定义 page.title 的插件 Tab | §4.4 字面「标题=插件名」；页面标题区分度更高，语义可辩护 | 备注 |
| NOTE-F4-02 | 备注 | 卡片图标为名称首字母方块而非 manifest 图标；「更新/卸载菜单」实为单独卸载按钮、更新能力未做 | 查看管理页卡片 | PluginSummary 本就不含 icon 字段（F3 契约）；更新属 P1 排期 | 备注 |
| NOTE-F4-03 | 备注 | 全量 suite 带 --coverage 运行时覆盖率报告被静默吞掉（复现 2 次）；按目录 include 的子集运行正常 | `npx vitest run --coverage...`（全量 vs 子集对照） | 疑与 ConnectionNavigatorTree 文件级收集崩溃干扰 v8 合并相关；分支既有问题，建议 CI 固定子集口径或先修复该既有失败 | 备注 |

E2E 说明：按本任务约定，AGENTS.md「Host UI 变更须同 PR 补 E2E」的硬规则在 F9 统一补齐（e2e/specs/plugins.spec.ts journeys），本次仅单测层面。

### F4 修复验证（commit ca2218bc，2026-08-22 验证 agent 会话）

只验不改；复核 diff + 实测，四项全部通过：

| ID | 验证方法 | 结果 |
|----|---------|------|
| BUG-F4-01 | WorkspaceView.tsx:44-54 effect：`pluginsLoaded` 门闸 + diff（插件缺失或 enabled=false → `closeByPlugin`）。单测断言真实覆盖行为：WorkspaceView.test.tsx「closes tabs of plugins that were disabled or removed by an external refresh (BUG-F4-01)」（停用→关 1 次、卸载→再关、enabled 插件永不触碰）+「does not diff-close tabs before the plugin store has loaded」（loaded=false 初载空列表不误关既有 Tab）；集成侧 WorkspaceIntegration.test.tsx:189 以真实 store 驱动外部刷新场景 | ✅ 通过 |
| BUG-F4-03 | Rust `inspect_plugin_package`（install.rs）：一次性 `.datazen-inspect-*` 临时目录跑与真实安装同套规则后清理；三态错误各有断言（not found / apiVersion 校验失败 / zip traversal）且 `count_inspect_dirs()==0`、plugins_dir 无 `acme.demo`；命令注册于 lib.rs:978。前端两步流转：InstallPluginDialog.test.tsx「walks the two-step flow…」（review 步断言 installFromPath **未调用** + 名称/版本/author/权限徽标渲染）、「never installs when cancelled from the review step」（Back/Cancel 均零 install 调用）；IPC 层 inspect_plugin_package_previews_manifest_without_writing 断言 list_plugins 保持为空 | ✅ 通过 |
| BUG-F4-04 | PluginManagementPage.tsx 初值 `'workspace'`；'all' 视图经 `allGroups` 按 Workspace/主题 分组渲染。测试：默认过滤=Workspace（主题插件初始隐藏 + workspace chip 高亮）、「renders the all view grouped into Workspace pages and Themes sections」（双贡献插件仅入 Workspace 组一次）、「hides empty groups in the all view and keeps the flat grid for single-kind filters」 | ✅ 通过 |
| BUG-F4-02 决议落实 | PRD ui-plugins.md v0.5→v0.6：「v0.6 变更记录（评审决议）」存在；§4.2 与 §4.4 多开条款均改为「同一插件页复用同一 Tab（点击已打开项聚焦既有 Tab）；多开留待后续版本评估」；本文件 BUG 跟踪表该 bug 状态为「已修复（产品决议：单实例）」 | ✅ 通过 |

执行命令：
- `cargo test -p datazen --lib plugins` → **108 passed / 0 failed**（含新增 inspect_plugin_package ×4）
- `npx vitest run src/windows/plugins src/windows/workspace` → **9 文件 60 tests 全绿**

⚠️ 合并前备注：ca2218bc 未包含工作区中的配套改动——`src/commands/plugins.ts`（`inspectPluginPackage` IPC 封装）与 en.ts/zh-CN.ts 各 +7 个 i18n 键。缺这三处时已提交的 InstallPluginDialog.tsx 无法通过类型检查、i18n 显示原始 key。主控填写测试 commit 号时须将这三个文件一并纳入提交。

### F6（RPC 桥，commit c77085c8）

- 测试 agent 会话，2026-08-22。规格依据：ui-plugins-implementation.md §3 全部（信封/握手时序/§3.2 API 表/权限映射/限流超时/错误码）与 §4.4。
- 新增测试文件（零功能代码改动，未 commit）：
  - `src/lib/__tests__/uiPluginBridge.security.test.ts`（28 例）：凭据白名单、栈/审计非泄露、畸形 payload、大小写变体、原型链键、原型污染遏制、跨 iframe source 隔离、detach 静默、限流配额生命周期、手动快照
  - `src/windows/workspace/__tests__/PluginPageShell.bridge.test.tsx`（5 例）：桥接线（attachBridge 参数、theme-pack-changed 推送、MutationObserver class 变更推送、卸载 detach、reload 重挂载重连）

#### 用例清单

既有开发单测（31 例，全部 PASS）：

| 组 | 场景 | 数量 | 结论 |
|----|------|------|------|
| uiPluginBridge.test | plugin.ready→host.ready 握手（apiVersion/locale/dark/tokens）、theme.apply 手动推送 | 2 | PASS |
| uiPluginBridge.test | 权限门 deny-by-default（context/command/storage 各 API 缺权限拒 + 全授权放行 + i18n E_NOT_IMPLEMENTED） | 6 | PASS |
| uiPluginBridge.test | 信封语义（reqId 回显/乱序完成、unknown type E_NOT_FOUND、异源消息忽略、detach 停答） | 4 | PASS |
| uiPluginBridge.test | 限流超时（第 21 并发 E_RATE_LIMIT+恢复、ui.notify 5s 冷却、30s E_TIMEOUT） | 3 | PASS |
| uiPluginBridge.test | context 白名单（store 路径/IPC 兜底路径/getActiveConnection 三态） | 3 | PASS |
| uiPluginBridge.test | command.invoke 错误映射（E_NOT_FOUND/E_BAD_REQUEST） | 2 | PASS |
| themeTokens.test | themes.css token 定义存在性 ×7、THEME_TOKENS↔themes.css 双向契约 ×2、buildThemeSnapshot dark/v/tokens 键集 ×2 | 11 | PASS |

新增测试单测（33 例，全部 PASS）：

| 编号组 | 场景 | 预期 | 实际 | 结论 |
|--------|------|------|------|------|
| SEC-01–04 | 凭据白名单 | IPC 兜底路径 getConnections 与 store 路径 getActiveConnection 输出**恰好 3 个 own keys**（构造式白名单证明，非 delete 式）；含 host/port/username/password/sshTunnel.password/privateKeyPath/passphrase/jump/options.tlsCa 的泄漏型 fixture 全量 marker 扫描零命中；INTERNAL 错误仅 message（≤500 截断），error.stack 标记不出现；审计日志带 `[ui-plugin:{id}]` 且不含 args 内容 | 符合 | PASS |
| SEC-05–12 | 权限门 vs 畸形路由 | 大小写/前导空格变体 → E_NOT_FOUND；`__proto__` 等 5 个原型链键 → 拒绝且 handler 不可达（当前回 E_PERMISSION，偏差见 BUG-F6-01）；target 非 `host`/缺失/ch 尾随空格 ×4 信封忽略；无 reqId 不应答 | 符合 | PASS |
| SEC-13–17 | command.invoke 畸形 payload | payload 整体缺失、configId 缺失/数字/空串 → E_BAD_REQUEST；args 为 string/number/boolean → E_BAD_REQUEST；args:null → input `{}`（钉住良性现行为）；数组 args 原样透传 | 符合 | PASS |
| SEC-18–20 | 原型污染遏制 | JSON.parse 构造 own `__proto__`/`constructor.prototype` 键的 args 与 storage value 原样过桥进 IPC；`Object.prototype` 往返后零污染；污染 reqId 仅作字面回显，信封 ok 字段不受影响 | 符合 | PASS |
| SEC-21–22 | storage/notify 校验 | storage.get/set/remove 空/缺/数字 key ×9 → E_BAD_REQUEST 且 IPC 零调用；ui.notify 缺 title/空 title/body 数字 ×3 → E_BAD_REQUEST 且 notification 零调用 | 符合 | PASS |
| SEC-23–24 | source 隔离与静默 | 双 iframe 双桥：B 帧消息仅 B 桥应答（pluginId 命名空间正确），A 桥全程静默；detach 后 plugin.ready 无 host.ready、storage.get 零 IPC 零应答 | 符合 | PASS |
| SEC-25–27 | 限流配额生命周期 | cap=2：占满→第 3 个 E_RATE_LIMIT；**完成 1 个恰好释放 1 槽**（h4 被受理、h5 再度受限）；超时同样释放容量（E_TIMEOUT 后新请求受理）；30 连发权限拒绝请求零 E_RATE_LIMIT（拒绝类响应不耗配额） | 符合 | PASS |
| SEC-28 | 手动主题快照 | pushThemeSnapshot 每次调用实时反映 dark 翻转，v=THEME_SNAPSHOT_VERSION(=2)，tokens 键集 == THEME_TOKENS | 符合 | PASS |
| SH-01–05 | PluginPageShell 桥接线 | ready 后恰 attach 1 次（iframe 元素/pluginId/manifest permissions/locale 正确）；`datazen:theme-pack-changed` 触发 pushThemeSnapshot；documentElement class 增/删触发推送而 lang 属性变更不触发；unmount detach 后事件不再推送；watchdog reload 重挂载后旧桥 detach+新桥 attach（各一次） | 符合 | PASS |

#### 安全专项复核结论（重点项）

| 复核项 | 结论 | 依据 |
|--------|------|------|
| 凭据泄露 | **通过** | toPublicConnection（uiPluginBridge.ts:92-98）为**构造式白名单**（`return {id,name,dbType}` 字面量），非 spread/delete 式清洗；store 缓存路径与 IPC 兜底路径、getConnections/getActiveConnection 两 API 全部经过它；SEC-01/02 以 own-keys 计数 + 密钥 marker 全文扫描双重验证 |
| 权限绕过 | **通过**（1 低危协议偏差 → BUG-F6-01） | 缺 configId/args 非对象均 E_BAD_REQUEST 先于 IPC；type 大小写敏感精确匹配无法旁路；原型链键名虽绕过 E_NOT_FOUND 门但被第二道权限门拒绝，handler 物理不可达、配额零消耗 |
| source 校验 | **通过** | onMessage 首行 `event.source !== iframe.contentWindow` 即弃；双桥交叉隔离实测；post-detach 含握手在内全静默；targetOrigin '*' 为 PRD §4.3 明示立场（opaque origin + source 校验兜底） |
| 限流恢复语义 | **通过** | inflight 于 dispatch finally 释放——正常完成、BridgeApiError、INTERNAL、E_TIMEOUT 四条路径等价释放；SEC-25/26/27 分别验证完成后/超时后恢复与拒绝类零消耗 |
| 其他观察（不计 bug） | — | ① `E_PLUGIN_DISABLED` 已定义但前端不可达：停用联动（BUG-F4-01 修复）先关 Tab→桥已 detach，属预留码；② ui.notify 在 invoke 前写 lastNotifyAt，通知失败也消耗 5s 冷却槽（规格未定义重试语义）；③ 审计日志走 console.info（webview console）而非 Rust tracing 链，M4「日志脱敏核查」时应确认持久化预期 |

#### 覆盖率（npx vitest run --coverage，scope 至两目标文件）

| 文件 | Stmts | Branch | Funcs | Lines | 未覆盖 |
|------|-------|--------|-------|-------|--------|
| src/lib/uiPluginBridge.ts | 94.83% | 83.80% | 100% | **99.27%** | 仅 464 行（dispatch switch 的 default 防御分支——API_ROUTES 门已前置拦截 unknown type，实际不可达死分支） |
| src/lib/themeTokens.ts | 100% | 100% | 100% | **100%** | — |

两文件 Lines 均 ≥80% 达标。

#### 执行命令与结果

- `npx vitest run src/lib/__tests__/uiPluginBridge.security.test.ts src/windows/workspace/__tests__/PluginPageShell.bridge.test.tsx` → **33/33 PASS**
- `npx vitest run src/lib/__tests__/uiPluginBridge.test.ts src/lib/__tests__/uiPluginBridge.security.test.ts src/lib/__tests__/themeTokens.test.ts --coverage --coverage.include=…` → **59/59 PASS**
- `npx vitest run`（全量）→ 220 文件：216 passed / **4 failed（全部为基线既有**：RunHistoryDrawer、WidgetEditorDrawer、ConnectionNavigatorTree[文件级]、ObjectBrowser；测试前后两次全量运行失败集合一致，**零新增失败**）；1696 tests passed
- `npx tsc --noEmit` → 报错仅位于 7 个 F6 无关存量文件（query.ts/ObjectFilterDialog.tsx/ConnectionPage.tsx/ContentView.tsx/ProcessListView.tsx/SavedTasksBanner.tsx/DataTransferWindow.tsx）；**F6 触碰文件（uiPluginBridge.ts/themeTokens.ts/PluginPageShell.tsx）及新增测试文件零错误**

#### Bug

| ID | 严重度 | 状态 |
|----|--------|------|
| BUG-F6-01 | 低危（协议卫生，无安全影响，不阻断） | 新建 |

## 回归测试

- [ ] 全量回归（cargo test -p datazen --lib + npx vitest run）
- [ ] 文档更新（架构文档 docs/architecture/backend/plugins.md、AGENTS.md 精简增补）
- [ ] 合并 main
