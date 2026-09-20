# IMPLEMENTATION: Redis 操作台优化

> 对应 PRD/DESIGN: `docs/todo/redis/PRD.md` · `DESIGN.md` · 状态: Draft（需求 R1–R14）
> 全部改动落在 `packages/drivers/redis/`（Rust crate + `ui/`）与宿主导航树两处元数据驱动接入点。每个 Task 标注涉及文件、测试落点与验收门槛；P0→P4 顺序执行，P2/P3 可在 P1 合并后并行。

## Phase 0 — 二进制安全通道（地基，R2/R7）

### Task 0.1 `ValueFrame` + `get_key_raw`（Rust）

- `src/lib.rs`（或 `types.rs`）：新增 `ValueFrame` 结构（DESIGN §3.1）。
- `src/redis_driver_on.rs`：`get_key_raw_on()` — `Vec<u8>` 直取 `GET`（不经 `Option<String>`），TTL/TYPE/逻辑长度/`MEMORY USAGE`（能力探测缓存到连接级 flag）。
- `src/commands.rs` + `commands_exec_dispatch.rs`：注册 `get_key_raw`（scope 进 `permissions/default.toml`）。
- 顺带修复：`scan_keys_with_info_on` 的 hash preview `HGETALL` → `HSCAN COUNT 3`。
- 测试：crate 内联 `#[cfg(test)]`（含 `\x00\xff` 往返）；真库验证放 E2E（Phase 4）。
- 验收：base64 帧对二进制 key 与 `redis-cli GET | xxd` 字节一致（单测断言 Vec<u8> 相等）。

### Task 0.2 前端 raw 通道 + 详情大小徽章（R7）

- `ui/types` (`src/types/index.ts` 驱动侧镜像)：`ValueFrame` TS 类型；`KeyDetail` 扩展。
- `ui/KeyEditors.tsx`：详情头部接 `get_key_raw`，渲染「大小: N B」徽章（`formatSize` 从 `KeyTable.tsx` 提取到 `ui/formatSize.ts` 共享）；编辑/保存路径不变（R1）。
- `ui/__tests__/keyEditorsInvokes.test.tsx` 扩展：新 payload 断言。
- 验收：PRD 验收 7；STRING 显示 STRLEN 字节、聚合显示元素数 + MEMORY 字节。

## Phase 1 — db 计数 + 仅无过期 + 层级浏览（R4/R5/R10）

### Task 1.1 `db_sizes` 命令（Rust）

- `src/redis_driver_db.rs`：`db_sizes()` — 按 `CONFIG databases`（沿用现有值）逐库 SELECT+DBSIZE，专用连接串行；返回 `[{db, keys}]`。
- `commands.rs`/dispatch/permissions 注册。
- 测试：内联单测（mock/解析层）；DBSIZE 精确值 E2E 验证。

### Task 1.2 导航树 db 计数（宿主，元数据驱动）

- 驱动 `ui/meta.ts`：`DatabaseTypeMeta` 增加 `dbCountsCommand?: string`（redis 填 `'db_sizes'`）。
- `src/windows/connection/navigator/`（树数据源 + `NavigatorTreeRow.tsx` `kv-db` 分支）：kv 连接展开 db 层时经 `execute_driver_command` 调 `dbCountsCommand`，行内渲染 `db0 (52)`；失败/未声明则不显示（零硬编码）。
- `RedisWorkbench.tsx` 侧栏：同款计数 + 进入 Items 拉取一次 + 现有刷新按钮联动 + 切 db 用已返回 `dbSize` 就地更新。
- 测试：宿主 navigator 单测（mock command）；`ui/__tests__/redisWorkbench.test.tsx` 扩展。
- 验收：PRD 验收 3。

### Task 1.3 `list_children` + 前端层级树（R5）

