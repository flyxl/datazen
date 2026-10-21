import { t } from '../../locales/t';

/**
 * The wording of the foreign-key jump entry (DB-07).
 *
 * It uses the standalone translation function on purpose — the same reason as
 * `dirtyCellText`: the label is built per cell, per render, and handing a
 * translate function down to every row would make each row subscribe to the
 * locale.
 *
 * The target label comes from the foreign-key group: `customers.id` for a
 * single-column key, `sales.parents.(x, y)` for a composite one. A composite
 * key names the table **and** the columns, because "jump to sales.parents"
 * alone does not say which row is meant.
 */
export function fkJumpLabel(targetLabel: string): string {
  const compositeAt = targetLabel.indexOf('.(');
  if (compositeAt < 0) return t('fkNav.jumpTo', { target: targetLabel });
  return t('fkNav.jumpToComposite', {
    // The label is `table.(column, column)`: the table is everything before
    // the bracket, the columns everything between it and the closing one.
    table: targetLabel.slice(0, compositeAt),
    columns: targetLabel.slice(compositeAt + 2, -1),
  });
}

/** The wording while the resolution of a target is in flight. */
export function fkResolvingLabel(): string {
  return t('fkNav.resolving');
}
