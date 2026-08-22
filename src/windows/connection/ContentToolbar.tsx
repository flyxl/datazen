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
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import { openDocsWindow } from '../../lib/windowManager';
import { DetailPanelToggle } from '../../components/DataTable/DetailPanelToggle';

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

  return (
    <div className="flex h-12 min-h-[48px] shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-4">
      {showNewQuery && (
        <Button variant="primary" className="h-8" onClick={onNewQuery}>
          <Plus className="h-4 w-4" />
          {t('connWin.newQuery')}
        </Button>
      )}
      {showNewTable && (
        <Button variant="secondary" className="h-8" onClick={onCreateTable}>
          <TableProperties className="h-4 w-4" />
          {t('connWin.newTable')}
        </Button>
      )}
      {showErDiagram && (
        <Button variant="secondary" className="h-8" onClick={onOpenErDiagram}>
          <GitFork className="h-4 w-4" />
          {t('erDiagram.title')}
        </Button>
      )}
      {showObjects && (
        <>
          <Button variant="secondary" className="h-8" onClick={onOpenObjects}>
            <Code2 className="h-4 w-4" />
            {t('objects.title')}
          </Button>
          <Button variant="secondary" className="h-8" onClick={onOpenPrivileges}>
            <KeyRound className="h-4 w-4" />
            {t('privileges.title')}
          </Button>
        </>
      )}
      {showBatchExport && showNewQuery && (
        <Button
          variant="secondary"
          className="h-8"
          data-testid="conn-toolbar-export"
          title={t('batchExport.title')}
          onClick={onBatchExport}
        >
          <Download className="h-4 w-4" />
          {t('batchExport.title')}
        </Button>
      )}

      <div className="flex-1" />

      <Button
        variant="ghost"
        className="h-7 w-7 !px-0"
        title={`${t('connWin.refresh')} (⌘R)`}
        onClick={onRefresh}
      >
        <RefreshCw className="h-3.5 w-3.5" />
      </Button>

      <Button
        variant="ghost"
        className="h-7 gap-1 px-2 text-xs"
        title={t('docs.openAiHelp')}
        onClick={() => openDocsWindow('ai')}
      >
        <BookOpen className="h-3.5 w-3.5" />
      </Button>

      <Button
        variant={aiChatOpen ? 'secondary' : 'ghost'}
        className="h-7 gap-1 px-2 text-xs"
        onClick={onToggleAiChat}
      >
        <MessageSquare className="h-3.5 w-3.5" />
      </Button>

      {detailPanelApplicable && <DetailPanelToggle open={detailOpen} onToggle={onToggleDetail} />}
    </div>
  );
}
