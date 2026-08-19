# DataZen 图表可视化 — 技术设计方案

> 本文档是图表可视化功能的详细技术设计，覆盖组件架构、数据流、类型定义、交互设计和实现计划。

## 一、设计原则

1. **SQL-first** — 图表是查询结果的另一种展示方式，不替代表格
2. **零配置起步** — 智能推荐图表类型和轴配置，用户也可以手动调整
3. **数据不二次加载** — 图表直接消费 `StatementResult`，不触发新的查询
4. **渐进增强** — 先做静态图表，再加交互和 AI

---

## 二、整体架构

### 2.1 在现有 UI 中的位置

```
QueryPanel
├── SQL 编辑器（CodeMirror）
├── Result Tab Bar（Result 1 | Result 2 | EXPLAIN）
│   └── 每个 Result Tab 内部：
│       ├── 视图切换栏 [📊 表格] [📈 图表]  ← 新增
│       ├── 表格视图 — ResultTable（现有）
│       └── 图表视图 — ChartView（新增）
└── 底部状态栏
```

**交互方式**：在每个 Result Tab 内部，通过 SegmentedControl 切换「表格 / 图表」视图。

### 2.2 组件层次

```
ChartView                          — 图表视图容器
├── ChartToolbar                   — 工具栏（图表类型、导出等）
│   ├── ChartTypeSelector          — 图表类型切换（柱/线/饼/散点/面积）
│   └── ChartExportButton          — 导出 PNG / SVG / CSV
├── AxisConfigurator               — 轴/字段配置面板
│   ├── XAxisField                 — X 轴字段选择
│   ├── YAxisField(s)              — Y 轴字段选择（支持多个）
│   ├── GroupByField               — 分组字段（可选）
│   └── AggregationSelector        — 聚合方式（SUM/AVG/COUNT/...）
├── ChartCanvas                    — 图表渲染区域（Recharts）
│   ├── BarChartRenderer
│   ├── LineChartRenderer
│   ├── PieChartRenderer
│   ├── ScatterChartRenderer
│   └── AreaChartRenderer
└── ChartEmptyState                — 数据不适合图表时的提示
```

---

## 三、数据模型

### 3.1 核心类型定义

```typescript
// src/types/chart.ts

/** 支持的图表类型 */
export type ChartType = 'bar' | 'line' | 'pie' | 'scatter' | 'area';

/** 聚合方式 */
export type AggregationType = 'none' | 'sum' | 'avg' | 'count' | 'min' | 'max' | 'distinct_count';

/** 字段的推断类型（用于智能推荐） */
export type InferredFieldType = 'numeric' | 'datetime' | 'categorical' | 'boolean' | 'unknown';

/** 字段信息（比 ColumnInfo 多了推断类型） */
export interface ChartField {
  name: string;
  dataType: string;           // 原始数据库类型（INT4, TEXT 等）
  inferredType: InferredFieldType;
  distinctCount?: number;     // 不同值数量（前 N 行采样）
  sampleValues?: unknown[];   // 前几个值（用于预览）
}

/** 图表配置 */
export interface ChartConfig {
  chartType: ChartType;
  xAxis: string | null;           // 字段名
  yAxes: string[];                // 字段名列表（支持多系列）
  groupBy: string | null;         // 分组字段名
  aggregation: AggregationType;   // 对 Y 轴的聚合方式
  sortBy: 'x_asc' | 'x_desc' | 'y_asc' | 'y_desc' | 'none';
  showLegend: boolean;
  showGrid: boolean;
  showValues: boolean;            // 是否在图表上显示数值标签
  colorScheme: string;            // 配色方案名称
}

/** 图表推荐结果 */
export interface ChartRecommendation {
  chartType: ChartType;
  xAxis: string;
  yAxes: string[];
  groupBy: string | null;
  aggregation: AggregationType;
  confidence: number;             // 0-1，推荐信心度
  reason: string;                 // 推荐原因（i18n key）
}

/** 处理后的图表数据点 */
export interface ChartDataPoint {
  [key: string]: string | number | null;  // Recharts 要求的扁平对象
}
```

### 3.2 数据转换管道

