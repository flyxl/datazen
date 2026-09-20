# PRD: Redis 操作台优化（Redis Console UX Overhaul）

> Feature ID: `REDIS_CONSOLE_UX` · Version: `1.0.0` · Status: Draft
> 附件参考截图：① redis-cli 风格键详情页（解码按钮组 + 视图按钮组 + 大小徽章）② Console 命令补全弹窗 ③ Console 键名候选列表

## 1. 背景与动机

DataZen 的 Redis 深度能力已集中在 `packages/drivers/redis`（驱动 crate + 前端 UI），宿主仅为薄 Tab 壳。当前操作台已具备：SCAN 游标分页键列表、前缀 folder 分组（Flat/Tree）、类型编辑器和 Console/Monitor/PubSub。但对照成熟 Redis 管理工具（RedisInsight / redis-desktop-manager / 截图参考产品）仍存在明显差距：

1. **二进制值不可用**：后端值管道是文本域（`GET` → `Option<String>`、preview 走 `String::from_utf8_lossy`），非 UTF-8 二进制数据在传输层即被损坏，无法查看、无法解码。
2. **无解码/视图体系**：仅有 STRING 的 gzip/zlib/deflate 浏览器端解压；无 Base64/Msgpack/Pickle/PHP Serialize/Java Serialized 解码，无 UTF-8/ASCII/Binary/JSON/YAML/XML/Hex/Base64 视图模式切换。
3. **db 键数不可见**：连接导航树和工作台侧栏的 `db0..db15` 均不显示键数量，用户需逐个点入才知道哪个库有数据。
4. **层级浏览弱**：Tree 模式只是对"已加载页"做客户端分组，folder 展开不能按需向后端取数，多级命名空间（`app:cache:session:1`）浏览体验差。
5. **搜索维度单一**：匹配模式只搜键名（glob MATCH），无法按值内容或键+值组合搜索；且无"仅无过期"过滤。
6. **大小信息缺失**：详情面板不显示 value 字节数；列表 size 列默认是逻辑长度（STRLEN/LLEN/…）而非字节。
7. **JSON 编辑原始态缺失**：ReJSON 编辑器只有树形路径编辑，无法查看/编辑原始 JSON 文本，无格式化/压缩。
8. **Console 补全弱**：仅有 Tab 触发的内联 ghost 提示（一行小字），无可见候选弹窗、无命令描述/语法/分组信息；key 参数位置无键名候选，且候选依赖已加载页的静态列表而非按需取数。
9. **高危命令无显著提醒**：`redisConsoleDanger.ts` 已实现四级危险分类（safe/write/danger/ultra-danger），但**未被 RedisConsole 接线使用**——输入、执行、结果全程无颜色/徽章/确认提醒；且 Redis 路径不经过后端 sql_guard，前端是唯一门闸。
10. **Redis 无 SafeMode**：SQL Editor 已有全局 SafeMode（Settings 开关 + 工具栏徽章 + 执行前硬阻断写操作），Redis 操作台完全不受其保护，同一"防误写"语义存在功能不对称。

## 2. 目标用户

| 角色 | 使用场景 |
|------|---------|
| 后端开发者 | 排查缓存/会话数据，值常为 gzip/Msgpack/Java 序列化的二进制 blob |
| 运维/DBA | 巡检大 key、过期策略，需要 db 键数概览与"仅无过期"过滤 |
| 数据分析师 | 按值内容检索样本数据，阅读 JSON/YAML/XML 配置值 |
| 测试工程师 | 构造与验证序列化格式（Pickle/PHP/Java）的测试数据 |

## 3. 功能范围（v1.0）

### 3.1 核心功能（Must Have）

