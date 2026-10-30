import { useI18n } from '../../hooks/useI18n';
import { usePlatform } from '../../hooks/usePlatform';
import { getDbLabel } from '../../lib/databaseTypes';
import { formatModKey } from '../../lib/keymap';
import { formatRowCount } from '../../lib/totalRowsDisplay';
import type { CountStrategy, DatabaseType } from '../../types';
import type { KvStatusBarBinding } from './useKvWorkspaceSlots';

export interface ContentStatusBarProps {
  databaseType?: DatabaseType;
  connectionName?: string;
  currentDatabase: string | null;
  tableName: string;
  columnCount: number;
  totalRows: number;
  /**
   * How much `totalRows` may be trusted.
   *
   * An estimate is shown as "about N rows", and a database that cannot count
   * says so — the alternative, a silent "0 rows", is a statement the back end
   * never made.
   */
  totalRowsKind?: CountStrategy | null;
  /**
   * Driver-contributed KV status bar. When present it takes the centre cluster —
   * which is always empty on a KV panel — and the rest of the footer is untouched,
   * so a driver without the `statusBar` capability keeps the byte-identical footer.
   */
  statusBarSlot?: KvStatusBarBinding;
}

export function ContentStatusBar({
  databaseType,
  connectionName,
  currentDatabase,
  tableName,
  columnCount,
  totalRows,
  totalRowsKind,
  statusBarSlot,
}: ContentStatusBarProps) {
  const { t } = useI18n();
  const platform = usePlatform();
  const isMac = platform === 'macos';
  const mod = formatModKey(platform);
  const keySep = isMac ? '' : '+';
  const Slot = statusBarSlot?.Component;

  /**
   * The row count in the footer's own words.
   *
   * The estimate keeps its "about" sign here too: a number that is an
   * approximation must not lose its approximation on the way to the footer.
   * The unknown state says that the count is unavailable instead of showing a
   * zero nobody counted.
   */
  const rowCountText =
    totalRowsKind === 'unsupported'
      ? t('pagination.rowsUnknown')
      : totalRowsKind === 'estimated'
        ? `${t('pagination.rowCountEstimated', { count: formatRowCount(totalRows) })} ${t('connWin.rowCount')}`
        : totalRows > 0
          ? `${formatRowCount(totalRows)} ${t('connWin.rowCount')}`
          : null;

  return (
    <footer
      role="status"
      className="flex h-10 min-h-[40px] shrink-0 items-center justify-between border-t border-edge bg-surface-alt px-4 text-xs text-fg-secondary"
    >
      <div className="flex items-center gap-2">
        <span className="inline-flex h-2 w-2 rounded-full bg-success" aria-hidden="true" />
        <span>{t('connWin.connected')}</span>
      </div>
      {Slot && statusBarSlot ? (
        <div
          className="flex min-w-0 flex-1 items-center gap-2 px-4"
          data-slot="kv-status-bar"
          data-testid="conn-status-kv-bar"
        >
          <Slot {...statusBarSlot.props} />
        </div>
      ) : (
        <div className="min-w-0 flex-1 truncate px-4 text-center text-fg-muted">
          {[
            databaseType ? getDbLabel(databaseType) : null,
            connectionName || null,
            currentDatabase,
            tableName,
            columnCount > 0 && `${columnCount} ${t('connWin.fields')}`,
            rowCountText,
          ]
            .filter(Boolean)
            .join(' · ')}
        </div>
      )}
      <div className="shrink-0 text-fg-muted">
        <kbd className="font-mono">
          {mod}
          {keySep}N
        </kbd>{' '}
        {t('common.newQuery')} ·{' '}
        <kbd className="font-mono">
          {mod}
          {keySep}W
        </kbd>{' '}
        {t('common.close')} · <kbd className="font-mono">Space</kbd> {t('detail.title')}
      </div>
    </footer>
  );
}
