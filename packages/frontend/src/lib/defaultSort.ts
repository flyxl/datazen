import type { DefaultTableSort } from '../types/settings';

/**
 * The panel's temporary override of the default order.
 *
 * `'inherit'` is not an order — it means "whatever the global setting says".
 */
export type DefaultOrderOverride = 'inherit' | DefaultTableSort;

/**
 * The order a read asks for, given the panel's override and the global
 * setting.
 *
 * The panel's own choice wins over the global setting; the global setting
 * wins over the driver's own default. The order of the two is the same one
 * the interface shows: a column the user clicked beats everything, and the
 * panel's temporary choice beats the setting that outlives the tab.
 */
export function resolveDefaultTableSort(
  override: DefaultOrderOverride,
  setting: DefaultTableSort,
): DefaultTableSort {
  return override === 'inherit' ? setting : override;
}