| # | 功能 | 描述 |
|---|------|------|
| R1 | **保留现有类型编辑器** | String/Hash/List/Set/Zset/Stream/ReJSON 编辑器全部保留，行为不回归；本需求为增量优化 |
| R2 | **二进制安全传输** | 原始值取数改为 base64 帧（新命令 `get_key_raw`），前端解码后按视图模式呈现；文本值路径保持向后兼容 |
| R3 | **JSON 编辑器三态** | ReJSON 编辑器与 STRING 的 JSON 视图支持「显示原文（raw 文本）/ 格式化（pretty）/ 压缩（minify）」三种显示模式，可在树形视图与文本视图间切换 |
| R4 | **导航树 db 键数** | ConnectionNavigatorTree 与工作台侧栏的 `db{n}` 行显示键数量，如 `db0 (52)`；懒加载 + 手动/进入时刷新 |
| R5 | **按目录层级显示 key** | Tree 模式升级为真正的层级浏览：展开 folder 时按前缀（`MATCH app:cache:*`）向后端按需取数，支持多级命名空间，折叠/刷新状态正确 |
| R6 | **键/值/全部三种搜索模式** | 搜索框升级为模式切换（键 / 值 / 全部）；值搜索受性能护栏约束（见 §5 非功能需求），可取消、有上限 |
| R7 | **value 字节大小显示** | 详情页显示「大小: N B/KB/MB」（STRING 用 STRLEN，聚合类型用元素数 + MEMORY USAGE 字节数）；列表 size 列语义明确化 |
| R8 | **解码（Codec）功能** | 解码按钮组：无 / GZip / Zlib / Deflate / Base64解码 / Msgpack / Pickle / PHP Serialize / Java Serialized；将原始字节解码后再交给视图渲染 |
| R9 | **视图（View）功能** | 视图按钮组：UTF-8 / ASCII / Binary / JSON 视图 / Unicode JSON / YAML / XML / Hex / Base64；对（解码后的）字节流选择呈现方式 |
| R10 | **仅无过期过滤** | 键列表增加「仅无过期」开关（TTL = -1 过滤） |
| R11 | **Console 命令补全弹窗** | 输入命令前缀时弹出候选列表：命令名 + 描述 + 语法行 + 分组标签（string/hash/list…），键盘（↑↓/Tab 接受/Enter 执行/Esc 关闭）与鼠标均可用；替换现有仅 Tab 内联提示 |
| R12 | **Console 键名补全** | 命令输入到 key 参数位置时，弹出当前 db 内以已输入前缀开头的键名候选列表（如 `SET ` → `user:2:profile`…，右侧标注 `key`），按需向后端取数、限量、防抖 |
| R13 | **高危命令显著提醒** | Console 全链路可视化危险等级（复用 `redisConsoleDanger.ts` 四级分类）：输入行实时徽章、补全弹窗条目着色/标注、执行前确认对话框（danger/ultra-danger）、结果行危险命令标红；FLUSHDB/FLUSHALL 仍受既有 `allowFlush` 驱动设置门控叠加 |
| R14 | **Redis SafeMode（复用全局开关）** | 与 SQL Editor 同语义的只读保护：**复用 Settings 中现有 `settings.safeMode` 开关**（不新增独立开关），开启时 Redis 全部写路径（Console 写/危险命令 + 工作台编辑器保存/删除/批量/Flush/发布）执行前硬阻断并提示"SafeMode 已开启"，与 SQL 端 `blockedSafeMode` / 数据网格只读语义一致；工具栏显示 Safe 徽章，运行时读取最新值（不落快照） |

### 3.2 增强功能（Should Have，v1.1）

| 功能 | 描述 |
|------|------|
| 值搜索覆盖聚合类型 | 值搜索 v1.0 仅 STRING（STRLEN 预算 + GETRANGE 截断）；v1.1 评估 HASH field/value、LIST 元素的抽样匹配 |
| 解码状态持久化 | 按 key pattern（如 `app:cache:*`）记忆解码/视图选择 |
| 自动格式探测 | 进入详情页时按 magic bytes 提示"检测到 GZip，点击解码"（现有 `valueLooksCompressed` 扩展） |
| Hex 编辑器 | Binary/Hex 视图下支持字节级编辑写回 |

### 3.3 不在范围内（v1.0）

- 反序列化对象的**编辑写回**（Msgpack/Pickle/Java 解码为只读查看；写回仅支持原文/字节级）
- Lua 脚本、Function、ACL、客户端断连管理等运维面板
- Cluster 拓扑视图改造（现有 cluster pin 保持）
- 值搜索的全文索引/持久化缓存（每次即时 SCAN，不落盘）

## 4. 用户交互流程

### 4.1 键浏览与层级展开

```
连接 Redis → 导航树展开 db0 (52)          ← R4：db 行内联键数
    ↓
进入 Items Tab → 侧栏 db0 (52) 高亮
    ↓
Tree 模式：根层显示一级 folder（app (6)、big (4)…）+ 根级裸键
    ↓ 点击 folder「app」展开
按前缀向后端取该层直接子项（app:cache (4)、app:token (2)…）   ← R5
    ↓ 点击叶子键
右侧详情面板：类型徽章 + TTL + 「大小: 2 B」                    ← R7
    解码: [无][GZip][Zlib]…    视图: [UTF-8][Hex]…             ← R8/R9
    ↓ 编辑保存（保持现有 per-type 编辑器）                      ← R1
```

