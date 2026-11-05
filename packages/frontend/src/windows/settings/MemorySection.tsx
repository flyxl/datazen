import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Check, Download, Pencil, RefreshCw, Trash2, X } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import { useConnectionStore } from '../../stores/connectionStore';
import { aiCommands } from '../../commands/ai';
import { normalizeMemorySettings } from '../../lib/aiMemorySettings';
import type {
  AiMemoryEntry,
  AiMemoryKind,
  AiMemorySettings,
  AiMemoryStatus,
  AiMemoryStats,
  AiSettingsConfig,
} from '../../types';
import { SectionTitle, ToggleRow } from './settingsUi';

const KINDS: AiMemoryKind[] = [
  'preference',
  'glossary',
  'relationship',
  'pattern',
  'correction',
  'environment',
  'negative',
  'schema_note',
];

const STATUSES: AiMemoryStatus[] = ['candidate', 'active', 'rejected', 'archived', 'stale'];

const CLOUD_POLICIES = ['normal_only', 'allow_internal', 'disable_cloud'] as const;

const EMPTY_STATS: AiMemoryStats = { total: 0, byStatus: {}, byKind: {}, byScope: {} };

function downloadText(filename: string, content: string, mime: string): void {
  if (typeof URL.createObjectURL !== 'function' || navigator.userAgent.includes('jsdom')) return;
  const url = URL.createObjectURL(new Blob([content], { type: mime }));
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

export function MemorySection({ config }: { config: AiSettingsConfig }) {
  const { t } = useI18n();
  const saveSettingsConfig = useAiStore((s) => s.saveSettingsConfig);
  const connections = useConnectionStore((s) => s.connections);

  const [memory, setMemory] = useState<AiMemorySettings>(() =>
    normalizeMemorySettings(config.memory),
  );
  const [pendingEnable, setPendingEnable] = useState(false);
  const [entries, setEntries] = useState<AiMemoryEntry[]>([]);
  const [stats, setStats] = useState<AiMemoryStats>(EMPTY_STATS);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [search, setSearch] = useState('');
  const [connectionFilter, setConnectionFilter] = useState('');
  const [kindFilter, setKindFilter] = useState('');
  const [statusFilter, setStatusFilter] = useState('');
  const [selected, setSelected] = useState<string[]>([]);
  const [editing, setEditing] = useState<{ id: string; subject: string; content: string } | null>(
    null,
  );
  const [refreshToken, setRefreshToken] = useState(0);
  // A deleted row must never flash back if a host refresh races the delete or
  // a stale list response is still in flight.
  const deletedIds = useRef<Set<string>>(new Set());

  useEffect(() => {
    setMemory(normalizeMemorySettings(config.memory));
  }, [config.memory]);

  const refresh = useCallback(() => setRefreshToken((value) => value + 1), []);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void (async () => {
        setLoading(true);
        setError(null);
        try {
          const params = {
            status: statusFilter ? (statusFilter as AiMemoryStatus) : undefined,
            limit: 200,
          };
          const page = search.trim()
            ? await aiCommands.memorySearch({ ...params, query: search.trim() })
            : await aiCommands.memoryList({
                ...params,
                connectionId: connectionFilter || undefined,
                kind: kindFilter ? (kindFilter as AiMemoryKind) : undefined,
              });
          let nextEntries = (page?.entries ?? []).filter(
            (entry) => !deletedIds.current.has(entry.id),
          );
          // The T12 search IPC predates the connection/kind filters. Apply
          // them client-side so search and filters compose predictably.
          if (search.trim()) {
            if (connectionFilter) {
              nextEntries = nextEntries.filter((entry) => entry.connectionId === connectionFilter);
            }
            if (kindFilter) {
              nextEntries = nextEntries.filter((entry) => entry.kind === kindFilter);
            }
          }
          setEntries(nextEntries);
          setSelected((current) => current.filter((id) => nextEntries.some((e) => e.id === id)));
          setStats((await aiCommands.memoryStats()) ?? EMPTY_STATS);
        } catch (cause) {
          setError(cause instanceof Error ? cause.message : String(cause));
        } finally {
          setLoading(false);
        }
      })();
    }, 200);
    return () => window.clearTimeout(timer);
  }, [search, connectionFilter, kindFilter, statusFilter, refreshToken]);

  const candidateEntries = useMemo(
    () => entries.filter((entry) => entry.status === 'candidate'),
    [entries],
  );
  const candidateCount = stats.byStatus.candidate ?? 0;
  const selectedCandidates = candidateEntries.filter((entry) => selected.includes(entry.id));
  const allSelected = entries.length > 0 && entries.every((entry) => selected.includes(entry.id));

  const persist = async (next: AiMemorySettings) => {
    const previous = memory;
    setMemory(next);
    const ok = await saveSettingsConfig({ ...config, memory: next });
    if (!ok) setMemory(previous);
  };

  const toggleEnabled = (enabled: boolean) => {
    if (enabled && !memory.privacyAcknowledged) {
      setPendingEnable(true);
      return;
    }
    void persist({ ...memory, enabled });
  };

  const confirmEnable = () => {
    setPendingEnable(false);
    void persist({ ...memory, enabled: true, privacyAcknowledged: true });
  };

  const setCloudPolicy = (value: string) => {
    if (!CLOUD_POLICIES.includes(value as (typeof CLOUD_POLICIES)[number])) return;
    void persist({ ...memory, cloudPolicy: value as AiMemorySettings['cloudPolicy'] });
  };

  const toggleSelected = (id: string) => {
    setSelected((current) =>
      current.includes(id) ? current.filter((value) => value !== id) : [...current, id],
    );
  };

  const toggleAll = () => {
    setSelected(allSelected ? [] : entries.map((entry) => entry.id));
  };

  const updateRow = async (id: string, patch: Parameters<typeof aiCommands.memoryUpdate>[1]) => {
    await aiCommands.memoryUpdate(id, patch);
    setEditing(null);
    refresh();
  };

  const confirmAll = async () => {
    const ids = candidateEntries.map((entry) => entry.id);
    await Promise.all(ids.map((id) => aiCommands.memoryConfirm(id)));
    setSelected([]);
    refresh();
  };

  const deleteOne = async (id: string) => {
    await aiCommands.memoryDelete(id);
    deletedIds.current.add(id);
    setEntries((current) => current.filter((entry) => entry.id !== id));
    setSelected((current) => current.filter((value) => value !== id));
    refresh();
  };

  const deleteSelected = async () => {
    const ids = [...selected];
    await Promise.all(ids.map((id) => aiCommands.memoryDelete(id)));
    for (const id of ids) deletedIds.current.add(id);
    setEntries((current) => current.filter((entry) => !ids.includes(entry.id)));
    setSelected([]);
    refresh();
  };

  const mergeSelected = async () => {
    const chosen = candidateEntries.filter((entry) => selected.includes(entry.id));
    if (chosen.length < 2) return;
    const mergedContent = Array.from(new Set(chosen.map((entry) => entry.content.trim())))
      .filter(Boolean)
      .join('\n\n');
    await aiCommands.memoryUpdate(chosen[0].id, {
      subject: chosen[0].subject,
      content: mergedContent,
      confidence: Math.max(...chosen.map((entry) => entry.confidence)),
      status: 'active',
    });
    await Promise.all(chosen.slice(1).map((entry) => aiCommands.memoryDelete(entry.id)));
    for (const entry of chosen.slice(1)) deletedIds.current.add(entry.id);
    setSelected([]);
    refresh();
  };

  const exportMemories = async (format: 'json' | 'markdown') => {
    setNotice(t('settings.ai.memory.exportNotice'));
    const content = await aiCommands.memoryExport({
      format,
      status: statusFilter ? (statusFilter as AiMemoryStatus) : undefined,
    });
    downloadText(
      `datazen-memory.${format === 'json' ? 'json' : 'md'}`,
      content,
      format === 'json' ? 'application/json' : 'text/markdown',
    );
  };

  const connectionOptions = useMemo(() => {
    const known = new Map(connections.map((connection) => [connection.id, connection.name]));
    for (const entry of entries) {
      if (entry.connectionId && !known.has(entry.connectionId)) {
        known.set(entry.connectionId, entry.connectionId);
      }
    }
    return [...known.entries()];
  }, [connections, entries]);

  return (
    <div className="space-y-5" data-testid="ai-memory-settings">
      <div>
        <SectionTitle hint={t('settings.ai.memory.description')}>
          {t('settings.ai.memory.title')}
        </SectionTitle>
        <p className="mt-1 text-xs text-fg-muted">{t('settings.ai.memory.privacySummary')}</p>
      </div>

      <div className="space-y-3 rounded-xl border border-edge/80 bg-surface p-3">
        <ToggleRow
          label={t('settings.ai.memory.enable')}
          hint={t('settings.ai.memory.enableHint')}
          checked={memory.enabled}
          onChange={toggleEnabled}
          testId="ai-memory-enabled"
        />
        <ToggleRow
          label={t('settings.ai.memory.autoLearn')}
          hint={t('settings.ai.memory.autoLearnHint')}
          checked={memory.autoLearn}
          onChange={(next) => void persist({ ...memory, autoLearn: next })}
          testId="ai-memory-auto-learn"
        />
        <div className="flex items-center justify-between gap-4">
          <div className="text-sm text-fg-secondary">
            {t('settings.ai.memory.cloudPolicy')}
            <p className="mt-0.5 text-[11px] text-fg-muted">
              {t('settings.ai.memory.cloudPolicyHint')}
            </p>
          </div>
          <select
            className="rounded border border-edge bg-surface px-2 py-1 text-xs text-fg"
            value={memory.cloudPolicy}
            onChange={(event) => setCloudPolicy(event.target.value)}
            disabled={!memory.enabled}
            aria-label={t('settings.ai.memory.cloudPolicy')}
            data-testid="ai-memory-cloud-policy"
          >
            <option value="normal_only">{t('settings.ai.memory.cloudNormalOnly')}</option>
            <option value="allow_internal">{t('settings.ai.memory.cloudAllowInternal')}</option>
            <option value="disable_cloud">{t('settings.ai.memory.cloudDisable')}</option>
          </select>
        </div>

        {pendingEnable && (
          <div
            className="rounded border border-accent/30 bg-accent/5 p-3 text-xs text-fg-secondary"
            data-testid="ai-memory-first-use"
          >
            <p className="font-medium text-fg">{t('settings.ai.memory.firstUseTitle')}</p>
            <p className="mt-1 text-fg-muted">{t('settings.ai.memory.firstUseBody')}</p>
            <div className="mt-2 flex gap-2">
              <Button
                variant="primary"
                size="sm"
                onClick={confirmEnable}
                data-testid="ai-memory-first-use-confirm"
              >
                {t('settings.ai.memory.firstUseConfirm')}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setPendingEnable(false)}
                data-testid="ai-memory-first-use-cancel"
              >
                {t('common.cancel')}
              </Button>
            </div>
          </div>
        )}

        {!memory.enabled && (
          <p className="text-[11px] text-fg-muted" data-testid="ai-memory-disabled-note">
            {t('settings.ai.memory.disabledNote')}
          </p>
        )}
      </div>

      <div className="space-y-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <SectionTitle>{t('settings.ai.memory.listTitle')}</SectionTitle>
          <div className="flex items-center gap-1">
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void exportMemories('json')}
              data-testid="ai-memory-export-json"
            >
              <Download className="mr-1 h-3 w-3" />
              JSON
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void exportMemories('markdown')}
              data-testid="ai-memory-export-markdown"
            >
              <Download className="mr-1 h-3 w-3" />
              Markdown
            </Button>
          </div>
        </div>
        <p className="text-[11px] text-fg-muted" data-testid="ai-memory-export-notice">
          {t('settings.ai.memory.exportWarning')}
        </p>

        <div className="flex flex-wrap items-center gap-2">
          <input
            className="min-w-[180px] flex-1 rounded border border-edge bg-surface px-2 py-1.5 text-xs text-fg outline-none focus:border-accent"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={t('settings.ai.memory.searchPlaceholder')}
            aria-label={t('settings.ai.memory.searchPlaceholder')}
            data-testid="ai-memory-search"
          />
          <select
            className="rounded border border-edge bg-surface px-2 py-1.5 text-xs text-fg"
            value={connectionFilter}
            onChange={(event) => setConnectionFilter(event.target.value)}
            aria-label={t('settings.ai.memory.allConnections')}
            data-testid="ai-memory-filter-connection"
          >
            <option value="">{t('settings.ai.memory.allConnections')}</option>
            {connectionOptions.map(([id, name]) => (
              <option key={id} value={id}>
                {name}
              </option>
            ))}
          </select>
          <select
            className="rounded border border-edge bg-surface px-2 py-1.5 text-xs text-fg"
            value={kindFilter}
            onChange={(event) => setKindFilter(event.target.value)}
            aria-label={t('settings.ai.memory.allKinds')}
            data-testid="ai-memory-filter-kind"
          >
            <option value="">{t('settings.ai.memory.allKinds')}</option>
            {KINDS.map((kind) => (
              <option key={kind} value={kind}>
                {t(`settings.ai.memory.kind.${kind}`)}
              </option>
            ))}
          </select>
          <select
            className="rounded border border-edge bg-surface px-2 py-1.5 text-xs text-fg"
            value={statusFilter}
            onChange={(event) => setStatusFilter(event.target.value)}
            aria-label={t('settings.ai.memory.allStatuses')}
            data-testid="ai-memory-filter-status"
          >
            <option value="">{t('settings.ai.memory.allStatuses')}</option>
            {STATUSES.map((status) => (
              <option key={status} value={status}>
                {t(`settings.ai.memory.status.${status}`)}
              </option>
            ))}
          </select>
          <Button
            variant="ghost"
            size="sm"
            onClick={refresh}
            data-testid="ai-memory-refresh"
            aria-label={t('settings.ai.memory.refresh')}
          >
            <RefreshCw className="h-3 w-3" />
          </Button>
        </div>

        <div className="flex flex-wrap items-center gap-2 rounded border border-edge/70 bg-surface p-2">
          <span
            className="text-xs font-medium text-fg-secondary"
            data-testid="ai-memory-candidates-title"
          >
            {t('settings.ai.memory.candidatesTitle')}{' '}
            <span
              className="ml-1 rounded-full bg-accent/15 px-1.5 text-[10px] text-accent"
              data-testid="ai-memory-candidate-count"
            >
              {candidateCount}
            </span>
          </span>
          <Button
            variant="primary"
            size="sm"
            disabled={candidateEntries.length === 0}
            onClick={() => void confirmAll()}
            data-testid="ai-memory-confirm-all"
          >
            {t('settings.ai.memory.confirmAll', { count: candidateEntries.length })}
          </Button>
          {candidateEntries.length === 0 && candidateCount > 0 && (
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setStatusFilter('candidate')}
              data-testid="ai-memory-show-candidates"
            >
              {t('settings.ai.memory.showCandidates')}
            </Button>
          )}
          {candidateEntries.length >= 2 && (
            <Button
              variant="secondary"
              size="sm"
              disabled={selectedCandidates.length < 2}
              onClick={() => void mergeSelected()}
              data-testid="ai-memory-merge-selected"
            >
              {t('settings.ai.memory.mergeSelected', { count: selectedCandidates.length })}
            </Button>
          )}
        </div>

        {error && <p className="text-xs text-red-500">{error}</p>}
        {notice && (
          <p className="text-xs text-accent" data-testid="ai-memory-notice">
            {notice}
          </p>
        )}
        {loading && <p className="text-xs text-fg-muted">{t('common.loading')}</p>}

        {entries.length === 0 && !loading ? (
          <p className="text-xs text-fg-muted" data-testid="ai-memory-empty">
            {t('settings.ai.memory.empty')}
          </p>
        ) : (
          <div className="space-y-1.5" data-testid="ai-memory-list">
            <div className="flex items-center gap-2 px-1 text-[11px] text-fg-muted">
              <input
                type="checkbox"
                checked={allSelected}
                onChange={toggleAll}
                aria-label={t('settings.ai.memory.selectAll')}
                data-testid="ai-memory-select-all"
              />
              <span>{t('settings.ai.memory.selectAll')}</span>
              <Button
                variant="ghost"
                size="sm"
                disabled={selected.length === 0}
                onClick={() => void deleteSelected()}
                data-testid="ai-memory-delete-selected"
              >
                <Trash2 className="mr-1 h-3 w-3" />
                {t('settings.ai.memory.deleteSelected', { count: selected.length })}
              </Button>
            </div>
            {entries.map((entry) => (
              <div
                key={entry.id}
                className="rounded border border-edge/70 bg-surface p-2"
                data-testid={`ai-memory-row-${entry.id}`}
              >
                <div className="flex items-start gap-2">
                  <input
                    type="checkbox"
                    checked={selected.includes(entry.id)}
                    onChange={() => toggleSelected(entry.id)}
                    aria-label={entry.subject}
                    data-testid={`ai-memory-select-${entry.id}`}
                  />
                  <div className="min-w-0 flex-1">
                    {editing?.id === entry.id ? (
                      <div className="space-y-1" data-testid={`ai-memory-edit-form-${entry.id}`}>
                        <input
                          className="w-full rounded border border-edge bg-surface px-2 py-1 text-xs"
                          value={editing.subject}
                          onChange={(event) =>
                            setEditing({ ...editing, subject: event.target.value })
                          }
                          data-testid="ai-memory-edit-subject"
                        />
                        <textarea
                          className="min-h-16 w-full rounded border border-edge bg-surface px-2 py-1 text-xs"
                          value={editing.content}
                          onChange={(event) =>
                            setEditing({ ...editing, content: event.target.value })
                          }
                          data-testid="ai-memory-edit-content"
                        />
                        <div className="flex gap-1">
                          <Button
                            variant="primary"
                            size="sm"
                            onClick={() =>
                              void updateRow(entry.id, {
                                subject: editing.subject,
                                content: editing.content,
                              })
                            }
                            data-testid="ai-memory-edit-save"
                          >
                            {t('common.save')}
                          </Button>
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => setEditing(null)}
                            data-testid="ai-memory-edit-cancel"
                          >
                            {t('common.cancel')}
                          </Button>
                        </div>
                      </div>
                    ) : (
                      <>
                        <div className="flex flex-wrap items-center gap-1.5">
                          <span className="text-xs font-medium text-fg">{entry.subject}</span>
                          <span className="rounded bg-surface-alt px-1 text-[9px] text-fg-muted">
                            {entry.status}
                          </span>
                          <span className="rounded bg-surface-alt px-1 text-[9px] text-fg-muted">
                            {entry.kind}
                          </span>
                          <span className="text-[9px] text-fg-muted">
                            {entry.scopeType} · {entry.confidence.toFixed(2)} · {entry.source}
                          </span>
                        </div>
                        <p className="mt-1 whitespace-pre-wrap text-[11px] text-fg-muted">
                          {entry.content}
                        </p>
                      </>
                    )}
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    {entry.status === 'candidate' && (
                      <>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => void aiCommands.memoryConfirm(entry.id).then(refresh)}
                          aria-label={t('settings.ai.memory.confirm')}
                          data-testid={`ai-memory-confirm-${entry.id}`}
                        >
                          <Check className="h-3 w-3" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => void aiCommands.memoryReject(entry.id).then(refresh)}
                          aria-label={t('settings.ai.memory.reject')}
                          data-testid={`ai-memory-reject-${entry.id}`}
                        >
                          <X className="h-3 w-3" />
                        </Button>
                      </>
                    )}
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() =>
                        setEditing({
                          id: entry.id,
                          subject: entry.subject,
                          content: entry.content,
                        })
                      }
                      aria-label={t('settings.ai.memory.edit')}
                      data-testid={`ai-memory-edit-${entry.id}`}
                    >
                      <Pencil className="h-3 w-3" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => void deleteOne(entry.id)}
                      aria-label={t('settings.ai.memory.delete')}
                      data-testid={`ai-memory-delete-${entry.id}`}
                    >
                      <Trash2 className="h-3 w-3" />
                    </Button>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="rounded border border-edge/70 bg-surface-alt/40 p-3 text-[11px] text-fg-muted">
        <p className="font-medium text-fg-secondary">{t('settings.ai.memory.privacyTitle')}</p>
        <p className="mt-1">{t('settings.ai.memory.privacyBody')}</p>
      </div>
    </div>
  );
}

export default MemorySection;
