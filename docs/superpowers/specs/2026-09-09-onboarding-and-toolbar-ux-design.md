# DataZen 首次激活三步向导与查询主工具栏重排设计规范 (PRD P0)

- **日期**：2026-09-09
- **状态**：已批准 (Approved)
- **目标**：实现 PRD P0 核心阶段功能，让新用户在 5 分钟内无门槛完成「启动 → 打开示例 SQLite → 运行首条查询 → 获得图表与 AI 洞察」完整闭环，并对主查询工具栏进行信息层级重排与响应式收纳，大幅降低误点击率与认知负荷。

---

## 1. 背景与核心问题

根据 `docs/todo/datazen-ux-optimization-prd.md` 中的用户体验审计：
1. **首次激活门槛高**：欢迎页仅陈列静态功能介绍卡片，新用户必须自行配置外部数据库连接（涉及主机、端口、用户名、密码或本地文件路径），若无现成数据库则直接流失。
2. **工具栏信息密度过大**：当前 `QueryEditorSection` 工具栏平铺排列了 12 个以上按钮（执行、执行策略、EXPLAIN、格式化、代码片段、刷新补全、事务开启/提交/回滚、安全模式、快捷键、历史记录、收藏夹、AI 助手）。缺乏主次分层，新用户难以快速定位核心操作；中窄屏下容易产生横向排版挤压。
3. **关键差异化能力难以被发现**：查询出数据后，结果区的「生成图表」与「AI 总结」缺少显式的任务流引导，导致大部分用户仅停留于最基础的 SQL 查询，未体会到 DataZen 区别于传统客户端的核心优势。

---

## 2. 方案目标与非目标

### 2.1 目标 (PRD P0 北极星)
- **一键示例体验**：欢迎页提供「打开示例 SQLite (快速体验)」主按钮，2 次点击内完成建库并进入工作台，无需网络与任何凭据。
- **全流程三步向导闭环**：
  - 步骤 1：打开示例库（或创建自有连接）。
  - 步骤 2：自动预置 `getting_started.sql` 聚合查询，引导运行首条查询。
  - 步骤 3：数据查出后，引导体验「生成图表」或「AI 解释/总结」，达成闭环。
- **全时可跳过与恢复**：用户随时可一键「跳过向导」，状态持久化记忆；未完成时重启应用可无缝恢复。
- **查询工具栏三级架构**：高频核心区（运行/保存/AI）常驻，高级辅助操作（格式化、EXPLAIN、代码片段、补全刷新、事务操作）收纳进 `[更多操作 ▾]` 溢出菜单。
- **工作区侧边栏双模支持**：左侧一级导航模式栏支持「40px 图标栏」与「120px 图标+文字展开栏」切换并持久化。

### 2.2 非目标
- 本期不涉及 P1 的驱动可用性分层与异构数据库大结果集性能重构。
- 本期不采集任何用户 SQL、数据及隐私信息。

---

## 3. 系统架构与模块职责划分

严格遵循架构规范与单文件规模控制（单文件 < 800 行），将逻辑解耦为高内聚、低耦合的模块：

```text
src/
├── stores/
│   └── onboardingStore.ts                 # 首次激活向导状态机（Zustand + 本地持久化）
├── commands/
│   └── sampleData.ts                      # 示例数据库 IPC 封装与种子数据生成器
├── windows/
│   ├── welcome/
│   │   ├── WelcomePage.tsx                # 重构：三步引导横幅 + 示例库主按钮 + 跳过入口
│   │   └── __tests__/WelcomePage.test.tsx # 欢迎页单测与跳过验证
│   └── connection/
│       ├── OnboardingGuideBar.tsx         # 工作区顶部 38px 非模态向导横条
│       ├── ConnectionPage.tsx             # 挂载引导条、左侧导航双模切换、示例 Tab 自动载入
│       ├── queryToolbarWidth.ts           # 升级断点算法与溢出宽度计算
│       └── query/
│           ├── QueryEditorSection.tsx     # 重排工具栏信息层级（核心区 / 溢出区 / 状态区）
│           └── QueryToolbarMoreMenu.tsx   # 更多操作溢出下拉菜单组件（含快捷键提示）
src-tauri/
└── src/
    └── commands/
        └── sample_db.rs                   # Rust 后端创建/确保持久化 sample_ecommerce.sqlite
```

---

## 4. 详细设计规范

### 4.1 首次激活状态机 (`onboardingStore.ts`)

采用状态机思维设计生命周期模型，具备明确的进入条件、状态内行为与退出条件：

