import { useEffect } from 'react';
import { FileJson, FileText, Plus, Trash2, X } from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { useAiStore } from '../../stores/aiStore';
import { cn } from '../../lib/cn';
import { Button } from '../ui/Button';

interface AiSessionHistoryProps {
  open: boolean;
  onClose: () => void;
}

/** Navigable conversation history for the current connection/database. */
export function AiSessionHistory({ open, onClose }: AiSessionHistoryProps) {
  const { t } = useI18n();
  const sessions = useAiStore((s) => s.chatSessions);
  const current = useAiStore((s) => s.chatSession);
  const loading = useAiStore((s) => s.sessionHistoryLoading);
  const historyError = useAiStore((s) => s.sessionHistoryError);
  const migrationError = useAiStore((s) => s.sessionMigrationError);
  const migrationSuccess = useAiStore((s) => s.sessionMigrationSuccess);
  const refresh = useAiStore((s) => s.refreshChatSessions);
  const selectSession = useAiStore((s) => s.selectChatSession);
  const startNew = useAiStore((s) => s.newChatSession);
  const deleteSession = useAiStore((s) => s.deleteChatSession);
  const exportSession = useAiStore((s) => s.exportChatSession);

  useEffect(() => {
    if (open) void refresh();
  }, [open, refresh]);

  if (!open) return null;

  return (
    <div
      className="shrink-0 border-b border-edge bg-surface-alt/40"
      data-testid="ai-session-history"
    >
      <div className="flex items-center gap-2 px-3 py-1.5">
        <span className="text-[11px] font-medium text-fg-secondary">
          {t('chat.sessions.title')}
        </span>
        <Button
          variant="ghost"
          className="ml-auto h-6 gap-1 px-1.5 text-[11px]"
          data-testid="ai-session-new"
          onClick={() => {
            startNew();
            onClose();
          }}
        >
          <Plus className="h-3 w-3" />
          {t('chat.sessions.new')}
        </Button>
        <Button
          variant="ghost"
          className="h-6 px-1.5"
          aria-label={t('chat.sessions.close')}
          onClick={onClose}
        >
          <X className="h-3 w-3" />
        </Button>
      </div>

      {migrationSuccess > 0 && !migrationError && (
        <div
          className="mx-3 mb-1 rounded border border-success/30 bg-success/5 px-2 py-1 text-[10px] text-success"
          data-testid="ai-session-migration-success"
        >
          {t('chat.sessions.migrationSuccess', { count: migrationSuccess })}
        </div>
      )}
      {migrationError && (
        <div
          className="mx-3 mb-1 rounded border border-danger/30 bg-danger/5 px-2 py-1 text-[10px] text-danger"
          data-testid="ai-session-migration-error"
        >
          {migrationError}
        </div>
      )}
      {historyError && (
        <div
          className="mx-3 mb-1 rounded border border-danger/30 bg-danger/5 px-2 py-1 text-[10px] text-danger"
          data-testid="ai-session-history-error"
        >
          {historyError}
        </div>
      )}

      <div className="max-h-56 overflow-y-auto px-2 pb-1">
        {loading && sessions.length === 0 && (
          <p className="px-1 py-2 text-[11px] text-fg-muted">{t('chat.sessions.loading')}</p>
        )}
        {!loading && sessions.length === 0 && (
          <p className="px-1 py-2 text-[11px] text-fg-muted">{t('chat.sessions.empty')}</p>
        )}
        {sessions.map((session) => {
          const active = session.id === current?.id;
          return (
            <div
              key={session.id}
              data-testid="ai-session-row"
              data-session-id={session.id}
              className={cn(
                'group flex items-center gap-1 rounded px-1.5 py-1',
                active ? 'bg-accent/10' : 'hover:bg-surface-alt',
              )}
            >
              <button
                type="button"
                className="min-w-0 flex-1 text-left"
                aria-current={active ? 'true' : undefined}
                onClick={() => {
                  void selectSession(session.id);
                  onClose();
                }}
              >
                <span
                  className={cn(
                    'block truncate text-[11px]',
                    active ? 'font-medium text-accent' : 'text-fg-secondary',
                  )}
                >
                  {session.title || session.preview || t('chat.sessions.untitled')}
                </span>
                <span className="block truncate text-[10px] text-fg-muted">
                  {t('chat.sessions.messages', { count: session.messageCount })}
                  {session.preview ? ` · ${session.preview}` : ''}
                </span>
              </button>
              <Button
                variant="ghost"
                className="h-5 px-1 text-fg-muted hover:text-danger"
                aria-label={t('chat.sessions.delete')}
                title={t('chat.sessions.delete')}
                onClick={() => void deleteSession(session.id)}
              >
                <Trash2 className="h-3 w-3" />
              </Button>
            </div>
          );
        })}
      </div>

      <div className="flex items-center gap-1 border-t border-edge/60 px-3 py-1">
        <span className="text-[10px] text-fg-muted">{t('chat.sessions.export')}</span>
        <Button
          variant="ghost"
          className="ml-auto h-6 gap-1 px-1.5 text-[10px]"
          data-testid="ai-session-export-json"
          disabled={!current?.messages.length}
          onClick={() => void exportSession('json')}
        >
          <FileJson className="h-3 w-3" />
          JSON
        </Button>
        <Button
          variant="ghost"
          className="h-6 gap-1 px-1.5 text-[10px]"
          data-testid="ai-session-export-markdown"
          disabled={!current?.messages.length}
          onClick={() => void exportSession('markdown')}
        >
          <FileText className="h-3 w-3" />
          Markdown
        </Button>
      </div>
    </div>
  );
}
