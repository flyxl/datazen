import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react';
import type { SchemaDiffPlan } from '../../../commands/schemaDiff';
import { SchemaDiffRightPanel } from '../SchemaDiffRightPanel';
import { SchemaDiffPlanPanel } from '../SchemaDiffPlanPanel';
import { SchemaDiffDeployPanel } from '../SchemaDiffDeployPanel';
import { SchemaDiffTableListPanel } from '../SchemaDiffTableListPanel';
import { SchemaDiffObjectsStep } from '../SchemaDiffObjectsStep';
import { SchemaDiffUnifiedObjectsPicker } from '../SchemaDiffUnifiedObjectsPicker';
import { formatSchemaDiffText, SchemaDiffPanel } from '../../../components/schema/SchemaDiffPanel';

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

describe('SchemaDiffObjectsStep target-only picker', () => {
  it('shows source/target identity and leaves target-only tables unchecked', () => {
    render(
      <SchemaDiffObjectsStep
        loading={false}
        tables={[
          {
            name: 'users',
            enabled: true,
            origin: 'both',
            sourceName: 'public.users',
            targetName: 'users',
          },
          {
            name: 'archive',
            enabled: false,
            origin: 'target-only',
            targetName: 'archive',
          },
        ]}
        onToggle={vi.fn()}
        onSelectAll={vi.fn()}
        onSelectNone={vi.fn()}
      />,
    );

    const rows = screen.getAllByTestId('schema-diff-table-row');
    expect(rows[0]).toHaveAttribute('data-table-origin', 'both');
    expect(rows[1]).toHaveAttribute('data-table-origin', 'target-only');
    expect(within(rows[1]!).getByRole('checkbox')).not.toBeChecked();
    expect(screen.getByTestId('schema-diff-table-origin-archive')).toHaveTextContent(
      'schemaDiff.targetOnly',
    );
  });
});

describe('SchemaDiffUnifiedObjectsPicker discovery errors', () => {
  it('surfaces partial catalog failures and offers an explicit retry', () => {
    const onRetry = vi.fn();
    render(
      <SchemaDiffUnifiedObjectsPicker
        loading={false}
        sourceObjects={[]}
        targetObjects={[]}
        selectedSourceKeys={[]}
        selectedTargetKeys={[]}
        errors={{ source: { function: 'permission denied' }, target: {} }}
        crossDialect={false}
        onToggleSource={vi.fn()}
        onToggleTarget={vi.fn()}
        onSelectAll={vi.fn()}
        onClearSelections={vi.fn()}
        onRetry={onRetry}
      />,
    );

    expect(screen.getByRole('status')).toHaveTextContent('schemaDiff.objectPartialLoad');
    expect(screen.getByTestId('schema-diff-object-error-source-function')).toHaveTextContent(
      'schemaDiff.objectLoadFailed',
    );
    fireEvent.click(screen.getByTestId('schema-diff-object-retry'));
    expect(onRetry).toHaveBeenCalledOnce();
  });
});

