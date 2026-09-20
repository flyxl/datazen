# DESIGN: Redis 操作台优化

> 对应 PRD: `docs/todo/redis/PRD.md`（R1–R14）· 状态: Draft
> 范围：`packages/drivers/redis`（Rust crate + 前端 UI）+ `src/windows/connection/navigator`（导航树 db 行）。宿主保持薄壳，不新增 Redis 专属逻辑。

## 1. 现状架构回顾（改动落点）

```text
前端                                                  后端 (datazen-driver-redis)
RedisConnectionView (tab 壳)
 └ RedisWorkbench ── useRedisKeyScan ──→ scan_keys ──→ redis_driver_on.rs (SCAN+逐键 TYPE/TTL/长度/preview)
      ├ KeyBrowserControls (类型/Flat-Tree/Memory)
      ├ KeyTable (虚拟表, keyTree.ts 客户端分组)
      └ KeyDetailEditor (KeyEditors.tsx 分发)
           ├ StringEditor (textarea + gzip 浏览器解压)
           ├ Hash/List/Set/Zset Editor (per-type 游标)
           ├ StreamEditor / JsonEditor (ReJSON)
宿主 NavigatorTreeRow 'kv-db' 行 ──→ list_databases(schema_catalog) ──→ redis_driver_db.rs::get_databases
```

关键缺口：值管道文本化（`get_key_detail_on` → `Option<String>`）、`get_key` 无界全量读、Tree 分组仅作用于已加载页、无 db 计数。

## 2. 总体设计

四条主线，正交分层：

1. **二进制安全通道（L0）**：新增 raw/base64 值传输 + 后端解码命令，是 R2/R7/R8/R9 的地基。
2. **键浏览升级**：层级按需取数（R5）、db 计数（R4）、仅无过期（R10）。
3. **受控值搜索**：后端有护栏的扫描任务命令 + 前端进度/取消（R6）。
4. **查看器 UI**：详情页「解码 × 视图」矩阵 + JSON 三态（R3/R7/R8/R9），复用 L0 通道。
5. **Console 智能补全**：命令元数据表 + 弹窗候选 + key 参数位键名懒取数（R11/R12），前端独立模块，不改执行路径。
6. **危险门闸与 SafeMode**：接线既有 `redisConsoleDanger.ts` 四级分类做全链路提醒，并复用宿主 `settings.safeMode` 单一开关 gate 全部写路径（R13/R14）。

## 3. 后端设计（`packages/drivers/redis/src/`）

### 3.1 数据模型：ValueFrame

所有新命令的统一原始值载体（serde → JSON，前端 TS 类型镜像进 `src/types/index.ts`）：

```rust
pub struct ValueFrame {
    pub key: String,
    pub key_type: String,        // "string" | "hash" | ...
    pub ttl: i64,                // 秒；-1 永久，-2 不存在
    pub logical_len: u64,        // STRLEN / HLEN / LLEN / SCARD / ZCARD / XLEN
    pub mem_bytes: Option<u64>,  // MEMORY USAGE（能力探测，None=不可用）
    pub raw_b64: Option<String>, // 仅 string：原始字节的 base64；超限时 None + truncated=true
    pub truncated: bool,
}
```

### 3.2 新命令清单（`commands.rs` 注册 + `commands_exec_dispatch.rs` 分发）

