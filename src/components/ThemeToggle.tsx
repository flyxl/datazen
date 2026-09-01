import { useState, useRef, useEffect } from 'react';
import { useSettingsStore } from '../stores/settingsStore';
import { cn } from '../lib/cn';
import type { ThemeMode } from '../types/theme';
import type { TranslationKey } from '../locales';
import { useI18n } from '../hooks/useI18n';
import { ThemedIcon } from './ThemedIcon';

const OPTIONS: { value: ThemeMode; key: TranslationKey; iconId: string }[] = [
  { value: 'light', key: 'theme.light', iconId: 'theme.light' },
  { value: 'dark', key: 'theme.dark', iconId: 'theme.dark' },
  { value: 'system', key: 'theme.system', iconId: 'theme.system' },
];

export function ThemeToggle() {
  const { t } = useI18n();
  const theme = useSettingsStore((s) => s.settings.theme);
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function onClick(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener('mousedown', onClick);
    return () => document.removeEventListener('mousedown', onClick);
  }, [open]);

  const current = OPTIONS.find((o) => o.value === theme.mode) ?? OPTIONS[2];

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="inline-flex h-8 w-8 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg transition-colors"
        title={t('theme.tooltip', { current: t(current.key) })}
      >
        <ThemedIcon id={current.iconId} className="h-4 w-4" />
      </button>

      {open && (
        <div className="absolute right-0 top-full z-50 mt-1 w-36 overflow-hidden rounded-lg border border-edge bg-surface-alt shadow-lg">
          {OPTIONS.map((opt) => (
            <button
              key={opt.value}
              type="button"
              onClick={() => {
                void updateSettings({ theme: { ...theme, mode: opt.value } });
                setOpen(false);
              }}
              className={cn(
                'flex w-full items-center gap-2 px-3 py-2 text-xs transition-colors',
                theme.mode === opt.value
                  ? 'bg-accent/10 text-accent'
                  : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
              )}
            >
              <ThemedIcon id={opt.iconId} className="h-3.5 w-3.5" />
              {t(opt.key)}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
