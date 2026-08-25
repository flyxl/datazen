import {
  BookOpen,
  Code2,
  Download,
  GitFork,
  KeyRound,
  MessageSquare,
  Plus,
  RefreshCw,
  TableProperties,
} from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { useCompactToolbar } from '../../hooks/useCompactToolbar';
import { openDocsWindow } from '../../lib/windowManager';
import { DetailPanelToggle } from '../../components/DataTable/DetailPanelToggle';
import { ToolbarShell } from '../../components/ui/ToolbarShell';
import { ToolbarButton } from '../../components/ui/ToolbarButton';

export interface ContentToolbarProps {
  showNewQuery: boolean;
  showNewTable: boolean;
  showErDiagram: boolean;
  showObjects: boolean;
  showBatchExport: boolean;
  aiChatOpen: boolean;
  detailPanelApplicable: boolean;
  detailOpen: boolean;
  onNewQuery: () => void;
  onCreateTable: () => void;
  onOpenErDiagram: () => void;
  onOpenObjects: () => void;
  onOpenPrivileges: () => void;
  onBatchExport: () => void;
  onToggleAiChat: () => void;
  onToggleDetail: () => void;
  onRefresh: () => void;
}

export function ContentToolbar({
  showNewQuery,
  showNewTable,
  showErDiagram,
  showObjects,
  showBatchExport,
  aiChatOpen,
  detailPanelApplicable,
  detailOpen,
  onNewQuery,
  onCreateTable,
  onOpenErDiagram,
  onOpenObjects,
  onOpenPrivileges,
  onBatchExport,
  onToggleAiChat,
  onToggleDetail,
  onRefresh,
}: ContentToolbarProps) {
  const { t } = useI18n();
  const { ref: toolbarRef, compact } = useCompactToolbar(960);

  return (
    <ToolbarShell ref={toolbarRef} className="h-12 min-h-[48px] px-4">
      {showNewQuery && (
        <ToolbarButton
          compact={compact}
          variant="primary"
          className="h-8"
          label={t('connWin.newQuery')}
          icon={<Plus className="h-4 w-4" />}
          onClick={onNewQuery}
        />
      )}
      {showNewTable && (
        <ToolbarButton
          compact={compact}
          variant="secondary"
          className="h-8"
          label={t('connWin.newTable')}
          icon={<TableProperties className="h-4 w-4" />}
          onClick={onCreateTable}
        />
      )}
      {showErDiagram && (
        <ToolbarButton
          compact={compact}
          variant="secondary"
          className="h-8"
          label={t('erDiagram.title')}
          icon={<GitFork className="h-4 w-4" />}
          onClick={onOpenErDiagram}
        />
      )}
      {showObjects && (
        <>
          <ToolbarButton
            compact={compact}
            variant="secondary"
            className="h-8"
            label={t('objects.title')}
            icon={<Code2 className="h-4 w-4" />}
            onClick={onOpenObjects}
          />
          <ToolbarButton
            compact={compact}
            variant="secondary"
            className="h-8"
            label={t('privileges.title')}
            icon={<KeyRound className="h-4 w-4" />}
            onClick={onOpenPrivileges}
          />
        </>
      )}
      {showBatchExport && showNewQuery && (
        <ToolbarButton
          compact={compact}
          variant="secondary"
          className="h-8"
          data-testid="conn-toolbar-export"
          title={t('batchExport.title')}
          label={t('batchExport.title')}
          icon={<Download className="h-4 w-4" />}
          onClick={onBatchExport}
        />
      )}

      <div className="flex-1" />

      <ToolbarButton
        compact
        variant="ghost"
        title={`${t('connWin.refresh')} (⌘R)`}
        label={t('connWin.refresh')}
        icon={<RefreshCw className="h-3.5 w-3.5" />}
        onClick={onRefresh}
      />

      <ToolbarButton
        compact
        variant="ghost"
        title={t('docs.openAiHelp')}
        label={t('docs.openAiHelp')}
        icon={<BookOpen className="h-3.5 w-3.5" />}
        onClick={() => openDocsWindow('ai')}
      />

      <ToolbarButton
        compact
        variant={aiChatOpen ? 'secondary' : 'ghost'}
        label="AI"
        icon={<MessageSquare className="h-3.5 w-3.5" />}
        onClick={onToggleAiChat}
      />

      {detailPanelApplicable && <DetailPanelToggle open={detailOpen} onToggle={onToggleDetail} />}
    </ToolbarShell>
  );
}
