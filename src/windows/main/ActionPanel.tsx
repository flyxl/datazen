import { DatabaseBackup, Download, FolderUp, FolderDown, Plus, RefreshCcw } from 'lucide-react';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';

export interface ActionPanelProps {
  onNewConnection: () => void;
  onBackup: () => void;
  onRestore: () => void;
  onDataSync: () => void;
  onExportConfig: () => void;
  onImportConfig: () => void;
}

export function ActionPanel({ onNewConnection, onBackup, onRestore, onDataSync, onExportConfig, onImportConfig }: ActionPanelProps) {
  const { t } = useI18n();
  const items = [
    { icon: DatabaseBackup, label: t('action.backup'), action: onBackup },
    { icon: Download, label: t('action.restore'), action: onRestore },
    { icon: RefreshCcw, label: t('action.dataSync'), action: onDataSync },
    { icon: FolderUp, label: t('action.exportConfig'), action: onExportConfig },
    { icon: FolderDown, label: t('action.importConfig'), action: onImportConfig },
    { icon: Plus, label: t('action.newConnection'), action: onNewConnection },
  ];

  return (
    <div className="flex h-full flex-col">
      {/* Logo */}
      <div className="flex flex-col items-center gap-3 px-3 pt-8 pb-4">
        <img
          src="/logo.png"
          alt="DataZen"
          className="h-24 w-24 drop-shadow-lg"
          draggable={false}
        />
        <span className="text-base font-bold tracking-wider text-fg">DataZen</span>
      </div>

      <div className="flex-1" />

      <div className="flex flex-col gap-0.5 p-3">
        {items.map((item) => (
          <button
            key={item.label}
            type="button"
            onClick={item.action}
            className={cn(
              'flex items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[13px] text-fg-secondary transition-colors',
              'hover:bg-surface-raised hover:text-fg',
            )}
          >
            <item.icon className="h-4 w-4 shrink-0" />
            {item.label}
          </button>
        ))}
      </div>
    </div>
  );
}
