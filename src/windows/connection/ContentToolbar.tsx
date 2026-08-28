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
import {
  estimateExpandedToolbarWidth,
  TOOLBAR_GAP,
  useCompactToolbar,
} from '../../hooks/useCompactToolbar';
import { openDocsWindow } from '../../lib/windowManager';
import { tid } from '../../lib/tid';
import { DetailPanelToggle } from '../../components/DataTable/DetailPanelToggle';
import { ToolbarShell } from '../../components/ui/ToolbarShell';
import { ToolbarButton } from '../../components/ui/ToolbarButton';

/** Minimum toolbar width (px) to show text labels for the visible left-side actions. */
export function contentToolbarExpandedMinWidth({
  showNewQuery,
  showNewTable,
  showErDiagram,
  showObjects,
  showBatchExport,
  detailPanelApplicable,
}: Pick<
  ContentToolbarProps,
  | 'showNewQuery'
  | 'showNewTable'
  | 'showErDiagram'
  | 'showObjects'
  | 'showBatchExport'
  | 'detailPanelApplicable'
>): number {
  let leftButtons = 0;
  if (showNewQuery) leftButtons += 1;
  if (showNewTable) leftButtons += 1;
  if (showErDiagram) leftButtons += 1;
  if (showObjects) leftButtons += 2;
  if (showBatchExport && showNewQuery) leftButtons += 1;

  const rightCluster = 120;
  const detailToggle = detailPanelApplicable ? 36 : 0;

  return estimateExpandedToolbarWidth({
    expandedButtonCount: leftButtons,
    fixedExtraWidth: TOOLBAR_GAP + rightCluster + detailToggle,
  });
}

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
  const expandedMinWidth = contentToolbarExpandedMinWidth({
    showNewQuery,
    showNewTable,
    showErDiagram,
    showObjects,
    showBatchExport,
    detailPanelApplicable,
  });
  const { ref: toolbarRef, compact } = useCompactToolbar(expandedMinWidth);

  return (
    <ToolbarShell ref={toolbarRef} className="h-12 min-h-[48px] px-4">
      {showNewQuery && (
        <ToolbarButton
          compact={compact}
          variant="primary"
          className="h-8"
          label={t('common.newQuery')}
          icon={<Plus className="h-4 w-4" />}
          onClick={onNewQuery}
          {...tid('conn-toolbar-new-query')}
        />
      )}
      {showNewTable && (
        <ToolbarButton
          compact={compact}
          variant="secondary"
          className="h-8"
          label={t('common.newTable')}
          icon={<TableProperties className="h-4 w-4" />}
          onClick={onCreateTable}
        />
      )}
      {showErDiagram && (
        <span data-testid="content-toolbar-er-diagram">
          <ToolbarButton
            compact={compact}
            variant="secondary"
            className="h-8"
            label={t('common.erDiagram')}
            icon={<GitFork className="h-4 w-4" />}
            onClick={onOpenErDiagram}
          />
        </span>
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
        data-testid="conn-toolbar-ai"
      />

      {detailPanelApplicable && <DetailPanelToggle open={detailOpen} onToggle={onToggleDetail} />}
    </ToolbarShell>
  );
}
