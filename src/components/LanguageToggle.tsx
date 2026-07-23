import { useEffect, useRef, useState } from 'react';
import { useI18n } from '../hooks/useI18n';
import { cn } from '../lib/cn';
import type { SupportedLocale, TranslationKey } from '../locales';
import { useSettingsStore } from '../stores/settingsStore';

const OPTIONS: {
  value: SupportedLocale;
  flag: string;
  key: TranslationKey;
}[] = [
  { value: 'en', flag: '🇬🇧', key: 'language.english' },
  { value: 'zh-CN', flag: '🇨🇳', key: 'language.chinese' },
];

export function LanguageToggle() {
  const { t, language } = useI18n();
  const updateSettings = useSettingsStore((s) => s.updateSettings);
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function onClick(event: MouseEvent) {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener('mousedown', onClick);
    return () => document.removeEventListener('mousedown', onClick);
  }, [open]);

  const current = OPTIONS.find((option) => option.value === language) ?? OPTIONS[0];

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((value) => !value)}
        className="inline-flex h-8 w-8 items-center justify-center rounded-md text-fg-muted transition-colors hover:bg-surface-raised hover:text-fg"
        title={t('language.tooltip', { current: t(current.key) })}
        aria-haspopup="menu"
        aria-expanded={open}
      >
        <span className="text-base leading-none" role="img" aria-hidden="true">
          {current.flag}
        </span>
      </button>

      {open && (
        <div
          className="absolute right-0 top-full z-50 mt-1 w-44 overflow-hidden rounded-lg border border-edge bg-surface-alt shadow-lg"
          role="menu"
        >
          {OPTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              role="menuitemradio"
              aria-checked={language === option.value}
              onClick={() => {
                void updateSettings({ language: option.value });
                setOpen(false);
              }}
              className={cn(
                'flex w-full items-center gap-2.5 px-3 py-2 text-xs transition-colors',
                language === option.value
                  ? 'bg-blue-500/10 text-blue-500'
                  : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
              )}
            >
              <span className="text-base leading-none" role="img" aria-hidden="true">
                {option.flag}
              </span>
              {t(option.key)}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
