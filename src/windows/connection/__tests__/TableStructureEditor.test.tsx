import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor, cleanup, screen } from '@testing-library/react';
import { TableStructureEditor } from '../TableStructureEditor';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const { mockPlanTableStructureChanges } = vi.hoisted(() => ({
  mockPlanTableStructureChanges: vi.fn().mockResolvedValue({ statements: [] }),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
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
  mockPlanTableStructureChanges.mockResolvedValue({ statements: [] });
});

afterEach(cleanup);

describe('TableStructureEditor mount (F1: no use_database IPC)', () => {
  it('renders without a session database switch when database prop is set', async () => {
    render(
      <TableStructureEditor
        dbSessionId="conn-1"
        databaseType="postgresql"
        database="mydb"
        schema="public"
        mode="create"
        onSuccess={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    // F1 removed the useDatabase-on-mount behavior; the editor must come up
    // directly (queries pin the database explicitly instead).
    await waitFor(() => {
      expect(screen.getByTestId('column-table')).toBeInTheDocument();
    });
  });
});
