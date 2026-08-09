import workflowGuideZh from '../../../docs/workflow-guide.md?raw';
import workflowGuideEn from '../../../docs/workflow-guide.en.md?raw';
import opsDashboardGuideZh from '../../../docs/ops-dashboard-guide.md?raw';
import opsDashboardGuideEn from '../../../docs/ops-dashboard-guide.en.md?raw';
import schemaDiffGuideZh from '../../../docs/schema-diff-guide.md?raw';
import schemaDiffGuideEn from '../../../docs/schema-diff-guide.en.md?raw';
import { renderWorkflowMarkdown } from './renderMarkdown';

export type DocsSectionId =
  | 'overview'
  | 'features'
  | 'ai'
  | 'context'
  | 'workflows'
  | 'opsDashboard'
  | 'schemaDiff';

export interface DocsSection {
  id: DocsSectionId;
  title: string;
  /** Trusted built-in HTML (no user input). */
  html: string;
}

const WORKFLOW_HTML_ZH = renderWorkflowMarkdown(workflowGuideZh);
const WORKFLOW_HTML_EN = renderWorkflowMarkdown(workflowGuideEn);
const OPS_DASHBOARD_HTML_ZH = renderWorkflowMarkdown(opsDashboardGuideZh);
const OPS_DASHBOARD_HTML_EN = renderWorkflowMarkdown(opsDashboardGuideEn);
const SCHEMA_DIFF_HTML_ZH = renderWorkflowMarkdown(schemaDiffGuideZh);
const SCHEMA_DIFF_HTML_EN = renderWorkflowMarkdown(schemaDiffGuideEn);

