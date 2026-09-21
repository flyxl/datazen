import { KeyRound, Binary, Layers } from 'lucide-react';
import { useI18n } from '../../../../src/hooks/useI18n';
import { cn } from '../../../../src/lib/cn';

export type SearchMode = 'key' | 'value' | 'all';

const MODES: { value: SearchMode; labelKey: string; Icon: typeof KeyRound }[] = [
  { value: 'key', labelKey: 'redis.search.modeKey', Icon: KeyRound },
  { value: 'value', labelKey: 'redis.search.modeValue', Icon: Binary },
  { value: 'all', labelKey: 'redis.search.modeAll', Icon: Layers },
];

export interface SearchModeTabsProps {
  mode: SearchMode;
  onChange: (mode: SearchMode) => void;
}

/** Key / Value / All search-scope tabs (R6). */
export function SearchModeTabs({ mode, onChange }: SearchModeTabsProps) {
  const { t } = useI18n();
  return (
    <div
      className="flex overflow-hidden rounded-md border border-edge"
      role="tablist"
      data-testid="redis-search-mode-tabs"
    >
      {MODES.map(({ value, labelKey, Icon }, idx) => (
        <button
          key={value}
          type="button"
          role="tab"
          aria-selected={mode === value}
          data-testid={`redis-search-mode-${value}`}
          onClick={() => onChange(value)}
          className={cn(
            'flex h-7 items-center gap-1 px-2 text-xs transition-colors',
            idx > 0 && 'border-l border-edge',
            mode === value
              ? 'bg-accent/10 text-accent'
              : 'text-fg-secondary hover:bg-surface-raised',
          )}
        >
          <Icon className="h-3.5 w-3.5" />
          {t(labelKey as 'redis.search.modeKey')}
        </button>
      ))}
    </div>
  );
}
