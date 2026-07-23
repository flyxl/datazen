import { useCallback, useEffect, useState } from 'react';
import { TitleBar } from '../../components/TitleBar';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useSettingsStore } from '../../stores/settingsStore';
import { useThemeListener } from '../../hooks/useThemeListener';
import { useI18n } from '../../hooks/useI18n';
import type { AppSettings } from '../../types';
import type { TranslationKey } from '../../locales';

const PAGE_SIZE_OPTIONS = [25, 50, 100, 200, 500];
const RESULT_LIMIT_OPTIONS = [1000, 2000, 5000, 10000, 50000];

const THEME_KEYS: { value: AppSettings['theme']; key: TranslationKey }[] = [
  { value: 'light', key: 'theme.light' },
  { value: 'dark', key: 'theme.dark' },
  { value: 'system', key: 'theme.system' },
];

const LANGUAGE_OPTIONS = [
  { value: 'en', label: '🇬🇧 English' },
  { value: 'zh-CN', label: '🇨🇳 简体中文' },
];

export function SettingsWindow() {
  useThemeListener();
  const { t } = useI18n();

  const settings = useSettingsStore((s) => s.settings);
  const loadSettings = useSettingsStore((s) => s.loadSettings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  const [draft, setDraft] = useState<AppSettings>(settings);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  useEffect(() => {
    setDraft(settings);
  }, [settings]);

  const updateField = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setDraft((prev) => ({ ...prev, [key]: value }));
    setSaved(false);
  };

  const handleSave = useCallback(async () => {
    await updateSettings(draft);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  }, [draft, updateSettings]);

  const handleClose = useCallback(async () => {
    if (!('__TAURI_INTERNALS__' in globalThis)) return;
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().close();
  }, []);

  const isDirty = JSON.stringify(draft) !== JSON.stringify(settings);

  const themeOptions = THEME_KEYS.map((tk) => ({
    value: tk.value,
    label: t(tk.key),
  }));

  return (
    <div className="flex h-screen flex-col bg-surface text-fg">
      {/* Title bar */}
      <TitleBar title={t('win.settings')} />

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-8 py-6">
        <h1 className="mb-6 text-lg font-semibold text-fg">{t('settings.title')}</h1>

        <div className="mx-auto max-w-lg space-y-6">
          {/* Language */}
          <SettingRow label={t('settings.language')}>
            <Select
              value={draft.language}
              options={LANGUAGE_OPTIONS}
              onChange={(v) => updateField('language', v)}
            />
          </SettingRow>

          {/* Theme */}
          <SettingRow label={t('settings.theme')}>
            <Select
              value={draft.theme}
              options={themeOptions}
              onChange={(v) => updateField('theme', v as AppSettings['theme'])}
            />
          </SettingRow>

          <Divider />

          {/* Data browsing section */}
          <SectionTitle>{t('settings.dataBrowsing')}</SectionTitle>

          <SettingRow label={t('settings.defaultPageSize')}>
            <Select
              value={draft.defaultPageSize}
              options={PAGE_SIZE_OPTIONS.map((v) => ({ value: String(v), label: `${v} ${t('common.rows')}` }))}
              onChange={(v) => updateField('defaultPageSize', Number(v))}
            />
          </SettingRow>

          <ToggleRow
            label={t('settings.limitSelect')}
            checked={draft.limitSelectResults}
            onChange={(v) => updateField('limitSelectResults', v)}
          />

          {draft.limitSelectResults && (
            <SettingRow label={t('settings.maxRows')}>
              <Select
                value={draft.queryResultLimit}
                options={RESULT_LIMIT_OPTIONS.map((v) => ({ value: String(v), label: `${v.toLocaleString()} ${t('common.rows')}` }))}
                onChange={(v) => updateField('queryResultLimit', Number(v))}
              />
            </SettingRow>
          )}

          <Divider />

          {/* Editor section */}
          <SectionTitle>{t('settings.editor')}</SectionTitle>

          <SettingRow label={t('settings.fontSize')}>
            <div className="flex items-center gap-3">
              <input
                type="range"
                min={10}
                max={24}
                step={1}
                value={draft.editorFontSize}
                onChange={(e) => updateField('editorFontSize', Number(e.target.value))}
                className="flex-1 accent-accent"
              />
              <span className="w-12 text-right text-sm tabular-nums text-fg-secondary">{draft.editorFontSize}px</span>
            </div>
          </SettingRow>

          <SettingRow label={t('settings.fontFamily')}>
            <input
              type="text"
              value={draft.editorFontFamily}
              onChange={(e) => updateField('editorFontFamily', e.target.value)}
              className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-500/25"
            />
          </SettingRow>

          <Divider />

          {/* Behavior section */}
          <SectionTitle>{t('settings.behavior')}</SectionTitle>

          <ToggleRow
            label={t('settings.confirmDelete')}
            checked={draft.confirmOnDelete}
            onChange={(v) => updateField('confirmOnDelete', v)}
          />

          <ToggleRow
            label={t('settings.autoCommit')}
            checked={draft.autoCommit}
            onChange={(v) => updateField('autoCommit', v)}
          />
        </div>
      </div>

      {/* Footer */}
      <footer className="flex shrink-0 items-center justify-end gap-3 border-t border-edge px-8 py-3">
        {saved && <span className="text-xs text-green-500">{t('settings.saved')}</span>}
        <Button variant="secondary" onClick={() => void handleClose()}>{t('common.close')}</Button>
        <Button variant="primary" disabled={!isDirty} onClick={() => void handleSave()}>
          {t('common.save')}
        </Button>
      </footer>
    </div>
  );
}

function SectionTitle({ children }: { children: React.ReactNode }) {
  return <h2 className="text-[13px] font-semibold uppercase tracking-wider text-fg-muted">{children}</h2>;
}

function Divider() {
  return <div className="h-px bg-edge" />;
}

function SettingRow({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex items-start gap-4">
      <div className="w-32 shrink-0 pt-2">
        <div className="text-sm text-fg-secondary">{label}</div>
        {hint && <div className="mt-0.5 text-[11px] text-fg-muted">{hint}</div>}
      </div>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

function ToggleRow({ label, checked, onChange }: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between">
      <div className="text-sm text-fg-secondary">{label}</div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors ${
          checked ? 'bg-accent' : 'bg-edge'
        }`}
      >
        <span
          className={`inline-block h-3.5 w-3.5 rounded-full bg-white shadow transition-transform ${
            checked ? 'translate-x-[18px]' : 'translate-x-[3px]'
          }`}
        />
      </button>
    </div>
  );
}