| 命令 | 参数 | 行为 | 服务需求 |
|------|------|------|---------|
| `get_key_raw` | `key`, `with_memory?` | TYPE/TTL/长度 + 对 string 执行 `GET` 取 `Vec<u8>` → base64；**不再**对聚合类型做无界 HGETALL（聚合类型编辑仍走既有分页命令） | R2/R7 |
| `db_sizes` | 无 | 对 `0..databases-1` 逐库 `SELECT`+`DBSIZE`（专用连接、pipeline 优先、串行回退），返回 `[{db, keys}]` | R4 |
| `list_children` | `prefix`, `cursor`, `count`, `sep?`, `no_ttl_only?`, `key_type?` | SCAN MATCH `{prefix}*`，服务端按"下一段分隔符"切分：裸键直接入结果；更深层折叠为 `{childFolder, count}`（用 `*` 续扫估算或首层精确计数，见 3.3） | R5 |
| `scan_values` | `pattern`, `mode`(key/value/all), `cursor`, 护栏参数 | 受控扫描任务，见 §3.4 | R6 |
| `scan_abort` | `task_id` | 置 abort 标志，`scan_values` 下一批前检查并返回 `cancelled` | R6 |
| `decode_value` | `raw_b64`, `codec`(msgpack/pickle/php_serialize/java_serialized) | 字节 → 受限解码 → `{ok, text?, json?, error?}`（输出统一为可读文本或 JSON 树字符串，交给前端视图层） | R8 |
| `count_no_ttl`（可选优化） | — | v1.1 再评估，v1.0 不做 | R10 |

改动（非新增）：

- `scan_keys_with_info_on`：增加可选 `no_ttl_only` 参数（逐键 TTL 已有，过滤即可）；**hash preview 从 HGETALL 改为 HSCAN COUNT 3**（修既有大 hash 隐患）；返回增加 `size_bytes: Option<u64>`（`with_memory` 时）。
- `get_databases`（`redis_driver_db.rs`）：payload 增加可选 `key_counts`，由前端在需要时另调 `db_sizes` 合并（保持 schema_catalog 路径轻量，导航树懒加载）。

### 3.3 层级展开算法（`list_children`）

分隔符沿用 `keyTree.ts` 约定（首个 `:` 或 `.`，改为可配 `sep`，默认 `:`）。对每个 SCAN 命中的 key：

```text
rest = key.strip_prefix(prefix)
seg  = rest 到下一个分隔符为止（无分隔符 → 裸键）
有分隔符 → folder = prefix+seg+sep（聚合计数 +1）
无分隔符 → leaf key 直接返回（带 ValueFrame 摘要字段）
```

游标语义：沿用 SCAN cursor（本层游标），folder 计数在游标走完前为"已见估算值"，UI 标注 `~`；游标归零后为精确值。这样**每层只传输直接子项**，解决"tree 只分组已加载页"的问题。folder 计数精确化代价（每个 folder 一次 `count_matching`）默认关闭，仅当该层 key 总数 < 5000 时自动精确。

落点：新文件 `ops_tree.rs`（纯函数切分 + 命令实现），便于单测。

### 3.4 值搜索护栏（`scan_values`）

状态：驱动内 `Mutex<Option<ScanTask>>`（同连接会话），新任务替换旧任务并置 abort。

```text
参数（默认/上限）：
  max_keys:        50_000 / 200_000     — 累计 SCAN 键数
  byte_budget:     64 MiB  / 256 MiB    — 累计 GETRANGE 字节
  per_value_peek:  4 KiB   / 32 KiB     — 单值只读前 N 字节
  count:           500    / 2000        — 每批 SCAN COUNT
  type_scope:      string-only（v1.0 固定）
节流：每批之间 yield（tokio::time::sleep 1ms），SCAN+逐键 (TTL? + STRLEN + GETRANGE) 用 pipeline 合并往返
匹配：glob pattern → 值做 case-insensitive 子串（bytes 层 memchr，不做 UTF-8 校验，支持 \xNN 转义）
返回：{task_id, done, cancelled, scanned_keys, matched: [{key, matched_in: key|value, frame摘要}], cursor}
```

前端以 `cursor` 连续轮询调用（同一 task 增量返回），实现进度条与"达到上限提前结束"提示。键模式仍走现有 `scan_keys`（不动）。

### 3.5 解码器（`decode_value`，新模块 `decode/`）

