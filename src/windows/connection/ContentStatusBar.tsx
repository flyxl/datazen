import { useI18n } from '../../hooks/useI18n';
import { usePlatform } from '../../hooks/usePlatform';
import { getDbLabel } from '../../lib/databaseTypes';
import type { DatabaseType } from '../../types';

export interface ContentStatusBarProps {
  databaseType?: DatabaseType;
  connectionName?: string;
  currentDatabase: string | null;
  tableName: string;
  columnCount: number;
  totalRows: number;
}

export function ContentStatusBar({
  databaseType,
  connectionName,
  currentDatabase,
  tableName,
  columnCount,
  totalRows,
}: ContentStatusBarProps) {
  const { t } = useI18n();
  const platform = usePlatform();
  const isMac = platform === 'macos';
  const mod = isMac ? '⌘' : 'Ctrl';
  const keySep = isMac ? '' : '+';

  return (
    <footer
      role="status"
      className="flex h-10 min-h-[40px] shrink-0 items-center justify-between border-t border-edge bg-surface-alt px-4 text-xs text-fg-secondary"
    >
      <div className="flex items-center gap-2">
        <span className="inline-flex h-2 w-2 rounded-full bg-success" aria-hidden="true" />
        <span>{t('connWin.connected')}</span>
      </div>
      <div className="truncate text-fg-muted">
        {[
          databaseType ? getDbLabel(databaseType) : null,
          connectionName || null,
          currentDatabase,
          tableName,
          columnCount > 0 && `${columnCount} ${t('connWin.fields')}`,
          totalRows > 0 && `${totalRows} ${t('connWin.rowCount')}`,
        ]
          .filter(Boolean)
          .join(' · ')}
      </div>
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
