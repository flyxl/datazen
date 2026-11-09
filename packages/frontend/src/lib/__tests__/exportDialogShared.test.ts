import { describe, expect, it } from 'vitest';
import { buildExportFormatOptions, EXPORT_FORMAT_OPTIONS } from '../exportDialogShared';

describe('buildExportFormatOptions (DBR-06)', () => {
  it('keeps the one unified list for page scopes with a key column', () => {
    const options = buildExportFormatOptions({ streamingScope: false, hasKeyColumn: true });
    expect(options.map((o) => o.value)).toEqual(EXPORT_FORMAT_OPTIONS.map((o) => o.value));
    expect(options.every((o) => !o.disabled)).toBe(true);
  });

  it('offers tsv and sql_update from the same list for both entry points', () => {
    const values = EXPORT_FORMAT_OPTIONS.map((o) => o.value);
    // 网格与表级此前一个缺 SQL UPDATE、一个缺 TSV；统一后同一张表从
    // 两个入口能选的格式必须一致。
    expect(values).toContain('tsv');
    expect(values).toContain('sql_update');
  });

  it('disables xlsx with the streaming reason for streaming scopes', () => {
    const options = buildExportFormatOptions({ streamingScope: true, hasKeyColumn: true });
    const xlsx = options.find((o) => o.value === 'xlsx')!;
    expect(xlsx.disabled).toBe(true);
    expect(xlsx.disabledReasonKey).toBe('export.formatDisabledStream');
    // The rest of the list stays enabled — a disabled format never hides
    // its neighbours (禁用并解释原因，而非移除).
    expect(options.filter((o) => !o.disabled).map((o) => o.value)).toEqual(
      expect.arrayContaining(['csv', 'tsv', 'json', 'markdown', 'sql_insert', 'sql_update']),
    );
  });

  it('disables sql_update without a key column', () => {
    const options = buildExportFormatOptions({ streamingScope: false, hasKeyColumn: false });
    const sqlUpdate = options.find((o) => o.value === 'sql_update')!;
    expect(sqlUpdate.disabled).toBe(true);
    expect(sqlUpdate.disabledReasonKey).toBe('export.formatDisabledNoKey');
  });
});
