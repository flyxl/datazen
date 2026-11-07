# 语言包安装（External Language Pack Install）设计档

- 状态：已实施（分支 `feature/language-pack-install`）
- 需求：设置页提供「安装外部语言包」；安装后新语言出现在语言下拉框（裁决 Q2）
- 用户裁决（2026-11-07）：Q1 包格式 = ZIP + 按现有语言文件的域拆分结构；Q2 安装即入下拉框；Q5 无内容上限；Q6 卸载在用包回退 en；方案文档落库（Q4）；表达去黑话（Q3，正文照做）

## 1. 功能行为（用户视角）

**安装**：设置 → 通用 → 语言包卡片 → 「安装语言包」→ 原生文件框选 `.zip` → 预览（语言名/代码/版本/作者/译文条数/覆盖域）→ 确认落盘。**装完语言下拉框即出现该语言**（经既有的 `registerLocale` 扩展语言通道 + `language-packs:changed` 跨窗广播，无需重启）。

**翻译兜底**：包可以只覆盖部分键——查找链 `registry[locale] ?? registry['en'] ?? key` 是共享引擎既有语义（`packages/ui/src/i18n.ts`），不是本功能新规则。

**卸载**：列表页移除（确认框）；**正在使用的包先切回 English 再移除**（en 永远内置）。重启有效：包落 `{appData}/language-packs/`，启动扫描自注册；坏包跳过 + warn 不炸 boot（与 wapp 扫描同语义）。

## 2. 包格式（Q1）

```
datazen-locale-ja.zip
├── manifest.json   # packVersion(=1)/code/label/version/author?/description?
├── core.json       # 13 个域文件，与 locales/domains.ts 权威拆分一一对应
├── connection.json
├── query.json
├── …（schema/settings/chart/backup/ai/onboarding/sync/workflows/dashboard/mcp）
└── mcp.json
```

域文件 = 扁平 `key → message` JSON（与源 `.ts` 字典同键同值；懒域在外装包里是普通文件——一次性整体注册，不参与 bundle 代码分割）。`packVersion` 顶层字段留前向演进（≠1 整包拒绝，宁拒不猜）。

## 3. 分层与落点

| 层 | 文件 | 职责 |
|---|---|---|
| 共享内核 | `crates/datazen/src/language_packs/{mod,manifest,install,manager,summary}.rs`（feature `language_packs`，desktop 显式开） | 校验规则单家、ZIP 两遍安检解包、staging 原子安装、扫描/列表/读译文/卸载；存储根参数化（镜像 wapps「内核不读 ambient 路径」） |
| 桌面宿主 | `crates/datazen-desktop/src/commands/language_packs.rs` + `bootstrap/app_state.rs`（AppState 字段 + 扫描）+ `bootstrap/run.rs`（命令注册） | 薄壳：文件对话框、pick 会话（≤8）、webdriver-only override 门、`spawn_blocking`、事件广播 |
| 前端 | `commands/languagePacks.ts` / `stores/languagePackStore.ts` / `lib/languagePackRuntime.ts` / `windows/settings/{LanguagePacksCard,InstallLanguagePackDialog}.tsx` | 两步确认流；`registerLocale` 通道注册；语言下拉合并已装包 |
| 基础库 | `packages/ui/src/i18n.ts` 增 `unregisterTranslations`（约 10 行） | 修两个既有缺口：卸载幽灵字典、同 code 重装新旧键合并污染 |
| 伴生工具 | `scripts/export-locale-pack.mjs` | 把仓库 8 个源码态语言（de/es/fr/ja/ko/pt-BR/ru/zh-TW）导出为可装包（内嵌极简 store-zip writer，无依赖） |

## 4. IPC 面（camelCase 线契约）

| 命令 | 应答 | 说明 |
|---|---|---|
| `list_language_packs` | `LanguagePackSummary[]`（code/label/version/author?/messageCount/fileName） | 元数据列表（不含译文） |
| `read_language_pack_messages` | `{ code, label, messages }` | label + 全量译文一次拿全（`registerLocale` 所需三件）；未知 code → NotFound |
| `inspect_language_pack_file_with_dialog` | `{ pickToken, packageLabel, manifest, messageCount, domains[] } \| null` | 选文件 + 只校验不落盘；null = 取消 |
| `install_language_pack` | `LanguagePackSummary` | pickToken/overridePath 解析后落盘 + emit |
| `remove_language_pack` | 空 | 删目录 + 内存表移除 + emit |

事件 `language-packs:changed`（镜像 `wapps:changed` 的跨窗刷新）。错误桶：NotFound（not found 文案）/ Validation（校验文案原样透传）/ Internal。pick 会话 ≤8、`resolve_override_path` webdriver 门、`spawn_blocking` 三条纪律沿 wapp 宿主壳形状。

## 5. 校验规则（规则单家 `manifest.rs`，安装与扫描共用）

**manifest**：未知字段拒（`deny_unknown_fields`）；`packVersion == 1`；`code` 匹配 `^[a-z]{2,3}(-[A-Za-z0-9]{2,8}){0,2}$` 且**非内置**（`en`/`zh-CN` 冲突拒装——内置永远赢）；`label` 非空无控制符；`version` 宽松 semver；author/description 可选、无控制符。