| codec | 实现 | 输出 | 降级策略 |
|-------|------|------|---------|
| msgpack | `rmpv`（纯 Rust，容忍任意嵌套） | JSON 树字符串 | 失败报 offset |
| pickle | **自研受限解释器** `decode/pickle.rs`：仅 PSETPROTOCOL/INT/FLOAT/UNICODE/BINBYTES/LIST/TUPLE/DICT/EMPTY*/APPEND/SETITEM/BINPUT 等数据类 opcode；遇 GLOBAL/REDUCE/INST 等 → `Err("含可执行对象，拒绝解析")` | JSON 树字符串 | 同上 |
| php_serialize | `php_serialize` crate 或自研标量/数组解析（约 200 行，无依赖优先） | JSON 树字符串 | 嵌套过深(>64) 截断 |
| java_serialized | 自研 `decode/java.rs`：解析 TC_* 流头 + String/装箱数值/字段名树（只读元数据，不还原类实例语义） | 结构文本 | 无法解析报"仅支持基础类型流" |

gzip/zlib/deflate/base64 **不走后端**：前端已有 `DecompressionStream` 路径（`stringKeyValue.ts`），保留并纳入统一管线。依赖新增仅 `rmpv`（+ 视选型 `php-serde`），控制在净室合规范围。

## 4. 前端设计（`packages/drivers/redis/ui/`）

### 4.1 查看管线（解码 × 视图）— R8/R9

新模块 `valueView/`（全部纯函数，可单测）：

```text
rawBytes(Uint8Array)
  → codecPipeline: none | gunzip | inflate | inflateRaw | base64Decode   (浏览器, 复用 stringKeyValue.ts)
  → backendDecode: msgpack | pickle | php | java  (调 decode_value, 返回文本/JSON 后重入视图层)
  → viewRenderers: utf8 | ascii | binary(0x1F8B 风格 hex+ascii 双栏) | hex | base64
                   | json(高亮树/文本) | unicodeJson(json 视图 + ensure_ascii 反转义)
                   | yaml | xml (yaml/xml → 文本解析失败时降级为 UTF-8 + 错误提示)
```

新组件：

- `ValueViewer.tsx` — 详情页查看器容器：`解码` 按钮组（水平 Tabs 风格，同截图）+ `视图` 按钮组 + 自动换行开关 + 大小徽章 + 复制/下载原文字节。状态机：`{codec, view, decodedBytes, error, busy}`；codec 变更后 view 重渲染，不改内存中的原始字节（写回永远基于原始字节，避免"解码态误保存"）。
- `valueView/codecs.ts` / `valueView/views.ts` — 纯逻辑；`views.ts` 内 YAML/XML 解析用轻量依赖（`yaml`、`fast-xml-parser` 已在/可加 dev→runtime 评估，优先复用仓库现有依赖）。
- StringEditor 改造：编辑区保留 textarea（R1 不回归），只读查看默认走 `ValueViewer`；「编辑」切换到可写 textarea（保存路径不变 `set_string`，新增二进制保存 `set_string_raw`（参数 base64）以支持 Hex 视图小改）。

### 4.2 JSON 三态 — R3

新组件 `JsonModeBar.tsx`（`[原文|格式化|压缩]` + 现有树形 toggle），复用于：

- `JsonEditor.tsx`（ReJSON）：树形视图保持默认；「原文」= `JSON.GET` 裸文本（现有 json_get 命令加 `raw=true` 参数返回未 pretty 字符串）；格式化/压缩为前端 `JSON.parse` → `stringify(x, null, 2)` / `stringify(x)`；原文 tab 可编辑，保存走既有 `json_set`。
- `StringEditor`：检测到 JSON（`looksLikeJsonText`）时挂载同款 ModeBar，替代现有单一 Format 按钮。

### 4.3 键浏览升级 — R5/R10

- `useRedisKeyScan.ts` → 抽出 `useKeyTree.ts`：树节点独立状态 `Map<prefix, {rows, cursor, done, expanded}>`，展开 folder 时调 `list_children`，折叠不清数据（缓存），刷新只重拉已展开层。
- `KeyTable.tsx`：tree 行模型增加 `depth`/`folderPrefix`/`pendingCount(~)`；Flat 模式行为不变。
- `KeyBrowserControls.tsx`：增加「仅无过期」toggle（透传 `no_ttl_only` 到 scan_keys/list_children）。

