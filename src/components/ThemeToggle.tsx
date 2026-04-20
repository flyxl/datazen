import { useState, useRef, useEffect } from 'react';
import { Sun, Moon, Monitor } from 'lucide-react';
import { useSettingsStore } from '../stores/settingsStore';
import { cn } from '../lib/cn';
import type { AppSettings } from '../types';

const OPTIONS: { value: AppSettings['theme']; label: string; Icon: typeof Sun }[] = [
  { value: 'light', label: '浅色', Icon: Sun },
  { value: 'dark', label: '深色', Icon: Moon },
  { value: 'system', label: '跟随系统', Icon: Monitor },
];

export function ThemeToggle() {
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

  const current = OPTIONS.find((o) => o.value === theme) ?? OPTIONS[2];
  const CurrentIcon = current.Icon;

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="inline-flex h-8 w-8 items-center justify-center rounded-md text-fg-muted hover:bg-surface-raised hover:text-fg transition-colors"
        title={`主题：${current.label}`}
      >
        <CurrentIcon className="h-4 w-4" />
      </button>

      {open && (
        <div className="absolute right-0 top-full z-50 mt-1 w-36 overflow-hidden rounded-lg border border-edge bg-surface-alt shadow-lg">
          {OPTIONS.map((opt) => (
            <button
              key={opt.value}
              type="button"
              onClick={() => {
                void updateSettings({ theme: opt.value });
                setOpen(false);
              }}
              className={cn(
                'flex w-full items-center gap-2 px-3 py-2 text-xs transition-colors',
                theme === opt.value
                  ? 'bg-blue-500/10 text-blue-500'
                  : 'text-fg-secondary hover:bg-surface-raised hover:text-fg',
              )}
            >
              <opt.Icon className="h-3.5 w-3.5" />
              {opt.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