```
StatementResult            ChartField[]           ChartConfig           ChartDataPoint[]
┌───────────┐  inferFields  ┌──────────┐  recommend  ┌──────────┐  transform  ┌──────────┐
│ columns   │──────────────→│ fields   │────────────→│ config   │────────────→│ points   │
│ rows      │               │ types    │             │ axes     │             │ [{x,y}]  │
│ dataType  │               │ samples  │             │ chartType│             │          │
└───────────┘               └──────────┘             └──────────┘             └──────────┘
         ↑                                                ↑
    查询结果                                        用户可手动调整
```

---

## 四、核心模块设计

### 4.1 字段类型推断（`inferFieldType`）

```typescript
// src/lib/chart/fieldInference.ts

export function inferFieldType(column: ColumnInfo, sampleValues: unknown[]): InferredFieldType {
  const dt = column.dataType.toLowerCase();

  // 1. 从数据库类型名推断
  if (/bool/.test(dt)) return 'boolean';
  if (/int|serial|double|numeric|decimal|real|float|money/.test(dt)) return 'numeric';
  if (/timestamp|date|time/.test(dt)) return 'datetime';

  // 2. 如果数据库类型无法判断（如 TEXT），从实际值推断
  const nonNull = sampleValues.filter((v) => v != null);
  if (nonNull.length === 0) return 'unknown';

  if (nonNull.every((v) => typeof v === 'number')) return 'numeric';
  if (nonNull.every((v) => typeof v === 'boolean')) return 'boolean';
  if (nonNull.every((v) => typeof v === 'string' && isDateLike(v))) return 'datetime';

  return 'categorical';
}

function isDateLike(s: string): boolean {
  return /^\d{4}-\d{2}-\d{2}/.test(s) || !isNaN(Date.parse(s));
}

export function inferAllFields(result: StatementResult): ChartField[] {
  const sampleSize = Math.min(result.rows.length, 50);
  return result.columns.map((col, colIdx) => {
    const samples = result.rows.slice(0, sampleSize).map((row) => row[colIdx]);
    const distinctValues = new Set(samples.filter((v) => v != null));
    return {
      name: col.name,
      dataType: col.dataType,
      inferredType: inferFieldType(col, samples),
      distinctCount: distinctValues.size,
      sampleValues: Array.from(distinctValues).slice(0, 5),
    };
  });
}
```

### 4.2 图表智能推荐（`recommendChart`）

```typescript
// src/lib/chart/recommend.ts

export function recommendChart(fields: ChartField[], rowCount: number): ChartRecommendation | null {
  const numerics = fields.filter((f) => f.inferredType === 'numeric');
  const datetimes = fields.filter((f) => f.inferredType === 'datetime');
  const categoricals = fields.filter((f) => f.inferredType === 'categorical');

  // 规则 1：1 个时间列 + 1+ 个数值列 → 折线图
  if (datetimes.length >= 1 && numerics.length >= 1) {
    return {
      chartType: 'line',
      xAxis: datetimes[0].name,
      yAxes: numerics.slice(0, 3).map((f) => f.name),
      groupBy: null,
      aggregation: 'none',
      confidence: 0.9,
      reason: 'chart.recommend.timeSeries',
    };
  }

  // 规则 2：1 个分类列（少量值）+ 1 个数值列 → 柱状图或饼图
  if (categoricals.length >= 1 && numerics.length >= 1) {
    const cat = categoricals[0];
    const distinctRatio = (cat.distinctCount ?? rowCount) / rowCount;

    if ((cat.distinctCount ?? 0) <= 8 && rowCount <= 20) {
      return {
        chartType: 'pie',
        xAxis: cat.name,
        yAxes: [numerics[0].name],
        groupBy: null,
        aggregation: 'none',
        confidence: 0.8,
        reason: 'chart.recommend.distribution',
      };
    }

    return {
      chartType: 'bar',
      xAxis: cat.name,
      yAxes: numerics.slice(0, 3).map((f) => f.name),
      groupBy: categoricals.length > 1 ? categoricals[1].name : null,
      aggregation: distinctRatio < 0.5 ? 'sum' : 'none',
      confidence: 0.85,
      reason: 'chart.recommend.comparison',
    };
  }

  // 规则 3：2+ 个数值列 → 散点图
  if (numerics.length >= 2 && categoricals.length === 0 && datetimes.length === 0) {
    return {
      chartType: 'scatter',
      xAxis: numerics[0].name,
      yAxes: [numerics[1].name],
      groupBy: null,
      aggregation: 'none',
      confidence: 0.7,
      reason: 'chart.recommend.correlation',
    };
  }

  // 规则 4：仅 1 个数值列 → 面积图（按行序）
  if (numerics.length === 1 && rowCount > 5) {
    return {
      chartType: 'area',
      xAxis: null,
      yAxes: [numerics[0].name],
      groupBy: null,
      aggregation: 'none',
      confidence: 0.5,
      reason: 'chart.recommend.trend',
    };
  }

  return null;
}
```