### 4.4 三模式搜索 — R6

- `KeyBrowserControls` 上方新增 `SearchModeTabs`（键/值/全部，截图同款三 tab）。
- 新 hook `useValueSearch.ts`：值/全部模式时启动 `scan_values` 轮询循环（requestAnimationFrame 节流、进度 `scanned/max`、取消按钮 → `scan_abort`）；结果列表复用 KeyTable（新增"命中来源"徽标列，全部模式下键命中在前）。
- 侧栏 pattern 搜索框保持现状（键模式）。

### 4.5 db 计数 — R4

- 驱动 UI 侧栏（`RedisWorkbench.tsx`）：`db{n} ({count})`；进入 Items tab 时后台请求 `db_sizes` 一次，侧栏顶部复用现有刷新按钮联动重取；切换 db 时只更新该 db 计数（用已有 `dbSize` 返回值，零额外命令）。
- 宿主 `NavigatorTreeRow.tsx` `kv-db` 分支 + 其数据源 store：kv 连接展开 db 层时经 `execute_driver_command(db_sizes)` 填充（走 driver command，宿主不 hardcode redis 语义；由驱动 meta 声明 `dbCountsCommand: 'db_sizes'` 元数据驱动，符合零硬编码约定）。
- `RedisWorkbench.tsx` 现 513 行，加计数与搜索 tab 后预计 ~600 行，仍在限内；`SearchModeTabs`/`DbSidebar` 视体量拆出。

### 4.6 大小显示 — R7

- `KeyDetail` 类型扩展为携带 `ValueFrame`（向后兼容：现有 `value` 字段保留）。
- 详情头部徽章：`大小: {formatSize(mem_bytes ?? logical_len)}` + 悬浮 tooltip 区分「逻辑长度 / 内存占用」；聚合类型显示 `{logical_len} 元素 · {mem_bytes} B`。
- 列表 size 列：勾选 Memory 前显示逻辑长度并加单位后缀语义（`20 条` vs `1.2 KB`），消除歧义。

### 4.7 Console 命令/键名补全弹窗 — R11/R12

现状：`RedisConsole.tsx` 仅有 Tab 触发的内联 ghost 提示（输入框旁一行小字），候选来自 `redisCommands.ts` 的纯命令名数组 + workbench 传入的静态 `keySuggestions`；无可见下拉列表、无描述/语法、key 候选非按需取数。

设计（保持 textarea，不引入 CodeMirror，避免影响现有执行/历史/危险命令门控）：

```text
ui/consoleCompletion/
  ├ commandMeta.ts      — 命令元数据表（新）
  ├ useCompletion.ts    — 上下文判定 + 候选状态机 hook（新）
  └ CompletionPopup.tsx — 弹窗渲染（新，替代内联 ghost）
```

1. **命令元数据**：`REDIS_COMMAND_META: {name, group, syntax, desc}[]`，覆盖 `redisCommands.ts` 现有 ~120 命令（group 用 redis 官方分类：string/hash/list/set/zset/generic/server/keyspace…；desc 一句英文说明，随驱动 locale `en.ts` 走 i18n 约定）。纯静态表，放驱动 ui 内，不进宿主。
2. **上下文判定**（`useCompletion`，三要素状态机）：
   - 解析光标前文本 → tokens；**token[0] 且在命令位** → 命令模式（前缀大小写不敏感过滤 `REDIS_COMMAND_META`）；
   - **token[0] 已完成、光标在 token[1] 位** → 键模式（前缀 = token[1]）；
   - token 数 ≥2 或光标在空白行 → 退出（popup=null）。进入/退出仅由文本与光标决定，无死锁。
