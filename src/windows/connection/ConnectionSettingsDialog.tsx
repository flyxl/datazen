import { useCallback, useEffect, useState } from 'react';
import { Dialog } from '../../components/ui/Dialog';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useSettingsStore } from '../../stores/settingsStore';
import type { AppSettings } from '../../types';
import { useI18n } from '../../hooks/useI18n';

interface ConnectionSettingsDialogProps {
  open: boolean;
  onClose: () => void;
}

const PAGE_SIZE_OPTIONS = [25, 50, 100, 200, 500];
const RESULT_LIMIT_OPTIONS = [1000, 2000, 5000, 10000, 50000];

export function ConnectionSettingsDialog({ open, onClose }: ConnectionSettingsDialogProps) {
  const { t } = useI18n();
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  const [draft, setDraft] = useState<AppSettings>(settings);

  useEffect(() => {
    if (open) setDraft(settings);
  }, [open, settings]);

  const handleSave = useCallback(async () => {
    await updateSettings(draft);
    onClose();
  }, [draft, updateSettings, onClose]);

  const updateField = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setDraft((prev) => ({ ...prev, [key]: value }));
  };

  return (
    <Dialog
      open={open}
      title={t('connSettings.title')}
      description={t('connSettings.description')}
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>{t('common.cancel')}</Button>
          <Button variant="primary" onClick={() => void handleSave()}>{t('common.save')}</Button>
        </>
      }
    >
      <div className="space-y-5">
        {/* Theme */}
        <div>
          <label className="mb-1 block text-xs font-medium text-fg-secondary">{t('settings.theme')}</label>
          <Select
            value={draft.theme}
            options={[
              { value: 'light', label: t('menu.themeLight') },
              { value: 'dark', label: t('menu.themeDark') },
              { value: 'system', label: t('menu.themeSystem') },
            ]}
            onChange={(v) => updateField('theme', v as AppSettings['theme'])}
          />
        </div>

        {/* Default page size */}
        <div>
          <label className="mb-1 block text-xs font-medium text-fg-secondary">{t('settings.defaultPageSize')}</label>
          <Select
            value={draft.defaultPageSize}
            options={PAGE_SIZE_OPTIONS.map((v) => ({ value: String(v), label: `${v} ${t('common.rows')}` }))}
            onChange={(v) => updateField('defaultPageSize', Number(v))}
          />
        </div>

        {/* Limit SELECT results toggle */}
        <div className="flex items-center justify-between">
          <div>
            <label className="text-xs font-medium text-fg-secondary">{t('settings.limitSelect')}</label>
            <p className="text-[11px] text-fg-muted">{t('settings.limitSelectHint')}</p>
          </div>
          <input
            type="checkbox"
            checked={draft.limitSelectResults}
            onChange={(e) => updateField('limitSelectResults', e.target.checked)}
            className="h-4 w-4 accent-accent"
          />
        </div>

        {/* Query result limit */}
        {draft.limitSelectResults && (
          <div>
            <label className="mb-1 block text-xs font-medium text-fg-secondary">{t('settings.maxRows')}</label>
            <Select
              value={draft.queryResultLimit}
              options={RESULT_LIMIT_OPTIONS.map((v) => ({ value: String(v), label: `${v.toLocaleString()} ${t('common.rows')}` }))}
              onChange={(v) => updateField('queryResultLimit', Number(v))}
            />
          </div>
        )}

        {/* Editor font size */}
        <div>
          <label className="mb-1 block text-xs font-medium text-fg-secondary">{`${t('settings.editor')}${t('settings.fontSize')}`}</label>
          <div className="flex items-center gap-2">
            <input
              type="range"
              min={10}
              max={24}
              step={1}
              value={draft.editorFontSize}
              onChange={(e) => updateField('editorFontSize', Number(e.target.value))}
              className="flex-1 accent-accent"
            />
            <span className="w-10 text-right text-xs text-fg-secondary">{draft.editorFontSize}px</span>
          </div>
        </div>

        {/* Editor font family */}
        <div>
          <label className="mb-1 block text-xs font-medium text-fg-secondary">{`${t('settings.editor')}${t('settings.fontFamily')}`}</label>
          <input
            type="text"
            value={draft.editorFontFamily}
            onChange={(e) => updateField('editorFontFamily', e.target.value)}
            className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-500/25"
          />
        </div>

        {/* Confirm on delete */}
        <div className="flex items-center justify-between">
          <div>
            <label className="text-xs font-medium text-fg-secondary">{t('settings.confirmDelete')}</label>
            <p className="text-[11px] text-fg-muted">{t('settings.confirmDeleteHint')}</p>
          </div>
          <input
            type="checkbox"
            checked={draft.confirmOnDelete}
            onChange={(e) => updateField('confirmOnDelete', e.target.checked)}
            className="h-4 w-4 accent-accent"
          />
        </div>

        {/* Auto commit */}
        <div className="flex items-center justify-between">
          <div>
            <label className="text-xs font-medium text-fg-secondary">{t('settings.autoCommit')}</label>
            <p className="text-[11px] text-fg-muted">{t('settings.autoCommitHint')}</p>
          </div>
          <input
            type="checkbox"
            checked={draft.autoCommit}
            onChange={(e) => updateField('autoCommit', e.target.checked)}
            className="h-4 w-4 accent-accent"
          />
        </div>
      </div>
    </Dialog>
  );
}
