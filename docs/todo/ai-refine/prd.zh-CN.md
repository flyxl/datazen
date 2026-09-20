# AI 功能优化 PRD

> 状态：Draft
>
> 范围：DataZen AI 模块（Chat / NL2SQL / 诊断 / EXPLAIN / NL Filter / Schema 文档 / 连接诊断 / 查询分析 / Workflow 生成 / MCP 工具 / 安全门 / Prompt）。
> 目标：修完 AI Review 中的全部 P0（正确性/可停/可信），显著提升 P1（体验与准确率），规划 P2（架构/可观测/商业化）。

## 1. 背景

AI Review 通读 `src/components/ai/*`、`src/stores/aiStore.ts`、`src-tauri/src/ai/*`、`src-tauri/src/commands/ai/*`、`packages/ai-api` 后发现三类系统性问题：

1. **不可停、不可见、不可信**：流式请求无法取消；NL2SQL 长时间无反馈；Tool Loop 无轮数/Token/时长护栏；JSON 解析遇解释性前缀即失败。
2. **状态全局污染**：Chat/NL2SQL/Filter/Explain 共用单例 store，多连接多 Tab 互相覆盖；思考（reasoning）与正文（content）双通道虽正确但显示层有两处状态残留 bug。
3. **上下文质量与安全表达不足**：DDL 丢失 FK/索引/注释；预算静态不随模型窗口缩放；严格模式下 Egress 提示隐藏，用户误以为不出网；全量连接清单无必要发送给模型。

本次优化一次性立项、分阶段交付，P0 必做，P1 争取同版本，P2 进后续版本。

## 2. 用户与场景

- 终端用户：在 QueryPanel 用 NL2SQL 生成 SQL；在侧边栏 Chat 问数、修 SQL；用诊断修报错；用 EXPLAIN 分析慢查询；用 NL Filter 快捷筛选。
- 企业用户：关心行数据不出境、凭据不泄露、有用量可审计。
- 驱动开发者：通过 `prompt_overrides()` 定制方言 Prompt。
- 模型运维：多 Provider（OpenAI/Anthropic/DeepSeek/Ollama/Custom），需能力矩阵与超时重试。

## 3. 目标

- 任何 AI 流式任务可取消，取消后保留已产出草稿。
- NL2SQL 生成过程可见（流式预览），可中途停止。
- 会话按连接隔离，多 Tab 不互相覆盖；思考与正文显示正确无残留。
- Tool Loop 有轮数/Token/时长上限，写类 MCP 工具执行前确认。
- 安全门统一为一条 gate 路径，凭据必脱敏，行数据默认不送，Egress 常驻提示。
- Schema 上下文含 FK/索引/注释，表召回按相关性排序，预算随模型窗口动态计算。
- Provider 能力按实测矩阵动态开关，超时重试合理，用量可见，有 👍/👎 反馈闭环。

## 4. 非目标

- 不新增新的 LLM Provider 类型（复用现有 5 类）。
- 不做云端 eval 平台（👎 只记本地 eval 集）。
- 不改变 Community/Pro 构建划分。
- 不放宽安全门默认值（默认仍严格）。

## 5. 功能需求

### P0：正确性/可停/可信

#### FR-01 流式可取消（对应评审 2.1）

- 所有流式 IPC（`ai_generate_sql`、`ai_chat`）支持 `ai_cancel{requestId}`。
- 前端每个 AI 输入区在 streaming 时显示停止按钮，点击后：
  - 中断后端 `stream_complete`（`CancellationToken`）；
  - 保留已收到的 `streamContent/streamReasoning` 为草稿消息（标记 `cancelled`），不丢弃；
  - NL2SQL 保留已生成的 SQL 片段在预览区。
- `AiChatPanel` 补传 `onStop`（当前缺失）；`WorkflowChatPanel.handleStop` 改为走真正取消而非本地拼消息。
- `AiProvider::cancel(request_id)` 各 Provider 给出实现（至少中断 HTTP 流）。

验收：生成中点停止 ≤500ms 内停；草稿保留；后端无残留任务（日志可证）。

#### FR-02 NL2SQL 流式预览（对应评审 2.2）

- `Nl2SqlPanel` 增加 SQL 输出预览区（复用 `SqlCodeBlock` 只读高亮），流式增量渲染（容错版 `extractSql`，fence 未闭合也能显示）。
- 生成中右侧操作按钮为“停止”；完成后恢复“应用 / 应用并图表化 / 复制”。
- 保留现有“完成才写入编辑器”语义：流式只写预览区，`done` 后一次性 `onSqlChange`，避免编辑器抖动；提供开关“流式同步到编辑器”（默认关）。
- `aiStore` NL2SQL 增加 `streamingSql`（原始累积）与 `generatedSql`（清洗后终值）分离。

验收：10s 生成全程可见；停止后预览保留；完成后编辑器写入且与预览一致。

#### FR-03 会话按连接隔离（对应评审 2.3）