- Rust 新文件 `src/ops_tree.rs`：切分纯函数 `split_child_segment(key, prefix, sep)` + 命令实现（DESIGN §3.3，folder 计数估算/小层精确策略）；注册命令。
- 测试：`ops_tree.rs` 内联单测覆盖多级/混分隔符/裸键/游标未完。
- 前端新 hook `ui/useKeyTree.ts`（从 `useRedisKeyScan.ts` 抽出 tree 状态：`Map<prefix, {rows, cursor, done}>`，展开拉数、折叠缓存、刷新重拉展开层）。
- `ui/KeyTable.tsx`：tree 行接 `useKeyTree` 数据源，`~count` 标注；Flat 路径不动。
- `ui/KeyBrowserControls.tsx` + `useRedisKeyScan.ts` + `ops.rs::scan_keys`：`no_ttl_only` 参数贯通（R10）。
- 测试：`ui/__tests__/keyTree.test.ts` 扩展 + 新 `useKeyTree.test.ts`（展开→折叠→刷新状态机旅程）。
- 验收：PRD 验收 4（逐层只请求当前层）。

## Phase 2 — 受控值搜索（R6）

### Task 2.1 `scan_values` / `scan_abort`（Rust）

- 新文件 `src/ops_value_search.rs`：`ScanTask` 状态（会话内 `Mutex<Option<..>>`）、护栏参数校验（clamp 到上限）、SCAN+pipeline(STRLEN+GETRANGE 0 peek) 批次循环、bytes 层子串匹配（`\xNN` 转义预处理）、批间 `sleep(1ms)`、abort 检查。
- 命令注册：`scan_values`（增量返回 `{task_id, scanned_keys, matched[], done, cancelled, limit_hit}`）、`scan_abort`。
- 测试：内联单测（护栏 clamp、匹配函数、abort 幂等）；真库限流验证进 E2E。
- 验收：10 万键 fixture 下 `instantiation_1s` 无飙升（手动记录，见 Phase 4 手工项）。

### Task 2.2 前端三模式搜索 UI（R6）

- 新组件 `ui/SearchModeTabs.tsx`（键/值/全部）+ hook `ui/useValueSearch.ts`（轮询循环、进度、取消；同一时刻仅 1 任务，切模式/改词自动 abort）。
- `ui/RedisWorkbench.tsx`：挂载 tab 组；值/全部模式时列表区切到搜索结果源（KeyTable 复用 + 命中来源徽标列）。
- 状态机三要素（AGENTS 原则）：进入（模式≠键 且 非空 pattern）、状态内（轮询+可取消）、退出（游标 done / abort / 切回键模式 / 清空输入）。
- 测试：`ui/__tests__/useValueSearch.test.ts`（fake timers 连续旅程：输入→切值→取消→重跑→done）。
- 验收：PRD 验收 5。

## Phase 3 — 解码 × 视图 × JSON 三态 × Console 补全 × 安全门闸（R3/R8/R9/R11/R12/R13/R14）

### Task 3.1 后端解码器（Rust）

- 新模块 `src/decode/`：`mod.rs`（`decode_value` 命令 + codec 枚举 + 50 MiB 输入上限）、`msgpack.rs`(rmpv)、`pickle.rs`（受限 opcode 解释器，GLOBAL/REDUCE/INST 拒绝）、`php.rs`、`java.rs`（TC_* 流头 + 基础类型/字段名树）。
- Cargo.toml 新增 `rmpv`（+ php 选型）；净室合规：仅解析、不执行。
- 测试：每个 codec 内联单测 + `tests/fixtures/` 字节样本（Python/PHP/Java 生成的 .bin）；恶意 pickle 用例必须 Err。
- 验收：四种 codec fixture 解码成功；执行类 opcode 100% 拒绝。

### Task 3.2 前端查看管线 + ValueViewer