### 4.3 数据转换器（`transformData`）

将 `StatementResult` + `ChartConfig` 转换为 Recharts 需要的 `ChartDataPoint[]`：

```typescript
// src/lib/chart/transform.ts

export function transformData(
  result: StatementResult,
  config: ChartConfig,
): ChartDataPoint[] {
  const records = result.rows.map((row) =>
    Object.fromEntries(result.columns.map((col, i) => [col.name, row[i]])),
  );

  if (config.aggregation === 'none') {
    return transformDirect(records, config);
  }
  return transformAggregated(records, config);
}

function transformDirect(
  records: Record<string, unknown>[],
  config: ChartConfig,
): ChartDataPoint[] {
  return records.map((rec, idx) => {
    const point: ChartDataPoint = {};
    if (config.xAxis) {
      point[config.xAxis] = formatAxisValue(rec[config.xAxis]);
    } else {
      point['__index'] = idx;
    }
    for (const y of config.yAxes) {
      point[y] = toNumber(rec[y]);
    }
    if (config.groupBy) {
      point[config.groupBy] = String(rec[config.groupBy] ?? '');
    }
    return point;
  });
}

function transformAggregated(
  records: Record<string, unknown>[],
  config: ChartConfig,
): ChartDataPoint[] {
  if (!config.xAxis) return [];

  const groups = new Map<string, Record<string, unknown>[]>();
  for (const rec of records) {
    const key = String(rec[config.xAxis] ?? '__null__');
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key)!.push(rec);
  }

  return Array.from(groups.entries()).map(([key, rows]) => {
    const point: ChartDataPoint = { [config.xAxis!]: key };
    for (const y of config.yAxes) {
      point[y] = aggregate(rows.map((r) => r[y]), config.aggregation);
    }
    return point;
  });
}

function aggregate(values: unknown[], type: AggregationType): number | null {
  const nums = values.map(toNumber).filter((n): n is number => n !== null);
  if (nums.length === 0) return null;
  switch (type) {
    case 'sum': return nums.reduce((a, b) => a + b, 0);
    case 'avg': return nums.reduce((a, b) => a + b, 0) / nums.length;
    case 'count': return values.length;
    case 'min': return Math.min(...nums);
    case 'max': return Math.max(...nums);
    case 'distinct_count': return new Set(values).size;
    default: return null;
  }
}

function toNumber(v: unknown): number | null {
  if (typeof v === 'number') return v;
  if (typeof v === 'string') { const n = Number(v); return isNaN(n) ? null : n; }
  return null;
}

function formatAxisValue(v: unknown): string | number {
  if (typeof v === 'number') return v;
  return String(v ?? '');
}
```

### 4.4 排序辅助

```typescript
function sortData(data: ChartDataPoint[], config: ChartConfig): ChartDataPoint[] {
  if (config.sortBy === 'none' || !config.xAxis) return data;
  const xKey = config.xAxis;
  const yKey = config.yAxes[0];
  return [...data].sort((a, b) => {
    switch (config.sortBy) {
      case 'x_asc': return String(a[xKey] ?? '').localeCompare(String(b[xKey] ?? ''));
      case 'x_desc': return String(b[xKey] ?? '').localeCompare(String(a[xKey] ?? ''));
      case 'y_asc': return (toNumber(a[yKey]) ?? 0) - (toNumber(b[yKey]) ?? 0);
      case 'y_desc': return (toNumber(b[yKey]) ?? 0) - (toNumber(a[yKey]) ?? 0);
      default: return 0;
    }
  });
}
```

---

## 五、组件设计

### 5.1 ChartView — 主容器

