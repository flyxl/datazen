import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Activity,
  ArrowDownRight,
  LayoutDashboard,
  Loader2,
  RefreshCw,
  Server,
  Users,
  Zap,
  Clock,
  HardDrive,
  Database,
  TerminalSquare,
} from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { ChartCanvas } from '../../components/chart/ChartCanvas';
import { cn } from '../../lib/cn';
import { driverCommands } from '../../commands/driver';
import { useI18n } from '../../hooks/useI18n';
import { SERVER_STATUS_SNAPSHOT_COMMAND } from '../../lib/driverCommandIds';
import type { ChartConfig, ChartDataPoint } from '../../types/chart';

import type { TranslationKey } from '../../locales';

export interface ServerStatusCache {
  status: Record<string, string | number | boolean | null>;
  variables?: { name: string; value: string | null }[];
  history?: Record<string, number[]>;
}

interface ServerStatusViewProps {
  connectionId: string;
  connectionName?: string;
  /** 面板 id（用于把数据写回对应 tab）。 */
  panelId?: string;
  /** 该 tab 已缓存的数据；存在时直接展示（数据绑定到 tab）。 */
  initialData?: ServerStatusCache;
  /** 加载/变化后把最新数据写回对应面板。 */
  onDataChange?: (data: ServerStatusCache) => void;
}

type StatusRecord = Record<string, string | number | boolean | null>;
interface StatusVariable {
  name: string;
  value: string | null;
}

const AUTO_REFRESH_OPTIONS = [
  { value: 0, key: 'serverStatus.refreshOff' as TranslationKey },
  { value: 5000, key: 'serverStatus.refresh5s' as TranslationKey },
  { value: 10000, key: 'serverStatus.refresh10s' as TranslationKey },
  { value: 30000, key: 'serverStatus.refresh30s' as TranslationKey },
];

const METRIC_LABELS: Record<string, TranslationKey> = {
  version: 'serverStatus.version',
  database: 'serverStatus.database',
  uptimeSeconds: 'serverStatus.uptime',
  connections: 'serverStatus.connections',
  maxConnections: 'serverStatus.maxConnections',
  activeQueries: 'serverStatus.activeQueries',
  databaseSize: 'serverStatus.databaseSize',
  qps: 'serverStatus.qps',
  slowQueries: 'serverStatus.slowQueries',
  networkIn: 'serverStatus.networkIn',
  networkOut: 'serverStatus.networkOut',
};

/**
 * 四个实时趋势仪表盘：由于 Host 驱动返回的「累积计数器」差分得到的每秒速率。
 * 驱动不返回某计数器时，对应趋势卡自动隐藏（数据驱动，Host 不感知驱动差异）。
 */
const TREND_SERIES: Record<string, string> = {
  qps: 'questionsCounter',
  sessions: 'newSessionsCounter',
  netIn: 'bytesInCounter',
  netOut: 'bytesOutCounter',
};

const HISTORY_MAX = 30;

function asStatusRecord(data: unknown): StatusRecord | null {
  if (!data || typeof data !== 'object' || Array.isArray(data)) return null;
  const out: StatusRecord = {};
  for (const [key, value] of Object.entries(data as Record<string, unknown>)) {
    if (value == null) {
      out[key] = null;
    } else if (
      typeof value === 'string' ||
      typeof value === 'number' ||
      typeof value === 'boolean'
    ) {
      out[key] = value;
    } else {
      out[key] = JSON.stringify(value);
    }
  }
  return out;
}

function parseStatusVariables(data: unknown): StatusVariable[] {
  if (!data || typeof data !== 'object' || Array.isArray(data)) return [];
  const arr = (data as Record<string, unknown>).statusVariables;
  if (!Array.isArray(arr)) return [];
  const out: StatusVariable[] = [];
  for (const item of arr) {
    if (item && typeof item === 'object') {
      const rec = item as Record<string, unknown>;
      const name = typeof rec.name === 'string' ? rec.name : String(rec.name ?? '');
      if (!name) continue;
      out.push({ name, value: rec.value == null ? null : String(rec.value) });
    }
  }
  return out;
}