- 新目录 `ui/valueView/`：`codecs.ts`（浏览器 gzip/zlib/deflate/base64，收编 `stringKeyValue.ts` 逻辑并保持其测试通过）、`views.ts`（utf8/ascii/binary/hex/base64/json/unicodeJson/yaml/xml 纯函数渲染模型）、`render.tsx`（各视图 React 渲染，hex/binary 复用虚拟列表防大值卡顿）。
- 新组件 `ui/ValueViewer.tsx`：解码按钮组 + 视图按钮组 + 自动换行 + 大小徽章 + 复制/下载字节（DESIGN §4.1/§5 状态机；codec≠none 时禁编辑）。
- `ui/KeyEditors.tsx` StringEditor：只读态挂 ValueViewer，编辑态保留 textarea；新增 `set_string_raw`（Rust 侧 base64→SET，小任务并入 3.1 或 0.1）。
- 测试：`ui/__tests__/valueView.test.ts`（\0/非法 UTF-8/大值截断 fixture）、`ValueViewer.test.tsx`（codec×view 状态旅程）、`stringKeyValue.test.ts` 保持绿。
- 验收：PRD 验收 1、2。

### Task 3.3 JSON 三态（R3）

- 新组件 `ui/JsonModeBar.tsx`（原文/格式化/压缩）。
- `ui/JsonEditor.tsx`（476 行）：接 ModeBar；原文 tab = textarea + `json_get` raw 参数（Rust `ops_json.rs` 加 `raw` 可选参数，返回未 pretty 字符串）；格式化/压缩前端处理；保存走 `json_set`。
- `ui/KeyEditors.tsx` StringEditor：JSON 检测时接同款 ModeBar（替换单一 Format 按钮）。
- 测试：`ui/__tests__/jsonModeBar.test.tsx`（四态切换无损、原文编辑保存 payload 断言）。
- 验收：PRD 验收 7。

### Task 3.4 Console 命令/键名补全弹窗（R11/R12，纯前端）

- `ui/redisCommands.ts`：新增 `REDIS_COMMAND_META`（name/group/syntax/desc），保留旧数组导出兼容；描述文案进 `locales/en.ts`（`redis.cmd.*` 或直接英文常量，按驱动 i18n 约定）。
- 新目录 `ui/consoleCompletion/`：`commandMeta.ts`、`useCompletion.ts`（上下文三态：命令位/键位/退出，DESIGN §4.7）、`CompletionPopup.tsx`（截图样式弹窗，`data-completion-index` 绑定）。
- `ui/RedisConsole.tsx`（341 行）：替换内联 ghost 为 popup；键盘跃迁（popup 可见时 ↑↓/Tab/Enter 接受、Esc 关闭，关闭后恢复执行/历史语义）；键候选复用 `scan_keys`（prefix MATCH、防抖 150ms、≤50 条、3s 缓存、竞态序号防护）；`keySuggestions` prop 语义降级为预热候选。
- 不新增后端命令；执行路径/危险命令门控/cluster pin 零改动。
- 测试：`ui/__tests__/useCompletion.test.ts`（token 判定纯函数 + 状态机跃迁）、`ui/__tests__/consoleCompletion.test.tsx`（连续击键旅程，fake timers 断防抖/缓存）、既有 `redisConsole.test.tsx` 回归保持绿。
- 验收：PRD 验收 8。

### Task 3.5 高危提醒 + SafeMode 门闸（R13/R14，纯前端，复用全局开关）