```typescript
// src/components/chart/ChartView.tsx

interface ChartViewProps {
  result: StatementResult;
}

function ChartView({ result }: ChartViewProps) {
  const fields = useMemo(() => inferAllFields(result), [result]);
  const recommendation = useMemo(() => recommendChart(fields, result.rows.length), [fields]);

  const [config, setConfig] = useState<ChartConfig>(() =>
    recommendation
      ? recommendationToConfig(recommendation)
      : defaultConfig(fields),
  );

  const data = useMemo(() => transformData(result, config), [result, config]);

  if (fields.filter((f) => f.inferredType === 'numeric').length === 0) {
    return <ChartEmptyState reason="noNumericField" />;
  }

  return (
    <div className="flex flex-col h-full">
      <ChartToolbar config={config} onChange={setConfig} onExport={handleExport} />
      <div className="flex flex-1 min-h-0">
        <AxisConfigurator
          fields={fields}
          config={config}
          onChange={setConfig}
          recommendation={recommendation}
        />
        <ChartCanvas
          data={data}
          config={config}
          fields={fields}
        />
      </div>
    </div>
  );
}
```

### 5.2 ChartToolbar — 工具栏

```
┌──────────────────────────────────────────────────────────────┐
│ [柱状图] [折线图] [饼图] [散点图] [面积图]  │  ☑ 图例  ☑ 网格  │  [📥 导出▾] │
└──────────────────────────────────────────────────────────────┘
```

- 图表类型切换使用 SegmentedControl + 图标
- 导出下拉菜单：PNG / SVG / 复制为图片
- 全屏按钮（可选）

### 5.3 AxisConfigurator — 轴配置面板

```
┌─────────────────────────┐
│ 📊 图表配置             │
├─────────────────────────┤
│ X 轴:  [department ▾]   │ ← Select，列出所有字段，高亮推荐
│ Y 轴:  [count     ▾]   │ ← Select，仅列 numeric/datetime
│        [+ 添加系列]     │ ← 多 Y 轴支持
│ 分组:  [无         ▾]   │ ← 可选，列出 categorical 字段
│ 聚合:  [求和       ▾]   │ ← 仅当需要时显示
│ 排序:  [X轴升序    ▾]   │
├─────────────────────────┤
│ 💡 推荐: 柱状图          │ ← 展示 AI/规则 推荐理由
│ "按部门分类的数值比较"   │
└─────────────────────────┘
```

宽度约 220px，位于图表区域左侧。可折叠。

### 5.4 ChartCanvas — 图表渲染

```typescript
// src/components/chart/ChartCanvas.tsx

function ChartCanvas({ data, config, fields }: ChartCanvasProps) {
  const chartRef = useRef<HTMLDivElement>(null);

  const ChartRenderer = {
    bar: BarChartRenderer,
    line: LineChartRenderer,
    pie: PieChartRenderer,
    scatter: ScatterChartRenderer,
    area: AreaChartRenderer,
  }[config.chartType];

  return (
    <div ref={chartRef} className="flex-1 min-h-0 p-4">
      <ResponsiveContainer width="100%" height="100%">
        <ChartRenderer data={data} config={config} fields={fields} />
      </ResponsiveContainer>
    </div>
  );
}
```

### 5.5 各图表渲染器

以 `BarChartRenderer` 为例：

```typescript
function BarChartRenderer({ data, config }: RendererProps) {
  const colors = getColorPalette(config.colorScheme);
  return (
    <BarChart data={data}>
      <CartesianGrid strokeDasharray="3 3" opacity={config.showGrid ? 0.3 : 0} />
      <XAxis dataKey={config.xAxis ?? '__index'} tick={{ fontSize: 12 }} />
      <YAxis tick={{ fontSize: 12 }} />
      <Tooltip
        contentStyle={{ background: 'var(--bg-surface)', border: '1px solid var(--border)' }}
      />
      {config.showLegend && <Legend />}
      {config.yAxes.map((yKey, i) => (
        <Bar key={yKey} dataKey={yKey} fill={colors[i % colors.length]}>
          {config.showValues && <LabelList dataKey={yKey} position="top" fontSize={11} />}
        </Bar>
      ))}
    </BarChart>
  );
}
```

---

## 六、配色方案