### 4.2 三模式搜索

```
搜索框输入 "session" + 选择模式 [键|值|全部]                    ← R6
    键   → SCAN MATCH *session*（现状，游标分页）
    值   → 后台受控扫描：SCAN 分批 + STRLEN 预算 + GETRANGE 截断匹配
           进度条 + 「已扫 12k/上限 50k 键」+ 取消按钮
    全部 → 键命中 ∪ 值命中（分组标注命中来源）
可叠加「仅无过期」过滤                                          ← R10
```

### 4.3 JSON 三态

```
ReJSON 键 → 编辑器默认树形视图（现状）
    Tabs: [树形] [原文] [格式化] [压缩]                          ← R3
    原文 = JSON-GET 裸文本 textarea（可编辑、可保存）
    格式化 = pretty-print（2 空格缩进）
    压缩 = minify 单行
STRING 值像 JSON → StringEditor 现有 Format 按钮升级为同款模式组
```

### 4.4 Console 智能补全（R11/R12）

```
db0> SE                    ← 命令位置：前缀匹配
   ┌──────────────────────────────────────┐
   │ SET                                  │
   │ Sets the string value of a key…  string│
   │ SET key value                        │
   │ SETEX …                    …         │  ↑↓ 选择 · Tab 接受 · Enter 执行 · Esc 关闭
   └──────────────────────────────────────┘
db0> SET                   ← 已输入命令、光标在 key 参数位置
   ┌──────────────────────────────────────┐
   │ user:2:profile                    key│
   │ config:app:debug                  key│  当前 db 内前缀匹配键名（懒取数、≤50 条）
   │ …                                    │
   └──────────────────────────────────────┘
```

- 命令候选含 名称/描述/语法/分组徽章；键候选含 键名 + `key` 标注。
- 补全仅作用于第一个 token（命令位）与第二个 token（key 位）；后续参数位不弹（v1.0 边界）。
- 现有 ⌘+Enter 执行、↑↓ 历史、危险命令门控行为不变。

### 4.5 高危提醒与 SafeMode（R13/R14）

```
db0> FLUSHALL                 ← 输入即分类 ultra-danger
  [⚠ ultra-danger 徽章]        ← 输入行右侧实时提示；补全列表中同项红色标注
  ⌘Enter 执行：
  SafeMode 开 → 直接阻断：「SafeMode 已开启，禁止执行写/危险命令」(error dialog，同 SQL 端文案语义)
  SafeMode 关 → danger/ultra-danger 弹确认框（列出命令与影响，需显式确认）
                FLUSHDB/FLUSHALL 额外要求 allowFlush=true（既有驱动设置，门控叠加）
  结果面板：危险命令的 result 行整行着色（danger 橙 / ultra-danger 红）

工具栏：[Safe] 徽章（safeMode=true 时显示，点击跳 Settings，与 SQL 端 QueryEditorSection 同款交互）
```

- 门控范围（v1.0）：Console 命令台 + Items 工作台全部写路径（保存值、HSET/HDEL、LRANGE 写、批量删除/改名/TTL、Flush 按钮、导入/RESTORE、PubSub 发布、编辑器增删行）。理由：SQL 端 SafeMode 不仅 gate 查询编辑器，还强制数据网格只读（`ContentView.tsx:201`），Redis 对齐该语义才是"类似功能"；只读浏览、值解码/视图切换不受影响。FLUSHDB/FLUSHALL 在此之上继续叠加既有 `allowFlush` 门控。

## 5. 非功能需求（性能护栏为 R6 的硬约束）

| 维度 | 要求 |
|------|------|
| **不拖垮 Redis 服务** | 值搜索单次任务：SCAN COUNT ≤ 500/批、键数上限默认 50k（可配，上限 200k）、累计读取字节预算默认 64 MiB、单值仅读前 4 KiB（GETRANGE 0 4095）；批间 sleep ≥ 1ms 让出服务线程；任意时刻**至多 1 个**运行中的扫描任务，新任务自动取消旧任务 |
| 可取消 | 搜索/解码大值均可前端取消，后端游标任务响应 abort |
| 补全取数 | 键名补全每次仅取 ≤50 条（SCAN COUNT 200 + MATCH 前缀，游标截断），输入防抖 150ms，同前缀结果缓存 3s；不阻塞击键 |
| 交互延迟 | 键列表翻页（200/页）P95 < 300ms（不含网络）；解码 ≤ 5 MiB 值 P95 < 1s |
| 内存 | 大值解码上限维持 50 MiB（超限时拒绝并提示，沿用现有 `DECOMPRESS_MAX_BYTES`） |
| 兼容 | 现有命令 payload 不变（新增命令/新增可选参数）；旧 E2E 旅程不回归 |
| 二进制正确性 | 任意字节序列（含 \0、非法 UTF-8）经 传输→视图→复制/下载 全链路无损 |
| 翻译 | 新增文案只改 `en.ts`（i18n-sync skill 统一补齐） |
| 文件规模 | 单文件 ≤ 800 行；`StreamEditor.tsx`(716) 接近上限，新增视图逻辑必须拆新文件；Rust 侧超限文件（`ops.rs` 1160）只减不增，新逻辑落新模块 |