3. **键候选取数**：复用现有 `scan_keys` 命令（`pattern={prefix}*`, `count=200`），前端截断 50 条展示；防抖 150ms + `Map<prefix,{items,ts}>` 3s 缓存 + 请求序号竞态防护（丢弃过期响应）。切换 db/刷新时清缓存。不新增后端命令。
4. **弹窗 UI**（`CompletionPopup.tsx`）：绝对定位于输入框上方（截图样式）；命令条目三行结构（名称加粗 / 描述截断 + 右侧 group 标签 / 灰色语法行），键条目单行（键名 + 右侧 `key` 标签）；虚拟滚动（候选 ≤50 无需虚拟化，上限内直接渲染）；鼠标 hover 选中、点击接受。
5. **键盘跃迁**：popup 打开时 ↑↓ 移动、Tab/Enter 接受插入（补全 token 后追加空格）、Esc 关闭并吞掉本次事件；popup 关闭时 Enter=执行、↑↓=历史（现有行为）。与历史冲突处理：仅当 popup 可见时重映射，状态机退出即恢复。
6. **数据属性解耦**（AGENTS 原则）：候选项 `data-completion-index` 绑定，点击接受不依赖几何坐标反查。
7. `keySuggestions` prop 保留为"预热候选"（已加载页的键名，零延迟），与懒取数结果合并去重。

### 4.8 高危提醒 + SafeMode 门闸 — R13/R14

现状事实（写入设计前已核实）：

- `redisConsoleDanger.ts` 已实现 `classifyDangerLevel`（safe/write/danger/ultra-danger 四级 + `requiresConfirmation` + `dangerBadgeColor`），但 **RedisConsole.tsx 未 import 使用**——分类器是死代码，需接线而非新造。
- SQL 端 SafeMode：全局单一开关 `settings.safeMode`（`src/types/index.ts:264`，Settings 页 `SettingsContent.tsx:613`），SQL 执行经 `useQueryExecutionGate.tsx` 在**执行时刻**读 live 值（`useSettingsStore.getState().settings.safeMode`，L132）硬阻断（`blockedSafeMode` error dialog）；数据网格侧 `ContentView.tsx:201` 用 safeMode 强制只读。工具栏 Safe 徽章见 `QueryEditorSection.tsx:451`。
- Redis 驱动 UI 已直接读宿主 settingsStore（`RedisWorkbench.tsx:72` 的 `driverSettings.allowFlush`），复用同一 store 无架构障碍。

设计：

1. **统一门闸 hook** `ui/useRedisGate.ts`（新，宿主 settingsStore 订阅）：
   ```ts
   gateWrite(level: DangerLevel | 'write-op'):
     safeMode(live)===true → showMessageDialog(blockedSafeMode) → return false
     level==='danger'|'ultra-danger' → confirmDangerDialog(command, level) → 用户确认才 true
     flush 类 → 还需 allowFlush===true（既有门控，叠加判断）
   ```
   - 与 SQL 端一致：**执行时刻读 live 值**（不落快照，避免竞态语义分歧）；`safeMode` 变化即时反映到徽章。
   - Console 路径：入参命令先 `classifyDangerLevel` 再 `gateWrite`；工作台编辑器路径（保存/删除/批量/Flush/发布/导入）统一以 `'write-op'` 级别过 `gateWrite`（SafeMode 阻断，但不弹 danger 确认——确认框只属 Console 语义）。
2. **Console 显著提醒（R13）**：
   - 输入行右侧实时徽章：对当前输入首 token `classifyDangerLevel`，非 safe 时渲染 `<Badge>`（复用 `dangerBadgeColor`，write=黄/danger=橙/ultra-danger=红+⚠），纯展示零执行耦合。
   - 补全弹窗（§4.7）条目按等级着色：danger/ultra-danger 命令行左侧 ⚠ 图标 + 组标签替换为等级标签；键盘旅程不变。
   - 结果行：`result.command` 对应等级非 safe 时整行左边框着色（现有 `text-danger` 错误色保留，二者正交）。
   - 执行前确认：`requiresConfirmation(level)` 为真时弹 `@datazen/ui` Dialog，正文列出命令名、等级、影响提示（如 FLUSHALL「清空当前节点所有 db 数据，不可逆」）；确认按钮需 1s 后才可点（防连击误触）。