export const DOCS_SECTIONS_ZH: DocsSection[] = [
  {
    id: 'overview',
    title: '应用概况',
    html: `
<p><strong>DataZen</strong> 是一款跨平台桌面数据库管理工具，面向开发者与数据分析场景，在统一界面中连接多种数据库，并集成 AI 辅助与可编排的 Workflow 自动化。</p>
<h3>它能帮你做什么</h3>
<ul>
  <li>管理多数据源连接（PostgreSQL、MySQL、MariaDB、SQLite、Redis 及插件驱动等）</li>
  <li>浏览 Schema、编写并执行 SQL、查看结果与图表</li>
  <li>用自然语言生成 SQL、诊断错误、分析执行计划</li>
  <li>通过上下文文件把业务规则、表说明等注入 AI</li>
  <li>用 Workflow 把「查询 + AI + 条件/循环」串成可复用流程</li>
  <li>用<strong>运营看板</strong>多图盯盘：后台定时刷新、阈值告警、历史回看与托盘常驻</li>
  <li>数据同步、备份恢复、配置导入导出、MCP 对接外部 AI 客户端</li>
  <li>用 <strong>Schema Diff</strong> 以源库为目标态对比结构，生成并受控部署 DDL</li>
</ul>
<h3>典型使用路径</h3>
<ol>
  <li>首页新建连接并打开连接窗口</li>
  <li>在设置中配置 AI Provider（API Key、模型）</li>
  <li>在查询编辑器或 AI 侧边栏使用 NL2SQL / 对话</li>
  <li>需要时添加上下文文件，提升生成准确度</li>
  <li>把重复分析沉淀为 Workflow，一键执行</li>
  <li>把核心业务指标做成运营看板组件，开启监控与告警</li>
  <li>环境对齐时用 Schema Diff：对比 → 计划 → 审阅 → 部署</li>
</ol>
`,
  },
  {
    id: 'features',
    title: '功能特色',
    html: `
<h3>多数据库统一体验</h3>
<p>连接表单、Schema 树、SQL 方言与备份能力由数据库元数据驱动，减少硬编码分支；插件可在构建时按需引入。</p>
<h3>查询与可视化</h3>
<ul>
  <li>多语句执行、查询历史与收藏</li>
  <li>结果表与图表双视图（柱/线/饼/散点/面积），支持自然语言调整图表配置</li>
  <li>EXPLAIN 可视化与 AI 解读</li>
  <li><strong>表结构编辑器</strong>：新建/改表、列与索引同草稿、SQL 预览后保存；控件启用由驱动按版本自报能力（不支持项会禁用并提示）；执行按语句逐步应用，非单事务</li>
</ul>
<h3>AI 辅助</h3>
<ul>
  <li><strong>NL2SQL</strong>：自然语言生成 SQL，完成后写入编辑器</li>
  <li><strong>AI 对话</strong>：结合当前库 Schema 回答问题、改写 SQL</li>
  <li><strong>错误诊断 / 执行计划分析</strong></li>
  <li><strong>上下文文件</strong>：把本地文档注入提示词</li>
  <li><strong>Workflow</strong>：YAML 编排查询与 AI 步骤</li>
</ul>
<h3>运营看板与监控</h3>
<ul>
  <li>独立窗口多图网格，组件绑定 SQL + 图表配置</li>
  <li>后台 MonitorEngine 定时刷新；监控连接与 UI 会话隔离</li>
  <li>桌面通知 / Webhook 阈值告警；运行历史落盘可回看</li>
  <li>可选系统托盘：关主窗仍可继续监控（详见「运营看板」专章）</li>
</ul>
<h3>运维与生态</h3>
<ul>
  <li>跨库数据同步、备份与恢复</li>
  <li><strong>Schema Diff Deploy</strong>：源=期望态 → DDL 计划 → 审阅部署（详见专章）</li>
  <li>可作为 MCP Server 被 Claude Desktop / Cursor 等调用（推荐 <code>datazen --mcp</code>）</li>
  <li>也可作为 MCP Client 连接外部工具</li>
</ul>
`,
  },
  {
    id: 'ai',
    title: 'AI 功能使用',
    html: `
<h3>准备工作</h3>
<ol>
  <li>打开 <strong>设置 → AI</strong>，选择 Provider（OpenAI / DeepSeek / 自定义 OpenAI 兼容等）</li>
  <li>填写 API Key、模型，保存并验证</li>
  <li>（可选）在「AI 上下文目录」中指定存放参考文件的文件夹</li>
</ol>
<h3>NL2SQL（查询面板）</h3>
<ol>
  <li>打开连接窗口，进入查询标签页</li>
  <li>点击工具栏「AI 生成 SQL」展开输入条</li>
  <li>用自然语言描述需求，例如：「按销售渠道统计购物车数量」</li>
  <li>需要时用 <code>@</code> 引用上下文文件（见下一节）</li>
  <li>点击「生成 SQL」；生成<strong>完成后</strong> SQL 会写入下方 SQL 编辑器</li>
  <li>检查后点击「执行」</li>
</ol>
<h3>AI 对话侧边栏</h3>
<ol>
  <li>连接窗口工具栏点击消息图标，打开右侧 AI 面板</li>
  <li>在「对话」标签中提问；默认会附带当前库的 Schema 摘要</li>
  <li>可用 <code>@</code> 附加上下文文件；发送后会注入到本次用户消息中</li>
</ol>
<h3>其他 AI 能力</h3>
<ul>
  <li><strong>错误诊断</strong>：查询失败时可让 AI 解释错误并建议修复 SQL</li>
  <li><strong>EXPLAIN 分析</strong>：对执行计划做瓶颈解读与优化建议</li>
  <li><strong>自然语言筛选</strong>：在表数据视图中用自然语言生成过滤条件</li>
</ul>
<p>若未配置 AI，相关面板会提示前往设置；配置后无需重启即可使用。</p>
`,
  },
  {
    id: 'context',
    title: '上下文文件',
    html: `
<h3>有什么用？</h3>
<p>AI 默认只能看到当前连接的 Schema（表结构摘要），并不知道你们团队的业务约定、字段含义、禁止改的表、示例 SQL 等。
<strong>上下文文件</strong>就是放在本地目录里的说明文档，在提问时按需注入到提示词中，让模型生成更贴近业务的结果。</p>
<p>典型用途：</p>
<ul>
  <li>表/字段中文说明、枚举含义</li>
  <li>常用查询模板、命名规范</li>
  <li>业务规则（例如软删除字段、租户隔离）</li>
  <li>临时粘贴的需求说明、接口文档片段</li>
</ul>
<h3>支持的文件类型与限制</h3>
<ul>
  <li>扩展名：<code>txt</code>、<code>md</code>、<code>sql</code>、<code>json</code>、<code>yaml</code>/<code>yml</code>、<code>csv</code>/<code>tsv</code>、<code>xml</code>、<code>ddl</code>、<code>schema</code></li>
  <li>单文件大小上限约 512 KB</li>
  <li>不会读取隐藏文件；路径有防穿越校验</li>
</ul>
<h3>上下文目录在哪里？</h3>
<ol>
  <li>打开 <strong>设置 → AI</strong></li>
  <li>找到「AI 上下文目录」</li>
  <li>留空则使用应用数据目录下的默认文件夹 <code>contexts</code></li>
  <li>也可指定任意本地文件夹，并把参考文件放进去</li>
</ol>
<h3>如何添加文件</h3>
<ol>
  <li>在上述目录中新建或复制文件（可用「打开目录」在访达/资源管理器中操作）</li>
  <li>也可建子目录分类存放；选择目录时会递归读取其中允许的文件</li>
</ol>
<h3>如何注入（@ 提及）</h3>
<p>在以下输入框中均可使用：</p>
<ul>
  <li>查询面板的 NL2SQL 输入</li>
  <li>连接窗口 AI 对话输入</li>
  <li>Workflow 窗口的「AI 创建工作流」输入</li>
</ul>
<ol>
  <li>输入 <code>@</code>，会弹出文件选择器（可继续输入关键字过滤）</li>
  <li>选择文件或目录后，输入框中出现 <code>@文件名</code> 芯片</li>
  <li>可添加多个；芯片上的 × 可移除</li>
  <li>点击生成 / 发送时，应用会读取这些文件内容，并以类似如下形式<strong>拼进提示词</strong>：</li>
</ol>
<pre>[Context: relative/path.md]
（文件正文）

（你的自然语言问题或对话内容）</pre>
<p><strong>注意：</strong></p>
<ul>
  <li>注入的是发送那一刻读到的文件内容，改文件后需重新引用再发送</li>
  <li>芯片对应路径会在发送后清空（避免误重复注入），文件本身仍保留在目录中</li>
  <li>上下文文件与「Schema 注入」是两套机制：Schema 由连接自动提供，文件需你用 <code>@</code> 显式选择</li>
</ul>
`,
  },
  {
    id: 'workflows',
    title: 'Workflow 工作流',
    html: '', // filled from docs/workflow-guide.md in getDocsSections
  },
  {
    id: 'opsDashboard',
    title: '运营看板',
    html: '', // filled from docs/ops-dashboard-guide.md in getDocsSections
  },
  {
    id: 'schemaDiff',
    title: 'Schema Diff',
    html: '', // filled from docs/schema-diff-guide.md in getDocsSections
  },
];

