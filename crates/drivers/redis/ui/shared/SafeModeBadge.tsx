import { useI18n } from '@datazen/ui';
import { useBoundSettingsStore } from '@datazen/driver-sdk';

/**
 * Renders the shared "Safe" badge when the global `settings.safeMode` is on.
 * Mirrors the SQL QueryEditor toolbar badge; subscribes to the live store value.
 */
export function SafeModeBadge() {
  const { t } = useI18n();
  const safeMode = useBoundSettingsStore((s) => s.settings.safeMode);
  if (!safeMode) return null;
  return (
    <span
      className="shrink-0 rounded bg-warning/15 px-1.5 py-0.5 text-[10px] font-medium text-warning"
      title={t('settings.safeMode')}
      data-testid="redis-safe-mode"
    >
      {'Safe'}
    </span>
  );
}