```ts
export type OnboardingStatus = 'not_started' | 'active' | 'completed' | 'skipped';

export interface OnboardingState {
  status: OnboardingStatus;
  step: 1 | 2 | 3;
  sampleConnectionId: string | null;
  queryExecuted: boolean;
  aiOrChartExplored: boolean;

  // 动作
  startOnboarding: () => void;
  advanceToStep: (step: 2 | 3) => void;
  markQueryExecuted: () => void;
  markAiOrChartExplored: () => void;
  completeOnboarding: () => void;
  skipOnboarding: () => void;
  resetOnboarding: () => void;
}
```

- **持久化 Key**：`datazen:onboarding-state-v1`。
- **状态流转规则**：
  1. 新用户打开应用，无已有连接时，`status === 'not_started'`。
  2. 点击「打开示例 SQLite」或「新建连接」，`status -> 'active'`，`step: 1`。
  3. 进入连接工作台后，检测到属于首次激活流程，自动打开 `getting_started.sql`，跃迁至 `step: 2`。
  4. 首次执行查询成功（行数 > 0）后，自动跃迁至 `step: 3`，结果栏高亮图表与 AI 操作。
  5. 用户点击「生成图表」或「AI 助手」后，标记 `aiOrChartExplored: true`；点击「完成向导」后 `status -> 'completed'`，引导条平滑退出。
  6. **跳过机制**：在欢迎页或工作台引导条的任意时刻，点击 `[跳过向导]` 或 `[✕]`，即刻将 `status` 设为 `'skipped'`，销毁引导条且后续不再自动打扰。

---

### 4.2 离线 SQLite 示例库机制 (`sample_ecommerce.sqlite`)

- **数据目录**：安全放置于 `{appData}/sample_ecommerce.sqlite`。
- **内置 Schema 与种子数据**：
  - `customers`：客户表（id, name, email, country, signup_date），预置 10 条真实国际化客户数据。
  - `products`：商品表（id, name, category, price, stock），预置 Electronics, Home & Office, Apparel 3 大类共 12 种商品。
  - `orders`：订单表（id, customer_id, product_id, quantity, total_amount, status, order_date），预置 35 条已完成/发货状态的消费流水。
- **预置初始查询 SQL**：
  ```sql
  -- 💡 新手任务 2/3: 点击上方 [▶ 运行] (或 ⌘+Enter) 查看各品类销售统计
  SELECT 
    p.category AS category,
    count(o.id) AS orders_count,
    round(sum(o.total_amount), 2) AS total_sales
  FROM orders o
  JOIN products p ON o.product_id = p.id
  GROUP BY p.category
  ORDER BY total_sales DESC;
  ```
  该语句具备多表关联（`JOIN`）、聚合统计（`count/sum/round`）、分组与降序排列，直观体现 SQL 的强大，同时查出即天然是一张柱状图/饼图所需的数据结构。

---

### 4.3 工作台顶部非模态引导条 (`OnboardingGuideBar.tsx`)

- **外观与尺寸**：高度 38px，停靠在工作区主区域顶部（导航树与内容视图之上），采用微带 Accent 柔和色调的底色（`bg-accent/10 border-b border-accent/20`）。
- **内容构成**：
  - `Pill` 步骤徽标：`向导 2/3` 或 `向导 3/3`。
  - 动态提示文本：
    - 步骤 2：`第 2 步：运行首条查询 — 已预置销售额统计语句，点击下方 [▶ 运行 (⌘+Enter)] 查出数据`。
    - 步骤 3：`第 3 步：数据洞察 — 查询已成功！点击结果栏右上角 [📊 生成图表] 或工具栏 [✨ AI 助手]`。
  - 右侧行动区：
    - 步骤 2 提供 `[一键运行并推进 →]` 主按钮；步骤 3 提供 `[完成向导 ✓]`。
    - 常驻 `[跳过向导]` 幽灵按钮与 `[✕]` 图标按钮。

---

### 4.4 查询主工具栏重排 (`QueryEditorSection.tsx` & `QueryToolbarMoreMenu.tsx`)

将原混乱平铺的 12+ 按钮重构为清晰的 4 个区块：

```text
[ 上下文选择器 (库/模式) ] | [ ▶ 运行 ▾ ] [ 💾 保存 ] [ ✨ AI 助手 ] | [ 更多操作 ▾ ] | (弹性空白) | [ TX ] [ Safe Mode ] [ 12ms · 10 行 ]
```

