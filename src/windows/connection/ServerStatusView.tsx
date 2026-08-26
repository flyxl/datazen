import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Activity,
  AlertTriangle,
  ArrowDownRight,
  ArrowUpRight,
  Clock,
  Database,
  Files,
  Gauge,
  HardDrive,
  LayoutDashboard,
  Loader2,
  RefreshCw,
  Users,
  Zap,
} from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { ChartCanvas } from '../../components/chart/ChartCanvas';
import { cn } from '../../lib/cn';
import { driverCommands } from '../../commands/driver';
import { useI18n } from '../../hooks/useI18n';
import { SERVER_STATUS_SNAPSHOT_COMMAND } from '../../lib/driverCommandIds';
import type { ChartConfig, ChartDataPoint } from '../../types/chart';
import {
  COMMAND_KEYS,
  HISTORY_WINDOW_MS,
  alignTimestamp,
  cardRateFromHistory,
  computeTrendTimeAxis,
  formatByteRate,
  formatBytes,
  latestSeriesValue,
  updateTrendHistory,
  type TrendPrevSample,
  type TrendSeries,
} from '../../lib/serverStatusTrends';

import type { TranslationKey } from '../../locales';

export interface ServerStatusCache {
  status: Record<string, string | number | boolean | null>;
  variables?: { name: string; value: string | null }[];
  history?: Record<string, TrendSeries>;
  /** 上次成功刷新时刻（wall-clock ms），用于「上次更新时间」显示。 */
  updatedAt?: number;
}

interface ServerStatusViewProps {
  dbSessionId: string;
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
  qps: 'common.queriesPerSec',
  slowQueries: 'serverStatus.slowQueries',
  networkIn: 'serverStatus.networkIn',
  networkOut: 'serverStatus.networkOut',
  deadlocks: 'serverStatus.deadlocks',
  tempFiles: 'serverStatus.tempFiles',
  tempBytes: 'serverStatus.tempBytes',
  walBytes: 'serverStatus.walRate',
  cacheHitRatio: 'serverStatus.cacheHitRatio',
  innodbHitRatio: 'serverStatus.innodbHitRatio',
  txsPerSec: 'common.queriesPerSec',
};

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

function parseStatementTotal(data: unknown): Record<string, unknown> | null {
  if (!data || typeof data !== 'object' || Array.isArray(data)) return null;
  const sc = (data as Record<string, unknown>).statementCounters;
  if (!sc || typeof sc !== 'object' || Array.isArray(sc)) return null;
  return sc as Record<string, unknown>;
}

function formatUptime(seconds: number, t: (key: TranslationKey) => string): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) {
    return t('serverStatus.uptimeFormatDays')
      .replace('{days}', String(days))
      .replace('{hours}', String(hours))
      .replace('{minutes}', String(minutes));
  }
  return t('serverStatus.uptimeFormat')
    .replace('{hours}', String(hours))
    .replace('{minutes}', String(minutes))
    .replace('{seconds}', String(seconds));
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