describe('SchemaDiffPanel target-only review', () => {
  it('does not present a target-only table as identical', () => {
    render(
      <SchemaDiffPanel
        diff={{
          table: 'archive',
          targetOnly: true,
          missingOnTarget: [],
          extraOnTarget: [],
          added: [],
          removed: [],
          changed: [],
        }}
      />,
    );

    expect(screen.getByTestId('schema-diff-target-only-detail')).toBeInTheDocument();
    expect(screen.queryByText('schemaDiff.schemaIdentical')).not.toBeInTheDocument();
  });

  it('renders source additions, target extras and changed column details', () => {
    render(
      <SchemaDiffPanel
        diff={{
          table: 'users',
          missingOnTarget: [
            { name: 'email', dataType: 'text', nullable: true, isPrimaryKey: true },
          ],
          extraOnTarget: [
            { name: 'legacy', dataType: 'integer', nullable: false, isPrimaryKey: true },
          ],
          added: [],
          removed: [],
          changed: [
            {
              name: 'name',
              source: {
                name: 'name',
                dataType: 'varchar(128)',
                nullable: true,
                isPrimaryKey: true,
              },
              target: {
                name: 'name',
                dataType: 'text',
                nullable: false,
                isPrimaryKey: true,
              },
              changes: ['type', 'nullable'],
            },
          ],
        }}
      />,
    );

    expect(screen.getByText('schemaDiff.missingOnTarget')).toBeInTheDocument();
    expect(screen.getByText('+ email (text, PK)')).toBeInTheDocument();
    expect(screen.getByText('schemaDiff.extraOnTarget')).toBeInTheDocument();
    expect(screen.getByText('- legacy (integer, NOT NULL, PK)')).toBeInTheDocument();
    expect(screen.getByText('schemaDiff.colChanged')).toBeInTheDocument();
    expect(screen.getByText('name')).toBeInTheDocument();
    expect(screen.getByText('schemaDiff.source: varchar(128), PK')).toBeInTheDocument();
    expect(screen.getByText('schemaDiff.target: text, NOT NULL, PK')).toBeInTheDocument();
    expect(screen.getByText('type, nullable')).toBeInTheDocument();
    expect(screen.queryByText('schemaDiff.schemaIdentical')).not.toBeInTheDocument();
  });

  it('formats a reviewed diff summary with all selected column changes', () => {
    expect(
      formatSchemaDiffText({
        table: 'users',
        missingOnTarget: [
          { name: 'email', dataType: 'text', nullable: false, isPrimaryKey: false },
        ],
        extraOnTarget: [
          { name: 'legacy', dataType: 'integer', nullable: true, isPrimaryKey: false },
        ],
        added: [],
        removed: [],
        changed: [
          {
            name: 'name',
            source: {
              name: 'name',
              dataType: 'varchar(128)',
              nullable: true,
              isPrimaryKey: false,
            },
            target: {
              name: 'name',
              dataType: 'text',
              nullable: true,
              isPrimaryKey: false,
            },
            changes: ['type'],
          },
        ],
      }),
    ).toBe(
      '-- Schema diff: users\n+ email text NOT NULL\n- legacy integer\n~ name: text -> varchar(128) (type)',
    );
  });

  it('test_tester renders and exports CHECK constraint changes', () => {
    const diff = {
      table: 'users',
      missingOnTarget: [],
      extraOnTarget: [],
      added: [],
      removed: [],
      changed: [],
      missingCheckConstraints: [{ name: 'users_age_check', expression: 'age >= 0' }],
      extraCheckConstraints: [{ name: 'users_status_check', expression: "status <> 'deleted'" }],
    };

    render(<SchemaDiffPanel diff={diff} />);
    expect(screen.getByText('schemaDiff.checkMissing')).toBeInTheDocument();
    expect(screen.getByText('+ users_age_check: CHECK (age >= 0)')).toBeInTheDocument();
    expect(screen.getByText('schemaDiff.checkExtra')).toBeInTheDocument();
    expect(
      screen.getByText("- users_status_check: CHECK (status <> 'deleted')"),
    ).toBeInTheDocument();
    expect(formatSchemaDiffText(diff)).toBe(
      "-- Schema diff: users\n+ users_age_check: CHECK (age >= 0)\n- users_status_check: CHECK (status <> 'deleted')",
    );
  });

  it('test_tester renders and exports table option changes', () => {
    const diff = {
      table: 'orders',
      missingOnTarget: [],
      extraOnTarget: [],
      added: [],
      removed: [],
      changed: [],
      tableOptions: {
        source: { comment: "owner's orders", engine: 'InnoDB', charset: 'utf8mb4' },
        target: { comment: 'legacy orders', engine: 'MyISAM', charset: 'latin1' },
        changes: ['comment', 'engine', 'charset'],
      },
    };

    render(<SchemaDiffPanel diff={diff} />);
    expect(screen.getByText('schemaDiff.tableOptions')).toBeInTheDocument();
    expect(screen.getByText("~ comment: legacy orders -> owner's orders")).toBeInTheDocument();
    expect(screen.getByText('~ engine: MyISAM -> InnoDB')).toBeInTheDocument();
    expect(screen.getByText('~ charset: latin1 -> utf8mb4')).toBeInTheDocument();
    expect(formatSchemaDiffText(diff)).toBe(
      "-- Schema diff: orders\n~ table comment: legacy orders -> owner's orders\n~ table engine: MyISAM -> InnoDB\n~ table charset: latin1 -> utf8mb4",
    );
  });

  it('supports legacy aliases and identifies an unchanged schema', () => {
    const { rerender } = render(
      <SchemaDiffPanel
        diff={{
          table: 'legacy_users',
          added: [{ name: 'email', dataType: 'text', nullable: true, isPrimaryKey: false }],
          removed: [{ name: 'old_id', dataType: 'integer', nullable: true, isPrimaryKey: false }],
          changed: [],
        }}
      />,
    );
    expect(screen.getByText('+ email (text)')).toBeInTheDocument();
    expect(screen.getByText('- old_id (integer)')).toBeInTheDocument();

    rerender(<SchemaDiffPanel diff={{ table: 'same', added: [], removed: [], changed: [] }} />);
    expect(screen.getByText('schemaDiff.schemaIdentical')).toBeInTheDocument();
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

it('blocks execution across rollback and result state transitions', () => {
  const props = {
    plan: samplePlan,
    targetLabel: 'target',
    useTransaction: true,
    onUseTransactionChange: vi.fn(),
    requireRollback: true,
    onRequireRollbackChange: vi.fn(),
    confirmText: '',
    onConfirmTextChange: vi.fn(),
    deploying: false,
    onDeploy: vi.fn(),
    result: null,
  };
  const { rerender } = render(<SchemaDiffDeployPanel {...props} />);
  expect(screen.getByTestId('schema-diff-deploy')).toBeEnabled();
  rerender(<SchemaDiffDeployPanel {...props} useTransaction={false} />);
  expect(screen.getByTestId('schema-diff-deploy')).toBeDisabled();
  rerender(<SchemaDiffDeployPanel {...props} plan={{ ...samplePlan, targetDialect: 'mysql' }} />);
  expect(screen.getByTestId('schema-diff-deploy')).toBeDisabled();
  rerender(<SchemaDiffDeployPanel {...props} />);
  expect(screen.getByTestId('schema-diff-deploy')).toBeEnabled();
  rerender(
    <SchemaDiffDeployPanel
      {...props}
      result={{
        status: 'unknown',
        executedCount: 1,
        statementCount: 1,
        errors: ['COMMIT outcome unknown'],
        statementResults: [],
      }}
    />,
  );
  expect(screen.getByTestId('schema-diff-deploy')).toBeDisabled();
  expect(screen.getByTestId('schema-diff-deploy-status')).toHaveTextContent('unknown');
});

it('forces the transaction control for reviewed SQLite table rebuilds', () => {
  const plan = {
    ...samplePlan,
    targetDialect: 'sqlite',
    statements: [{ ...samplePlan.statements[0], requiresTransaction: true }],
  };
  render(
    <SchemaDiffDeployPanel
      plan={plan}
      targetLabel="SQLite target"
      useTransaction={false}
      onUseTransactionChange={vi.fn()}
      requireRollback={false}
      onRequireRollbackChange={vi.fn()}
      confirmText=""
      onConfirmTextChange={vi.fn()}
      deploying={false}
      onDeploy={vi.fn()}
      result={null}
    />,
  );

  const transactionCheckbox = screen.getAllByRole('checkbox')[0];
  expect(transactionCheckbox).toBeChecked();
  expect(transactionCheckbox).toBeDisabled();
  expect(screen.getByText('(schemaDiff.transactionRequired)')).toBeInTheDocument();
  expect(screen.getByTestId('schema-diff-deploy')).toBeEnabled();
});

describe('[tester] deployment review control journey', () => {
  it('forwards edits and keeps deployment closed until requirements clear', () => {
    const onUseTransactionChange = vi.fn();
    const onRequireRollbackChange = vi.fn();
    const onConfirmTextChange = vi.fn();
    const onDeploy = vi.fn();
    const plan = {
      ...samplePlan,
      statements: [{ ...samplePlan.statements[0], risk: 'destructive' as const }],
    };
    const props = {
      plan,
      targetLabel: 'test',
      useTransaction: true,
      onUseTransactionChange,
      requireRollback: true,
      onRequireRollbackChange,
      confirmText: '',
      onConfirmTextChange,
      deploying: false,
      onDeploy,
      result: null,
    };
    const { rerender } = render(<SchemaDiffDeployPanel {...props} />);
    expect(screen.getByTestId('schema-diff-deploy')).toBeDisabled();
    fireEvent.click(screen.getAllByRole('checkbox')[0]);
    expect(onUseTransactionChange).toHaveBeenCalledWith(false);
    fireEvent.click(screen.getAllByRole('checkbox')[1]);
    expect(onRequireRollbackChange).toHaveBeenCalledWith(false);
    fireEvent.change(screen.getByPlaceholderText('DEPLOY'), { target: { value: 'DEPLOY' } });
    expect(onConfirmTextChange).toHaveBeenCalledWith('DEPLOY');
    rerender(<SchemaDiffDeployPanel {...props} confirmText="DEPLOY" useTransaction={false} />);
    expect(screen.getByTestId('schema-diff-deploy')).toBeDisabled();
    rerender(<SchemaDiffDeployPanel {...props} confirmText="DEPLOY" />);
    expect(screen.getByTestId('schema-diff-deploy')).toBeEnabled();
    fireEvent.click(screen.getByTestId('schema-diff-deploy'));
    expect(onDeploy).toHaveBeenCalledTimes(1);
    rerender(
      <SchemaDiffDeployPanel
        {...props}
        confirmText="DEPLOY"
        result={{
          status: 'unknown',
          executedCount: 1,
          statementCount: 1,
          errors: ['Commit outcome unknown'],
          statementResults: [],
        }}
      />,
    );
    expect(screen.getByTestId('schema-diff-deploy')).toBeDisabled();
    expect(screen.getByTestId('schema-diff-deploy-status')).toHaveTextContent('unknown');
  });
});
