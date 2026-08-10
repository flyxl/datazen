import { describe, expect, it } from 'vitest';
import type { DatabaseTypeMeta } from '../databaseMeta';
import { canOpenStructureEditor } from './canOpenStructureEditor';

const sqlTableMeta = {
  label: 'Test',
  shortLabel: 'T',
  iconBg: '',
  iconColor: '',
  defaultPort: 0,
  defaultHost: '',
  defaultUser: '',
  quoteChar: '"',
  connectionMode: 'server',
  supportsSSH: false,
  supportsSSL: false,
  supportsBackup: false,
  supportsTables: true,
  isKeyValue: false,
  supportsSQL: true,
  category: 'sql',
  connectionView: 'sql',
  databaseFieldType: 'name',
  connectionForm: 'standard',
} satisfies DatabaseTypeMeta;

describe('canOpenStructureEditor', () => {
  it('returns true for SQL table drivers', () => {
    expect(canOpenStructureEditor(sqlTableMeta)).toBe(true);
  });

  it('returns false when structureEditor.enabled is false', () => {
    expect(
      canOpenStructureEditor({
        ...sqlTableMeta,
        structureEditor: { enabled: false, columnTypes: [], defaultColumnType: '', fields: {}, indexMethods: [] },
      }),
    ).toBe(false);
  });

  it('returns false for key-value and document drivers', () => {
    expect(canOpenStructureEditor({ ...sqlTableMeta, isKeyValue: true, supportsSQL: false })).toBe(false);
    expect(
      canOpenStructureEditor({
        ...sqlTableMeta,
        connectionView: 'document',
        supportsSQL: false,
        category: 'document',
      }),
    ).toBe(false);
  });

  it('returns false when supportsSQL is false', () => {
    expect(canOpenStructureEditor({ ...sqlTableMeta, supportsSQL: false })).toBe(false);
  });

  it('returns false for undefined meta', () => {
    expect(canOpenStructureEditor(undefined)).toBe(false);
  });
});
