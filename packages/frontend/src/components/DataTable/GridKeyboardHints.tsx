/**
 * The keyboard hints of the grid (DB-02).
 *
 * They are a **description** of the grid, not a panel in it: the element is
 * read by a screen reader through `aria-describedby` and stays out of the way
 * of the grid itself. The wording lives in the English domain pack; the
 * component only decides where it is shown.
 */
import { useI18n } from '../../hooks/useI18n';

export function GridKeyboardHints({ id }: { id: string }) {
  const { t } = useI18n();
  return (
    <div id={id} className="sr-only">
      <p>{t('tableView.shortcut.hint')}</p>
      <ul>
        <li>{t('tableView.shortcut.enterEdit')}</li>
        <li>{t('tableView.shortcut.extendSelection')}</li>
        <li>{t('tableView.shortcut.moveByPage')}</li>
        <li>{t('tableView.shortcut.moveToEdge')}</li>
        <li>{t('tableView.shortcut.selectAllRows')}</li>
        <li>{t('tableView.shortcut.tabLeavesGrid')}</li>
      </ul>
    </div>
  );
}
