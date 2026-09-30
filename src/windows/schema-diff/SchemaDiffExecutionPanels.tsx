import type { ReactNode } from 'react';
import { Copy } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Spinner } from '../../components/ui/Spinner';
import { SchemaDiffPanel } from '../../components/schema/SchemaDiffPanel';
import { useI18n } from '../../hooks/useI18n';
import type {
  SchemaDiffDeployResult,
  SchemaDiffPlan,
  ColumnTypeOverride,
} from '../../commands/schemaDiff';
import type { TableSchemaDiff } from '../../types';
import { SchemaDiffRightPanel } from './SchemaDiffRightPanel';
import { SchemaDiffTableListPanel } from './SchemaDiffTableListPanel';

type ExecutionStep = 'compare' | 'plan' | 'deploy';
type ClipboardFeedback = 'summary' | 'sql' | 'config' | null;

interface SchemaDiffExecutionPanelsProps {
  step: ExecutionStep;
  loading: boolean;
  diffs: TableSchemaDiff[];
  selectedDiff: TableSchemaDiff | null;
  selectedTables: string[];
  selectedTable: string | null;
  onSelectTable: (table: string) => void;
  tableHasDiff: Record<string, boolean>;
  tableListWidth: number;
  tableListResizeRef: (element: HTMLDivElement | null) => void;
  clipboardFeedback: ClipboardFeedback;
  onCopySummary: () => void;
  plan: SchemaDiffPlan | null;
  planActions: ReactNode;
  allowDestructive: boolean;
  onAllowDestructiveChange: (value: boolean) => void;
  includeIndexes: boolean;
  onIncludeIndexesChange: (value: boolean) => void;
  typeOverrides: ColumnTypeOverride[];
  onTypeOverrideChange: (table: string, column: string, targetType: string) => void;
  onApplyTypeOverrides: () => void;
  targetLabel: string;
  useTransaction: boolean;
  onUseTransactionChange: (value: boolean) => void;
  requireRollback: boolean;
  onRequireRollbackChange: (value: boolean) => void;
  confirmText: string;
  onConfirmTextChange: (value: string) => void;
  deployResult: SchemaDiffDeployResult | null;
  onRegenerate: () => void;
  onDeploy: () => void;
}

export function SchemaDiffExecutionPanels({
  step,
  loading,
  diffs,
  selectedDiff,
  selectedTables,
  selectedTable,
  onSelectTable,
  tableHasDiff,
  tableListWidth,
  tableListResizeRef,
  clipboardFeedback,
  onCopySummary,
  plan,
  planActions,
  allowDestructive,
  onAllowDestructiveChange,
  includeIndexes,
  onIncludeIndexesChange,
  typeOverrides,
  onTypeOverrideChange,
  onApplyTypeOverrides,
  targetLabel,
  useTransaction,
  onUseTransactionChange,
  requireRollback,
  onRequireRollbackChange,
  confirmText,
  onConfirmTextChange,
  deployResult,
  onRegenerate,
  onDeploy,
}: SchemaDiffExecutionPanelsProps) {
  const { t } = useI18n();
  return (
    <>
      {step === 'compare' && (
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
          {loading && diffs.length === 0 ? (
            <div className="flex flex-1 items-center justify-center gap-2 text-sm text-fg-muted">
              <Spinner size="lg" />
              {t('schemaDiff.compare')}
            </div>
          ) : (
            <div className="flex min-h-0 flex-1 overflow-hidden rounded-lg border border-edge">
              <SchemaDiffTableListPanel
                className="flex-none"
                style={{ width: tableListWidth }}
                tables={selectedTables}
                selectedTable={selectedTable}
                onSelect={onSelectTable}
                tableHasDiff={diffs.length > 0 ? tableHasDiff : undefined}
              />
              <div
                ref={tableListResizeRef}
                className="w-1 shrink-0 cursor-col-resize bg-transparent hover:bg-accent/30"
              />
              <div
                className="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto bg-surface-alt/30"
                data-testid="schema-diff-detail-panel"
              >
                {selectedDiff ? (
                  <div className="p-4">
                    <div className="mb-3 flex items-center justify-between gap-2">
                      <h3 className="font-mono text-sm font-medium text-fg">
                        {selectedDiff.table}
                      </h3>
                      {diffs.length > 0 && (
                        <Button variant="secondary" size="sm" onClick={onCopySummary}>
                          <Copy className="h-4 w-4" />
                          {clipboardFeedback === 'summary'
                            ? t('common.copied')
                            : t('schemaDiff.copySummary')}
                        </Button>
                      )}
                    </div>
                    <SchemaDiffPanel diff={selectedDiff} />
                  </div>
                ) : (
                  <div className="flex flex-1 items-center justify-center p-4 text-sm text-fg-muted">
                    {t('schemaDiff.selectTableHint')}
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      )}

      {step === 'plan' && (
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden">
          {loading && !plan && (
            <div className="flex items-center gap-2 text-sm text-fg-muted">
              <Spinner size="lg" />
              {t('schemaDiff.generating')}
            </div>
          )}
          {planActions}
          <SchemaDiffRightPanel
            className="min-h-0 flex-1 border border-edge"
            activeTab="plan"
            onTabChange={() => {}}
            plan={plan}
            allowDestructive={allowDestructive}
            includeIndexes={includeIndexes}
            onAllowDestructiveChange={onAllowDestructiveChange}
            onIncludeIndexesChange={onIncludeIndexesChange}
            onRegenerate={onRegenerate}
            regenerating={loading && Boolean(plan)}
            typeOverrides={typeOverrides}
            onTypeOverrideChange={onTypeOverrideChange}
            onApplyTypeOverrides={onApplyTypeOverrides}
            targetLabel={targetLabel}
            useTransaction={useTransaction}
            onUseTransactionChange={onUseTransactionChange}
            requireRollback={requireRollback}
            onRequireRollbackChange={onRequireRollbackChange}
            confirmText={confirmText}
            onConfirmTextChange={onConfirmTextChange}
            deploying={false}
            onDeploy={() => {}}
            deployResult={null}
            hideTabs
          />
        </div>
      )}

      {step === 'deploy' && (
        <SchemaDiffRightPanel
          className="min-h-0 flex-1 border border-edge"
          activeTab="deploy"
          onTabChange={() => {}}
          plan={plan}
          allowDestructive={allowDestructive}
          includeIndexes={includeIndexes}
          onAllowDestructiveChange={onAllowDestructiveChange}
          onIncludeIndexesChange={onIncludeIndexesChange}
          onRegenerate={onRegenerate}
          regenerating={loading && Boolean(plan)}
          targetLabel={targetLabel}
          useTransaction={useTransaction}
          onUseTransactionChange={onUseTransactionChange}
          requireRollback={requireRollback}
          onRequireRollbackChange={onRequireRollbackChange}
          confirmText={confirmText}
          onConfirmTextChange={onConfirmTextChange}
          deploying={loading}
          onDeploy={onDeploy}
          deployResult={deployResult}
          hideTabs
          hideDeployButton
        />
      )}
    </>
  );
}