- 新 hook `ui/useRedisGate.ts`：`gateWrite(level)` — live 读 `useSettingsStore.getState().settings.safeMode`（对齐 SQL 端 `useQueryExecutionGate.tsx:132` 的 live-read 语义）→ 阻断走 `showMessageDialog`；danger/ultra-danger 弹确认 Dialog（1s 防连击）；flush 类叠加既有 `allowFlush` 判断。零新 settings key、零新开关。
- 新组件 `ui/SafeModeBadge.tsx`：订阅 `settings.safeMode`，true 时渲染「Safe」徽章、点击跳 Settings（交互对齐 `QueryEditorSection.tsx:451`）。
- `ui/RedisConsole.tsx`：接线死代码 `redisConsoleDanger.ts`（现未被任何 UI import）——输入行实时徽章、执行前 `gateWrite(classifyDangerLevel(cmd))`、结果行等级着色；与 3.4 的补全弹窗联动（danger 条目 ⚠ 着色）。
- 工作台写路径接 `gateWrite('write-op')`：`KeyEditors.tsx` 保存、`Hash/List/Set/Zset/Stream/JsonEditor` 增删、`BatchBar.tsx` 批量、`ImportExport.tsx` 导入、`PubSubPanel.tsx` 发布、`KeyWorkbenchDialogs.tsx` flush/rename/delete。逐点小改，不改数据流。
- 后端纵深（exec 侧 safeMode 参数拒绝）v1.0 不做，DESIGN §4.8.4 已标注信任边界。
- 测试：`ui/__tests__/useRedisGate.test.ts`（live 值语义：执行中途切开关下次调用即生效）、`redisConsoleDanger.test.ts` 扩展（分类→门闸组合）、SafeMode 阻断/放行旅程（见 DESIGN §7）；`ui/__tests__/` 各编辑器测试补 mock gate。
- 验收：PRD 验收 9。

## Phase 4 — 回归、E2E 与收尾

### Task 4.1 E2E 扩展

- `e2e/setup-demo-data.sh`：增补二进制 STRING、gzip+msgpack 值、多级命名空间键（`demo:lvl:a:b:c`）、无过期/有过期混合。
- `packages/drivers/redis/e2e/redis.ts`：db 计数断言、层级逐层展开、Hex 视图字节数、值搜索取消旅程、解码渲染。
- 运行：`pnpm e2e`（驱动专属脚本，不进默认矩阵）。

### Task 4.2 i18n 与文件规模治理

- `node scripts/i18n-sync-check.mjs` → i18n-sync skill 补齐全部语言（含既有 8 个占位文件的存量欠账）。
- 检查 `StreamEditor.tsx`(716) 未被继续加码；`RedisWorkbench.tsx` 超 ~650 行时拆 `DbSidebar.tsx`/`ItemsToolbar.tsx`。
- Rust 超限文件（`ops.rs`/`connect.rs`/`ops_stream.rs`）本特性只允许旁路新文件，不追加内容。

### Task 4.3 三维影响度自查（AGENTS 要求）

1. 缺陷闭环：R1–R14 逐条对照 PRD 验收标准勾验。
2. 同类不误伤：7 类编辑器回归（单测 + E2E 既有旅程全绿）；`get_key` 旧消费者（批量对话框、导入导出）不变。
3. 下一步顺畅：解码只读态给出"编辑原文"引导路径；值搜索 done 后自然回到键模式。

## 提交与验证命令速查

```bash
cargo test -p datazen-driver-redis                 # Rust 单元（每 Task 随做随跑）
pnpm test:unit:drivers                             # 驱动 UI 单测
node scripts/i18n-sync-check.mjs                   # 翻译完整性（Phase 4）
pnpm e2e:skip-build                                # 本地已编译 binary 快速 E2E
packages/drivers/redis/e2e 显式脚本                 # 驱动 E2E（不进默认矩阵）
```

## 里程碑与依赖

```text
P0 (0.1→0.2) ──┬─→ P1 (1.1→1.2, 1.3)  ─┬─→ P3 (3.1→3.2→3.3; 3.4/3.5 独立) ─→ P4
               └─→ P2 (2.1→2.2)        ─┘        （3.2 依赖 0.1 的 raw 通道）
```

- P0 是唯一硬地基；P1 与 P2 相互独立可并行（不同文件，冲突面仅 `commands.rs` 注册表与 `RedisWorkbench.tsx` 挂载点，按轨道约定各自追加）。
- Task 3.4（Console 补全）与 3.5（安全门闸）为纯前端、零后端依赖，可与 P1/P2 并行提前执行；两者共享 `RedisConsole.tsx` 挂载点，同轨道串行（先 3.5 后 3.4 亦可，合并时以 3.4 的弹窗组件消费 3.5 的等级着色）。
- 预估：P0 小、P1 中、P2 中、P3 大（pickle/java 解析器为主要工作量；3.4/3.5 各中小）、P4 小。