- `chatSessions: Record<sessionKey, ChatSession>`，`sessionKey = connectionId || dbSessionId + '::' + database`。
- NL2SQL 状态保留全局单例（与 QueryPanel 输入绑定）但增加 `tableKey` 记忆；`parsedFilters` 按 `dbSessionId::database::table` 存储；`explainAnalysis` 按 `hash(sql+plan)` 缓存，切 Tab 不丢。
- Chat 历史持久化到 localStorage（上限 50 轮/会话，可清），重进连接可恢复。
- 修复显示层两处 bug：
  1. `AiChatPanel.tsx:325` 思考提示条件对齐 Workflow（`!streamContent && !streamReasoning`）；
  2. `onAiStreamError / clearChat / clearWorkflowChat` 补清 `streamReasoning`。

验收：A/B 两个连接各聊 3 轮互不串；切 Query Tab 回来 NL2SQL/Explain 结果仍在。

#### FR-04 JSON 鲁棒解析 + 长任务反馈（对应评审 2.4）

- `parse_ai_json` 升级：扫描首个完整 JSON（平衡括号 + 字符串/转义感知），而非要求首字符即 `{`/`[`；`strip_markdown_fences` 处理多个 fence 取首个合法 JSON。
- 解析失败自动一次 repair 重问（system 追加“只返 JSON”，temperature 0），仍失败才报错，错误信息带 `scenario + finish_reason + 脱敏长度`。
- `diagnose/explain/parse_filter/schema_doc/connection_diagnose/analyze_queries` 改为流式进度事件（至少 `started/progress/done`），前端骨架屏 + 超时（60s）可退可重试。

验收：模型前导一句“好的…”仍能解析；长任务全程有进度；超时可取消。

#### FR-05 Tool Loop 护栏（对应评审 2.5）

- `max_rounds` 可配（默认 6，上限 10），新增总 Token 预算（默认 24k）与总时长（默认 180s），超限优雅收尾（已产出内容 + 截断说明）。
- tool 结果逐条限 2KB、每轮合计限 12KB，超限摘要化（保留 schema 结构、截断行样本并标注）。
- `Unknown tool` 不再回塞模型，直接终止该 tool 并报错。
- DB 只读 tool（`list_*`/`search`/`get_schema`）同轮可并行；MCP 写类 tool（由 `input_schema` 标注 `x-write: true` 或配置名单）执行前弹窗二次确认。
- MCP 工具 loading 文案显示当前真实执行 tool（修 `findMcpToolName` 取首个的 bug）。

验收：恶意/发散 tool 调用 6 轮内收敛；写工具必确认；大结果不撑爆 context。

#### FR-06 安全门统一（对应评审 2.6）

- 删除 `redact_for_egress(strict: bool)` 旧路，全量走 `redact_for_gate(gate)`；删除 `sanitize_json_with_gate` 恒真条件死代码。
- 敏感 key 判定：裸 `key` 改为 token 边界匹配 + 白名单（`keyboard/monkey/turnkey` 不命中）；结果键改前缀/正则（`result* / rows? / data / payload / sample*`）替代枚举穷举。
- 新增单测覆盖白名单与新旧行为一致性（严格默认输出不变）。

验收：旧严格单测全过；`monkey/keyboard` 不再误杀；死代码清零。

### P1：体验与准确率

#### FR-07 Markdown 渲染

- `AiMessageContent` 支持 GFM 子集（加粗/列表/表格/行内码），经 sanitize 后渲染，表格横滚；保留纯文本回退。
- 流式时增量渲染不闪烁（按 block  diff，代码块未闭合按纯文本占位）。

#### FR-08 代码块动作

- SQL 块：方言标签 + 行号 + 全屏 + `运行 / 插入到编辑器 / 新建查询页 / 复制`。
- 非 SQL 块加轻量高亮（yaml/json/python/sh），Workflow YAML 块保留现有 `复制/预览/保存` 并加 schema 校验提示。

#### FR-09 Chat 交互细节

- 自动跟随：仅用户在底部（距底 <120px）才 `scrollIntoView`，否则显示“回到底部” pill（未读数）。
- `ChatBubble key` 改消息 id；`role==='tool'` 独立浅色样式且可折叠；`ask_questions` 历史可回看（提交后折叠而非消失），answers 同时存 `id+label`。
- `QuestionBlock` 自由文本与选项互斥提示（选了选项再输文本视为补充说明并标注）。

#### FR-10 ContextPicker 性能与隔离

- 表/文件服务端搜索（`search_tables` + `context_list_files(query)`），输入防抖 200ms；列表虚拟化（>200 行）。
- `keydown` 监听改到 textarea 本地，不再 window capture 劫持；recent 按 `connectionId` 分桶（上限 8/桶）。
- `.ctx.yaml` 校验表存在性，缺失表 chips 标红并 tooltip 提示；支持 `schema.*` 前缀与 `*` 通配（解析为表名列表，上限 200）。

