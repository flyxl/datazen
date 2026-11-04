import { Asterisk, Clock, Search } from 'lucide-react';
import { Button, Input, cn, useI18n } from '@datazen/ui';

/**
 * Row R2 of the key-tree column header (PRD §3.2 屏 B 左列): the pattern input
 * plus the filters that shape what the pattern means.
 *
 * The two chips are toggles over existing scan parameters, not decoration:
 *  - `* 模糊` wraps a *literal* input into `*input*` when applying (see
 *    {@link toScanPattern}); a pattern that already carries a glob char is sent
 *    verbatim, so the chip can never silently widen a hand-written glob;
 *  - `仅无过期` maps to `noTtlOnly` (server-side filter, re-scans).
 *
 * 模糊 and 仅无过期 deliberately do *not* behave alike, and the apply button is
 * what makes that legible: `noTtlOnly` is an argument to the scan that is
 * re-issued on toggle, so it re-runs by itself, while 模糊 only rewrites the
 * pattern at apply time — the same moment `Enter` uses. Without a visible way to
 * apply, clicking 模糊 read as a dead chip next to a live one. The button gives
 * that moment a target, and it is the one control that makes every combination
 * of input + both chips reachable without knowing the keyboard.
 */

export interface KeyTreeSearchRowProps {
  pattern: string;
  onPatternChange: (pattern: string) => void;
  /** `Enter` / the apply button — resolves the pattern and restarts the scan. */
  onApply: () => void;
  /**
   * `Esc` — the row's exit transition: clear the input **and** the applied
   * filter (redis-tree-ui-BUG-001). The pattern now owns the tree rows, so
   * clearing only the text would leave the tree filtered by a pattern nothing on
   * screen mentions; a cleared filter has to take effect the same way `Enter`
   * does.
   */
  onClearFilter: () => void;
  fuzzy: boolean;
  onFuzzyChange: (fuzzy: boolean) => void;
  noTtlOnly: boolean;
  onNoTtlOnlyChange: (noTtlOnly: boolean) => void;
  /** Value / all scope: the input holds a substring query, not a glob. */
  scope: 'key' | 'value' | 'all';
}

export function KeyTreeSearchRow({
  pattern,
  onPatternChange,
  onApply,
  onClearFilter,
  fuzzy,
  onFuzzyChange,
  noTtlOnly,
  onNoTtlOnlyChange,
  scope,
}: KeyTreeSearchRowProps) {
  const { t } = useI18n();

  return (
    <div className="flex items-center gap-1.5" data-testid="redis-tree-search-row">
      <div className="relative min-w-0 flex-1">
        <Search className="pointer-events-none absolute left-2 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-fg-muted" />
        <Input
          value={pattern}
          onChange={(e) => onPatternChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') onApply();
            if (e.key === 'Escape') onClearFilter();
          }}
          placeholder={scope === 'key' ? t('redis.searchKeys') : t('redis.search.valuePlaceholder')}
          /*
           * Key scope only: the value / all scopes take a substring the backend
           * matches anywhere, so telling those users about key prefixes would be
           * the wrong instruction. The placeholder already says "prefix"; this is
           * where the two opt-outs live.
           */
          title={scope === 'key' ? t('redis.search.keyPatternHint') : undefined}
          className="h-7 pl-7 text-xs"
          data-testid="redis-search-input"
          data-scope={scope}
          data-fuzzy={fuzzy ? 'on' : 'off'}
        />
      </div>
      <Chip
        testId="redis-tree-chip-fuzzy"
        active={fuzzy}
        activeValue={fuzzy ? 'on' : 'off'}
        title={t('redis.tree.fuzzyHint')}
        /*
         * `Asterisk`, not the `Sparkles` this used to wear. Sparkles read as
         * "something clever happens here", which is the wrong promise: fuzzy is
         * a plain substring match, and a user who believes it is magic will not
         * form the mental model they need. The asterisk is the wildcard they can
         * also type by hand, so the chip and the `*` label beside it name the
         * same thing the pattern field does.
         */
        Icon={Asterisk}
        label="*"
        onClick={() => onFuzzyChange(!fuzzy)}
      />
      <Chip
        testId="redis-tree-chip-no-ttl"
        active={noTtlOnly}
        activeValue={noTtlOnly ? 'on' : 'off'}
        title={t('redis.noTtlOnly')}
        Icon={Clock}
        label={t('redis.noTtlOnly')}
        onClick={() => onNoTtlOnlyChange(!noTtlOnly)}
      />
      {/*
        The explicit apply. Same call as `Enter`, same destination — this exists
        so the 模糊 modifier has a visible moment of effect, not so the keyboard
        shortcut stops working.
      */}
      <Button
        variant="ghost"
        className="h-7 w-7 shrink-0 p-0"
        title={t('redis.search.apply')}
        aria-label={t('redis.search.apply')}
        data-testid="redis-search-apply"
        onClick={onApply}
      >
        <Search className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}

interface ChipProps {
  testId: string;
  active: boolean;
  activeValue: string;
  title: string;
  Icon: typeof Search;
  label: string;
  onClick: () => void;
}

function Chip({ testId, active, activeValue, title, Icon, label, onClick }: ChipProps) {
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      aria-pressed={active}
      data-testid={testId}
      data-active={activeValue}
      onClick={onClick}
      className={cn(
        'flex h-7 shrink-0 items-center gap-1 rounded-full border px-2 text-[11px] transition-colors',
        active
          ? 'border-accent/50 bg-accent/10 text-accent'
          : 'border-edge text-fg-secondary hover:bg-surface-raised hover:text-fg',
      )}
    >
      <Icon className="h-3.5 w-3.5" />
      <span>{label}</span>
    </button>
  );
}
