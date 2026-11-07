import { useEffect, useState } from 'react';
import { Button } from '../ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { aiCommands } from '../../commands/ai';

const MAX_NOTE_BYTES = 4 * 1024;

export function ConnectionAiNotes({ connectionId }: { connectionId: string }) {
  const { t } = useI18n();
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError('');
    void (async () => {
      try {
        // Keep the host call inside the async body: `legacyInvoke` throws
        // synchronously when no platform client is bound (unit tests), and a
        // thrown effect would take the dialog down instead of showing an error.
        const value = await aiCommands.connectionNotesGet(connectionId);
        if (!cancelled) setContent(value ?? '');
      } catch (cause) {
        if (!cancelled) setError(cause instanceof Error ? cause.message : String(cause));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [connectionId]);

  const bytes = new TextEncoder().encode(content).length;
  const overLimit = bytes > MAX_NOTE_BYTES;

  const handleSave = async () => {
    if (overLimit) return;
    setSaving(true);
    setError('');
    try {
      await aiCommands.connectionNotesSet(connectionId, content);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="mt-6 border-t border-edge/60 pt-4" data-testid="connection-ai-notes">
      <div className="mb-1 text-[11px] font-semibold uppercase tracking-wider text-fg-muted">
        {t('connection.notes.title')}
      </div>
      <p className="mb-2 text-[11px] text-fg-muted">{t('connection.notes.hint')}</p>
      <textarea
        data-testid="connection-notes-input"
        className="min-h-[80px] w-full resize-y rounded border border-edge bg-surface px-2 py-1.5 text-sm text-fg outline-none focus:border-accent disabled:opacity-60"
        value={content}
        disabled={loading}
        onChange={(event) => setContent(event.target.value)}
        placeholder={t('connection.notes.placeholder')}
      />
      <div className="mt-1 flex items-center justify-between gap-2">
        <span
          className={`text-[11px] ${overLimit ? 'text-danger' : 'text-fg-muted'}`}
          data-testid="connection-notes-count"
        >
          {bytes} / {MAX_NOTE_BYTES} bytes
          {overLimit && ` · ${t('connection.notes.overLimit')}`}
        </span>
        <Button
          variant="secondary"
          className="h-7 px-2 text-xs"
          disabled={saving || loading || overLimit}
          onClick={() => void handleSave()}
          data-testid="connection-notes-save"
        >
          {saving ? t('settings.ai.saving') : saved ? t('common.success') : t('common.save')}
        </Button>
      </div>
      {error && <p className="mt-1 text-[11px] text-danger">{error}</p>}
    </div>
  );
}