/** 把 wall-clock ms 格式化为 `HH:mm:ss`（本地时区）。 */
function formatClock(ts: number): string {
  const d = new Date(ts);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function numberFrom(value: string | number | boolean | null): number | null {
  if (typeof value === 'number') return value;
  if (typeof value === 'string' && value.trim() !== '') {
    const n = Number(value);
    return Number.isFinite(n) ? n : null;
  }
  return null;
}

/**
 * 把多个同一 timeline 的时间戳序列合并成 recharts 数据点：
 * x = 真实时间戳（ms），各 series 值按 `v` 展开；t 会被 renderer 的
 * `formatAxisTick` 显示为 `HH:mm:ss`。
 */
function toChartData(
  keys: string[],
  history: Record<string, { t: number; v: number }[]>,
): ChartDataPoint[] {
  // 取所有序列的时间戳并集（通常各序列共享同一采样时刻，直接对齐）。
  const tsSet = new Map<number, ChartDataPoint>();
  for (const k of keys) {
    const series = history[k];
    if (!series) continue;
    for (const p of series) {
      let pt = tsSet.get(p.t);
      if (!pt) {
        pt = { t: p.t };
        tsSet.set(p.t, pt);
      }
      pt[k] = p.v;
    }
  }
  return [...tsSet.entries()]
    .map(([t, pt]) => ({ ...pt, t }))
    .sort((a, b) => (a.t as number) - (b.t as number));
}

/** 构造带图例的折线/面积图配置。传 timeAxis 时启用固定 1h 时间窗口。 */
function seriesChartConfig(
  chartType: 'line' | 'area',
  keys: string[],
  timeAxis?: { domain: [number, number]; ticks: number[] },
): ChartConfig {
  return {
    chartType,
    xAxis: 't',
    yAxes: keys,
    groupBy: null,
    aggregation: 'none',
    sortBy: 'none',
    showLegend: true,
    showGrid: true,
    showValues: false,
    colorScheme: 'default',
    timeDomain: timeAxis?.domain,
    timeTicks: timeAxis?.ticks,
  };
}

/** 数据卡片：数据驱动，字段缺失时隐藏。 */
interface MetricCard {
  key: string;
  label: string;
  value: string;
  icon: React.ReactNode;
  /** 是否具备渲染数据。 */
  available: boolean;
}

/** 全尺寸图表卡片。 */
interface ChartCard {
  key: string;
  label: string;
  /** 各趋势系列的展示名（图例）。 */
  seriesLabels: string[];
  /** 趋势 series key（对应 serverStatusTrends.TREND_SERIES 或 cmd_*）。 */
  seriesKeys: string[];
  chartType: 'line' | 'area';
  /** 全宽（true 时占满一行）。 */
  fullWidth?: boolean;
}

export function ServerStatusView({
  dbSessionId,
  connectionName,
  initialData,
  onDataChange,
}: ServerStatusViewProps) {
  const { t } = useI18n();
  const [loading, setLoading] = useState(true);
  /** 仅手动刷新时点亮按钮旋转图标；自动刷新保持静默（不闪按钮）。 */
  const [buttonLoading, setButtonLoading] = useState(false);
  const [lastUpdatedAt, setLastUpdatedAt] = useState<number | null>(initialData?.updatedAt ?? null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<StatusRecord | null>(initialData?.status ?? null);
  const [statusVariables, setStatusVariables] = useState<StatusVariable[]>(
    initialData?.variables ?? [],
  );
  const [history, setHistory] = useState<Record<string, TrendSeries>>(initialData?.history ?? {});
  // 图表时间窗口的右缘（最新对齐后的采样时刻）；用于固定 1h domain 与刻度。
  const [chartNow, setChartNow] = useState(() => {
    const ts = Object.values(initialData?.history ?? {})
      .flat()
      .map((p) => p.t)
      .pop();
    return ts ?? Date.now();
  });
  const [autoRefresh, setAutoRefresh] = useState(5000);
  const [viewTab, setViewTab] = useState<'dashboard' | 'variables' | 'details'>('dashboard');

  const SUB_TABS: { id: 'dashboard' | 'variables' | 'details'; label: string }[] = [
    { id: 'dashboard', label: t('serverStatus.dashboardTitle') },
    { id: 'variables', label: t('serverStatus.statusVarsTitle') },
    { id: 'details', label: t('serverStatus.detailTitle') },
  ];

  const prevRef = useRef<TrendPrevSample>({ ts: 0, values: {} });
  const historyRef = useRef<Record<string, TrendSeries>>(initialData?.history ?? {});

  const persist = useCallback(
    (
      record: StatusRecord | null,
      variables: StatusVariable[],
      hist: Record<string, TrendSeries>,
      updatedAt: number,
    ) => {
      if (record == null) return;
      onDataChange?.({ status: record, variables, history: hist, updatedAt });
    },
    [onDataChange],
  );

  const load = useCallback(
    async (opts?: { manual?: boolean }) => {
      // 首帧/切换连接仍显示整页加载；按钮旋转图标仅手动刷新点亮，自动刷新静默。
      setLoading(true);
      if (opts?.manual) setButtonLoading(true);
      setError(null);
      try {
        const result = await driverCommands.execute({
          dbSessionId,
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
        const statements = parseStatementTotal(raw);
        const { history: nextHist, prev: nextPrev } = updateTrendHistory({
          record,
          now: Date.now(),
          periodMs: autoRefresh,
          prev: prevRef.current,
          history: historyRef.current,
          statements,
        });

        historyRef.current = nextHist;
        prevRef.current = nextPrev;
        setStatus(record);
        setStatusVariables(variables);
        setHistory(nextHist);
        const alignedNow = alignTimestamp(Date.now(), autoRefresh);
        setChartNow(alignedNow);
        setLastUpdatedAt(Date.now());
        persist(record, variables, nextHist, Date.now());
      } catch (e) {
        const msg =
          typeof e === 'string' ? e : e instanceof Error ? e.message : t('serverStatus.loadFailed');
        setError(msg);
        setStatus(null);
      } finally {
        setLoading(false);
        setButtonLoading(false);
      }
    },
    [dbSessionId, t, persist, autoRefresh],
  );

  // 按 dbSessionId 拉取；切换服务器详情 tab 时若组件被复用，必须重新加载。
  // 故意不依赖 load：onDataChange 每帧可能是新引用，纳入依赖会重置 history 并无限重拉。
  useEffect(() => {
    historyRef.current = initialData?.history ?? {};
    prevRef.current = { ts: 0, values: {} };
    setStatus(initialData?.status ?? null);
    setStatusVariables(initialData?.variables ?? []);
    setHistory(initialData?.history ?? {});
    setLastUpdatedAt(initialData?.updatedAt ?? null);
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dbSessionId]);

  useEffect(() => {
    if (autoRefresh <= 0) return;
    const id = window.setInterval(() => {
      void load();
    }, autoRefresh);
    return () => window.clearInterval(id);
  }, [autoRefresh, load]);

  // ---- 数据卡片（数据驱动，字段缺失自动隐藏） ----
  const cards = useMemo<MetricCard[]>(() => {
    if (!status) return [];
    const uptime = numberFrom(status.uptimeSeconds);
    const connections = numberFrom(status.connections);
    const maxConnections = numberFrom(status.maxConnections);
    const hasCacheHit =
      status.cacheHitRatio != null && status.cacheHitRatio !== '' ? status.cacheHitRatio : null;
    const hasInnodb =
      status.innodbHitRatio != null && status.innodbHitRatio !== '' ? status.innodbHitRatio : null;
    const netInLatest = latestSeriesValue(history.netIn);
    const netOutLatest = latestSeriesValue(history.netOut);
    const qpsValue = cardRateFromHistory(history, 'qps', status.qps);
    const deadlocks = numberFrom(status.deadlocks);
    const tempFiles = numberFrom(status.tempFiles);
    const walBytes = numberFrom(status.walBytes);

    const defs: MetricCard[] = [
      {
        key: 'qps',
        label: t('common.queriesPerSec'),
        icon: <Activity className="h-4 w-4 text-emerald-400" />,
        value: qpsValue,
        available: qpsValue !== '—',
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
        available: connections != null,
      },
      {
        key: 'cacheHit',
        label: t('serverStatus.cacheHitRatio'),
        value:
          hasCacheHit != null ? String(hasCacheHit) : hasInnodb != null ? String(hasInnodb) : '—',
        icon: <Database className="h-4 w-4 text-violet-400" />,
        available: hasCacheHit != null || hasInnodb != null,
      },
      {
        key: 'activeQueries',
        label: t('serverStatus.activeQueries'),
        value: String(numberFrom(status.activeQueries) ?? '—'),
        icon: <Zap className="h-4 w-4 text-amber-400" />,
        available: numberFrom(status.activeQueries) != null,
      },
      {
        key: 'netIn',
        label: t('serverStatus.networkIn'),
        value: netInLatest != null ? formatByteRate(netInLatest) : '—',
        icon: <ArrowDownRight className="h-4 w-4 text-cyan-400" />,
        available: netInLatest != null,
      },
      {
        key: 'netOut',
        label: t('serverStatus.networkOut'),
        value: netOutLatest != null ? formatByteRate(netOutLatest) : '—',
        icon: <ArrowUpRight className="h-4 w-4 text-cyan-400" />,
        available: netOutLatest != null,
      },
      {
        key: 'deadlocks',
        label: t('serverStatus.deadlocks'),
        value: deadlocks != null ? String(deadlocks) : '—',
        icon: <AlertTriangle className="h-4 w-4 text-rose-400" />,
        available: deadlocks != null,
      },
      {
        key: 'tempFiles',
        label: t('serverStatus.tempFiles'),
        value: tempFiles != null ? String(tempFiles) : '—',
        icon: <Files className="h-4 w-4 text-orange-400" />,
        available: tempFiles != null,
      },
      {
        key: 'slowQueries',
        label: t('serverStatus.slowQueries'),
        value: String(numberFrom(status.slowQueries) ?? '—'),
        icon: <AlertTriangle className="h-4 w-4 text-rose-400" />,
        available: numberFrom(status.slowQueries) != null,
      },
      {
        key: 'walRate',
        label: t('serverStatus.walRate'),
        value: walBytes != null ? formatBytes(walBytes) : '—',
        icon: <HardDrive className="h-4 w-4 text-sky-400" />,
        available: walBytes != null,
      },
      {
        key: 'uptime',
        label: t('serverStatus.uptime'),
        value: uptime != null ? formatUptime(uptime, t) : '—',
        icon: <Clock className="h-4 w-4 text-cyan-400" />,
        available: uptime != null,
      },
    ];
    return defs.filter((c) => c.available);
  }, [status, history, t]);

  // ---- 图表卡片（数据驱动：无数据则隐藏） ----
  const chartCards = useMemo<ChartCard[]>(() => {
    const has = (keys: string[]) => keys.some((k) => (history[k]?.length ?? 0) > 0);
    // MySQL / 通用：每秒查询（QPS）
    const transactions: ChartCard = {
      key: 'transactions',
      label: t('common.queriesPerSec'),
      seriesKeys: ['qps'],
      seriesLabels: [t('serverStatus.trendTotal')],
      chartType: 'area',
    };
    const netTraffic: ChartCard = {
      key: 'netTraffic',
      label: t('serverStatus.chartNetIn'),
      seriesKeys: ['netIn', 'netOut'],
      seriesLabels: [t('serverStatus.networkIn'), t('serverStatus.networkOut')],
      chartType: 'line',
    };
    const commands: ChartCard = {
      key: 'commands',
      label: t('serverStatus.chartCommands'),
      seriesKeys: COMMAND_KEYS.map((c) => `cmd_${c}`),
      seriesLabels: COMMAND_KEYS.map((c) => t(`serverStatus.cmd${cap(c)}` as TranslationKey)),
      chartType: 'line',
    };
    const sessions: ChartCard = {
      key: 'sessions',
      label: t('serverStatus.chartSessions'),
      seriesKeys: ['sessions'],
      seriesLabels: [t('serverStatus.chartSessions')],
      chartType: 'area',
    };
    const sessionsState: ChartCard = {
      key: 'sessionsState',
      label: t('serverStatus.chartServerSessions'),
      seriesKeys: ['sessionTotal', 'sessionActive', 'sessionIdle'],
      seriesLabels: [
        t('serverStatus.trendTotal'),
        t('serverStatus.trendActive'),
        t('serverStatus.trendIdle'),
      ],
      chartType: 'line',
    };
    const blockIO: ChartCard = {
      key: 'blockIO',
      label: t('serverStatus.chartBlockIO'),
      seriesKeys: ['blksRead', 'blksHit'],
      seriesLabels: [t('serverStatus.trendBlockRead'), t('serverStatus.trendBlockHit')],
      chartType: 'line',
    };
    const tupleWrite: ChartCard = {
      key: 'tupleWrite',
      label: t('serverStatus.chartTupleWrite'),
      seriesKeys: ['tupInserted', 'tupUpdated', 'tupDeleted'],
      seriesLabels: [
        t('serverStatus.cmdInsert'),
        t('serverStatus.cmdUpdate'),
        t('serverStatus.cmdDelete'),
      ],
      chartType: 'line',
    };
    const tupleRead: ChartCard = {
      key: 'tupleRead',
      label: t('serverStatus.chartTupleRead'),
      seriesKeys: ['tupFetched', 'tupReturned'],
      seriesLabels: [t('serverStatus.trendFetched'), t('serverStatus.trendReturned')],
      chartType: 'line',
    };
    const txRate: ChartCard = {
      key: 'txRate',
      label: t('serverStatus.chartTransactions'),
      seriesKeys: ['commits', 'rollbacks'],
      seriesLabels: [t('serverStatus.trendCommit'), t('serverStatus.trendRollback')],
      chartType: 'area',
      fullWidth: true,
    };

    const panels: ChartCard[] = [];
    if (has(['qps'])) panels.push(transactions);
    if (has(['sessionTotal', 'sessionActive', 'sessionIdle'])) panels.push(sessionsState);
    if (has(['sessions'])) panels.push(sessions);
    if (has(['netIn', 'netOut'])) panels.push(netTraffic);
    if (has(['cmd_insert', 'cmd_select', 'cmd_update', 'cmd_delete'])) panels.push(commands);
    if (has(['blksRead', 'blksHit'])) panels.push(blockIO);
    if (has(['tupInserted', 'tupUpdated', 'tupDeleted'])) panels.push(tupleWrite);
    if (has(['tupFetched', 'tupReturned'])) panels.push(tupleRead);
    if (has(['commits', 'rollbacks'])) panels.push(txRate);
    return panels;
  }, [history, t]);

  // 时间窗口：右缘＝最新采样；左缘取「数据实际跨度」与 1min/1h 上下限之间的值，
  // 因此冷启动从最近 1 分钟开始，随数据积累逐步拉长到 1 小时（见 computeTrendTimeAxis）。
  const timeAxis = useMemo(() => {
    const lastSample = Math.max(chartNow, 0);
    // 全部序列里最早的一个采样点，作为数据实际跨度左端。
    let earliest = Infinity;
    for (const series of Object.values(history)) {
      if (!series || series.length === 0) continue;
      const firstT = series[0].t;
      if (firstT < earliest) earliest = firstT;
    }
    if (!Number.isFinite(earliest)) earliest = lastSample - HISTORY_WINDOW_MS;
    return computeTrendTimeAxis(earliest, lastSample);
  }, [chartNow, history]);

  function cap(s: string): string {
    return s.length > 0 ? s.charAt(0).toUpperCase() + s.slice(1) : s;
  }

  const detailRows = useMemo(() => {
    if (!status) return [];
    const internalKeys = new Set([
      'questionsCounter',
      'newSessionsCounter',
      'bytesInCounter',
      'bytesOutCounter',
      'xactCommitCounter',
      'xactRollbackCounter',
      'blksReadCounter',
      'blksHitCounter',
      'tupFetchedCounter',
      'tupReturnedCounter',
      'tupInsertedCounter',
      'tupUpdatedCounter',
      'tupDeletedCounter',
      'tempBytesCounter',
      'walBytes',
      'tempBytes',
      'sessionTotal',
      'sessionActive',
      'sessionIdle',
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
      'txsPerSec',
      'slowQueries',
      'networkIn',
      'networkOut',
      'cacheHitRatio',
      'innodbHitRatio',
      'deadlocks',
      'tempFiles',
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
        {lastUpdatedAt != null && (
          <span
            className="mr-2 flex items-center gap-1 text-xs text-fg-muted"
            title={t('serverStatus.lastUpdatedTitle')}
            data-testid="server-status-last-updated"
          >
            <Clock className="h-3 w-3" />
            {formatClock(lastUpdatedAt)}
          </span>
        )}
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
          onClick={() => void load({ manual: true })}
          disabled={buttonLoading}
          data-testid="server-dashboard-refresh"
        >
          {buttonLoading ? (
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
          <div className="flex flex-col gap-4">
            {/* 数据卡片：2×4 网格（数据驱动，缺字段隐藏） */}
            <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
              {cards.map((card) => (
                <div
                  key={card.key}
                  className="flex items-start gap-3 rounded-lg border border-edge bg-surface p-3"
                >
                  <div className="mt-0.5 shrink-0">{card.icon}</div>
                  <div className="min-w-0">
                    <div className="truncate text-xs text-fg-muted">{card.label}</div>
                    <div className="mt-0.5 truncate font-mono text-lg font-medium text-fg">
                      {card.value}
                    </div>
                  </div>
                </div>
              ))}
            </div>

            {/* 全尺寸图表卡片网格 */}
            {chartCards.length > 0 && (
              <>
                <div className="flex items-center gap-2">
                  <Activity className="h-3.5 w-3.5 text-fg-muted" />
                  <span className="text-sm font-medium text-fg">
                    {t('serverStatus.chartTitle')}
                  </span>
                </div>
                <div className="grid grid-cols-1 gap-3 xl:grid-cols-2">
                  {chartCards.map((panel) => (
                    <div
                      key={panel.key}
                      className={cn(
                        'rounded-lg border border-edge bg-surface',
                        panel.fullWidth && 'xl:col-span-2',
                      )}
                    >
                      <div className="flex items-center gap-2 border-b border-edge px-4 py-3">
                        <Gauge className="h-4 w-4 text-fg-muted" />
                        <span className="text-sm font-medium text-fg">{panel.label}</span>
                      </div>
                      {/* `relative` 必须与 ChartCanvas 的 `absolute inset-0` 配对：
                         否则画布定位基准会漂移到页面级祖先，recharts 按全屏尺寸
                         绘制导致折线横飞覆盖整页。与 ChartView.tsx 的用法一致。 */}
                      <div className="relative h-56 p-2">
                        <ChartCanvas
                          data={toChartData(panel.seriesKeys, history)}
                          config={seriesChartConfig(panel.chartType, panel.seriesKeys, timeAxis)}
                        />
                      </div>
                    </div>
                  ))}
                </div>
              </>
            )}
          </div>
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
