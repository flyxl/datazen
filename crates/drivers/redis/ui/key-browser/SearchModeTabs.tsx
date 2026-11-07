import { Tabs, cn, useI18n } from '@datazen/ui';

export type SearchMode = 'key' | 'value' | 'all';

const MODES: { value: SearchMode; labelKey: string }[] = [
  { value: 'key', labelKey: 'redis.search.modeKey' },
  { value: 'value', labelKey: 'redis.search.modeValue' },
  { value: 'all', labelKey: 'redis.search.modeAll' },
];

export interface SearchModeTabsProps {
  mode: SearchMode;
  onChange: (mode: SearchMode) => void;
}

/**
 * Key / Value / All search-scope tabs (R6).
 *
 * Text only. Each of the three used to carry an icon, and the group shares one
 * `h-7` row with the pattern input, the two filter chips and the apply button —
 * at that width the icons cost more horizontal room than the labels needed, and
 * `key` / `value` / `all` are already unambiguous words. The selected state is
 * carried by `bg-accent/10 text-accent` plus `aria-selected`, so nothing that
 * identified the scope depended on the icon.
 *
 * This strip owns no panel (the key tree is the panel), so it runs `Tabs` in
 * bar-only mode and keeps the segmented chrome: one `h-7` row, a `border-l`
 * divider between segments. The tab semantics — `role`, `aria-selected`, the
 * roving tabindex and arrow/Home/End — come from the shared shell, which is
 * where the keyboard support this switcher lacked now lives.
 */
export function SearchModeTabs({ mode, onChange }: SearchModeTabsProps) {
  const { t } = useI18n();
  return (
    <Tabs
      items={MODES.map(({ value, labelKey }) => ({
        id: value,
        label: t(labelKey as 'redis.search.modeKey'),
        testId: `redis-search-mode-${value}`,
      }))}
      activeId={mode}
      onChange={(id) => {
        // Narrow through the list rather than casting — see JsonModeBar.
        const next = MODES.find((entry) => entry.value === id);
        if (next) onChange(next.value);
      }}
      className="flex overflow-hidden rounded-md border border-edge"
      getTabClassName={({ index, selected }) =>
        cn(
          'flex h-7 items-center px-2 text-xs transition-colors',
          index > 0 && 'border-l border-edge',
          selected ? 'bg-accent/10 text-accent' : 'text-fg-secondary hover:bg-surface-raised',
        )
      }
      testId="redis-search-mode-tabs"
    />
  );
}
