import type { SchemaDiffDeployResult, SchemaDiffPlan } from '../../commands/schemaDiff';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { SchemaDiffDeployPanel } from './SchemaDiffDeployPanel';
import { SchemaDiffPlanPanel } from './SchemaDiffPlanPanel';

export type SchemaDiffRightPanelTab = 'plan' | 'deploy';

export interface SchemaDiffRightPanelProps {
  activeTab: SchemaDiffRightPanelTab;
  onTabChange: (tab: SchemaDiffRightPanelTab) => void;
  plan: SchemaDiffPlan | null;
  allowDestructive: boolean;
  includeIndexes: boolean;
  onAllowDestructiveChange: (value: boolean) => void;
  onIncludeIndexesChange: (value: boolean) => void;
  onRegenerate: () => void;
  regenerating?: boolean;
  targetLabel: string;
  useTransaction: boolean;
  onUseTransactionChange: (value: boolean) => void;
  requireRollback: boolean;
  onRequireRollbackChange: (value: boolean) => void;
  confirmText: string;
  onConfirmTextChange: (value: string) => void;
  deploying: boolean;
  onDeploy: () => void;
  deployResult: SchemaDiffDeployResult | null;
  className?: string;
}

export function SchemaDiffRightPanel({
  activeTab,
  onTabChange,
  plan,
  allowDestructive,
  includeIndexes,
  onAllowDestructiveChange,
  onIncludeIndexesChange,
  onRegenerate,
  regenerating,
  targetLabel,
  useTransaction,
  onUseTransactionChange,
  requireRollback,
  onRequireRollbackChange,
  confirmText,
  onConfirmTextChange,
  deploying,
  onDeploy,
  deployResult,
  className,
}: SchemaDiffRightPanelProps) {
  const { t } = useI18n();

  return (
    <div
      className={cn('flex min-h-0 min-w-0 flex-[1.4] flex-col bg-surface', className)}
      data-testid="schema-diff-right-panel"
    >
      <div className="flex shrink-0 border-b border-edge">
        <button
          type="button"
          className={cn(
            'px-4 py-2 text-xs font-medium',
            activeTab === 'plan'
              ? 'border-b-2 border-accent text-fg'
              : 'text-fg-muted hover:text-fg',
          )}
          onClick={() => onTabChange('plan')}
          data-testid="schema-diff-plan-tab"
        >
          {t('schemaDiff.stepPlan')}
        </button>
        <button
          type="button"
          className={cn(
            'px-4 py-2 text-xs font-medium',
            activeTab === 'deploy'
              ? 'border-b-2 border-accent text-fg'
              : 'text-fg-muted hover:text-fg',
          )}
          onClick={() => onTabChange('deploy')}
          data-testid="schema-diff-deploy-tab"
        >
          {t('schemaDiff.stepReview')}
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-auto p-4">
        {activeTab === 'plan' && (
          <div data-testid="schema-diff-plan-panel">
            {plan ? (
              <SchemaDiffPlanPanel
                plan={plan}
                allowDestructive={allowDestructive}
                includeIndexes={includeIndexes}
                onAllowDestructiveChange={onAllowDestructiveChange}
                onIncludeIndexesChange={onIncludeIndexesChange}
                onRegenerate={onRegenerate}
                regenerating={regenerating}
              />
            ) : (
              <div className="flex h-full min-h-[8rem] items-center justify-center text-sm text-fg-muted">
                {t('schemaDiff.emptyPlan')}
              </div>
            )}
          </div>
        )}

        {activeTab === 'deploy' && (
          <div data-testid="schema-diff-deploy-panel">
            {plan ? (
              <SchemaDiffDeployPanel
                plan={plan}
                targetLabel={targetLabel}
                useTransaction={useTransaction}
                onUseTransactionChange={onUseTransactionChange}
                requireRollback={requireRollback}
                onRequireRollbackChange={onRequireRollbackChange}
                confirmText={confirmText}
                onConfirmTextChange={onConfirmTextChange}
                deploying={deploying}
                onDeploy={onDeploy}
                result={deployResult}
              />
            ) : (
              <div className="flex h-full min-h-[8rem] items-center justify-center text-sm text-fg-muted">
                {t('schemaDiff.generatePlan')}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