### 6.1 预定义调色板

```typescript
// src/lib/chart/colors.ts

export const COLOR_PALETTES: Record<string, string[]> = {
  default: ['#6366f1', '#f59e0b', '#10b981', '#ef4444', '#8b5cf6', '#06b6d4', '#f97316', '#84cc16'],
  ocean:   ['#0ea5e9', '#0284c7', '#0369a1', '#075985', '#0c4a6e', '#164e63', '#155e75', '#0e7490'],
  forest:  ['#22c55e', '#16a34a', '#15803d', '#166534', '#14532d', '#365314', '#3f6212', '#4d7c0f'],
  warm:    ['#f97316', '#ef4444', '#f59e0b', '#ec4899', '#e11d48', '#be123c', '#9f1239', '#881337'],
  mono:    ['#6b7280', '#4b5563', '#374151', '#1f2937', '#9ca3af', '#d1d5db', '#e5e7eb', '#f3f4f6'],
};

export function getColorPalette(name: string): string[] {
  return COLOR_PALETTES[name] ?? COLOR_PALETTES.default;
}
```

### 6.2 暗色主题适配

DataZen 默认暗色主题，Recharts 组件通过 CSS 变量适配：

- 坐标轴文字：`var(--text-secondary)`
- 网格线：`var(--border)` + 低透明度
- Tooltip 背景：`var(--bg-surface)`
- 图例文字：`var(--text-primary)`

---

## 七、导出功能

### 7.1 图表导出

```typescript
// src/lib/chart/export.ts

export async function exportChart(
  chartElement: HTMLElement,
  format: 'png' | 'svg',
  filename: string,
): Promise<void> {
  if (format === 'svg') {
    const svgEl = chartElement.querySelector('svg');
    if (!svgEl) return;
    const svgData = new XMLSerializer().serializeToString(svgEl);
    await saveFile(svgData, `${filename}.svg`, 'image/svg+xml');
  } else {
    // 使用 html-to-image 库转 PNG
    const { toPng } = await import('html-to-image');
    const dataUrl = await toPng(chartElement, { backgroundColor: '#1a1a2e' });
    await saveFromDataUrl(dataUrl, `${filename}.png`);
  }
}
```

### 7.2 导出依赖

新增依赖：`html-to-image`（~8KB gzip，无额外依赖）。

---

## 八、状态管理

### 8.1 图表配置存储

图表配置不需要持久化到数据库，存在 panelStore 的 QueryExecState 中即可：

```typescript
// 扩展 QueryTab
export interface QueryTab {
  // ... 现有字段 ...
  chartConfig?: ChartConfig;       // 图表配置（用户调整后的）
  resultViewMode?: 'table' | 'chart';  // 当前显示模式
}
```

### 8.2 查询级别记忆

同一查询再次执行时，如果列结构相同，保留之前的图表配置。判断方式：

```typescript
function isSameSchema(prev: ColumnInfo[], next: ColumnInfo[]): boolean {
  if (prev.length !== next.length) return false;
  return prev.every((c, i) => c.name === next[i].name && c.dataType === next[i].dataType);
}
```

---

## 九、交互设计

### 9.1 视图切换

在 `ResultTable` 上方添加 SegmentedControl：

```
[📋 表格] [📈 图表]
```

切换行为：
- 从表格切到图表：首次自动运行推荐算法，后续保留配置
- 从图表切到表格：不丢失图表配置
- 快捷键：`Ctrl+Shift+G`（Toggle Graph）

### 9.2 Tooltip 交互

鼠标悬停数据点时显示：
- 字段名 + 值
- 百分比（饼图）
- 自定义格式化（数值千分位、日期格式等）

### 9.3 未来交互扩展（P2+）

- 点击图表数据点 → 在表格中高亮对应行
- 框选图表区域 → 筛选表格数据
- 拖拽字段到轴区域 → 调整配置

---

## 十、技术选型确认

### 10.1 依赖

| 库 | 版本 | 大小（gzip） | 用途 |
|----|------|-------------|------|
| `recharts` | ^2.x | ~100KB | 图表渲染 |
| `html-to-image` | ^1.x | ~8KB | 图表导出为 PNG |

### 10.2 不引入的库