function parseStatementTotal(data: unknown): number | null {
  if (!data || typeof data !== 'object' || Array.isArray(data)) return null;
  const sc = (data as Record<string, unknown>).statementCounters;
  if (!sc || typeof sc !== 'object' || Array.isArray(sc)) return null;
  let total = 0;
  for (const v of Object.values(sc as Record<string, unknown>)) {
    const n = typeof v === 'number' ? v : Number(v);
    if (Number.isFinite(n)) total += n;
  }
  return total;
}

function formatUptime(seconds: number, t: (key: TranslationKey) => string): string {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const secs = seconds % 60;
  return t('serverStatus.uptimeFormat')
    .replace('{hours}', String(hours))
    .replace('{minutes}', String(minutes))
    .replace('{seconds}', String(secs));
}

function formatValue(
  key: string,
  value: string | number | boolean | null,
  t: (key: TranslationKey) => string,
): string {
  if (value == null || value === '') return '—';
  if (key === 'uptimeSeconds' && typeof value === 'number') {
    return formatUptime(value, t);
  }
  return String(value);
}

function labelForKey(key: string, t: (key: TranslationKey) => string): string {
  const labelKey = METRIC_LABELS[key];
  return labelKey ? t(labelKey) : key;
}

function numberFrom(value: string | number | boolean | null): number | null {
  if (typeof value === 'number') return value;
  if (typeof value === 'string' && value.trim() !== '') {
    const n = Number(value);
    return Number.isFinite(n) ? n : null;
  }
  return null;
}

function latestValue(series?: number[]): number | null {
  if (!series || series.length === 0) return null;
  return series[series.length - 1];
}

function formatRate(value: number): string {
  if (value < 10) return value.toFixed(2);
  if (value < 1000) return value.toFixed(1);
  if (value < 1000000) return `${(value / 1000).toFixed(1)}K`;
  return `${(value / 1000000).toFixed(1)}M`;
}

/** 把一段历史序列转成 ChartCanvas(line) 所需的 recharts 数据点。 */
function toChartData(series: number[]): ChartDataPoint[] {
  return series.map((v, i) => ({ t: i, value: v }));
}

/** 构造一个紧凑的折线图配置。 */
function lineConfig(): ChartConfig {
  return {
    chartType: 'line',
    xAxis: 't',
    yAxes: ['value'],
    groupBy: null,
    aggregation: 'none',
    sortBy: 'none',
    showLegend: false,
    showGrid: true,
    showValues: false,
    colorScheme: 'default',
  };
}

interface MetricCard {
  key: string;
  label: string;
  value: string;
  icon: React.ReactNode;
}

interface TrendCard {
  key: string;
  label: string;
  icon: React.ReactNode;
  color: string;
  unit: string;
  series: number[];
  latest: number | null;
}

