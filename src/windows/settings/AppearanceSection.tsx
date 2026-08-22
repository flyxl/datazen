import { useEffect, useMemo, useState } from 'react';
import { PackageOpen } from 'lucide-react';
import { Badge } from '../../components/ui/Badge';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { encodePluginThemePackId, parsePluginThemePackId } from '../../lib/themePackApply';
import { usePluginStore } from '../../stores/pluginStore';
import { useSettingsStore } from '../../stores/settingsStore';
import type { PluginSummary, PluginThemeSummary } from '../../types/plugin';
import { SectionTitle } from './settingsUi';

interface ThemeCardEntry {
  plugin: PluginSummary;
  theme: PluginThemeSummary;
  /** Encoded id persisted in `settings.theme.packId`. */
  packId: string;
}

/** Deterministic accent hue for a theme's color swatch (no plugin hardcoding). */
function hueForTheme(pluginId: string, themeId: string): number {
  const input = `${pluginId}:${themeId}`;
  let hash = 0;
  for (let i = 0; i < input.length; i += 1) {
    hash = (hash * 31 + input.charCodeAt(i)) | 0;
  }
  return Math.abs(hash) % 360;
}

export function AppearanceSection() {
  const { t } = useI18n();
  const plugins = usePluginStore((s) => s.plugins);
  const loaded = usePluginStore((s) => s.loaded);
  const settings = useSettingsStore((s) => s.settings);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!loaded) void usePluginStore.getState().fetch();
  }, [loaded]);

  // PRD §4.5: only themes contributed by *enabled* plugins are switchable here.
  const cards = useMemo<ThemeCardEntry[]>(
    () =>
      plugins
        .filter((p) => p.enabled)
        .flatMap((plugin) =>
          plugin.themes.map((theme) => ({
            plugin,
            theme,
            packId: encodePluginThemePackId(plugin.id, theme.id),
          })),
        ),
    [plugins],
  );

  const activePackId = settings.theme.packId;
  const activePluginThemeMissing =
    parsePluginThemePackId(activePackId) !== null && !cards.some((c) => c.packId === activePackId);

  const handleApply = async (card: ThemeCardEntry) => {
    if (card.packId === useSettingsStore.getState().settings.theme.packId) return;
    setError(null);
    try {
      const theme = useSettingsStore.getState().settings.theme;
      await updateSettings({ theme: { ...theme, packId: card.packId } });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div data-testid="appearance-section">
      <SectionTitle>{t('settings.appearance')}</SectionTitle>
      <p className="text-xs text-fg-muted">{t('settings.appearance.subtitle')}</p>

      {cards.length === 0 ? (
        <div
          data-testid="appearance-empty"
          className="mt-6 flex flex-col items-center gap-2 rounded-lg border border-dashed border-edge px-4 py-10 text-center"
        >
          <PackageOpen className="h-6 w-6 text-fg-muted" />
          <p className="text-sm text-fg-secondary">{t('settings.appearance.emptyTitle')}</p>
          <p className="max-w-sm text-xs text-fg-muted">{t('settings.appearance.emptyHint')}</p>
        </div>
      ) : (
        <div
          className="mt-4 grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3"
          data-testid="appearance-theme-grid"
        >
          {cards.map((card) => {
            const isActive = card.packId === activePackId;
            return (
              <button
                key={card.packId}
                type="button"
                data-testid="appearance-theme-card"
                data-theme-id={card.theme.id}
                data-plugin-id={card.plugin.id}
                aria-pressed={isActive}
                onClick={() => void handleApply(card)}
                className={cn(
                  'flex flex-col gap-2.5 rounded-lg border p-3 text-left transition-colors',
                  isActive
                    ? 'border-accent/60 bg-accent/10'
                    : 'border-edge bg-surface-alt hover:border-accent/40',
                )}
              >
                <div className="flex items-center gap-2.5">
                  <span
                    aria-hidden
                    className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md text-base font-semibold text-white"
                    style={{
                      backgroundColor: `hsl(${hueForTheme(card.plugin.id, card.theme.id)} 45% 42%)`,
                    }}
                  >
                    {card.theme.name.slice(0, 1).toUpperCase()}
                  </span>
                  <div className="min-w-0">
                    <div className="truncate text-sm font-semibold text-fg">{card.theme.name}</div>
                    <div className="truncate text-[11px] text-fg-muted">
                      v{card.plugin.version} · {card.plugin.name}
                    </div>
                  </div>
                </div>
                <div className="flex flex-wrap items-center gap-1">
                  {card.theme.modes.map((mode) => (
                    <Badge key={mode}>{mode}</Badge>
                  ))}
                  {isActive && (
                    <Badge tone="accent" data-testid="appearance-current-badge">
                      {t('settings.appearance.current')}
                    </Badge>
                  )}
                </div>
              </button>
            );
          })}
        </div>
      )}

      {activePluginThemeMissing && cards.length > 0 && (
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
