import { t } from '../../locales/t';

/**
 * The wording of the in-cell foreign-key selector's entry (DB-20).
 *
 * It uses the standalone translation function for the same reason as
 * `fkCellText`: the label is built per cell, per render, and handing a
 * translate function down to every row would make each row subscribe to the
 * locale.
 */
export function fkSelectorLabel(targetLabel: string): string {
  return t('fkSelector.pickFrom', { target: targetLabel });
}
