import { useI18n } from '@datazen/ui';
import { cn } from '@datazen/ui';
import type { JsonDisplayMode } from './jsonModes';

export interface JsonModeBarProps {
  modes: readonly JsonDisplayMode[];
  active: JsonDisplayMode;
  onSelect: (mode: JsonDisplayMode) => void;
  className?: string;
}

/**
 * R3 — segmented control for the JSON display modes (tree / raw / pretty /
 * minify). Presentational only; the owning editor holds the text + save logic.
 * Shared between the ReJSON editor and the STRING JSON view.
 */
export function JsonModeBar({ modes, active, onSelect, className }: JsonModeBarProps) {
  const { t } = useI18n();
  return (
    <div
      role="tablist"
      data-testid="redis-json-mode-bar"
      className={cn(
        'inline-flex items-center gap-1 rounded-md border border-edge p-0.5',
        className,
      )}
    >
      {modes.map((mode) => (
        <button
          key={mode}
          type="button"
          role="tab"
          aria-selected={active === mode}
          data-testid={`redis-json-mode-${mode}`}
          className={cn(
            'rounded px-2 py-0.5 text-[11px] transition-colors',
            active === mode
              ? 'bg-accent/15 text-accent'
              : 'text-fg-secondary hover:bg-surface-raised',
          )}
          onClick={() => onSelect(mode)}
        >
          {t(`redis.json.mode.${mode}`)}
        </button>
      ))}
    </div>
  );
}