**消息**：键 `^[A-Za-z0-9_][A-Za-z0-9_.:-]*$`（首字符字母数字下划线，其余允许点冒号横线下划线）；**拒 `__proto__`/`constructor`/`prototype` 与一切 `__` 前缀键**——前端注册表用 `Object.assign` 合并，这类键触发原型污染，Rust 校验侧是唯一可靠闸门。值：字符串、允许 `\n`、拒其余 C0 控制符与 DEL。

**实施期发现与裁决（两处，均以仓库真实数据为准）**：
1. **空串值合法**：en 源字典自带空值键（`pagination.pageOf: ''`、`valueSearch.status.idle: ''`），且运行时契约明言「An empty string is a real translation, not a miss」——撤销 v2 的空值拒绝，只保留控制符拒绝。
2. **跨域键冲突合法、确定性合并**：en 源字典存在跨域重复键（`common.aiNotConfigured` 同时在 ai 与 core 域）；运行时 `Object.assign`「后注册者胜」、内置全量合并按域文件字母序展开。包读取沿同一裁决：**域按字母序合并、后者胜**（安装与内置对同一份源数据产出同一个生效值）。v2 的跨域冲突拒绝撤销。

**ZIP 构造**（两遍安检，fail-closed）：目录条目拒；根层白名单文件名拒多余（manifest + 13 域）；嵌套路径拒；隐藏文件拒；符号链接拒；路径穿越/绝对路径拒（`datazen-infra::path_safety`）；单条目与整包压缩比 >100 拒（ZIP 炸弹）；写盘读取器强制实际字节 ≤ 声明值。**无内容数量上限**（裁决 Q5：键数、单条译文长度、包体积均不设限；条目白名单结构上封死海量条目）。

## 6. 安装流程（镜像 wapp 安装器）

选文件（对话框/override）→ 只检不装（物化进系统临时目录跑同一套规则）→ 用户确认 → 解包 `{packs_dir}/.staging-{uuid}/` → 全量校验 → 旧装备份 `{code}.old.bak` → 原子 rename 转正 → 删备份。失败零残留（staging 清理断言钉在测试里）。

## 7. 前端注册与切换时序

- `ensureLanguagePackMessages(locale)`：内置 locale 直通；外装 code → `read_language_pack_messages` → `registerLocale(code, label, messages)`。未安装（NotFound）→ 引擎兜底链接管。
- `startLocaleSync`（唯一 `setLocale` 驱动点，不变式保持）：**内置语言同步切换（历史契约——settingsStore.language 变更后的同步读取/渲染依赖它，`retest-round1-fixes.tester` 钉住）**；包语言先 ensure 再 setLocale（异步微任务）。乱序防护 = 目标 locale 记账、最后者胜（守卫两侧都做 `?? 'en'` 归一化）。启动 seed 同理（持久化语言为外装包时首帧短暂 en，与主题包异步生效同等待遇）。
- 卸载在用包：前端先 `updateSettings({ language: 'en' })`（settingsStore → localeSync → setLocale 切走）→ 再调 remove 命令 → `unregisterTranslations` + `unregisterLocale`。
- 语言下拉选项 = 内置 + 已装包（store 元数据，label 不需要字典）+ 其余扩展 locale（去重）。

## 8. 边界与已知限制（如实交付面）

1. 外装语言下原生菜单/托盘标签回退 en（`resources/menu-labels.json` 编译期内嵌的既有兜底链）；将来可通过包内菜单词条扩展。
2. 系统语言自动推导（`i18n_locale.rs`）只认内置两语言。
3. 持久化语言为外装包时首帧短暂 en 兜底。
4. 无启用/停用开关：装即可选、卸即移除。
5. 列表显示译文条数，不显示覆盖率百分比。
6. server/web 宿主不在本刀（web 未落地；将来如需包服务独立排刀）。

## 9. 测试

- **共享层单测**（44 钉）：manifest/键值校验矩阵（含原型污染家族）；安装全链（原子性、同 code 重装覆盖零残留、inspect 不触 packs_dir、失败零残留）；真实数据语义（空串值合法、跨域重复键 last-wins）；manager（扫描跳坏包、目录名与 code 一致性、读译文合并、卸载）；summary camelCase 线契约钉子。
- **宿主命令测试**：impl 函数级（install→list→read→remove 往返、NotFound/Validation 桶映射、失败不登记）。
- **前端 vitest**（27 用例）：localeSync 先注册后切换次序 + last-writer-wins（慢读不回灌）；runtime 注册缓存与未装 no-op；store fetch/remove 错误面；卡片（在用徽章/移除在用包先切 en 的次序断言/错误内联）；对话框两步流各状态。
- **E2E**（`e2e/specs/settings-language-pack.ts`，webdriver overridePath 注入夹具）：装 → 下拉出现日本語 → 切换 → 断言译文 → 移除在用包 → 回退 English + 持久化 `language: 'en'` + 下拉项消失。
- **导出脚本 × 安装器互检**：`export-locale-pack.mjs ja` 产出的真实包（3078 条/13 域）过安装器全链（临时验证，未留库——路径依赖沙箱）。

## 10. 维护注记

- 导出真包：`node scripts/export-locale-pack.mjs <code> [outDir]`（8 个源语言即刻可分发）。
- 新增域：同步改 `LANGUAGE_PACK_DOMAINS`（manifest.rs）、`DOMAINS`（export-locale-pack.mjs）、`domains.ts`（前端权威）三处——前两处以第三处为准。
- 包格式的 v2 演进：升 `PACK_VERSION` 并在 `manifest.rs` 加迁移分支；未知版本保持整包拒绝。
