import { useCallback, useRef, useState } from 'react';
import { BarChart3, Download, LineChart as LineChartIcon, MessageSquare, PieChart as PieChartIcon, ScatterChart as ScatterChartIcon, TrendingUp } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { exportChartAsPng, exportChartAsSvg } from '../../lib/chart/export';
import { parseNlChartConfig } from '../../lib/chart/nlConfig';
import type { ChartConfig, ChartField, ChartType } from '../../types/chart';

interface ChartToolbarProps {
  config: ChartConfig;
  onChange: (config: ChartConfig) => void;
  chartRef: React.RefObject<HTMLDivElement | null>;
  fields?: ChartField[];
}

const CHART_TYPES: { type: ChartType; icon: React.ElementType; labelKey: string }[] = [
  { type: 'bar', icon: BarChart3, labelKey: 'chart.type.bar' },
  { type: 'line', icon: LineChartIcon, labelKey: 'chart.type.line' },
  { type: 'pie', icon: PieChartIcon, labelKey: 'chart.type.pie' },
  { type: 'scatter', icon: ScatterChartIcon, labelKey: 'chart.type.scatter' },
  { type: 'area', icon: TrendingUp, labelKey: 'chart.type.area' },
];

export function ChartToolbar({ config, onChange, chartRef, fields = [] }: ChartToolbarProps) {
  const { t } = useI18n();
  const [exportOpen, setExportOpen] = useState(false);
  const [nlInput, setNlInput] = useState('');
  const [nlOpen, setNlOpen] = useState(false);
  const nlInputRef = useRef<HTMLInputElement>(null);
  const exportRef = useRef<HTMLDivElement>(null);

  const handleNlSubmit = useCallback(() => {
    if (!nlInput.trim()) return;
    const result = parseNlChartConfig(nlInput, fields, config);
    if (result.matched) {
      onChange(result.config as ChartConfig);
      setNlInput('');
    }
  }, [nlInput, fields, config, onChange]);

  const handleExport = async (format: 'png' | 'svg') => {
    setExportOpen(false);
    const el = chartRef.current;
    if (!el) return;
    const filename = `chart-${Date.now()}`;
    if (format === 'png') await exportChartAsPng(el, filename);
    else await exportChartAsSvg(el, filename);
  };

  return (
    <div className="flex shrink-0 items-center gap-1 border-b border-edge bg-surface-alt px-2 py-1">
      {/* Chart type selector */}
      <div className="flex items-center gap-0.5 rounded-md bg-surface p-0.5">
        {CHART_TYPES.map(({ type, icon: Icon, labelKey }) => (
          <button
            key={type}
            type="button"
            title={t(labelKey as never)}
            aria-label={t(labelKey as never)}
            className={cn(
              'rounded px-2 py-1 transition-colors',
              config.chartType === type
                ? 'bg-accent/20 text-accent'
                : 'text-fg-muted hover:text-fg-secondary',
            )}
            onClick={() => onChange({ ...config, chartType: type })}
          >
            <Icon className="h-3.5 w-3.5" />
          </button>
        ))}
      </div>

      <span className="mx-1 h-4 w-px bg-edge" />

      {/* Toggle options */}
      <label className="flex items-center gap-1 text-xs text-fg-secondary cursor-pointer select-none">
        <input
          type="checkbox"
          checked={config.showLegend}
          onChange={(e) => onChange({ ...config, showLegend: e.target.checked })}
          className="accent-accent h-3 w-3"
        />
        {t('chart.legend')}
      </label>
      <label className="flex items-center gap-1 text-xs text-fg-secondary cursor-pointer select-none ml-2">
        <input
          type="checkbox"
          checked={config.showGrid}
          onChange={(e) => onChange({ ...config, showGrid: e.target.checked })}
          className="accent-accent h-3 w-3"
        />
        {t('chart.grid')}
      </label>
      <label className="flex items-center gap-1 text-xs text-fg-secondary cursor-pointer select-none ml-2">
        <input
          type="checkbox"
          checked={config.showValues}
          onChange={(e) => onChange({ ...config, showValues: e.target.checked })}
          className="accent-accent h-3 w-3"
        />
        {t('chart.values')}
      </label>

      <div className="flex-1" />

      {/* NL config input */}
      {nlOpen ? (
        <div className="flex items-center gap-1">
          <input
            ref={nlInputRef}
            type="text"
            value={nlInput}
            onChange={(e) => setNlInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') handleNlSubmit();
              if (e.key === 'Escape') { setNlOpen(false); setNlInput(''); }
            }}
            placeholder={t('chart.nlPlaceholder')}
            className="h-6 w-40 rounded border border-edge bg-surface px-2 text-xs text-fg focus:border-accent focus:outline-none"
            autoFocus
          />
        </div>
      ) : (
        <button
          type="button"
          className="flex items-center gap-1 rounded px-2 py-1 text-xs text-fg-muted hover:text-fg-secondary hover:bg-surface transition-colors"
          onClick={() => { setNlOpen(true); setTimeout(() => nlInputRef.current?.focus(), 50); }}
          title={t('chart.nlHint')}
        >
          <MessageSquare className="h-3.5 w-3.5" />
        </button>
      )}

      {/* Export dropdown */}
      <div className="relative" ref={exportRef}>
        <button
          type="button"
          className="flex items-center gap-1 rounded px-2 py-1 text-xs text-fg-muted hover:text-fg-secondary hover:bg-surface transition-colors"
          onClick={() => setExportOpen(!exportOpen)}
        >
          <Download className="h-3.5 w-3.5" />
          {t('chart.export')}
        </button>
        {exportOpen && (
          <>
            <div className="fixed inset-0 z-40" onClick={() => setExportOpen(false)} />
            <div className="absolute right-0 top-full z-50 mt-1 w-32 rounded-md border border-edge bg-surface-raised py-1 shadow-lg">
              <button
                type="button"
                className="w-full px-3 py-1.5 text-left text-xs text-fg-secondary hover:bg-surface-alt hover:text-fg"
                onClick={() => void handleExport('png')}
              >
                PNG
              </button>
              <button
                type="button"
                className="w-full px-3 py-1.5 text-left text-xs text-fg-secondary hover:bg-surface-alt hover:text-fg"
                onClick={() => void handleExport('svg')}
              >
                SVG
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
