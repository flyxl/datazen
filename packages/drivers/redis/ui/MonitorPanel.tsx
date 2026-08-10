import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react';
import {
  ChevronDown,
  ChevronRight,
  Loader2,
  RefreshCw,
  Trash2,
} from 'lucide-react';
import { Button } from '../../../../src/components/ui/Button';
import { Dialog } from '../../../../src/components/ui/Dialog';
import { useI18n } from '../../../../src/hooks/useI18n';
import { cn } from '../../../../src/lib/cn';
import { pluginInvoke } from '../../../../src/plugins/generated';
import { useSettingsStore } from '../../../../src/stores/settingsStore';
import { parseInfoSections, type InfoSection } from './infoParse';
import { StreamOverview } from './StreamOverview';
import { ClusterNodePicker } from './ClusterNodePicker';
import { readClusterRouting, resolvePinnedNodeAddr } from './settingsHelpers';

export interface MonitorPanelProps {
  connectionId: string;
  dbIndex?: number;
  pinnedNodeAddr?: string;
  onPinnedNodeAddrChange?: (addr: string) => void;
}

type MonitorSubPage = 'info' | 'memory' | 'slowlog' | 'streams';

interface MemorySample {
  key: string;
  bytes: number;
}

interface MemorySampleResult {
  samples: MemorySample[];
  truncated: boolean;
}

interface SlowlogEntry {
  id: number;
  timestamp: number;
  durationUs: number;
  command: string[];
  clientAddr?: string | null;
  clientName?: string | null;
}

const SLOWLOG_COUNT = 100;
const MEMORY_SAMPLE_LIMIT = 200;

