import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import type { SchemaDiffPlan } from '../../../commands/schemaDiff';
import { SchemaDiffRightPanel } from '../SchemaDiffRightPanel';
import { SchemaDiffPlanPanel } from '../SchemaDiffPlanPanel';
import { SchemaDiffDeployPanel } from '../SchemaDiffDeployPanel';
import { SchemaDiffTableListPanel } from '../SchemaDiffTableListPanel';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
    language: 'en',
  }),
}));

afterEach(() => {
  cleanup();
});

const samplePlan: SchemaDiffPlan = {
  table: 'users',
  tables: ['users'],
  sourceDialect: 'postgres',
  targetDialect: 'postgres',
  sameDialect: true,
  statements: [
    {
      sql: 'ALTER TABLE users ADD COLUMN email text;',
      risk: 'additive',
      rollbackSql: null,
      summary: 'Add email',
    },
  ],
  warnings: [],
  requirements: [],
  rollbackCompleteness: { complete: true, missing: [] },
};

describe('SchemaDiffTableListPanel', () => {
  it('renders table rows and selection', () => {
    const onSelect = vi.fn();
    render(
      <SchemaDiffTableListPanel
        tables={['users', 'orders']}
        selectedTable="users"
        onSelect={onSelect}
        tableHasDiff={{ users: true, orders: false }}
      />,
    );

    expect(screen.getByTestId('schema-diff-table-list')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-table-row-users')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-table-row-orders')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('schema-diff-table-row-orders'));
    expect(onSelect).toHaveBeenCalledWith('orders');
  });
});

describe('SchemaDiffRightPanel', () => {
  it('switches between plan and deploy tabs', () => {
    const onTabChange = vi.fn();
    render(
      <SchemaDiffRightPanel
        activeTab="plan"
        onTabChange={onTabChange}
        plan={samplePlan}
        allowDestructive={false}
        includeIndexes
        onAllowDestructiveChange={vi.fn()}
        onIncludeIndexesChange={vi.fn()}
        onRegenerate={vi.fn()}
        targetLabel="local (postgres)"
        useTransaction
        onUseTransactionChange={vi.fn()}
        requireRollback={false}
        onRequireRollbackChange={vi.fn()}
        confirmText=""
        onConfirmTextChange={vi.fn()}
        deploying={false}
        onDeploy={vi.fn()}
        deployResult={null}
      />,
    );

    expect(screen.getByTestId('schema-diff-right-panel')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-plan-panel')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-allow-destructive')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('schema-diff-deploy-tab'));
    expect(onTabChange).toHaveBeenCalledWith('deploy');
  });

  it('asks the user to generate a plan before a plan exists', () => {
    render(
      <SchemaDiffRightPanel
        activeTab="plan"
        onTabChange={vi.fn()}
        plan={null}
        allowDestructive={false}
        includeIndexes
        onAllowDestructiveChange={vi.fn()}
        onIncludeIndexesChange={vi.fn()}
        onRegenerate={vi.fn()}
        targetLabel="local (postgres)"
        useTransaction
        onUseTransactionChange={vi.fn()}
        requireRollback={false}
        onRequireRollbackChange={vi.fn()}
        confirmText=""
        onConfirmTextChange={vi.fn()}
        deploying={false}
        onDeploy={vi.fn()}
        deployResult={null}
      />,
    );

    const panels = screen.getAllByTestId('schema-diff-plan-panel');
    expect(panels[panels.length - 1]).toHaveTextContent('schemaDiff.generatePlan');
  });
});