export function ServerStatusView({
  connectionId,
  connectionName,
  initialData,
  onDataChange,
}: ServerStatusViewProps) {
  const { t } = useI18n();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<StatusRecord | null>(initialData?.status ?? null);
  const [statusVariables, setStatusVariables] = useState<StatusVariable[]>(
    initialData?.variables ?? [],
  );
  const [history, setHistory] = useState<Record<string, number[]>>(initialData?.history ?? {});
  const [autoRefresh, setAutoRefresh] = useState(5000);
  const [viewTab, setViewTab] = useState<'dashboard' | 'variables' | 'details'>('dashboard');

  const SUB_TABS: { id: 'dashboard' | 'variables' | 'details'; label: string }[] = [
    { id: 'dashboard', label: t('serverStatus.dashboardTitle') },
    { id: 'variables', label: t('serverStatus.statusVarsTitle') },
    { id: 'details', label: t('serverStatus.detailTitle') },
  ];

  const prevRef = useRef<{ ts: number; values: Record<string, number | null> }>({
    ts: 0,
    values: {},
  });

  const persist = useCallback(
    (record: StatusRecord | null, variables: StatusVariable[], hist: Record<string, number[]>) => {
      if (record == null) return;
      onDataChange?.({ status: record, variables, history: hist });
    },
    [onDataChange],
  );

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await driverCommands.execute({
        connectionId,
        command: SERVER_STATUS_SNAPSHOT_COMMAND,
        input: {},
      });
      const raw = result?.data;
      const record = asStatusRecord(raw);
      if (!record) {
        setError(t('serverStatus.invalidResponse'));
        setStatus(null);
        return;
      }
      const variables = parseStatusVariables(raw);

      // 用驱动返回的累积计数器做跨刷新差分，得到每秒速率。
      const now = Date.now();
      const prev = prevRef.current;
      const dtSec = prev.ts > 0 ? (now - prev.ts) / 1000 : 0;
      const rateFor = (counterKey: string): number | null => {
        const cur = numberFrom(record[counterKey]);
        const prevVal = prev.values[counterKey];
        if (cur == null || prevVal == null || dtSec <= 0) return null;
        const delta = cur - prevVal;
        return delta < 0 ? 0 : delta / dtSec;
      };

      const nextHist: Record<string, number[]> = { ...history };
      for (const chartKey of Object.keys(TREND_SERIES)) {
        const rate = rateFor(TREND_SERIES[chartKey]);
        if (rate != null) {
          const series = nextHist[chartKey] ?? [];
          nextHist[chartKey] = [...series, rate].slice(-HISTORY_MAX);
        }
      }
      const cmdTotal = parseStatementTotal(raw);
      if (cmdTotal != null && prev.values['__cmd_total'] != null && dtSec > 0) {
        const rate = (cmdTotal - prev.values['__cmd_total']!) / dtSec;
        if (rate >= 0) {
          const series = nextHist.commands ?? [];
          nextHist.commands = [...series, rate].slice(-HISTORY_MAX);
        }
      }

      setStatus(record);
      setStatusVariables(variables);
      setHistory(nextHist);
      persist(record, variables, nextHist);

      prevRef.current = {
        ts: now,
        values: {
          questionsCounter: numberFrom(record.questionsCounter),
          newSessionsCounter: numberFrom(record.newSessionsCounter),
          bytesInCounter: numberFrom(record.bytesInCounter),
          bytesOutCounter: numberFrom(record.bytesOutCounter),
          __cmd_total: cmdTotal,
        },
      };
    } catch (e) {
      const msg =
        typeof e === 'string' ? e : e instanceof Error ? e.message : t('serverStatus.loadFailed');
      setError(msg);
      setStatus(null);
    } finally {
      setLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connectionId, t, persist]);

  // 挂载后总是拉取最新仪表盘；initialData 仅用于首帧展示（数据绑定到 tab，不残留陈旧内容）。
  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (autoRefresh <= 0) return;
    const id = window.setInterval(() => {
      void load();
    }, autoRefresh);
    return () => window.clearInterval(id);
  }, [autoRefresh, load]);

  const trendCards = useMemo<TrendCard[]>(
    () => [
      {
        key: 'qps',
        label: t('serverStatus.chartQps'),
        icon: <Activity className="h-4 w-4 text-emerald-400" />,
        color: 'stroke-emerald-400',
        unit: '/s',
        series: history.qps ?? [],
        latest: latestValue(history.qps),
      },
      {
        key: 'sessions',
        label: t('serverStatus.chartSessions'),
        icon: <Users className="h-4 w-4 text-blue-400" />,
        color: 'stroke-blue-400',
        unit: '/s',
        series: history.sessions ?? [],
        latest: latestValue(history.sessions),
      },
      {
        key: 'netIn',
        label: t('serverStatus.chartNetIn'),
        icon: <ArrowDownRight className="h-4 w-4 text-cyan-400" />,
        color: 'stroke-cyan-400',
        unit: 'B/s',
        series: history.netIn ?? [],
        latest: latestValue(history.netIn),
      },
      {
        key: 'commands',
        label: t('serverStatus.chartCommands'),
        icon: <TerminalSquare className="h-4 w-4 text-amber-400" />,
        color: 'stroke-amber-400',
        unit: '/s',
        series: history.commands ?? [],
        latest: latestValue(history.commands),
      },
    ],
    [t, history],
  );

  const visibleTrends = trendCards.filter((c) => c.series.length > 1);
  const hasTrend = visibleTrends.length > 0;

  const detailRows = useMemo(() => {
    if (!status) return [];
    const internalKeys = new Set([
      'questionsCounter',
      'newSessionsCounter',
      'bytesInCounter',
      'bytesOutCounter',
      'statementCounters',
      'statusVariables',
    ]);
    const order = [
      'version',
      'database',
      'uptimeSeconds',
      'connections',
      'maxConnections',
      'activeQueries',
      'databaseSize',
      'qps',
      'slowQueries',
      'networkIn',
      'networkOut',
    ];
    const keys = [
      ...order.filter((k) => k in status && !internalKeys.has(k)),
      ...Object.keys(status)
        .filter((k) => !order.includes(k) && !internalKeys.has(k))
        .sort(),
    ];
    return keys.map((key) => ({
      key,
      label: labelForKey(key, t),
      value: formatValue(key, status[key], t),
    }));
  }, [status, t]);

  const cards = useMemo<MetricCard[]>(() => {
    if (!status) return [];
    const uptime = numberFrom(status.uptimeSeconds);
    const connections = numberFrom(status.connections);
    const maxConnections = numberFrom(status.maxConnections);
    const activeQueries = numberFrom(status.activeQueries);
    const version = status.version;
    const database = status.database;
    return [
      {
        key: 'qps',
        label: t('serverStatus.qps'),
        value: String(status.qps ?? '—'),
        icon: <Activity className="h-4 w-4 text-emerald-400" />,
      },
      {
        key: 'connections',
        label: t('serverStatus.connections'),
        value:
          connections != null && maxConnections != null
            ? `${connections} / ${maxConnections}`
            : connections != null
              ? String(connections)
              : '—',
        icon: <Users className="h-4 w-4 text-blue-400" />,
      },
      {
        key: 'activeQueries',
        label: t('serverStatus.activeQueries'),
        value: String(activeQueries ?? '—'),
        icon: <Zap className="h-4 w-4 text-amber-400" />,
      },
      {
        key: 'uptime',
        label: t('serverStatus.uptime'),
        value: uptime != null ? formatUptime(uptime, t) : '—',
        icon: <Clock className="h-4 w-4 text-cyan-400" />,
      },
      {
        key: 'databaseSize',
        label: t('serverStatus.databaseSize'),
        value: formatValue('databaseSize', status.databaseSize, t),
        icon: <HardDrive className="h-4 w-4 text-violet-400" />,
      },
      {
        key: 'version',
        label: t('serverStatus.version'),
        value: version != null ? String(version) : '—',
        icon: <Server className="h-4 w-4 text-slate-400" />,
      },
      {
        key: 'database',
        label: t('serverStatus.database'),
        value: database != null ? String(database) : '—',
        icon: <Database className="h-4 w-4 text-rose-400" />,
      },
    ];
  }, [status, t]);

  if (loading && !status && !initialData?.status) {
    return (
      <div className="flex min-h-0 flex-1 items-center justify-center p-6 text-sm text-fg-muted">
        <Loader2 className="mr-2 h-4 w-4 animate-spin" />
        {t('serverStatus.loading')}
      </div>
    );
  }

  if (error && !status) {
    return (
      <div className="flex min-h-0 flex-1 items-center justify-center p-6 text-sm text-danger">
        {error}
      </div>
    );
  }

  if (!status) {
    return (
      <div className="flex min-h-0 flex-1 items-center justify-center p-6 text-sm text-fg-muted">
        {t('serverStatus.empty')}
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-2 border-b border-edge bg-surface-alt px-4 py-3">
        <LayoutDashboard className="h-4 w-4 text-emerald-400" />
        <span className="text-sm font-medium text-fg">{t('serverStatus.dashboardTitle')}</span>
        {connectionName && (
          <span className="rounded bg-surface-raised px-1.5 py-0.5 text-xs text-fg">
            {connectionName}
          </span>
        )}
        {status.version != null && status.version !== '' && (
          <span className="rounded bg-surface-raised px-1.5 py-0.5 text-xs text-fg-muted">
            {String(status.version)}
          </span>
        )}
        <div className="flex-1" />
        <div className="flex items-center gap-1.5 text-xs text-fg-muted">
          {t('serverStatus.autoRefresh')}
          <Select
            value={autoRefresh}
            options={AUTO_REFRESH_OPTIONS.map((opt) => ({
              value: String(opt.value),
              label: t(opt.key),
            }))}
            onChange={(v) => setAutoRefresh(Number(v))}
            className="w-20 text-xs"
            title="auto-refresh"
          />
        </div>
        <Button
          variant="secondary"
          className="h-8 gap-1 text-xs"
          onClick={() => void load()}
          disabled={loading}
          data-testid="server-dashboard-refresh"
        >
          {loading ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <RefreshCw className="h-3.5 w-3.5" />
          )}
          {t('serverStatus.refresh')}
        </Button>
      </div>

      <div
        className="flex shrink-0 items-center border-b border-edge bg-surface-alt"
        role="tablist"
        aria-label={t('serverStatus.dashboardTitle')}
      >
        <div className="flex min-w-0 flex-1">
          {SUB_TABS.map((tab) => (
            <button
              key={tab.id}
              type="button"
              role="tab"
              aria-selected={viewTab === tab.id}
              className={cn(
                'relative px-5 py-2 text-[13px] transition-colors',
                viewTab === tab.id
                  ? 'bg-surface text-fg font-medium'
                  : 'text-fg-secondary hover:text-fg',
              )}
              onClick={() => setViewTab(tab.id)}
              data-testid={`server-view-tab-${tab.id}`}
            >
              {tab.label}
            </button>
          ))}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-auto p-4">
        {viewTab === 'dashboard' && (
          <>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4">
              {cards.map((card) => (
                <div
                  key={card.key}
                  className="flex items-start gap-3 rounded-lg border border-edge bg-surface p-3"
                >
                  <div className="mt-0.5 shrink-0">{card.icon}</div>
                  <div className="min-w-0">
                    <div className="truncate text-xs text-fg-muted">{card.label}</div>
                    <div className="mt-0.5 truncate font-mono text-base font-medium text-fg">
                      {card.value}
                    </div>
                  </div>
                </div>
              ))}
            </div>

            {hasTrend && (
              <div className="mt-4">
                <div className="mb-2 flex items-center gap-2">
                  <Activity className="h-3.5 w-3.5 text-fg-muted" />
                  <span className="text-sm font-medium text-fg">
                    {t('serverStatus.chartTitle')}
                  </span>
                </div>
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4">
                  {visibleTrends.map((card) => (
                    <div key={card.key} className="rounded-lg border border-edge bg-surface p-3">
                      <div className="flex items-center gap-2">
                        {card.icon}
                        <span className="truncate text-xs text-fg-muted">{card.label}</span>
                      </div>
                      <div className="my-1 font-mono text-lg font-medium text-fg">
                        {card.latest != null ? `${formatRate(card.latest)}${card.unit}` : '—'}
                      </div>
                      <div className="relative h-24">
                        <ChartCanvas
                          data={toChartData(card.series)}
                          config={lineConfig()}
                          compact
                        />
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </>
        )}

        {viewTab === 'variables' &&
          (statusVariables.length > 0 ? (
            <div className="rounded-lg border border-edge bg-surface">
              <div className="flex items-center gap-2 border-b border-edge px-3 py-2">
                <Database className="h-3.5 w-3.5 text-fg-muted" />
                <span className="text-sm font-medium text-fg">
                  {t('serverStatus.statusVarsTitle')}
                </span>
                <span className="rounded bg-surface-raised px-1.5 text-[11px] text-fg-muted">
                  {statusVariables.length}
                </span>
              </div>
              <div className="max-h-[calc(100vh-260px)] overflow-auto p-3">
                <table className="w-full text-sm">
                  <tbody>
                    {statusVariables.map((v) => (
                      <tr key={v.name} className="border-b border-edge/60">
                        <th className="w-64 py-1.5 pr-4 text-left font-mono font-normal text-fg-secondary">
                          {v.name}
                        </th>
                        <td className="break-all py-1.5 font-mono text-fg">{v.value ?? '—'}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          ) : (
            <div className="flex h-full items-center justify-center text-sm text-fg-muted">
              {t('serverStatus.emptyVariables')}
            </div>
          ))}

        {viewTab === 'details' && (
          <div className="rounded-lg border border-edge bg-surface">
            <div className="flex items-center gap-2 border-b border-edge px-3 py-2">
              <Database className="h-3.5 w-3.5 text-fg-muted" />
              <span className="text-sm font-medium text-fg">{t('serverStatus.detailTitle')}</span>
            </div>
            <div className="p-3">
              <table className="w-full text-sm">
                <tbody>
                  {detailRows.map((row) => (
                    <tr key={row.key} className="border-b border-edge/60">
                      <th className="w-48 py-1.5 pr-4 text-left font-medium text-fg-secondary">
                        {row.label}
                      </th>
                      <td className="py-1.5 font-mono text-fg">{row.value}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