function formatBytes(size: number): string {
  if (!size || size < 0) return '—';
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDurationUs(us: number): string {
  if (us < 1000) return `${us} µs`;
  if (us < 1_000_000) return `${(us / 1000).toFixed(2)} ms`;
  return `${(us / 1_000_000).toFixed(2)} s`;
}

function formatTimestamp(ts: number): string {
  return new Date(ts * 1000).toLocaleString();
}

function formatSlowlogClient(entry: SlowlogEntry): string {
  const parts: string[] = [];
  if (entry.clientAddr) parts.push(entry.clientAddr);
  if (entry.clientName) parts.push(entry.clientName);
  return parts.join(' · ') || '—';
}

export function MonitorPanel({
  connectionId,
  dbIndex = 0,
  pinnedNodeAddr = '',
  onPinnedNodeAddrChange,
}: MonitorPanelProps) {
  const { t } = useI18n();
  const pluginSettings = useSettingsStore((s) => s.settings.pluginSettings);
  const clusterRouting = readClusterRouting(pluginSettings?.redis);
  const nodeAddr = resolvePinnedNodeAddr(clusterRouting, pinnedNodeAddr);
  const [subPage, setSubPage] = useState<MonitorSubPage>('info');

  const subPages = useMemo(
    () =>
      [
        { id: 'info' as const, label: t('redis.info') },
        { id: 'memory' as const, label: t('redis.memory') },
        { id: 'slowlog' as const, label: t('redis.slowlog') },
        { id: 'streams' as const, label: t('redis.streamOverview') },
      ] satisfies Array<{ id: MonitorSubPage; label: string }>,
    [t],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-1 border-b border-edge bg-surface-alt px-3 py-2">
        {subPages.map((page) => (
          <button
            key={page.id}
            type="button"
            className={cn(
              'rounded-md px-3 py-1.5 text-xs transition-colors',
              subPage === page.id
                ? 'bg-surface-raised font-medium text-fg'
                : 'text-fg-secondary hover:bg-surface-raised/60 hover:text-fg',
            )}
            onClick={() => setSubPage(page.id)}
          >
            {page.label}
          </button>
        ))}
        <div className="flex-1" />
        <ClusterNodePicker
          connectionId={connectionId}
          compact
          value={pinnedNodeAddr}
          onChange={onPinnedNodeAddrChange}
        />
      </div>

      {subPage === 'info' ? (
        <InfoPane connectionId={connectionId} nodeAddr={nodeAddr} />
      ) : subPage === 'memory' ? (
        <MemoryPane connectionId={connectionId} dbIndex={dbIndex} />
      ) : subPage === 'slowlog' ? (
        <SlowlogPane connectionId={connectionId} />
      ) : (
        <StreamOverview connectionId={connectionId} dbIndex={dbIndex} />
      )}
    </div>
  );
}

function InfoPane({
  connectionId,
  nodeAddr,
}: {
  connectionId: string;
  nodeAddr: string | null;
}) {
  const { t } = useI18n();
  const [sections, setSections] = useState<InfoSection[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const raw = await pluginInvoke<string>('redis', 'info', {
        connectionId,
        section: null,
        nodeAddr,
      });
      const parsed = parseInfoSections(raw);
      setSections(parsed);
      setExpanded(new Set(parsed.map((s) => s.name)));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setSections([]);
    } finally {
      setLoading(false);
    }
  }, [connectionId, nodeAddr]);

  useEffect(() => {
    void load();
  }, [load]);

  const toggleSection = useCallback((name: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(name)) {
        next.delete(name);
      } else {
        next.add(name);
      }
      return next;
    });
  }, []);

  return (
    <MonitorPaneShell
      loading={loading}
      error={error}
      onRefresh={() => void load()}
      empty={!loading && !error && sections.length === 0}
      emptyMessage={t('redis.infoEmpty')}
    >
      <div className="divide-y divide-edge">
        {sections.map((section) => {
          const open = expanded.has(section.name);
          return (
            <div key={section.name}>
              <button
                type="button"
                className="flex w-full items-center gap-2 px-4 py-2 text-left text-sm font-medium text-fg hover:bg-surface-raised/50"
                onClick={() => toggleSection(section.name)}
              >
                {open ? (
                  <ChevronDown className="h-4 w-4 shrink-0 text-fg-muted" />
                ) : (
                  <ChevronRight className="h-4 w-4 shrink-0 text-fg-muted" />
                )}
                <span>{section.name}</span>
                <span className="text-xs font-normal text-fg-muted">
                  ({section.entries.length})
                </span>
              </button>
              {open && (
                <div className="border-t border-edge bg-surface">
                  <table className="w-full border-collapse text-[13px]">
                    <tbody>
                      {section.entries.map((entry) => (
                        <tr
                          key={`${section.name}:${entry.key}`}
                          className="border-b border-edge/60 last:border-b-0"
                        >
                          <td className="w-[40%] px-4 py-1.5 align-top font-mono text-xs text-fg-secondary">
                            {entry.key}
                          </td>
                          <td className="px-4 py-1.5 align-top font-mono text-xs text-fg">
                            {entry.value}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </MonitorPaneShell>
  );
}

function MemoryPane({
  connectionId,
  dbIndex,
}: {
  connectionId: string;
  dbIndex: number;
}) {
  const { t } = useI18n();
  const [result, setResult] = useState<MemorySampleResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await pluginInvoke<MemorySampleResult>('redis', 'memory_sample', {
        connectionId,
        dbIndex,
        limit: MEMORY_SAMPLE_LIMIT,
      });
      setResult(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setResult(null);
    } finally {
      setLoading(false);
    }
  }, [connectionId, dbIndex]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <MonitorPaneShell
      loading={loading}
      error={error}
      onRefresh={() => void load()}
      empty={!loading && !error && (result?.samples.length ?? 0) === 0}
      emptyMessage={t('redis.memoryEmpty')}
    >
      {result?.truncated && (
        <div className="border-b border-warning/20 bg-warning/10 px-4 py-2 text-xs text-warning">
          {t('redis.memoryTruncated')}
        </div>
      )}
      <div className="min-h-0 flex-1 overflow-auto">
        <table className="w-full border-collapse text-[13px]">
          <thead className="sticky top-0 bg-surface-alt">
            <tr className="border-b border-edge text-left text-xs text-fg-secondary">
              <th className="px-4 py-2 font-medium">{t('redis.key')}</th>
              <th className="px-4 py-2 font-medium">{t('redis.bytes')}</th>
            </tr>
          </thead>
          <tbody>
            {result?.samples.map((sample) => (
              <tr key={sample.key} className="border-b border-edge/60">
                <td className="px-4 py-1.5 font-mono text-xs text-fg">{sample.key}</td>
                <td className="px-4 py-1.5 font-mono text-xs text-fg-secondary">
                  {formatBytes(sample.bytes)}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </MonitorPaneShell>
  );
}

function SlowlogPane({ connectionId }: { connectionId: string }) {
  const { t } = useI18n();
  const [entries, setEntries] = useState<SlowlogEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [resetOpen, setResetOpen] = useState(false);
  const [resetBusy, setResetBusy] = useState(false);
  const [resetError, setResetError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await pluginInvoke<SlowlogEntry[]>('redis', 'slowlog_get', {
        connectionId,
        count: SLOWLOG_COUNT,
      });
      setEntries(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setEntries([]);
    } finally {
      setLoading(false);
    }
  }, [connectionId]);

  useEffect(() => {
    void load();
  }, [load]);

  const handleReset = useCallback(async () => {
    setResetBusy(true);
    setResetError(null);
    try {
      await pluginInvoke('redis', 'slowlog_reset', {
        connectionId,
        confirm: true,
      });
      setResetOpen(false);
      await load();
    } catch (e) {
      setResetError(e instanceof Error ? e.message : String(e));
    } finally {
      setResetBusy(false);
    }
  }, [connectionId, load]);

  return (
    <>
      <MonitorPaneShell
        loading={loading}
        error={error}
        onRefresh={() => void load()}
        empty={!loading && !error && entries.length === 0}
        emptyMessage={t('redis.slowlogEmpty')}
        toolbar={
          <Button
            variant="secondary"
            className="h-7 gap-1 px-2 text-xs text-danger hover:text-danger"
            onClick={() => {
              setResetError(null);
              setResetOpen(true);
            }}
            disabled={loading}
          >
            <Trash2 className="h-3.5 w-3.5" />
            {t('redis.resetSlowlog')}
          </Button>
        }
      >
        <div className="min-h-0 flex-1 overflow-auto">
          <table className="w-full border-collapse text-[13px]">
            <thead className="sticky top-0 bg-surface-alt">
              <tr className="border-b border-edge text-left text-xs text-fg-secondary">
                <th className="px-3 py-2 font-medium">{t('redis.slowlogId')}</th>
                <th className="px-3 py-2 font-medium">{t('redis.slowlogTimestamp')}</th>
                <th className="px-3 py-2 font-medium">{t('redis.slowlogDuration')}</th>
                <th className="px-3 py-2 font-medium">{t('redis.slowlogCommand')}</th>
                <th className="px-3 py-2 font-medium">{t('redis.slowlogClient')}</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => (
                <tr key={entry.id} className="border-b border-edge/60 align-top">
                  <td className="px-3 py-2 font-mono text-xs text-fg-secondary">{entry.id}</td>
                  <td className="px-3 py-2 text-xs text-fg-secondary">
                    {formatTimestamp(entry.timestamp)}
                  </td>
                  <td className="px-3 py-2 font-mono text-xs text-fg">
                    {formatDurationUs(entry.durationUs)}
                  </td>
                  <td className="max-w-[420px] px-3 py-2 font-mono text-xs text-fg">
                    <span className="break-all">{entry.command.join(' ')}</span>
                  </td>
                  <td className="px-3 py-2 text-xs text-fg-muted">
                    {formatSlowlogClient(entry)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </MonitorPaneShell>

      <Dialog
        open={resetOpen}
        title={t('redis.confirmResetSlowlog')}
        description={t('redis.confirmResetSlowlogMessage')}
        onClose={() => {
          if (!resetBusy) setResetOpen(false);
        }}
        footer={
          <>
            <Button
              variant="secondary"
              className="h-8 px-3 text-xs"
              onClick={() => setResetOpen(false)}
              disabled={resetBusy}
            >
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              className="h-8 px-3 text-xs"
              onClick={() => void handleReset()}
              disabled={resetBusy}
            >
              {resetBusy ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                t('redis.slowlogReset')
              )}
            </Button>
          </>
        }
      >
        {resetError ? (
          <div className="rounded-md border border-danger/20 bg-danger/10 px-3 py-2 text-sm text-danger">
            {resetError}
          </div>
        ) : null}
      </Dialog>
    </>
  );
}

function MonitorPaneShell({
  loading,
  error,
  onRefresh,
  empty,
  emptyMessage,
  toolbar,
  children,
}: {
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  empty: boolean;
  emptyMessage: string;
  toolbar?: ReactNode;
  children: ReactNode;
}) {
  const { t } = useI18n();

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-3 py-2">
        <Button
          variant="secondary"
          className="h-7 gap-1 px-2 text-xs"
          onClick={onRefresh}
          disabled={loading}
        >
          {loading ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <RefreshCw className="h-3.5 w-3.5" />
          )}
          {t('redis.refresh')}
        </Button>
        {toolbar}
        <div className="flex-1" />
      </div>

      {error && (
        <div className="border-b border-danger/20 bg-danger/10 px-4 py-3 text-sm text-danger">
          {error}
        </div>
      )}

      {loading && !error && (
        <div className="flex flex-1 items-center justify-center gap-2 text-sm text-fg-muted">
          <Loader2 className="h-5 w-5 animate-spin" />
          {t('redis.monitorLoading')}
        </div>
      )}

      {!loading && !error && empty && (
        <div className="flex flex-1 items-center justify-center text-sm text-fg-muted">
          {emptyMessage}
        </div>
      )}

      {!loading && !error && !empty && (
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">{children}</div>
      )}
    </div>
  );
}
