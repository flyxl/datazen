import { cn } from '../../lib/cn';
import type { I18nKey } from '../../locales';

/**
 * The connector of one condition: how it joins the condition before it.
 *
 * Two states of the same control:
 *
 * - **inherited** — the row has no connector of its own and follows the
 *   request wide logic. The badge shows that value and says so in its title.
 * - **explicit** — the row carries its own connector. Clicking a value makes
 *   the row explicit, which is what takes the request wide logic out of the
 *   picture.
 *
 * The buttons are plain `type="button"` elements: the filter panel does not
 * live inside the grid surface, so the buttons must not join the grid's
 * roving tabindex nor the global shortcut table.
 */
export function FilterConnectorToggle({
  index,
  value,
  explicit,
  differsFromGlobal,
  disabled = false,
  onSelect,
  t,
}: {
  /** The row this connector belongs to. */
  index: number;
  /** The connector in effect for this row. */
  value: 'and' | 'or';
  /** Whether the row carries its own connector. */
  explicit: boolean;
  /** Whether the effective connector differs from the request wide logic. */
  differsFromGlobal: boolean;
  disabled?: boolean;
  onSelect: (connector: 'and' | 'or') => void;
  t: (key: I18nKey) => string;
}) {
  return (
    <div
      className={cn(
        'inline-flex shrink-0 items-center rounded border border-edge',
        differsFromGlobal && 'border-accent/60 bg-accent/10',
        !explicit && 'opacity-70',
      )}
      data-testid="filter-connector"
      data-filter-index={index}
      data-connector={value}
      title={explicit ? undefined : t('filter.connectorInherited')}
    >
      <button
        type="button"
        className={cn(
          'px-1 py-0.5 text-[11px]',
          value === 'and' ? 'bg-accent/15 text-accent' : 'text-fg-muted',
        )}
        onClick={() => onSelect('and')}
        disabled={disabled}
        aria-pressed={value === 'and'}
        data-testid="filter-connector-and"
        data-filter-index={index}
      >
        {t('filter.and')}
      </button>
      <button
        type="button"
        className={cn(
          'px-1 py-0.5 text-[11px]',
          value === 'or' ? 'bg-accent/15 text-accent' : 'text-fg-muted',
        )}
        onClick={() => onSelect('or')}
        disabled={disabled}
        aria-pressed={value === 'or'}
        data-testid="filter-connector-or"
        data-filter-index={index}
      >
        {t('filter.or')}
      </button>
    </div>
  );
}
