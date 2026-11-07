import { useI18n } from '../../hooks/useI18n';
import { Select } from '../ui/Select';
import type { DefaultOrderOverride } from '../../lib/defaultSort';

export interface DefaultOrderSelectProps {
  /** The panel's current override of the global setting. */
  value: DefaultOrderOverride;
  onChange: (value: DefaultOrderOverride) => void;
  /** How many primary key columns the table has. */
  primaryKeyCount: number;
  disabled?: boolean;
}

function isOverride(value: string): value is DefaultOrderOverride {
  return (
    value === 'inherit' ||
    value === 'primaryKeyAsc' ||
    value === 'primaryKeyDesc' ||
    value === 'none'
  );
}

/**
 * The panel's temporary override of the default order.
 *
 * The panel's choice beats the global setting for this tab only; it is not
 * persisted. "Do not sort" is offered here rather than hidden: without an
 * order the rows of two pages may repeat or be skipped, and the grid says so
 * next to the pager — but the user has to be able to ask for it.
 *
 * "Primary key, descending" is greyed out for a composite primary key: the
 * sort channel carries one column, and quietly sorting by the first key only
 * would be a different order than the one the entry names. The reason is in
 * the tooltip, because a greyed-out entry without a reason is a dead end.
 */
export function DefaultOrderSelect({
  value,
  onChange,
  primaryKeyCount,
  disabled,
}: DefaultOrderSelectProps) {
  const { t } = useI18n();
  const composite = primaryKeyCount > 1;
  const unavailable = composite ? t('defaultSort.primaryKeyDescUnavailable') : undefined;

  return (
    <div className="flex items-center gap-1">
      <span className="hidden sm:inline">{t('defaultSort.panelLabel')}</span>
      <Select
        className="h-8 w-[148px]"
        value={value}
        disabled={disabled}
        title={unavailable}
        options={[
          { value: 'inherit', label: t('defaultSort.inherit') },
          { value: 'primaryKeyAsc', label: t('defaultSort.primaryKeyAsc') },
          {
            value: 'primaryKeyDesc',
            label: t('defaultSort.primaryKeyDesc'),
            disabled: composite,
            title: unavailable,
          },
          { value: 'none', label: t('defaultSort.none') },
        ]}
        onChange={(next) => {
          if (isOverride(next)) onChange(next);
        }}
        data-testid="pagination-default-order"
      />
    </div>
  );
}
