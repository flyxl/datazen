import { useCallback, useState } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { mergePluginSettings } from '../../plugin-sdk/settings';
import { PLUGIN_SETTINGS_ENTRIES } from '../../plugins/generated';
import { useSettingsStore } from '../../stores/settingsStore';
import { JsonSchemaSettingsForm } from './JsonSchemaSettingsForm';

function SectionTitle({ children }: { children: React.ReactNode }) {
  return <h2 className="text-[13px] font-semibold uppercase tracking-wider text-fg-muted">{children}</h2>;
}

function PluginBlockTitle({ children }: { children: React.ReactNode }) {
  return <h3 className="text-sm font-medium text-fg">{children}</h3>;
}

export function PluginSettingsSection() {
  const { t } = useI18n();
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const [saved, setSaved] = useState(false);

  const entries = PLUGIN_SETTINGS_ENTRIES;

  const handlePluginChange = useCallback(
    async (pluginId: string, next: unknown) => {
      await updateSettings({
        pluginSettings: mergePluginSettings(settings.pluginSettings, pluginId, next),
      });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    },
    [settings.pluginSettings, updateSettings],
  );

  const readPluginValue = (pluginId: string): unknown => {
    return settings.pluginSettings[pluginId] ?? {};
  };

  return (
    <>
      <SectionTitle>{t('settings.extensions.title')}</SectionTitle>

      {entries.length === 0 ? (
        <p className="text-xs text-fg-muted">{t('settings.extensions.empty')}</p>
      ) : (
        <div className="space-y-6">
          {entries.map((entry) => {
            const value = readPluginValue(entry.pluginId);
            const onChange = (next: unknown) => void handlePluginChange(entry.pluginId, next);

            return (
              <div
                key={entry.pluginId}
                className="space-y-3 rounded-md border border-edge bg-surface p-4"
              >
                <PluginBlockTitle>{entry.label}</PluginBlockTitle>
                {entry.SettingsSection ? (
                  <entry.SettingsSection value={value} onChange={onChange} />
                ) : entry.schema ? (
                  <JsonSchemaSettingsForm schema={entry.schema} value={value} onChange={onChange} />
                ) : null}
              </div>
            );
          })}

          {saved && (
            <div className="flex items-center gap-3">
              <span className="text-xs text-green-500">{t('settings.saved')}</span>
            </div>
          )}
        </div>
      )}
    </>
  );
}