#### FR-11 Schema 上下文质量

- `format_compact_ddl` 补 `FK->表(列)` + 常用索引名 + 列注释（截断 40 字）+ 默认值（非敏感）。
- token 估算换 `chars×系数`（CJK 1.6/ASCII 0.3 近似）或 tokenizer 近似，替代 `len/4`。
- NL2SQL/Chat 组装前做表召回：问题关键词 × 表名/列名/注释本地打分取 Top30，再拼 DDL；fallback 扫描按“当前表 > 最近查询表 > 字母序”。
- `budget.rs` 动态化：`min(model_window×0.15, 硬上限)`，model_window 来自 Provider `ModelInfo`，未知模型回退静态值。
- 表名列表超 200 即截断（无论 tool 与否），超 500 强制 `search_tables` 引导（保留现有行为并前移阈值判断到召回前）。

#### FR-12 Prompt 方言小抄 + 异步加载

- `PromptResolver::load_templates` 全 async 化；`resolve` 取 `driver_type` 去掉 `serde_json::to_value` 绕路；`render_template` 后断言无残留 `{{}}`（残留即 warn + 替换空串）。
- 各驱动 `prompt_overrides()` 可追加方言小抄（引号/分页/布尔/日期函数/自增写法），`nl2sql.md` 模板新增 `{{dialect_notes}}` 占位。
- 首次 AI 调用前预热 `ensure_ready`（空闲时），避免首问卡顿。

#### FR-13 Egress 常驻提示 + 连接清单收敛

- `AiEgressNotice` 常驻一行：`发送 schema/表名到 <provider/model> · 行数据：不发送（严格）/采样（采样）/发送（放开）· 预估 X tokens`，点击展开详情（发送了哪些表、截断说明）。
- 普通 `ai_chat` 不再发送全量连接清单；仅 `workflow_generate` 需要时发送；`list_connections` tool 保留供模型按需调用。

### P2：架构/可观测/商业化

#### FR-14 Provider 能力矩阵

- `ai_get_providers` 返回实测 `supports_tools/supports_streaming/supports_reasoning/max_window`，前端按矩阵动态禁用 tool 开关与推理折叠；Ollama/DeepSeek/Custom 兼容矩阵文档化。

#### FR-15 超时重试

- 连接 8s、首 token 60s、chunk 60s；429 按 `Retry-After` 退避重试 1 次；400 透出脱敏 `code/message`；日志保留 size-only 原则。

#### FR-16 用量观测

- 气泡 footer 显示 `prompt/completion/total + 耗时`；设置页 AI 区增加本期估算（按模型单价表本地估算，声明非账单）。

#### FR-17 反馈闭环

- 每条 assistant 消息 `👍/👎 + 复制 + 重试 + 固定 SQL`；`👎` 记本地 eval 集（`scenario/model/脱敏prompt/输出hash/原因选项`），设置页可导出 JSONL。

#### FR-18 Workflow 生成复用

- `WorkflowChatPanel` 复用 Chat 的 `PromptSeed/SchemaContextPipeline`，去掉 `connectionId->dbSessionId` 脆弱映射（改走 `activeConnectionStore` 统一解析）；YAML 块加 dry-run 预览（字段校验 + 变量表）。

#### FR-19 组件复用

- 抽 `AiEmptyState / ChatBubble / useAutoScroll / EgressLine` 共享，消除 5 处 `aiNotConfigured` 空态与 2 套气泡重复；中英文连接清单文案走 i18n。

#### FR-20 MCP 工具治理

- MCP 工具在设置页标注 `读/写` 分类（schema 启发 + 手动覆盖）；AI 面板显示本次可用 MCP 工具数；写工具默认需确认（与 FR-05 联动）。

## 6. 验收标准（汇总）

| 编号 | 验收 |
|------|------|
| AC-01 | 任一流式任务可停止，500ms 内停，草稿保留 |
| AC-02 | NL2SQL 全程可见预览，停止/完成语义正确 |
| AC-03 | 双连接会话隔离；切 Tab 状态不丢；reasoning/content 无串无残留 |
| AC-04 | 前导解释性文本不导致 JSON 失败；长任务有进度可退 |
| AC-05 | Tool Loop 6 轮默认收敛；写工具必确认；大结果被摘要化 |
| AC-06 | 安全门单路径；旧严格单测全过；误杀消除 |
| AC-07 | Markdown/代码块动作可用；虚拟化列表不卡；表召回 Top30 准确率抽测 ≥80% |
| AC-08 | Egress 常驻提示准确；普通 chat 不发全量连接清单 |
| AC-09 | 用量/反馈/MCP 治理可见可用 |

## 7. 里程碑

- M1（P0）：FR-01~FR-06 + AC-01~AC-06。
- M2（P1）：FR-07~FR-13 + AC-07~AC-08。
- M3（P2）：FR-14~FR-20 + AC-09。