## 6. 验收标准

1. **二进制往返**：向测试库写入含 `\x00\x01\xff` 的 STRING，DataZen 详情页 Hex 视图字节与 `redis-cli --no-raw DUMP` 长度一致，Base64 视图与 `GET` 的 base64 编码一致。
2. **解码**：对 gzip 压缩的 JSON 值，点「GZip」+「JSON 视图」正确渲染解压后 JSON；Msgpack/Pickle/PHP/Java 样例值各解码成功（样例 fixture 入库）。
3. **db 键数**：导航树 16 个 db 全部显示 `(n)`，与 `redis-cli -n i DBSIZE` 一致；点击刷新后误差为 0。
4. **层级浏览**：对 `a:b:c:d:1` 四级键，逐层展开仅请求当前层数据（Network/命令日志验证），折叠再展开状态不丢失。
5. **搜索**：10 万键库上值搜索不使 redis `instantiation_1s`/延迟明显恶化（护栏参数生效、可取消、进度可见）；键模式结果与 SCAN MATCH 一致。
6. **大小**：详情页「大小」与 `STRLEN`/`MEMORY USAGE` 一致；聚合类型同时显示元素数。
7. **JSON 三态**：树形/原文/格式化/压缩四视图切换无损，原文编辑保存后 ReJSON 数据正确。
8. **Console 补全**：`SE` 弹出 SET/SETEX/SETNX… 候选（含描述+语法+分组）；`SET ` 后弹出当前 db 键名前缀候选；↑↓/Tab/Enter/Esc 键盘旅程逐步正确；10 万键库上补全不阻塞击键（防抖+限量生效）。
9. **高危提醒与 SafeMode**：输入 `FLUSHALL` 即时出现 ultra-danger 徽章；SafeMode 开启时 Console 执行 SET、工作台保存值、批量删除均被阻断且文案指向 SafeMode；关闭 SafeMode 后 danger/ultra-danger 仍弹确认框、allowFlush=false 时 Flush 入口仍隐藏；Safe 徽章与 Settings 开关联动实时刷新。
10. **回归**：`pnpm test:unit:drivers`、`cargo test -p datazen-driver-redis`、Redis E2E 全绿；现有 7 类编辑器行为不变。

## 7. 风险与开放问题

| 风险/问题 | 应对 |
|------|------|
| Msgpack/Pickle/Java 解码放前端会引入大依赖（JS 反序列化库对 Java Serialized 支持差） | 解码统一放 **Rust 后端**（新命令 `decode_value`），前端只做视图渲染与 gzip/zlib/deflate（已有浏览器能力） |
| Pickle 反序列化有任意代码执行风险（恶意值） | 后端实现**受限解析**：仅解析 pickle 基础 opcode（int/str/bytes/list/dict/tuple/None/bool/float），遇 `REDUCE`/`GLOBAL` 等执行类 opcode 拒绝并提示 |
| Java Serialized 解析复杂 | v1.0 仅解析流头部 + 常见 String/数值/对象字段名树（只读元数据），无法解析时明确报错而非 panic |
| 值搜索在大库上仍可能慢 | 默认只扫当前 db；UI 明示"值搜索为尽力而为"；提供按 db 并行度=1 的队列 |
| `MEMORY USAGE` 在旧 Redis（<4.0）不可用 | 能力探测，降级为仅显示逻辑长度 + STRLEN 字节 |
| db 数量非 16（`CONFIG databases`） | 沿用现有 `get_databases` 结果驱动行数，键数命令按实际数量循环 DBSIZE |
| Redis 命令不经后端 sql_guard，SafeMode/危险门闸仅在前端 | v1.0 接受该信任边界（与 allowFlush 同层级）；MCP/Workflow 旁路写路径的 server 侧 safeMode 拒绝列为 v1.1 评估项（DESIGN §4.8.4） |
