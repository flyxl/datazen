import type {
  ColumnTypeOverride,
  SchemaDiffDeployResult,
  SchemaDiffPlan,
} from '../../commands/schemaDiff';
import { useI18n } from '../../hooks/useI18n';
import { cn } from '../../lib/cn';
import { Tabs } from '../../components/ui/Tabs';
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
  typeOverrides?: ColumnTypeOverride[];
  onTypeOverrideChange?: (table: string, column: string, targetType: string) => void;
  onApplyTypeOverrides?: () => void;
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
  /** Hide Plan / Deploy tab strip (wizard mode shows one step at a time). */
  hideTabs?: boolean;
  hideDeployButton?: boolean;
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
  deploying,
  onDeploy,
  deployResult,
  className,
  hideTabs = false,
  hideDeployButton = false,
}: SchemaDiffRightPanelProps) {
  const { t } = useI18n();

  return (
    <div
      className={cn('flex min-h-0 min-w-0 flex-[1.4] flex-col bg-surface', className)}
      data-testid="schema-diff-right-panel"
    >
      <div className="flex shrink-0 border-b border-edge">
        {!hideTabs && (
          <Tabs
            items={[
              { id: 'plan', label: t('schemaDiff.stepPlan'), testId: 'schema-diff-plan-tab' },
              { id: 'deploy', label: t('schemaDiff.stepReview'), testId: 'schema-diff-deploy-tab' },
            ]}
            activeId={activeTab}
            onChange={(id) => onTabChange(id as SchemaDiffRightPanelTab)}
            className="flex shrink-0 border-b border-edge"
            getTabClassName={({ selected }) =>
              cn(
                'px-4 py-2 text-xs font-medium',
                selected ? 'border-b-2 border-accent text-fg' : 'text-fg-muted hover:text-fg',
              )
            }
            ariaLabel={t('common.schemaDiff')}
          />
        )}
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
                typeOverrides={typeOverrides}
                onTypeOverrideChange={onTypeOverrideChange}
                onApplyTypeOverrides={onApplyTypeOverrides}
              />
            ) : (
              <div className="flex h-full min-h-[8rem] items-center justify-center text-sm text-fg-muted">
                {t('schemaDiff.generatePlan')}
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
                hideDeployButton={hideDeployButton}
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
