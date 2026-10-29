import {
  BookOpen,
  Code2,
  Download,
  GitFork,
  KeyRound,
  MessageSquare,
  Plus,
  TableProperties,
} from 'lucide-react';
import { useI18n } from '../../hooks/useI18n';
import { useKvSlotSelectedKey } from '../../hooks/useKvSlotSelectedKey';
import {
  estimateExpandedToolbarWidth,
  TOOLBAR_GAP,
  useCompactToolbar,
} from '../../hooks/useCompactToolbar';
import { openDocsWindow } from '../../lib/windowManager';
import { hasKvAiFacts } from '../../lib/kvAiContext';
import { tid } from '../../lib/tid';
import { DetailPanelToggle } from '../../components/DataTable/DetailPanelToggle';
import { ToolbarShell } from '../../components/ui/ToolbarShell';
import { ToolbarButton } from '../../components/ui/ToolbarButton';
import type { KvContextBarBinding } from './useKvWorkspaceSlots';
import type { KvSlotState } from '@datazen/driver-sdk';

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
  /**
   * Driver-contributed KV context bar for the 48px left cluster. Absent (or not
   * provided by this build) ⇒ the toolbar keeps its plain spacer, exactly as
   * before KV slots existed.
   */
  contextBarSlot?: KvContextBarBinding;
  /**
   * State relay of the active KV panel; `undefined` unless the active panel belongs
   * to a key-value driver (`isKeyValue` metadata, so no driver id is named here).
   * The toolbar needs it for the AI button: while no key is in scope the assistant
   * would be handed nothing about a KV panel, so the button is not rendered at
   * all instead of opening a chat with no context.
   */
  kvPanelState?: KvSlotState;
  onNewQuery: () => void;
  onCreateTable: () => void;
  onOpenErDiagram: () => void;
  onOpenObjects: () => void;
  onOpenPrivileges: () => void;
  onBatchExport: () => void;
  onToggleAiChat: () => void;
  onToggleDetail: () => void;
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
  contextBarSlot,
  kvPanelState,
  onNewQuery,
  onCreateTable,
  onOpenErDiagram,
  onOpenObjects,
  onOpenPrivileges,
  onBatchExport,
  onToggleAiChat,
  onToggleDetail,
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
  const ContextBar = contextBarSlot?.Component;

  // On a KV panel the assistant has exactly one host-owned
  // fact to be told about — the selected key — so without one the button is not
  // rendered. Relational panels keep their existing behaviour (`contextTables`).
  const kvSelectedKey = useKvSlotSelectedKey(kvPanelState);
  const showAiChat = !kvPanelState || hasKvAiFacts({ selectedKey: kvSelectedKey });

  return (
    <ToolbarShell ref={toolbarRef} className="h-12 min-h-[48px] px-3">
      {/*
        KV context bar: the driver's 48px left cluster. It takes the spacer's
        flex-1 so the band is actually filled instead of pushing an empty gap
        into the middle (P-1). Absent ⇒ the plain spacer below, unchanged.
      */}
      {ContextBar && contextBarSlot && (
        <div
          className="flex min-w-0 flex-1 items-center gap-2"
          data-slot="kv-context-bar"
          data-testid="conn-toolbar-kv-context-bar"
        >
          <ContextBar {...contextBarSlot.props} compact={compact} />
        </div>
      )}
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
          data-testid="content-toolbar-new-table"
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
            data-testid="content-toolbar-er-diagram-button"
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
            data-testid="content-toolbar-objects"
          />
          <ToolbarButton
            compact={compact}
            variant="secondary"
            className="h-8"
            label={t('privileges.title')}
            icon={<KeyRound className="h-4 w-4" />}
            onClick={onOpenPrivileges}
            data-testid="content-toolbar-privileges"
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

      {!ContextBar && <div className="flex-1" />}

      <ToolbarButton
        compact
        variant="ghost"
        title={t('docs.openAiHelp')}
        label={t('docs.openAiHelp')}
        icon={<BookOpen className="h-3.5 w-3.5" />}
        onClick={() => openDocsWindow('ai')}
      />

      {showAiChat && (
        <ToolbarButton
          compact
          variant={aiChatOpen ? 'secondary' : 'ghost'}
          // On a KV panel the tooltip says what the assistant will be told about;
          // elsewhere the button keeps its pre-track title (nothing else changes).
          title={kvPanelState ? t('redis.ai.context.tooltip') : undefined}
          label="AI"
          icon={<MessageSquare className="h-3.5 w-3.5" />}
          onClick={onToggleAiChat}
          data-testid="conn-toolbar-ai"
        />
      )}

      {detailPanelApplicable && <DetailPanelToggle open={detailOpen} onToggle={onToggleDetail} />}
    </ToolbarShell>
  );
}
