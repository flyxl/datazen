import { useEffect, useMemo, useState } from 'react';
import { Check, Clock, Copy, ExternalLink, Search, Trash2 } from 'lucide-react';
import { Dialog } from '../ui/Dialog';
import { Button } from '../ui/Button';
import { Select } from '../ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { useConnectionStore } from '../../stores/connectionStore';
import { queryCommands } from '../../commands/query';
import type { QueryHistoryEntry } from '../../types';
import { cn } from '../../lib/cn';
import { tid } from '../../lib/tid';

export interface GlobalQueryHistoryDialogProps {
  open: boolean;
  onClose: () => void;
  onSelectQuery?: (entry: QueryHistoryEntry) => void;
  initialConnectionId?: string;
}

export function GlobalQueryHistoryDialog({
  open,
  onClose,
  onSelectQuery,
  initialConnectionId,
}: Readonly<GlobalQueryHistoryDialogProps>) {
  const { t } = useI18n();
  const connections = useConnectionStore((s) => s.connections);

  const [history, setHistory] = useState<QueryHistoryEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [search, setSearch] = useState('');
  const [selectedConn, setSelectedConn] = useState<string>(initialConnectionId ?? 'all');
  const [statusFilter, setStatusFilter] = useState<'all' | 'success' | 'failed'>('all');
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);

  const connectionMap = useMemo(() => {
    return new Map(connections.map((c) => [c.id, c.name]));
  }, [connections]);

  useEffect(() => {
    if (!open) return;
    setLoading(true);
    queryCommands
      .getQueryHistory(200)
      .then((entries) => {
        setHistory(Array.isArray(entries) ? entries : []);
      })
      .catch(() => {
        setHistory([]);
      })
      .finally(() => {
        setLoading(false);
      });
  }, [open]);

  useEffect(() => {
    if (initialConnectionId) {
      setSelectedConn(initialConnectionId);
    }
  }, [initialConnectionId]);

  const filteredHistory = useMemo(() => {
    let result = history;
    if (selectedConn !== 'all') {
      result = result.filter((item) => item.connectionId === selectedConn);
    }
    if (statusFilter === 'success') {
      result = result.filter((item) => item.success);
    } else if (statusFilter === 'failed') {
      result = result.filter((item) => !item.success);
    }
    if (search.trim()) {
      const q = search.trim().toLowerCase();
      result = result.filter(
        (item) =>
          item.sql.toLowerCase().includes(q) ||
          (item.database && item.database.toLowerCase().includes(q)),
      );
    }
    return result;
  }, [history, selectedConn, statusFilter, search]);

  const handleCopy = (id: string, sql: string) => {
    void navigator.clipboard?.writeText(sql);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  const handleClearHistory = async () => {
    try {
      await queryCommands.clearQueryHistory();
      setHistory([]);
      setConfirmClear(false);
    } catch {
      // safe fallback
    }
  };

  return (
    <Dialog
      open={open}
      title={t('query.historyTitle') || '查询历史'}
      onClose={onClose}
      className="max-w-3xl"
      testId="global-query-history-dialog"
    >
      <div className="flex flex-col gap-3 min-h-[420px] max-h-[70vh]">
        {/* Filter bar */}
        <div className="flex flex-wrap items-center gap-2 border-b border-edge pb-3">
          <div className="relative flex-1 min-w-[200px]">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-4 w-4 text-fg-muted" />
            <input
              type="text"
              placeholder={t('common.search') || '搜索 SQL...'}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="h-8 w-full rounded-md border border-edge bg-surface pl-8 pr-3 text-xs text-fg outline-none focus:border-accent"
              {...tid('global-history-search')}
            />
          </div>

          <div className="w-36">
            <Select
              value={selectedConn}
              onChange={(val) => setSelectedConn(val)}
              className="h-8 text-xs"
              options={[
                { value: 'all', label: t('query.historyScopeAll') || '全部连接' },
                ...connections.map((c) => ({ value: c.id, label: c.name })),
              ]}
            />
          </div>

          <div className="w-28">
            <Select
              value={statusFilter}
              onChange={(val) => setStatusFilter(val as 'all' | 'success' | 'failed')}
              className="h-8 text-xs"
              options={[
                { value: 'all', label: '全部状态' },
                { value: 'success', label: '仅成功' },
                { value: 'failed', label: '仅失败' },
              ]}
            />
          </div>

          {history.length > 0 && (
            <div className="ml-auto">
              {confirmClear ? (
                <div className="flex items-center gap-1">
                  <span className="text-[11px] text-danger">确定清空?</span>
                  <Button
                    variant="danger"
                    size="sm"
                    className="h-7 px-2 text-xs"
                    onClick={() => void handleClearHistory()}
                  >
                    确认
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 px-2 text-xs"
                    onClick={() => setConfirmClear(false)}
                  >
                    取消
                  </Button>
                </div>
              ) : (
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-8 px-2 text-xs text-fg-muted hover:text-danger"
                  onClick={() => setConfirmClear(true)}
                  title="清空历史记录"
                >
                  <Trash2 className="h-3.5 w-3.5 mr-1" />
                  清空
                </Button>
              )}
            </div>
          )}
        </div>

        {/* Content list */}
        <div className="flex-1 overflow-y-auto space-y-2 pr-1">
          {loading ? (
            <div className="flex h-48 items-center justify-center text-xs text-fg-muted">
              加载中...
            </div>
          ) : filteredHistory.length === 0 ? (
            <div className="flex h-48 flex-col items-center justify-center text-center text-fg-muted">
              <Clock className="h-8 w-8 opacity-30 mb-2" />
              <p className="text-xs">
                {history.length === 0 ? '暂无查询历史' : '无匹配的历史记录'}
              </p>
            </div>
          ) : (
            filteredHistory.map((item) => {
              const connName = connectionMap.get(item.connectionId) || item.connectionId;
              return (
                <div
                  key={item.id}
                  className="group flex flex-col gap-1.5 rounded-lg border border-edge/80 bg-surface-alt p-3 transition-colors hover:border-accent/40 hover:bg-surface-raised"
                  {...tid('global-history-item')}
                >
                  <div className="flex items-center justify-between text-[11px] text-fg-muted">
                    <div className="flex items-center gap-2">
                      <span
                        className={cn(
                          'h-1.5 w-1.5 rounded-full shrink-0',
                          item.success ? 'bg-success' : 'bg-danger',
                        )}
                      />
                      <span className="font-semibold text-fg-secondary">{connName}</span>
                      <span>·</span>
                      <span className="text-fg-muted">{item.database || 'default'}</span>
                      <span>·</span>
                      <span>{item.executionTimeMs}ms</span>
                      {item.rowsAffected != null && (
                        <>
                          <span>·</span>
                          <span>{item.rowsAffected} 行</span>
                        </>
                      )}
                      <span>·</span>
                      <span className="text-[10px] text-fg-muted/70">
                        {new Date(item.executedAt).toLocaleString()}
                      </span>
                    </div>

                    <div className="flex items-center gap-1.5 opacity-90 group-hover:opacity-100">
                      <button
                        type="button"
                        onClick={() => handleCopy(item.id, item.sql)}
                        className="flex items-center gap-1 rounded px-2 py-0.5 text-[11px] text-fg-muted hover:bg-surface hover:text-accent transition-colors"
                        title="复制 SQL"
                      >
                        {copiedId === item.id ? (
                          <>
                            <Check className="h-3 w-3 text-success" />
                            <span className="text-success">已复制</span>
                          </>
                        ) : (
                          <>
                            <Copy className="h-3 w-3" />
                            <span>复制</span>
                          </>
                        )}
                      </button>

                      {onSelectQuery && (
                        <button
                          type="button"
                          onClick={() => {
                            onSelectQuery(item);
                            onClose();
                          }}
                          className="flex items-center gap-1 rounded bg-accent/10 px-2 py-0.5 text-[11px] font-medium text-accent hover:bg-accent/20 transition-colors"
                          title="在查询面板中打开"
                        >
                          <ExternalLink className="h-3 w-3" />
                          <span>在连接中打开</span>
                        </button>
                      )}
                    </div>
                  </div>

                  <pre className="max-h-24 overflow-x-auto rounded bg-surface/70 p-2 font-mono text-xs text-fg-secondary whitespace-pre-wrap break-all">
                    {item.sql}
                  </pre>

                  {item.errorMessage && (
                    <div className="text-[11px] text-danger bg-danger/10 rounded px-2 py-1 truncate">
                      {item.errorMessage}
                    </div>
                  )}
                </div>
              );
            })
          )}
        </div>
      </div>
    </Dialog>
  );
}
