import { describe, expect, it } from 'vitest';
import {
  driverExportScope,
  minScope,
  resolveExportScope,
  supportsAnyExport,
  supportsFullTableExport,
} from '../exportCapability';
import type { DatabaseTypeMeta } from '../databaseMeta';

const meta = (partial: Partial<DatabaseTypeMeta>): DatabaseTypeMeta => ({
  label: 'x',
  shortLabel: 'X',
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
  connectionForm: 'x',
  ...partial,
});

describe('resolveExportScope', () => {
  it('defaults to full_table when the field is absent', () => {
    expect(resolveExportScope(meta({}))).toBe('full_table');
  });

  it('resolves an explicit scope', () => {
    expect(resolveExportScope(meta({ exportScope: 'none' }))).toBe('none');
    expect(resolveExportScope(meta({ exportScope: 'loaded_only' }))).toBe('loaded_only');
  });

  it('defaults to full_table for undefined meta (unknown driver registry entry)', () => {
    expect(resolveExportScope(undefined)).toBe('full_table');
    expect(driverExportScope(undefined)).toBe('full_table');
  });
});

describe('supportsFullTableExport', () => {
  it.each([
    ['none', false],
    ['loaded_only', false],
    ['full_table', true],
  ] as const)('%s → %s', (scope, expected) => {
    expect(supportsFullTableExport(scope)).toBe(expected);
  });
});

describe('supportsAnyExport', () => {
  it('permits export except when none', () => {
    expect(supportsAnyExport('none')).toBe(false);
    expect(supportsAnyExport('loaded_only')).toBe(true);
    expect(supportsAnyExport('full_table')).toBe(true);
  });
});

describe('minScope', () => {
  it('picks the most restricted scope', () => {
    expect(minScope('full_table', 'full_table')).toBe('full_table');
    expect(minScope('full_table', 'loaded_only')).toBe('loaded_only');
    expect(minScope('loaded_only', 'full_table')).toBe('loaded_only');
    expect(minScope('loaded_only', 'none')).toBe('none');
    expect(minScope('none', 'full_table')).toBe('none');
  });
});