export const DOCS_SECTIONS_EN: DocsSection[] = [
  {
    id: 'overview',
    title: 'Overview',
    html: `
<p><strong>DataZen</strong> is a cross-platform desktop database client for developers and analysts. It unifies multiple databases in one UI and adds AI assistance plus orchestrated Workflows.</p>
<h3>What you can do</h3>
<ul>
  <li>Manage connections (PostgreSQL, MySQL, MariaDB, SQLite, Redis, and optional plugin drivers)</li>
  <li>Browse schemas, write and run SQL, inspect results and charts</li>
  <li>Generate SQL from natural language, diagnose errors, analyze plans</li>
  <li>Inject business docs via context files</li>
  <li>Automate “query + AI + branching/loops” with Workflows</li>
  <li>Monitor metrics on an <strong>Ops Dashboard</strong> with background refresh, threshold alerts, history, and an optional system tray</li>
  <li>Sync data, backup/restore, import/export config, and expose MCP to external AI apps</li>
  <li>Align schemas with <strong>Schema Diff</strong> (source = desired → plan → review → deploy)</li>
</ul>
`,
  },
  {
    id: 'features',
    title: 'Highlights',
    html: `
<h3>Multi-database UX</h3>
<p>Forms, schema trees, SQL dialects, and backup options are driven by database metadata; plugins can be included at build time.</p>
<h3>Query &amp; charts</h3>
<ul>
  <li>Multi-statement execution, history, and favorites</li>
  <li>Table/chart views with NL chart tweaks</li>
  <li>EXPLAIN visualization and AI analysis</li>
  <li><strong>Table structure editor</strong>: create/alter tables with columns and indexes in one draft, preview SQL then save; controls follow driver-reported version caps (unsupported actions are disabled with a hint); execution applies statements step-by-step, not as one atomic transaction</li>
</ul>
<h3>AI</h3>
<ul>
  <li>NL2SQL, chat with schema context, diagnostics, plan analysis</li>
  <li>Context files and YAML Workflows</li>
</ul>
<h3>Ops Dashboard &amp; monitoring</h3>
<ul>
  <li>Multi-chart grid in a dedicated window; each widget binds SQL + chart config</li>
  <li>Background MonitorEngine with connection isolation from UI sessions</li>
  <li>Desktop / webhook threshold alerts and persisted run history</li>
  <li>Optional system tray so monitoring can continue after closing the main window (see the Ops Dashboard chapter)</li>
</ul>
<h3>Ops &amp; ecosystem</h3>
<ul>
  <li>Cross-database sync, backup/restore</li>
  <li><strong>Schema Diff Deploy</strong>: source = desired → DDL plan → gated deploy (see dedicated chapter)</li>
  <li>MCP Server for Claude Desktop / Cursor (<code>datazen --mcp</code>) and MCP Client</li>
</ul>
`,
  },
  {
    id: 'ai',
    title: 'Using AI',
    html: `
<h3>Setup</h3>
<ol>
  <li>Open <strong>Settings → AI</strong> and configure a provider (API key, model)</li>
  <li>Optionally set the AI context directory</li>
</ol>
<h3>NL2SQL</h3>
<ol>
  <li>In a connection window, open a query tab and enable “AI Generate SQL”</li>
  <li>Describe the query in natural language; use <code>@</code> for context files</li>
  <li>When generation <strong>finishes</strong>, SQL is written into the SQL editor</li>
  <li>Review and execute</li>
</ol>
<h3>AI chat sidebar</h3>
<p>Toggle the message icon in the connection toolbar. Chat includes schema context by default; <code>@</code> files are injected into the user message on send.</p>
`,
  },
  {
    id: 'context',
    title: 'Context files',
    html: `
<h3>Why</h3>
<p>Schema alone is not enough for business rules. Context files are local documents you attach with <code>@</code> so the model sees naming conventions, field meanings, and examples.</p>
<h3>Formats &amp; limits</h3>
<ul>
  <li>Extensions: txt, md, sql, json, yaml/yml, csv/tsv, xml, ddl, schema</li>
  <li>~512 KB per file; hidden files skipped; path traversal blocked</li>
</ul>
<h3>Directory</h3>
<p><strong>Settings → AI → AI Context Directory</strong>. Empty means the default <code>contexts</code> folder under the app data directory.</p>
<h3>How to inject</h3>
<ol>
  <li>Type <code>@</code> in NL2SQL, AI chat, or Workflow AI-create input</li>
  <li>Pick files/folders; chips appear in the input</li>
  <li>On send/generate, contents are prepended as <code>[Context: path]</code> blocks</li>
</ol>
`,
  },
  {
    id: 'workflows',
    title: 'Workflows',
    html: '', // filled from docs/workflow-guide.en.md in getDocsSections
  },
  {
    id: 'opsDashboard',
    title: 'Ops Dashboard',
    html: '', // filled from docs/ops-dashboard-guide.en.md in getDocsSections
  },
  {
    id: 'schemaDiff',
    title: 'Schema Diff',
    html: '', // filled from docs/schema-diff-guide.en.md in getDocsSections
  },
];

export function getDocsSections(lang: string): DocsSection[] {
  const sections = lang.startsWith('zh') ? DOCS_SECTIONS_ZH : DOCS_SECTIONS_EN;
  const zh = lang.startsWith('zh');
  return sections.map((s) => {
    if (s.id === 'workflows') {
      return { ...s, html: zh ? WORKFLOW_HTML_ZH : WORKFLOW_HTML_EN };
    }
    if (s.id === 'opsDashboard') {
      return { ...s, html: zh ? OPS_DASHBOARD_HTML_ZH : OPS_DASHBOARD_HTML_EN };
    }
    if (s.id === 'schemaDiff') {
      return { ...s, html: zh ? SCHEMA_DIFF_HTML_ZH : SCHEMA_DIFF_HTML_EN };
    }
    return s;
  });
}
