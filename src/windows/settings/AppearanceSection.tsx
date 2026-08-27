import { useEffect, useMemo, useState } from 'react';
import { Select } from '../../components/ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { encodePluginThemePackId, parsePluginThemePackId } from '../../lib/themePackApply';
import { useExtensionStore } from '../../stores/extensionStore';
import { useSettingsStore } from '../../stores/settingsStore';
import type { ThemeMode } from '../../types/theme';
import type { ExtensionSummary } from '../../types/extension';
import { SectionTitle, SettingRow } from './settingsUi';

/** Sentinel option value representing the built-in default theme (packId = null). */
const BUILTIN_PACK_VALUE = '__builtin__';

const MODE_OPTIONS: { value: ThemeMode; key: 'theme.light' | 'theme.dark' | 'theme.system' }[] = [
  { value: 'light', key: 'theme.light' },
  { value: 'dark', key: 'theme.dark' },
  { value: 'system', key: 'theme.system' },
];

interface ThemeOption {
  packId: string;
  value: string;
  label: string;
}

/** Flatten enabled plugins into a single theme-option list (no plugin hardcoding). */
function collectThemeOptions(plugins: ExtensionSummary[]): ThemeOption[] {
  return plugins
    .filter((p) => p.enabled)
    .flatMap((plugin) =>
      plugin.themes.map((theme) => ({
        packId: encodePluginThemePackId(plugin.id, theme.id),
        value: encodePluginThemePackId(plugin.id, theme.id),
        label: `${theme.name}`,
      })),
    );
}

export function AppearanceSection() {
  const { t } = useI18n();
  const plugins = useExtensionStore((s) => s.extensions);
  const loaded = useExtensionStore((s) => s.loaded);
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!loaded) void useExtensionStore.getState().fetch();
  }, [loaded]);

  // PRD §4.5: only themes contributed by *enabled* plugins are switchable here.
  const themeOptions = useMemo(() => collectThemeOptions(plugins), [plugins]);

  const activePackId = settings.theme.packId;
  const activePluginThemeMissing =
    parsePluginThemePackId(activePackId) !== null &&
    !themeOptions.some((o) => o.packId === activePackId);

  const handleModeChange = (mode: string) => {
    const theme = useSettingsStore.getState().settings.theme;
    void updateSettings({ theme: { ...theme, mode: mode as ThemeMode } }).catch((e) => {
      setError(e instanceof Error ? e.message : String(e));
    });
  };

  const handleThemeChange = async (value: string) => {
    const theme = useSettingsStore.getState().settings.theme;
    const packId = value === BUILTIN_PACK_VALUE ? null : value;
    if (packId === theme.packId) return;
    setError(null);
    try {
      await updateSettings({ theme: { ...theme, packId } });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div data-testid="appearance-section">
      <SectionTitle>{t('settings.appearance')}</SectionTitle>
      <p className="text-xs text-fg-muted">{t('settings.appearance.subtitle')}</p>

      <div className="mt-4 space-y-4">
        <SettingRow label={t('settings.colorScheme')}>
          <Select
            value={settings.theme.mode}
            options={MODE_OPTIONS.map((m) => ({ value: m.value, label: t(m.key) }))}
            onChange={handleModeChange}
          />
        </SettingRow>

        <SettingRow
          label={t('settings.theme.pack')}
          hint={themeOptions.length === 0 ? t('settings.appearance.emptyHint') : undefined}
        >
          <Select
            value={activePackId ?? BUILTIN_PACK_VALUE}
            options={[
              { value: BUILTIN_PACK_VALUE, label: t('settings.theme.packDefault') },
              ...themeOptions.map((o) => ({ value: o.value, label: o.label })),
            ]}
            onChange={handleThemeChange}
          />
        </SettingRow>
      </div>

      {activePluginThemeMissing && (
        <p className="mt-3 text-xs text-amber-500" data-testid="appearance-orphan-hint">
          {t('settings.appearance.missingHint')}
        </p>
      )}

      {error && (
        <p className="mt-3 text-xs text-red-500" data-testid="appearance-error">
          {t('settings.appearance.applyError', { error })}
        </p>
      )}

      {/* Reserved for future appearance options (density, font size, …). */}
      <div className="mt-8 border-t border-edge pt-4">
        <h3 className="text-sm font-medium text-fg-secondary">{t('settings.appearance.more')}</h3>
        <p className="mt-1 text-xs text-fg-muted" data-testid="appearance-more-placeholder">
          {t('settings.appearance.morePlaceholder')}
        </p>
      </div>
    </div>
  );
}
