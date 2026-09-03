import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import type { SchemaDiffPlan } from '../../../commands/schemaDiff';
import { SchemaDiffRightPanel } from '../SchemaDiffRightPanel';
import { SchemaDiffPlanPanel } from '../SchemaDiffPlanPanel';
import { SchemaDiffTableListPanel } from '../SchemaDiffTableListPanel';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
    language: 'en',
  }),
}));

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
});