| 库 | 原因 |
|----|------|
| ECharts | 包体积过大（~400KB），配置式 API 与 React 风格不一致 |
| D3 | 学习曲线陡峭，P3 阶段的 EXPLAIN 树再考虑引入 |
| Plotly | 包体积最大（~1MB），样式难定制 |
| Chart.js | 非 React 原生，需要 wrapper |

---

## 十一、文件结构

```
src/
├── components/
│   └── chart/                         ← 新增目录
│       ├── ChartView.tsx              — 主容器
│       ├── ChartToolbar.tsx           — 工具栏
│       ├── AxisConfigurator.tsx        — 轴配置面板
│       ├── ChartCanvas.tsx            — 图表渲染分发
│       ├── ChartEmptyState.tsx        — 空状态/不可视化提示
│       └── renderers/                 — 各类型渲染器
│           ├── BarChartRenderer.tsx
│           ├── LineChartRenderer.tsx
│           ├── PieChartRenderer.tsx
│           ├── ScatterChartRenderer.tsx
│           └── AreaChartRenderer.tsx
├── lib/
│   └── chart/                         ← 新增目录
│       ├── fieldInference.ts          — 字段类型推断
│       ├── recommend.ts              — 智能推荐
│       ├── transform.ts              — 数据转换
│       ├── colors.ts                 — 配色方案
│       └── export.ts                 — 导出功能
├── types/
│   └── chart.ts                       ← 新增类型文件
└── windows/
    └── connection/
        └── QueryPanel.tsx             ← 修改：添加视图切换
```

---

## 十二、实施计划

### Phase 1：MVP 图表（预计 3-4 天）

| 步骤 | 内容 | 涉及文件 |
|------|------|---------|
| 1.1 | 安装 recharts 依赖 | package.json |
| 1.2 | 定义类型 | types/chart.ts |
| 1.3 | 实现字段推断 + 推荐引擎 | lib/chart/fieldInference.ts, recommend.ts |
| 1.4 | 实现数据转换器 | lib/chart/transform.ts |
| 1.5 | 实现 ChartView 组件 | components/chart/ChartView.tsx |
| 1.6 | 实现柱状图 + 折线图渲染器 | components/chart/renderers/ |
| 1.7 | 集成到 QueryPanel | windows/connection/QueryPanel.tsx |
| 1.8 | 暗色主题适配 | CSS 变量 |
| 1.9 | 单元测试 | lib/chart/__tests__/ |

### Phase 2：完善图表类型 + 导出（预计 2 天）

| 步骤 | 内容 |
|------|------|
| 2.1 | 饼图 + 散点图 + 面积图渲染器 |
| 2.2 | 图表导出 PNG / SVG |
| 2.3 | 轴配置面板 UI |
| 2.4 | 多 Y 轴支持 |

### Phase 3：交互增强（预计 2 天）

| 步骤 | 内容 |
|------|------|
| 3.1 | 图表 ↔ 表格联动（点击图表高亮表格行） |
| 3.2 | 配色方案切换 |
| 3.3 | 数值格式化（千分位、百分比） |
| 3.4 | 图表配置持久化到 panelStore |

### Phase 4：AI 集成（预计 2-3 天）

| 步骤 | 内容 |
|------|------|
| 4.1 | AI 对话生成图表（NL2SQL + 推荐图表类型 + 自动渲染） |
| 4.2 | 自然语言调整图表配置（"换成饼图"、"按销量排序"） |
| 4.3 | AI 异常标注 |

---

## 十三、边界条件处理

| 场景 | 处理方式 |
|------|---------|
| 结果集为空（0 行） | 显示 ChartEmptyState，提示"无数据可视化" |
| 结果集超大（>5000 行） | 图表采样展示前 1000 行 + 警告，完整数据看表格 |
| 无数值列 | 显示 ChartEmptyState，提示"需要至少一个数值列" |
| 全部 NULL | 图表上不显示对应数据点，Tooltip 显示 NULL |
| 类别过多（>50 个分类） | 默认只显示 Top 20 + "其他"分组 |
| JSON/数组列 | 不作为图表候选字段 |
| 多语句结果 | 每个 StatementResult 独立渲染，切换结果 Tab 时切换图表 |
| 窗口缩放 | Recharts ResponsiveContainer 自适应 |
