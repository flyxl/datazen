import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { ArrowLeft } from 'lucide-react';
import { ThemedIcon } from '../../components/ThemedIcon';
import { Button } from '../../components/ui/Button';
import { PathInput } from '../../components/ui/PathInput';
import { Select } from '../../components/ui/Select';
import { useSettingsStore } from '../../stores/settingsStore';
import { useSettings } from '../../hooks/useSettings';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { settingsCommands } from '../../commands/settings';
import type { AppSettings } from '../../types';
import { getExtensionLocales } from '../../locales';
import { UpdateSection } from './UpdateSection';
import { PluginSettingsSection } from './PluginSettingsSection';
import { AiSettingsSection } from './AiSettingsSection';
import { PromptSettingsSection } from './PromptSettingsSection';
import { McpSettingsSection } from './McpSettingsSection';
import { McpClientSection } from './McpClientSection';
import { settingsSectionIconId } from '../../lib/hostLucideMap';
import { SectionTitle, SettingRow, ToggleRow } from './settingsUi';
import { DataCleanupSection } from './DataCleanupSection';
import { AppearanceSection } from './AppearanceSection';
import { parseSettingsSection, SETTINGS_SECTIONS, type SettingsSection } from './settingsSections';

const PAGE_SIZE_OPTIONS = [25, 50, 100, 200, 500];
const RESULT_LIMIT_OPTIONS = [1000, 2000, 5000, 10000, 50000];

const LOG_LEVEL_OPTIONS: { value: AppSettings['logLevel']; label: string }[] = [
  { value: 'trace', label: 'Trace' },
  { value: 'debug', label: 'Debug' },
  { value: 'info', label: 'Info' },
  { value: 'warn', label: 'Warn' },
  { value: 'error', label: 'Error' },
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

export interface SettingsContentProps {
  /** Initial section; updates when the prop changes (e.g. menu deep-link). */
  initialSection?: string;
  /** Show legacy Close button in the draft/save footer (standalone sub-window). */
  showCloseButton?: boolean;
  onClose?: () => void;
  onBack?: () => void;
}

export function SettingsContent({
  initialSection,
  showCloseButton = false,
  onClose,
  onBack,
}: Readonly<SettingsContentProps>) {
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

  const [activeSection, setActiveSection] = useState<SettingsSection>(() =>
    parseSettingsSection(initialSection),
  );

  useEffect(() => {
    if (initialSection !== undefined) {
      setActiveSection(parseSettingsSection(initialSection));
    }
  }, [initialSection]);

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

  const isDirty = JSON.stringify(draft) !== JSON.stringify(settings);

  return (
    <>
      <div className="flex min-h-0 flex-1">
        <nav
          className="flex w-[180px] shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-edge bg-surface-alt px-2 pb-3"
          data-testid="settings-nav"
        >
          {onBack && (
            <Button
              variant="ghost"
              className="mb-1 h-8 w-full justify-start gap-1.5 px-2 text-xs"
              onClick={onBack}
              data-testid="settings-back"
              title={t('common.back')}
            >
              <ArrowLeft className="h-3.5 w-3.5 shrink-0" />
              {t('common.back')}
            </Button>
          )}
          {SETTINGS_SECTIONS.map((sec) => {
            const isActive = activeSection === sec.id;
            return (
              <button
                key={sec.id}
                type="button"
                data-testid={`settings-nav-${sec.id}`}
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

        <div className="flex-1 overflow-y-auto px-8 py-6" data-testid="settings-content">
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
                  <PathInput
                    value={draft.logPath}
                    onChange={(v) => updateField('logPath', v)}
                    placeholder={defaultLogPath || t('settings.logPathPlaceholder')}
                    dialogOptions={{ directory: true }}
                  />
                </SettingRow>

                <div className="flex items-center gap-3">
                  <Button variant="secondary" onClick={() => void settingsCommands.openLogDir()}>
                    {t('common.viewLogs')}
                  </Button>
                </div>

                <p className="text-xs text-fg-muted">{t('settings.logRestartNote')}</p>
              </>
            )}

            {activeSection === 'appearance' && <AppearanceSection />}
            {activeSection === 'ai' && <AiSettingsSection />}
            {activeSection === 'prompts' && <PromptSettingsSection />}
            {activeSection === 'mcpServer' && <McpSettingsSection />}
            {activeSection === 'mcpClient' && <McpClientSection />}
            {activeSection === 'extensions' && <PluginSettingsSection />}
          </div>
        </div>
      </div>

      {['general', 'dataBrowsing', 'editor', 'behavior', 'logging'].includes(activeSection) && (
        <footer className="flex shrink-0 items-center justify-end gap-3 border-t border-edge px-8 py-3">
          {saved && <span className="text-xs text-green-500">{t('settings.saved')}</span>}
          {showCloseButton && onClose && (
            <Button variant="secondary" onClick={() => void onClose()}>
              {t('common.close')}
            </Button>
          )}
          <Button variant="primary" disabled={!isDirty} onClick={() => void handleSave()}>
            {t('common.save')}
          </Button>
        </footer>
      )}
    </>
  );
}
