import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { TitleBar } from '../../components/TitleBar';
import { ThemedIcon } from '../../components/ThemedIcon';
import { Button } from '../../components/ui/Button';
import { Select } from '../../components/ui/Select';
import { useSettingsStore } from '../../stores/settingsStore';
import { useSettings } from '../../hooks/useSettings';
import { useI18n } from '../../hooks/useI18n';
import { getUrlParam } from '../../lib/windowKind';
import { cn } from '../../lib/cn';
import { settingsCommands } from '../../commands/settings';
import type { AppSettings } from '../../types';
import type { ThemeMode } from '../../types/theme';
import type { TranslationKey } from '../../locales';
import { getExtensionLocales } from '../../locales';
import { ThemePackSection } from './ThemePackSection';
import { UpdateSection } from './UpdateSection';
import { PluginSettingsSection } from './PluginSettingsSection';
import { AiSettingsSection } from './AiSettingsSection';
import { PromptSettingsSection } from './PromptSettingsSection';
import { McpSettingsSection } from './McpSettingsSection';
import { McpClientSection } from './McpClientSection';
import { settingsSectionIconId } from '../../lib/hostLucideMap';
import { SectionTitle, SettingRow, ToggleRow } from './settingsUi';
import { DataCleanupSection } from './DataCleanupSection';

const PAGE_SIZE_OPTIONS = [25, 50, 100, 200, 500];
const RESULT_LIMIT_OPTIONS = [1000, 2000, 5000, 10000, 50000];

const LOG_LEVEL_OPTIONS: { value: AppSettings['logLevel']; label: string }[] = [
  { value: 'trace', label: 'Trace' },
  { value: 'debug', label: 'Debug' },
  { value: 'info', label: 'Info' },
  { value: 'warn', label: 'Warn' },
  { value: 'error', label: 'Error' },
];

const THEME_KEYS: { value: ThemeMode; key: TranslationKey }[] = [
  { value: 'light', key: 'theme.light' },
  { value: 'dark', key: 'theme.dark' },
  { value: 'system', key: 'theme.system' },
];

const BUILTIN_LANGUAGE_OPTIONS = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'zh-TW', label: '繁體中文' },
  { value: 'en', label: 'English' },
  { value: 'es', label: 'Español' },
  { value: 'fr', label: 'Français' },
  { value: 'de', label: 'Deutsch' },
  { value: 'ja', label: '日本語' },
  { value: 'pt-BR', label: 'Português (Brasil)' },
  { value: 'ru', label: 'Русский' },
  { value: 'ko', label: '한국어' },
];

type SettingsSection =
  | 'general'
  | 'dataBrowsing'
  | 'editor'
  | 'behavior'
  | 'logging'
  | 'ai'
  | 'prompts'
  | 'mcpServer'
  | 'mcpClient'
  | 'extensions';

const SECTIONS: { id: SettingsSection; labelKey: TranslationKey }[] = [
  { id: 'general', labelKey: 'settings.general' },
  { id: 'dataBrowsing', labelKey: 'settings.dataBrowsing' },
  { id: 'editor', labelKey: 'settings.editor' },
  { id: 'behavior', labelKey: 'settings.behavior' },
  { id: 'logging', labelKey: 'settings.logging' },
  { id: 'ai', labelKey: 'settings.ai' },
  { id: 'prompts', labelKey: 'settings.prompts' },
  { id: 'mcpServer', labelKey: 'mcp.title' },
  { id: 'mcpClient', labelKey: 'mcpClient.title' },
  { id: 'extensions', labelKey: 'settings.extensions.title' },
];

