# Wave 5 跨轨台账

> 开发期产物，**交付时必须删除**。

## 轨道状态

| 轨 | 分支 | worktree | 状态 |
| --- | --- | --- | --- |
| A `fk-navigation-ui` | `feature/fk-navigation-ui` | `.worktrees/datazen-fk-ui` | 复测中（`d1d00010`） |
| B `filters` | `feature/filters` | `.worktrees/datazen-filters` | 复测中（`1ec9edd3`） |

## ⭐ 本波最重要的发现：错误码此前**在桌面 UI 里一个都到不了**

轨 A 报出并修复：宿主 `CommandError::Serialize` 对普通错误产出**纯字符串**；`packages/backend-client/src/errors.ts:225-230` 的 `deserializeApiError` 对**非 record** 的拒绝一律替换成 `'Backend rejected the request with an unreadable payload.'` ⇒ **`grid.*` 前缀被整句丢掉**。

⇒ Wave 1–4 累计的 8 个错误码、27 处 throw 点，**在 UI 里全部不可达**。修在**桌面适配器**（`src/platform/tauriBackendTransport.ts`）——**只有那里还看得到原始字符串**。跨轨副作用：此后所有命令失败时前端看到 `"<method> failed: <真实消息>"` 而不是那句无信息的话。

**教训（已写进契约 §8.2 〔生产者盘点〕）**：**错误码的验收必须包含端到端往返**（Rust 抛出 → IPC → transport → client → classify），**只测分类器等于没测**。本批之前所有针对错误码的测试都停在分类器。

## 本波的两条分册自相矛盾（已订正契约）

1. **07 的 `data-dt-fk` 值域**：00 §4.4 + 12-testing 说"值即引用目标标识"，07 写 `"true"` ⇒ 以 00 为准。
2. **08 的 `join_filters`**：§9.1 两条断言互斥、§10 第 12 条在**同一格里**自相矛盾（说该渲染成 `(A OR B) AND C`，下一句又论证数据库解析成 `A OR (B AND C)`、结果集是错的）⇒ 以 **SQL 优先级**为准。左到右渲染会**静默改变结果集**。

## 跨轨债务（Wave 6 前要处理）

| # | 事项 | 负责 | 触发条件 |
| --- | --- | --- | --- |
| F1 | `src/types/index.ts` **825**（基线 839，本波一度 913，已回落） | 下一个碰它的轨 | D1 拆分债仍在 |
| F2 | `packages/driver-api/src/traits.rs` 1245 / `reuse.rs` 1459（本波之前就越线） | 下一个碰它的轨 | 拆 trait 会破所有驱动 |
| F3 | `grid.filter.*` 两个码**前端未登记**（`gridErrors.ts` / `DataGridCapabilities`） | Wave 6 的 08 前端半 | 08 §5 Step 8 / Q-12 |
| F4 | 12 个新算子的 i18n 标签 + `filter.like` 文案 | 同上 | 08 §5 Step 15 / §8 |
| F5 | `data-sync` / `data-transfer` **不支持**新算子 ⇒ 显式拒绝 | 已知，不改 | 静默按等值比较会返回错行集 |
| F6 | `In` 算子收到非数组非字符串值 ⇒ 现在渲染 `col IN ('5')`（此前静默丢弃） | 行为变更，已接受 | 只影响病态输入 |
| F7 | **08 的前端半**（Step 7-11/14-16）一直没人认领 | **Wave 6** | 见下波排法 |
| F8 | 探针**回落路径**（clickhouse/duckdb/rqlite/turso）值会进 SQL 文本 | 已知残留 | 这四个驱动现网都硬编码 `foreign_keys: Vec::new()`，走不到 |
| F9 | 真实 PG / SQL Server / MySQL / SQLite 的外键跳转端到端 | R 回归 | 本环境无 DB；FK2-D1 正是 mock 看不见的那类缺陷 |
| F10 | i18n 同步：每个非英语 locale 缺 170 key（16 个是本波新增，其余既有） | 发布前 | `node scripts/i18n-sync-check.mjs` 是发布硬门禁 |
| **F11** | **`grid.selection.*` 三个码登记了却零生产者**（Wave 1 的 `grid-selection`）：越界一律 `clampCellCoord → null`，**从不抛错** | **需要一次显式决定** | 轨 B 新加的门禁 `src/lib/__tests__/gridErrors.registration.test.ts` 把它们列进 `PRODUCER_PENDING`，并加了一条断言：**它们一旦有了生产者，门禁就红**，逼着把 `PRODUCER_PENDING` 删掉。**二选一**：①让选择层改抛错（**行为变更**：静默夹取 → 明确报错，需独立轨 + 复测）；②撤登记并从契约 §8.2 删行。**不能一直挂着**——契约自己写着"只登记有生产者的码" |
| F12 | `src/lib/gridErrors.ts` 的文件头注释仍写「Every entry here MUST have a real producer」，而 `grid.selection.*` 三条没有 | 协调者（合流后改注释） | 注释要写明 `PRODUCER_PENDING` 这个例外与它自清的机制 |

