import { useCallback, useState } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { mergeDriverSettings } from '../../lib/driverSettings';
import { DRIVER_SETTINGS_ENTRIES } from '../../extensions/generated';
import { useSettingsStore } from '../../stores/settingsStore';
import type { AppSettings } from '../../types';
import { JsonSchemaSettingsForm } from './JsonSchemaSettingsForm';

function SectionTitle({ children }: { children: React.ReactNode }) {
  return (
    <h2 className="text-[13px] font-semibold uppercase tracking-wider text-fg-muted">{children}</h2>
  );
}

function DriverBlockTitle({ children }: { children: React.ReactNode }) {
  return <h3 className="text-sm font-medium text-fg">{children}</h3>;
}

export interface DriverSettingsSectionProps {
  settings?: AppSettings;
  onSettingsChange?: (partial: Partial<AppSettings>) => void;
}

export function DriverSettingsSection({
  settings: draftSettings,
  onSettingsChange,
}: DriverSettingsSectionProps = {}) {
  const { t } = useI18n();
  const storedSettings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const settings = draftSettings ?? storedSettings;
  const [saved, setSaved] = useState(false);

  const entries = DRIVER_SETTINGS_ENTRIES;

  const handleDriverChange = useCallback(
    async (driverId: string, next: unknown) => {
      const latest = settings.driverSettings;
      const partial = {
        driverSettings: mergeDriverSettings(latest, driverId, next),
      };
      if (onSettingsChange) onSettingsChange(partial);
      else await updateSettings(partial);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    },
    [onSettingsChange, settings, updateSettings],
  );

  const readDriverValue = (driverId: string): unknown => {
    return settings.driverSettings[driverId] ?? {};
  };

  return (
    <>
      <SectionTitle>{t('settings.extensions.title')}</SectionTitle>

      {entries.length === 0 ? (
        <p className="text-xs text-fg-muted">{t('settings.extensions.empty')}</p>
      ) : (
        <div className="space-y-6">
          {entries.map((entry) => {
            const value = readDriverValue(entry.driverId);
            const onChange = (next: unknown) => void handleDriverChange(entry.driverId, next);

            return (
              <div
                key={entry.driverId}
                className="space-y-3 rounded-md border border-edge bg-surface p-4"
              >
                <DriverBlockTitle>{entry.label}</DriverBlockTitle>
                {entry.SettingsSection ? (
                  <entry.SettingsSection value={value} onChange={onChange} />
                ) : entry.schema ? (
                  <JsonSchemaSettingsForm schema={entry.schema} value={value} onChange={onChange} />
                ) : null}
              </div>
            );
          })}

          {!onSettingsChange && saved && (
            <div className="flex items-center gap-3">
              <span className="text-xs text-green-500">{t('settings.saved')}</span>
            </div>
          )}
        </div>
      )}
    </>
  );
}
