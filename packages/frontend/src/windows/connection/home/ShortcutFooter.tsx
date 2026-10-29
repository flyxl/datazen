import { useI18n } from '../../../hooks/useI18n';
import {
  formatShortcutForDisplay,
  getActionShortcut,
  type KeymapAction,
} from '../../../lib/keymap';
import { useSettingsStore } from '../../../stores/settingsStore';

/**
 * Shortcut footer for the landing page. Key positions come from the active
 * keymap preset / custom keymap (same source as ContentView) — nothing is
 * hard-coded. Actions without a registered shortcut are skipped (soft
 * degradation).
 */
export function ShortcutFooter() {
  const { t } = useI18n();
  const keymapPreset = useSettingsStore((s) => s.settings.keymapPreset);
  const customKeymap = useSettingsStore((s) => s.settings.customKeymap);

  const candidates: { action: KeymapAction; label: string }[] = [
    { action: 'newQuery', label: t('keymap.action.newQuery') },
    { action: 'saveQuery', label: t('keymap.action.saveQuery') },
    { action: 'formatSql', label: t('keymap.action.formatSql') },
  ];

  const items = candidates
    .map((candidate) => ({
      ...candidate,
      shortcut: getActionShortcut(candidate.action, keymapPreset, customKeymap),
    }))
    .filter((item) => typeof item.shortcut === 'string' && item.shortcut.length > 0);

  if (items.length === 0) {
    return null;
  }

  return (
    <div
      className="flex flex-wrap items-center justify-center gap-x-5 gap-y-1 pb-1 text-xs text-fg-muted"
      data-testid="home-shortcut-footer"
    >
      {items.map(({ action, label, shortcut }) => (
        <span key={action} className="inline-flex items-center gap-1.5">
          <kbd className="rounded border border-edge bg-surface px-1.5 py-0.5 font-mono text-[10px] text-fg-secondary">
            {formatShortcutForDisplay(shortcut)}
          </kbd>
          <span>{label}</span>
        </span>
      ))}
    </div>
  );
}
