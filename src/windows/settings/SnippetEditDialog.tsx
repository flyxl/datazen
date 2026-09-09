import { useState, useEffect } from 'react';
import { Dialog } from '../../components/ui/Dialog';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import type { SqlSnippetItem } from '../../components/sql-editor/snippets/types';

export interface SnippetEditDialogProps {
  open: boolean;
  snippet: SqlSnippetItem | null;
  isDuplicate?: boolean;
  existingPrefixes?: string[];
  onClose: () => void;
  onSave: (snippet: SqlSnippetItem) => void;
}

const PREFIX_REGEX = /^[A-Za-z_][A-Za-z0-9_*]*$/;

export function SnippetEditDialog({
  open,
  snippet,
  isDuplicate = false,
  existingPrefixes = [],
  onClose,
  onSave,
}: Readonly<SnippetEditDialogProps>) {
  const { t } = useI18n();

  const [prefix, setPrefix] = useState('');
  const [description, setDescription] = useState('');
  const [template, setTemplate] = useState('');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      if (snippet) {
        setPrefix(isDuplicate ? `${snippet.prefix}_copy` : snippet.prefix);
        const resolvedDesc = t(snippet.descriptionKey as Parameters<typeof t>[0]);
        setDescription(
          resolvedDesc === snippet.descriptionKey ? snippet.descriptionKey : resolvedDesc,
        );
        setTemplate(snippet.template);
      } else {
        setPrefix('');
        setDescription('');
        setTemplate('');
      }
      setError(null);
    }
  }, [open, snippet, isDuplicate, t]);

  if (!open) return null;

  const handleSave = () => {
    const trimmedPrefix = prefix.trim();
    const trimmedTemplate = template.trim();

    if (!trimmedPrefix || !PREFIX_REGEX.test(trimmedPrefix)) {
      setError(t('query.snippets.prefixInvalid'));
      return;
    }

    const isDuplicatePrefix = existingPrefixes.some(
      (p) => p.toLowerCase() === trimmedPrefix.toLowerCase(),
    );
    if (isDuplicatePrefix) {
      setError(t('query.snippets.prefixDuplicate'));
      return;
    }

    if (!trimmedTemplate) {
      setError(t('query.snippets.templateRequired'));
      return;
    }

    onSave({
      id: snippet && !isDuplicate ? snippet.id : crypto.randomUUID(),
      prefix: trimmedPrefix,
      descriptionKey: description.trim() || trimmedPrefix,
      template: trimmedTemplate,
    });
    onClose();
  };

  const title = isDuplicate
    ? t('query.snippets.duplicate')
    : snippet
      ? t('query.snippets.edit')
      : t('query.snippets.add');

  return (
    <Dialog
      open={open}
      title={title}
      onClose={onClose}
      className="max-w-xl"
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button variant="primary" onClick={handleSave}>
            {t('common.save')}
          </Button>
        </div>
      }
    >
      <div className="space-y-4 py-2">
        {error && (
          <div className="rounded-md bg-destructive/15 p-2 text-xs text-destructive">{error}</div>
        )}

        <div className="space-y-1.5">
          <label className="text-xs font-medium text-fg-secondary">
            {t('query.snippets.prefix')} <span className="text-destructive">*</span>
          </label>
          <input
            type="text"
            value={prefix}
            onChange={(e) => {
              setPrefix(e.target.value);
              setError(null);
            }}
            placeholder={t('query.snippets.prefixPlaceholder')}
            className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/25"
          />
          <p className="text-[11px] text-fg-muted">{t('query.snippets.prefixHint')}</p>
        </div>

        <div className="space-y-1.5">
          <label className="text-xs font-medium text-fg-secondary">
            {t('query.snippets.description')}
          </label>
          <input
            type="text"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder={t('query.snippets.descriptionPlaceholder')}
            className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/25"
          />
        </div>

        <div className="space-y-1.5">
          <label className="text-xs font-medium text-fg-secondary">
            {t('query.snippets.template')} <span className="text-destructive">*</span>
          </label>
          <textarea
            value={template}
            onChange={(e) => {
              setTemplate(e.target.value);
              setError(null);
            }}
            placeholder={t('query.snippets.templatePlaceholder')}
            rows={6}
            className="w-full resize-y rounded-md border border-edge bg-surface p-2.5 font-mono text-xs text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/25"
          />
          <div className="rounded border border-edge bg-surface-alt/70 p-2 text-[11px] text-fg-muted">
            <span className="font-semibold text-fg-secondary">
              {t('query.snippets.syntaxGuideTitle')}{' '}
            </span>
            {t('query.snippets.syntaxGuide')}
          </div>
        </div>
      </div>
    </Dialog>
  );
}