3. **SafeMode 徽章（R14）**：`RedisConsole.tsx` 与 `RedisWorkbench.tsx` 工具栏各挂一个 `SafeModeBadge`（新小组件 `ui/SafeModeBadge.tsx`，从 QueryEditorSection 的展示语义对齐：safeMode=true 显示，点击跳 Settings 外观/安全区），不新增任何独立开关、不写新 settings key。
4. **后端纵深（可选增强，默认不做）**：Redis 命令不走 sql_guard，前端是唯一门闸。v1.1 评估在 `exec`/写类 command 的 Rust 侧接受 `safeMode: bool` 参数做二次拒绝（防未来 MCP/workflow 旁路）；v1.0 明确不做，文档标注该信任边界。
5. **i18n**：新增文案 `redis.danger.*` / `redis.safeMode.blocked` 只进 `locales/en.ts`；blocked 文案与 SQL 端 `query.editor.executionConfirm.blockedSafeMode` 保持同义。



## 5. 数据流示例（详情页打开一个 gzip+msgpack 值）

```text
用户点击 key
 → get_key_raw {raw_b64: "H4sIAAAA...", logical_len: 812, mem_bytes: 1040}
 → ValueViewer 初始 {codec: none, view: utf8} → utf8 渲染乱码（二进制）
 → 用户点「GZip」→ 浏览器 DecompressionStream(rawBytes) → 新字节
 → 用户点「Msgpack」→ decode_value(raw_b64=gzip解码后bytes, codec=msgpack) → JSON 字符串
 → 用户点「JSON 视图」→ JSON 树渲染
 全程：原始 raw_b64 不变；保存按钮禁用（codec≠none 时只读），避免解码态写回。
```

## 6. 兼容性与边界

- 旧命令 payload 全部不变；`get_key` 保留（聚合类型编辑器继续用），仅新详情查看走 `get_key_raw`。
- Redis < 4.0：`MEMORY USAGE`/`OBJECT` 缺失 → mem_bytes=None；Redis < 6.2 `GETRANGE` 全版本可用，值搜索无版本门槛；`SCAN` 全版本可用。
- Cluster 模式：`db_sizes`/`scan_values` 在非 cluster 仅可用（meta 声明能力，UI 隐藏入口）；cluster 现有 per-node pin 浏览保持。
- 大值：`get_key_raw` 超 `MAX_RAW_BYTES`(50 MiB) 时 `truncated=true`、raw_b64=None，UI 提示仅可逻辑编辑（沿用现有上限）。
- 安全：`decode_value` 全部只读解析、受限 opcode、输入长度上限；无 `unwrap`/panic 路径（panic-policy）。

## 7. 测试设计（驱动测试落点规则）

| 层 | 位置 | 内容 |
|----|------|------|
| Rust 单元 | `decode/` 各文件 `#[cfg(test)]` | 每种 codec 的 fixture 解码 + 恶意 pickle(GLOBAL/REDUCE) 拒绝 + java 截断流 |
| Rust 单元 | `ops_tree.rs` | 切分算法：多级/无分隔符/`sep` 混用/folder 估算 |
| Rust 集成 | crate 现有内联风格 | `scan_values` 护栏：max_keys/byte_budget/abort 幂等 |
| 前端单测 | `ui/__tests__/valueView.test.ts` 等 | codecs/views 纯函数（含 \0、非法 UTF-8 fixture）；JsonModeBar 三态；useValueSearch 状态机（fake timers + mock invoke） |
| 旅程测试 | `ui/__tests__/` | 连续击键搜索：切模式→输入→取消→重跑 的每步状态跃迁断言（AGENTS 状态机原则）；Console 补全击键旅程：`SE`→弹窗→`TAB`→` `→键候选弹出→`ENTER` 接受→Esc 退出恢复历史语义，逐断言；SafeMode 旅程：开→Console 执行 SET 被阻断→关→write 直通→danger 命令弹确认→取消不执行 |
| E2E | `packages/drivers/redis/e2e/redis.ts` 扩展 | seed 二进制/gzip/msgpack 键（`setup-demo-data.sh` 增补），验证 db 计数、层级展开、Hex 视图字节数 |
