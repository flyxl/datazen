import { useState, useCallback, useMemo, useRef, useEffect } from 'react';
import { Plus, Copy, Edit2, Trash2, Code2, Download, Upload } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Badge } from '../../components/ui/Badge';
import { useI18n } from '../../hooks/useI18n';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { BUILTIN_SQL_SNIPPETS } from '../../components/sql-editor/snippets/builtinSnippets';
import type { SqlSnippetItem } from '../../components/sql-editor/snippets/types';
import type { AppSettings } from '../../types';
import { exportEditorSettings, importEditorSettings } from '../../lib/settingsExport';
import { SnippetEditDialog } from './SnippetEditDialog';
import { useSettingsStore } from '../../stores/settingsStore';

export interface SqlSnippetsCardProps {
  settings: AppSettings;
  onUpdateSnippets: (snippets: SqlSnippetItem[]) => Promise<void> | void;
}

export function SqlSnippetsCard({ settings, onUpdateSnippets }: Readonly<SqlSnippetsCardProps>) {
  const { t } = useI18n();
  const [confirmDelete, confirmDeleteDialog] = useConfirmDialog();
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  const [dialogState, setDialogState] = useState<{
    open: boolean;
    snippet: SqlSnippetItem | null;
    isDuplicate: boolean;
  }>({
    open: false,
    snippet: null,
    isDuplicate: false,
  });

  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const statusTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const showStatus = useCallback((msg: string) => {
    setStatusMessage(msg);
    if (statusTimerRef.current) {
      clearTimeout(statusTimerRef.current);
    }
    statusTimerRef.current = setTimeout(() => {
      setStatusMessage(null);
      statusTimerRef.current = null;
    }, 3000);
  }, []);

  useEffect(() => {
    return () => {
      if (statusTimerRef.current) {
        clearTimeout(statusTimerRef.current);
      }
    };
  }, []);

  const customSnippets = useMemo(() => settings.sqlSnippets ?? [], [settings.sqlSnippets]);

  const existingPrefixes = useMemo(() => {
    const allPrefixes = [
      ...BUILTIN_SQL_SNIPPETS.map((s) => s.prefix),
      ...customSnippets.map((s) => s.prefix),
    ];
    const ownPrefix =
      dialogState.snippet && !dialogState.isDuplicate ? dialogState.snippet.prefix : null;
    return ownPrefix ? allPrefixes.filter((p) => p !== ownPrefix) : allPrefixes;
  }, [customSnippets, dialogState.snippet, dialogState.isDuplicate]);

  const handleExport = useCallback(async () => {
    try {
      const json = exportEditorSettings(settings);
      await navigator.clipboard.writeText(json);
      showStatus(t('query.exportSettingsSuccess'));
    } catch (e) {
      showStatus(String(e));
    }
  }, [settings, showStatus, t]);

  const handleImport = useCallback(async () => {
    try {
      const text = await navigator.clipboard.readText();
      const imported = importEditorSettings(text);
      await updateSettings(imported);
      showStatus(t('query.importSettingsSuccess'));
    } catch {
      showStatus(t('query.importSettingsInvalid'));
    }
  }, [updateSettings, showStatus, t]);

  const handleSaveSnippet = useCallback(
    async (snippet: SqlSnippetItem) => {
      const existingIdx = customSnippets.findIndex((s) => s.id === snippet.id);
      let updated: SqlSnippetItem[];
      if (existingIdx >= 0) {
        updated = [...customSnippets];
        updated[existingIdx] = snippet;
      } else {
        updated = [...customSnippets, snippet];
      }
      await onUpdateSnippets(updated);
    },
    [customSnippets, onUpdateSnippets],
  );

  const handleDeleteSnippet = useCallback(
    async (snippet: SqlSnippetItem) => {
      const ok = await confirmDelete({
        title: t('query.snippets.deleteConfirmTitle'),
        message: t('query.snippets.deleteConfirmMessage', { prefix: snippet.prefix }),
        kind: 'warning',
      });
      if (!ok) return;

      const updated = customSnippets.filter((s) => s.id !== snippet.id);
      await onUpdateSnippets(updated);
    },
    [confirmDelete, customSnippets, onUpdateSnippets, t],
  );

  const resolveSnippetDesc = (descriptionKey: string) => {
    const translated = t(descriptionKey as Parameters<typeof t>[0]);
    return translated === descriptionKey ? descriptionKey : translated;
  };

  return (
    <div className="space-y-4 rounded-lg border border-edge bg-surface-alt p-3.5">
      {confirmDeleteDialog}

      {/* Header */}
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-center gap-2">
          <Code2 className="h-4 w-4 text-accent" />
          <span className="text-xs font-semibold uppercase tracking-wider text-fg">
            {t('query.snippets')}
          </span>
        </div>

        <div className="flex items-center gap-1.5 flex-wrap">
          <Button
            variant="primary"
            size="sm"
            className="h-7 gap-1 text-xs"
            onClick={() => setDialogState({ open: true, snippet: null, isDuplicate: false })}
          >
            <Plus className="h-3.5 w-3.5" />
            {t('query.snippets.add')}
          </Button>

          <Button
            variant="secondary"
            size="sm"
            className="h-7 gap-1 text-xs"
            onClick={handleExport}
            title={t('query.exportSettings')}
          >
            <Download className="h-3 w-3" />
            {t('query.exportSettings')}
          </Button>

          <Button
            variant="secondary"
            size="sm"
            className="h-7 gap-1 text-xs"
            onClick={handleImport}
            title={t('query.importSettings')}
          >
            <Upload className="h-3 w-3" />
            {t('query.importSettings')}
          </Button>
        </div>
      </div>

      {statusMessage && <p className="text-xs text-accent">{statusMessage}</p>}

      {/* Custom Snippets */}
      <div className="space-y-2">
        <div className="flex items-center justify-between text-[11px] font-medium text-fg-muted uppercase tracking-wider">
          <span>
            {t('query.snippets.custom')} ({customSnippets.length})
          </span>
        </div>

        {customSnippets.length === 0 ? (
          <div className="rounded border border-dashed border-edge/70 p-4 text-center text-xs text-fg-muted">
            {t('query.snippets.emptyCustom')}
          </div>
        ) : (
          <div className="divide-y divide-edge rounded border border-edge bg-surface">
            {customSnippets.map((item) => (
              <div
                key={item.id}
                className="flex items-center justify-between gap-3 p-2.5 hover:bg-surface-raised transition-colors"
              >
                <div className="flex items-center gap-2.5 min-w-0 flex-1">
                  <kbd className="min-w-14 rounded border border-edge bg-surface-raised px-1.5 py-0.5 text-center font-mono text-[11px] font-semibold text-accent">
                    {item.prefix}
                  </kbd>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5">
                      <span className="truncate text-xs font-medium text-fg">
                        {resolveSnippetDesc(item.descriptionKey)}
                      </span>
                      <Badge tone="accent" className="px-1.5 py-0 text-[10px]">
                        {t('query.snippets.custom')}
                      </Badge>
                    </div>
                    <p
                      className="truncate font-mono text-[11px] text-fg-muted"
                      title={item.template}
                    >
                      {item.template.replace(/\n/g, ' ')}
                    </p>
                  </div>
                </div>

                <div className="flex items-center gap-1 shrink-0">
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 w-7 p-0 text-fg-secondary hover:text-fg"
                    onClick={() =>
                      setDialogState({ open: true, snippet: item, isDuplicate: false })
                    }
                    title={t('query.snippets.edit')}
                  >
                    <Edit2 className="h-3.5 w-3.5" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 w-7 p-0 text-destructive/80 hover:text-destructive"
                    data-testid={`delete-snippet-${item.id}`}
                    onClick={() => handleDeleteSnippet(item)}
                    title={t('common.delete')}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Builtin Snippets */}
      <div className="space-y-2 pt-2">
        <div className="flex items-center justify-between text-[11px] font-medium text-fg-muted uppercase tracking-wider">
          <span>
            {t('query.snippets.builtin')} ({BUILTIN_SQL_SNIPPETS.length})
          </span>
        </div>

        <div className="divide-y divide-edge rounded border border-edge bg-surface">
          {BUILTIN_SQL_SNIPPETS.map((item) => (
            <div
              key={item.id}
              className="flex items-center justify-between gap-3 p-2.5 hover:bg-surface-raised transition-colors"
            >
              <div className="flex items-center gap-2.5 min-w-0 flex-1">
                <kbd className="min-w-14 rounded border border-edge bg-surface-raised px-1.5 py-0.5 text-center font-mono text-[11px] font-semibold text-fg">
                  {item.prefix}
                </kbd>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5">
                    <span className="truncate text-xs font-medium text-fg">
                      {resolveSnippetDesc(item.descriptionKey)}
                    </span>
                    <Badge tone="neutral" className="px-1.5 py-0 text-[10px] text-fg-muted">
                      {t('query.snippets.builtin')}
                    </Badge>
                  </div>
                  <p className="truncate font-mono text-[11px] text-fg-muted" title={item.template}>
                    {item.template.replace(/\n/g, ' ')}
                  </p>
                </div>
              </div>

              <div className="flex items-center gap-1 shrink-0">
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 px-2 text-xs gap-1 text-fg-secondary hover:text-fg"
                  onClick={() => setDialogState({ open: true, snippet: item, isDuplicate: true })}
                  title={t('query.snippets.duplicate')}
                >
                  <Copy className="h-3 w-3" />
                  <span className="hidden sm:inline">{t('query.snippets.duplicate')}</span>
                </Button>
              </div>
            </div>
          ))}
        </div>
      </div>

      <SnippetEditDialog
        open={dialogState.open}
        snippet={dialogState.snippet}
        isDuplicate={dialogState.isDuplicate}
        existingPrefixes={existingPrefixes}
        onClose={() => setDialogState({ open: false, snippet: null, isDuplicate: false })}
        onSave={handleSaveSnippet}
      />
    </div>
  );
}