describe('SchemaDiffPlanPanel empty plans', () => {
  afterEach(() => {
    cleanup();
  });

  it('distinguishes a clean plan from a warning-only plan', () => {
    const baseProps = {
      allowDestructive: false,
      includeIndexes: true,
      onAllowDestructiveChange: vi.fn(),
      onIncludeIndexesChange: vi.fn(),
      onRegenerate: vi.fn(),
      regenerating: false,
    };
    const cleanPlan: SchemaDiffPlan = {
      ...samplePlan,
      statements: [],
      warnings: [],
    };
    const { unmount } = render(<SchemaDiffPlanPanel plan={cleanPlan} {...baseProps} />);
    expect(screen.getByTestId('schema-diff-empty-plan')).toHaveTextContent(
      'schemaDiff.emptyPlanNoDiff',
    );

    unmount();
    const skippedPlan: SchemaDiffPlan = {
      ...cleanPlan,
      warnings: ['Skipped DROP COLUMN old_col because destructive changes are disabled'],
    };
    render(<SchemaDiffPlanPanel plan={skippedPlan} {...baseProps} />);
    expect(screen.getByTestId('schema-diff-empty-plan')).toHaveTextContent(
      'schemaDiff.emptyPlanSkipped',
    );
  });

  it('renders backfill and unsupported requirements above statements', () => {
    const planWithRequirements: SchemaDiffPlan = {
      ...samplePlan,
      requirements: [
        {
          kind: 'Backfill',
          table: 'users',
          column: 'status',
          reason: 'Populate existing rows before enforcing NOT NULL.',
        },
        {
          kind: 'Unsupported',
          table: 'users',
          column: 'meta',
          reason: 'Operation is not supported by mysql',
        },
      ],
    };
    render(
      <SchemaDiffPlanPanel
        plan={planWithRequirements}
        allowDestructive={false}
        includeIndexes
        onAllowDestructiveChange={vi.fn()}
        onIncludeIndexesChange={vi.fn()}
        onRegenerate={vi.fn()}
      />,
    );

    const panel = screen.getByTestId('schema-diff-plan-requirements');
    expect(panel).toHaveTextContent('schemaDiff.requirement.backfillTitle');
    expect(panel).toHaveTextContent('users.status');
    expect(panel).toHaveTextContent('schemaDiff.requirement.backfillHint');
    expect(panel).toHaveTextContent('schemaDiff.requirement.unsupportedTitle');
    expect(panel).toHaveTextContent('users.meta: Operation is not supported by mysql');
  });

  it('shows rollback completeness status at the bottom', () => {
    const partialPlan: SchemaDiffPlan = {
      ...samplePlan,
      statements: [
        ...samplePlan.statements,
        {
          sql: 'DROP INDEX idx_users_email;',
          risk: 'destructive',
          rollbackSql: null,
          summary: 'DROP INDEX idx_users_email',
        },
      ],
      rollbackCompleteness: {
        complete: false,
        missing: ['DROP INDEX idx_users_email'],
      },
    };
    const { container, unmount } = render(
      <SchemaDiffPlanPanel
        plan={partialPlan}
        allowDestructive={false}
        includeIndexes
        onAllowDestructiveChange={vi.fn()}
        onIncludeIndexesChange={vi.fn()}
        onRegenerate={vi.fn()}
      />,
    );
    expect(within(container).getByTestId('schema-diff-rollback-status')).toHaveTextContent(
      'schemaDiff.rollback.partial',
    );

    unmount();
    const { container: availableContainer } = render(
      <SchemaDiffPlanPanel
        plan={samplePlan}
        allowDestructive={false}
        includeIndexes
        onAllowDestructiveChange={vi.fn()}
        onIncludeIndexesChange={vi.fn()}
        onRegenerate={vi.fn()}
      />,
    );
    expect(within(availableContainer).getByTestId('schema-diff-rollback-status')).toHaveTextContent(
      'schemaDiff.rollback.available',
    );
  });

  it('renders type suggestions notice and handles override change and apply', () => {
    const onOverrideChange = vi.fn();
    const onApply = vi.fn();
    const planWithSug: SchemaDiffPlan = {
      ...samplePlan,
      typeSuggestions: [
        {
          table: 'demo_customers',
          column: 'region',
          sourceType: 'text',
          suggestedType: 'VARCHAR(255)',
          currentType: 'VARCHAR(255)',
          reason: 'Indexed column; MySQL requires explicit key prefix length',
          isKeyOrIndexed: true,
        },
      ],
    };

    render(
      <SchemaDiffPlanPanel
        plan={planWithSug}
        allowDestructive={false}
        includeIndexes
        onAllowDestructiveChange={vi.fn()}
        onIncludeIndexesChange={vi.fn()}
        onRegenerate={vi.fn()}
        onTypeOverrideChange={onOverrideChange}
        onApplyTypeOverrides={onApply}
      />,
    );

    const notice = screen.getByTestId('schema-diff-type-suggestions');
    expect(notice).toHaveTextContent('schemaDiff.typeSuggestions.title');
    expect(notice).toHaveTextContent('demo_customers.region');
    expect(notice).toHaveTextContent('text');
    expect(notice).toHaveTextContent('Indexed column; MySQL requires explicit key prefix length');

    const input = within(notice).getByDisplayValue('VARCHAR(255)');
    fireEvent.change(input, { target: { value: 'VARCHAR(64)' } });
    expect(onOverrideChange).toHaveBeenCalledWith('demo_customers', 'region', 'VARCHAR(64)');

    const applyBtn = within(notice).getByText('schemaDiff.typeSuggestions.apply');
    fireEvent.click(applyBtn);
    expect(onApply).toHaveBeenCalled();
  });
});

describe('SchemaDiffDeployPanel', () => {
  it('renders committed deploy result with count and no errors', () => {
    render(
      <SchemaDiffDeployPanel
        plan={samplePlan}
        targetLabel="demo_db"
        useTransaction
        onUseTransactionChange={vi.fn()}
        requireRollback={false}
        onRequireRollbackChange={vi.fn()}
        confirmText=""
        onConfirmTextChange={vi.fn()}
        deploying={false}
        onDeploy={vi.fn()}
        result={{
          status: 'committed',
          executedCount: 4,
          statementCount: 4,
          errors: [],
          statementResults: [],
        }}
      />,
    );

    expect(screen.getByTestId('schema-diff-deploy-result')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-deploy-status')).toHaveTextContent('committed');
    expect(screen.getByTestId('schema-diff-deploy-count')).toHaveTextContent('4/4');
    expect(screen.queryByTestId('schema-diff-deploy-errors')).not.toBeInTheDocument();
  });

  it('renders failed deploy result with error messages list', () => {
    render(
      <SchemaDiffDeployPanel
        plan={samplePlan}
        targetLabel="demo_db"
        useTransaction
        onUseTransactionChange={vi.fn()}
        requireRollback={false}
        onRequireRollbackChange={vi.fn()}
        confirmText=""
        onConfirmTextChange={vi.fn()}
        deploying={false}
        onDeploy={vi.fn()}
        result={{
          status: 'failed',
          executedCount: 1,
          statementCount: 4,
          errors: ['Query failed: Multiple primary key defined'],
          statementResults: [],
        }}
      />,
    );

    expect(screen.getByTestId('schema-diff-deploy-result')).toBeInTheDocument();
    expect(screen.getByTestId('schema-diff-deploy-status')).toHaveTextContent('failed');
    expect(screen.getByTestId('schema-diff-deploy-count')).toHaveTextContent('1/4');
    const errorsList = screen.getByTestId('schema-diff-deploy-errors');
    expect(errorsList).toBeInTheDocument();
    expect(errorsList).toHaveTextContent('Multiple primary key defined');
  });
});