1. **核心操作区（全视口常驻）**：
   - 上下文选择器：多库/Schema 路径切换。
   - `[▶ 运行]`：主强调色高亮按钮，右侧带策略下拉小三角（运行选中/运行全文），Tooltip 显式标注 `⌘+Enter`。
   - `[💾 保存]`：一键保存为常用/收藏查询。
   - `[✨ AI 助手]`：直接展开/折叠智能 NL2SQL 辅助面板。
2. **更多操作收纳区 (`QueryToolbarMoreMenu.tsx`)**：
   - 以 `[更多操作 ▾]`（带设置小滑块或更多图标）统一承载：
     - `格式化 SQL`（右侧标明 `⇧⌥F`）
     - `EXPLAIN 执行计划`
     - `代码片段 (Snippets)...`
     - `刷新补全缓存`
     - `开始事务 (BEGIN)` / `提交 (COMMIT)` / `回滚 (ROLLBACK)`
   - **事务保护自适应浮现**：当底层检测到 `inTransaction === true` 时，提交与回滚按钮自动从更多菜单中提取并**强行并排展示在工具栏**上，附带高亮 `TX` 徽章，绝对避免用户因菜单折叠而遗忘提交或回滚事务。
3. **状态与审计区（固定右侧）**：
   - `Safe Mode` 保护标签（只读/破坏性拦截门禁）。
   - 耗时与结果统计：`12 ms · 10 行`。

---

### 4.5 工作区侧边栏双模支持 (`ConnectionPage.tsx`)

- **功能**：左侧一级的 `WorkspaceModeButton` 纵向栏（连接、工作流、仪表盘、工作区、扩展、设置）支持展开/折叠。
- **状态存储**：在 `uiStore` 中增加 `sidebarMode: 'icons' | 'expanded'`，默认 `'icons'`（宽 40px），展开时宽 120px，图标右侧显示文本标签。
- **切换入口**：在左侧栏底端设置图标上方提供一个 `[«]` / `[»]` 紧凑折叠按钮，一键切换并记忆。

---

## 5. 验收标准与测试规范

### 5.1 自动化测试矩阵

| 测试文件 | 验证重点 |
|---|---|
| `src/stores/__tests__/onboardingStore.test.ts` | 验证状态初始化、步骤跃迁（1→2→3→completed）、跳过持久化、重启恢复、重置逻辑 |
| `src/windows/welcome/__tests__/WelcomePage.test.tsx` | 验证欢迎页引导横幅、一键示例按钮点击、直接跳过按钮点击、状态同步 |
| `src/windows/connection/__tests__/OnboardingGuideBar.test.tsx` | 验证引导条渲染、步骤展示、一键执行事件派发、跳过及关闭回调 |
| `src/windows/connection/__tests__/QueryToolbarMoreMenu.test.tsx` | 验证更多菜单项渲染、快捷键标注、点击格式化/EXPLAIN回调、事务状态展开 |
| `src/windows/connection/__tests__/queryToolbarWidth.test.ts` | 验证多级断点计算、极窄宽度下的收缩表现、事务激活时宽度的自适应调整 |
| `src/windows/connection/__tests__/onboardingJourney.test.tsx` | **连续旅程测试**：模拟从欢迎页点击示例库 → 载入 SQL → 模拟执行成功 → 步骤跃迁到3 → 触发图表完成向导的全流程状态机校验 |

### 5.2 验收指标对齐
- 新用户启动欢迎页后，点击「打开示例 SQLite」在 ≤ 500ms 内完成库创建并进入工作台。
- 工作台立即呈现预置 SQL，按 `⌘+Enter` 或点击「运行」在 ≤ 50ms 内展示 10 条聚合分析结果。
- 结果呈现后，引导条自动跃迁到步骤 3，点击「生成图表」可打开图表抽屉看到可视化。
- 用户在任意环节点击「跳过向导」，向导条立即收起且刷新/重开应用后不再出现。
- 工具栏在 800px、1024px、1440px 视窗下排版自然，无任何文字截断或图标溢出。

---

## 6. Spec 自检核查 (Self-Review)

1. **占位符扫描**：无任何未决项（TBD/TODO），数据表结构、SQL 语句、状态流转代码均已给出具体定义。
2. **内部一致性**：状态机模型、欢迎页重构、顶部引导条与工具栏重排的职责划分完全符合 PRD P0 要求与 React/Tauri 架构。
3. **范围控制**：严格聚焦于 P0 核心的首次激活体验与工作台信息层级重排，不越界引入 P1 驱动分层或大结果集虚拟化。
4. **歧义消除**：明确了跳过向导为持久化退出；明确了事务模式下提交/回滚按钮强制浮现的规则。