export function SettingsWindow() {
  useSettings();
  const { t } = useI18n();

  const settings = useSettingsStore((s) => s.settings);
  const loadSettings = useSettingsStore((s) => s.loadSettings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  const [draft, setDraft] = useState<AppSettings>(settings);
  const [saved, setSaved] = useState(false);
  const [defaultLogPath, setDefaultLogPath] = useState('');
  const settingsHydrated = useRef(false);

  const languageOptions = useMemo(
    () => [...BUILTIN_LANGUAGE_OPTIONS, ...getExtensionLocales()],
    [],
  );

  const [activeSection, setActiveSection] = useState<SettingsSection>(() => {
    const fromUrl = getUrlParam('section');
    if (fromUrl && SECTIONS.some((s) => s.id === fromUrl)) {
      return fromUrl as SettingsSection;
    }
    return 'general';
  });

  useEffect(() => {
    void loadSettings().then(() => {
      setDraft(useSettingsStore.getState().settings);
      settingsHydrated.current = true;
    });
  }, [loadSettings]);

  useEffect(() => {
    void settingsCommands
      .getLogPath()
      .then(setDefaultLogPath)
      .catch(() => {});
  }, []);

  // Theme pack applies immediately via updateSettings; merge packId only so other draft edits persist.
  useEffect(() => {
    if (!settingsHydrated.current) return;
    setDraft((prev) => ({
      ...prev,
      theme: { ...prev.theme, packId: settings.theme.packId },
    }));
  }, [settings.theme.packId]);

  const updateField = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setDraft((prev) => ({ ...prev, [key]: value }));
    setSaved(false);
  };

  const handleSave = useCallback(async () => {
    await updateSettings(draft);
    setDraft(useSettingsStore.getState().settings);
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
      <TitleBar title={t('win.settings')} />

      <div className="flex min-h-0 flex-1">
        {/* Sidebar */}
        <nav className="flex w-[180px] shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-edge bg-surface-alt px-2 py-3">
          {SECTIONS.map((sec) => {
            const isActive = activeSection === sec.id;
            return (
              <button
                key={sec.id}
                type="button"
                onClick={() => setActiveSection(sec.id)}
                className={cn(
                  'flex items-center gap-2.5 rounded-md px-3 py-2 text-left text-[13px] transition-colors',
                  isActive
                    ? 'bg-accent/15 font-medium text-accent'
                    : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
                )}
              >
                <ThemedIcon id={settingsSectionIconId(sec.id)} className="h-4 w-4 shrink-0" />
                {t(sec.labelKey)}
              </button>
            );
          })}
        </nav>

        {/* Content */}
        <div className="flex-1 overflow-y-auto px-8 py-6">
          <div className="mx-auto max-w-lg space-y-5">
            {activeSection === 'general' && (
              <>
                <SectionTitle>{t('settings.general')}</SectionTitle>

                <SettingRow label={t('settings.language')}>
                  <Select
                    value={draft.language}
                    options={languageOptions}
                    onChange={(v) => updateField('language', v)}
                  />
                </SettingRow>

                <SettingRow label={t('settings.theme')}>
                  <Select
                    value={draft.theme.mode}
                    options={themeOptions}
                    onChange={(v) => updateField('theme', { ...draft.theme, mode: v as ThemeMode })}
                  />
                </SettingRow>

                <ThemePackSection />

                <UpdateSection
                  checkOnStartup={draft.checkForUpdatesOnStartup}
                  onCheckOnStartupChange={(v) => updateField('checkForUpdatesOnStartup', v)}
                />
              </>
            )}

            {activeSection === 'dataBrowsing' && (
              <>
                <SectionTitle>{t('settings.dataBrowsing')}</SectionTitle>

                <SettingRow label={t('settings.defaultPageSize')}>
                  <Select
                    value={draft.defaultPageSize}
                    options={PAGE_SIZE_OPTIONS.map((v) => ({
                      value: String(v),
                      label: `${v} ${t('common.rows')}`,
                    }))}
                    onChange={(v) => updateField('defaultPageSize', Number(v))}
                  />
                </SettingRow>

                <SettingRow label={t('settings.connectionPoolSize')}>
                  <input
                    type="number"
                    min={1}
                    max={100}
                    value={draft.connectionPoolSize ?? 10}
                    onChange={(e) => {
                      const n = Number(e.target.value);
                      if (!Number.isFinite(n)) return;
                      updateField('connectionPoolSize', Math.min(100, Math.max(1, Math.round(n))));
                    }}
                    className="h-9 w-24 rounded-md border border-edge bg-surface px-3 text-sm tabular-nums text-fg outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-500/25"
                    data-testid="settings-connection-pool-size"
                  />
                </SettingRow>
                <p className="text-xs text-fg-muted -mt-2">
                  {t('settings.connectionPoolSizeHint')}
                </p>

                <ToggleRow
                  label={t('settings.limitSelect')}
                  checked={draft.limitSelectResults}
                  onChange={(v) => updateField('limitSelectResults', v)}
                />

                {draft.limitSelectResults && (
                  <SettingRow label={t('settings.maxRows')}>
                    <Select
                      value={draft.queryResultLimit}
                      options={RESULT_LIMIT_OPTIONS.map((v) => ({
                        value: String(v),
                        label: `${v.toLocaleString()} ${t('common.rows')}`,
                      }))}
                      onChange={(v) => updateField('queryResultLimit', Number(v))}
                    />
                  </SettingRow>
                )}

                <ToggleRow
                  label={t('settings.autoChartOnQuery')}
                  checked={draft.autoChartOnQuery === true}
                  onChange={(v) => updateField('autoChartOnQuery', v)}
                />
                <p className="text-xs text-fg-muted -mt-2">{t('settings.autoChartOnQueryHint')}</p>
              </>
            )}

            {activeSection === 'editor' && (
              <>
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
                    <span className="w-12 text-right text-sm tabular-nums text-fg-secondary">
                      {draft.editorFontSize}px
                    </span>
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
              </>
            )}

            {activeSection === 'behavior' && (
              <>
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

                <ToggleRow
                  label={t('settings.safeMode')}
                  hint={t('settings.safeModeHint')}
                  checked={draft.safeMode}
                  onChange={(v) => updateField('safeMode', v)}
                />

                <DataCleanupSection />
              </>
            )}

            {activeSection === 'logging' && (
              <>
                <SectionTitle>{t('settings.logging')}</SectionTitle>

                <SettingRow label={t('settings.logLevel')}>
                  <Select
                    value={draft.logLevel}
                    options={LOG_LEVEL_OPTIONS}
                    onChange={(v) => updateField('logLevel', v as AppSettings['logLevel'])}
                  />
                </SettingRow>

                <SettingRow label={t('settings.logPath')}>
                  <input
                    type="text"
                    value={draft.logPath}
                    onChange={(e) => updateField('logPath', e.target.value)}
                    placeholder={defaultLogPath || t('settings.logPathPlaceholder')}
                    className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-500/25"
                  />
                </SettingRow>

                <div className="flex items-center gap-3">
                  <Button variant="secondary" onClick={() => void settingsCommands.openLogDir()}>
                    {t('settings.viewLogs')}
                  </Button>
                </div>

                <p className="text-xs text-fg-muted">{t('settings.logRestartNote')}</p>
              </>
            )}

            {activeSection === 'ai' && <AiSettingsSection />}
            {activeSection === 'prompts' && <PromptSettingsSection />}
            {activeSection === 'mcpServer' && <McpSettingsSection />}
            {activeSection === 'mcpClient' && <McpClientSection />}
            {activeSection === 'extensions' && <PluginSettingsSection />}
          </div>
        </div>
      </div>

      {/* Footer - only show for general settings that use draft/save */}
      {['general', 'dataBrowsing', 'editor', 'behavior', 'logging'].includes(activeSection) && (
        <footer className="flex shrink-0 items-center justify-end gap-3 border-t border-edge px-8 py-3">
          {saved && <span className="text-xs text-green-500">{t('settings.saved')}</span>}
          <Button variant="secondary" onClick={() => void handleClose()}>
            {t('common.close')}
          </Button>
          <Button variant="primary" disabled={!isDirty} onClick={() => void handleSave()}>
            {t('common.save')}
          </Button>
        </footer>
      )}
    </div>
  );
}
