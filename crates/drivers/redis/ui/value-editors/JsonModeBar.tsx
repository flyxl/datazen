import { Tabs, cn, useI18n } from '@datazen/ui';
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
 *
 * The strip owns no panel — the editor body is the panel — so it runs `Tabs` in
 * its bar-only mode and keeps the segmented-control chrome (inline box, tight
 * `text-[11px]` segments) rather than the default bottom-bordered bar. What it
 * stops owning is the tab semantics: `role`/`aria-selected`, the roving
 * tabindex and the arrow/Home/End keys now come from the shared shell, which is
 * what gives this switcher keyboard access it did not have before.
 */
export function JsonModeBar({ modes, active, onSelect, className }: JsonModeBarProps) {
  const { t } = useI18n();
  return (
    <Tabs
      items={modes.map((mode) => ({
        id: mode,
        label: t(`redis.json.mode.${mode}`),
        testId: `redis-json-mode-${mode}`,
      }))}
      activeId={active}
      onChange={(id) => {
        // Narrow through the list rather than casting: `Tabs` hands back a bare
        // string, and only a member of `modes` is a real `JsonDisplayMode`.
        const next = modes.find((mode) => mode === id);
        if (next) onSelect(next);
      }}
      className={cn(
        'inline-flex items-center gap-1 rounded-md border border-edge p-0.5',
        className,
      )}
      getTabClassName={({ selected }) =>
        cn(
          'rounded px-2 py-0.5 text-[11px] transition-colors',
          selected ? 'bg-accent/15 text-accent' : 'text-fg-secondary hover:bg-surface-raised',
        )
      }
      testId="redis-json-mode-bar"
    />
  );
}
