import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import type { SchemaDiffPlan } from '../../../commands/schemaDiff';
import { SchemaDiffRightPanel } from '../SchemaDiffRightPanel';
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
});
