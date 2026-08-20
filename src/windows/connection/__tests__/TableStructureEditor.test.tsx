import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor, cleanup } from '@testing-library/react';
import { TableStructureEditor } from '../TableStructureEditor';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const { mockUseDatabase, mockPlanTableStructureChanges } = vi.hoisted(() => ({
  mockUseDatabase: vi.fn().mockResolvedValue(undefined),
  mockPlanTableStructureChanges: vi.fn().mockResolvedValue({ statements: [] }),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    useDatabase: (...args: unknown[]) => mockUseDatabase(...args),
    getTableSchema: vi.fn().mockResolvedValue(null),
  },
}));

vi.mock('../../../commands/structure', () => ({
  structureCommands: {
    getStructureCapabilities: vi.fn().mockResolvedValue({ createTable: true }),
    planTableStructureChanges: (...args: unknown[]) => mockPlanTableStructureChanges(...args),
  },
}));

vi.mock('../../../commands/query', () => ({
  queryCommands: {
    executeQuery: vi.fn().mockResolvedValue({}),
  },
}));

vi.mock('../../../lib/exportTableStructure', () => ({
  exportTableStructureToFile: vi.fn(),
}));

vi.mock('../../../lib/databaseTypes', () => ({
  DB_REGISTRY: {
    postgresql: {
      supportsSQL: true,
      structureEditor: {
        enabled: true,
        defaultColumnType: 'text',
        indexMethods: ['btree'],
        columnTypes: [{ value: 'text', label: 'text' }],
        fields: {},
      },
    },
  },
}));

vi.mock('../structure/StructureColumnTable', () => ({
  StructureColumnTable: () => <div data-testid="column-table" />,
}));

vi.mock('../structure/StructureIndexTable', () => ({
  StructureIndexTable: () => null,
  suggestedIndexName: () => 'idx_new',
}));

vi.mock('../structure/StructurePlanPreview', () => ({
  StructurePlanPreview: () => null,
}));

beforeEach(() => {
  vi.clearAllMocks();
  mockUseDatabase.mockResolvedValue(undefined);
  mockPlanTableStructureChanges.mockResolvedValue({ statements: [] });
});

afterEach(cleanup);

describe('TableStructureEditor useDatabase', () => {
  it('calls useDatabase on mount when database prop is set', async () => {
    render(
      <TableStructureEditor
        connectionId="conn-1"
        databaseType="postgresql"
        database="mydb"
        schema="public"
        mode="create"
        onSuccess={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    await waitFor(() => {
      expect(mockUseDatabase).toHaveBeenCalledWith('conn-1', 'mydb');
    });
  });
});