## 本波又一条方法论：**重构会静默弄丢回归锁**

轨 B 的 Tester 用变异抓出：拆分测试文件（`tests.rs` 791 行 → `tests.rs` / `join_tests.rs` / `test_support.rs`）时，`overlong_regex_pattern_is_refused` **被删掉了** —— 而"用例还在、只是断言变了"和"整条分支无人看守"是两件事。**证明方法**：把 `filter_sql/mod.rs` 的长度守卫 `> MAX_REGEX_PATTERN_LENGTH` 改成 `> usize::MAX`（等于关掉）⇒ **395 tests 依然全绿** ⇒ 这条分支没有任何测试。

⇒ **两条通用做法**（已在本批用上）：
1. **针对性变异**当验收手段：要证明"某分支被守着"，就**关掉它**，看有没有用例变红。这比"用例存在"强一个量级，成本却只有几分钟。
2. **测试面清单**：像轨 A 的 `ACTION_NAMES`（冻结 store 的 action 名单）那样，为**大测试文件**冻结一份**用例名清单**（`src/lib/__tests__/…`），重构拆分时清单对不上就红 ⇒ **丢用例立刻可见**，不靠人眼比对 diff。

## Wave 6 排法（Wave 5 合流后执行）

**一轨前端 + 一轨后端**，不排两轨前端（08 与 02 的 i18n 都落 `src/locales/en/query.ts`，同波会冲突）：

| 波 | 前端轨 | 后端轨 |
| --- | --- | --- |
| 6 | **08 的前端半**（算子元数据 / 值控件拆分 / 逐条连接符 / `grid.filter.*` 登记 / i18n / 命名视图 UI / E2E） | **09 计数**（`estimate_table_rows` 落地 ⇒ 桌面 `unsupportedMethods` 少一条 ⇒ 那条"注册 vs 声明"交叉核对测试会红，是设计好的提醒） |
| 7 | 02 键盘 | 11 键集分页（`query_executor` 链③） |
| 8 | 03 剪贴板 | 10 结果网格 |
| 9 | 05 编辑器 | 12 测试（E2E 契约矩阵，最后一波） |

**交接给 Wave 6 的硬信息**：
1. `grid.filter.*` 两个码**后端已贴**，前端**必须**登记，否则 `classifyGridError` 返回 `unknown`；
2. 错误码的验收**必须**包含端到端往返（见上面那条教训）；
3. 筛选类型的唯一真源是 `src/types/filters.ts`（`src/types/index.ts` 用 `export *` 再导出，消费方 import 不变）；
4. `filter.mixedLogic` **已删净**（`grep -rn "filter.mixedLogic" src/` → 0），逐条连接符走 `FilterCondition.connector?`；
5. 筛选 SQL 的构造与判定**全在共享层** `packages/feature-core/src/services/query_executor/filter_sql/`，前端只消费 `filter.*` 词条与能力位。
