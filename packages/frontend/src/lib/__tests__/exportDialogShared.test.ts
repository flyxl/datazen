import { describe, expect, it } from 'vitest';

import {
  ARRAY_EXPORT_FORMAT_OPTIONS,
  buildExportFormatOptions,
  OBJECT_EXPORT_FORMAT_OPTIONS,
} from '../exportDialogShared';
import type { ExportScope } from '../exportData';

/**
 * DBR-06, restored for the frontend-DataFetcher architecture
 * (plan §2.8, task 6): the format list carries its own availability.
 * Desktop serves every format for every scope; web has no SQL path, so
 * the streaming scopes disable every non-CSV format with its reason
 * instead of hiding them; SQL UPDATE needs a key column. XLSX is never
 * disabled — SheetJS (frontend) and the Rust BinaryFormatter both serve
 * it now.
 */

const ALL_ARRAY = ['csv', 'tsv', 'json', 'markdown', 'xlsx', 'sql_insert', 'sql_update'];
const ALL_OBJECT = ['csv', 'json', 'markdown', 'xlsx', 'sql_insert', 'sql_update'];

describe('buildExportFormatOptions', () => {
  it.each(['current_page', 'selected', 'entire_table', 'filtered_table'] as const)(
    'on desktop enables every format for the %s scope when a key column exists',
    (scope: ExportScope) => {
      expect(
        buildExportFormatOptions({
          options: ARRAY_EXPORT_FORMAT_OPTIONS,
          scope,
          isDesktop: true,
          hasKeyColumn: true,
        }).map((o) => o.value),
      ).toEqual(ALL_ARRAY);
      expect(
        buildExportFormatOptions({
          options: OBJECT_EXPORT_FORMAT_OPTIONS,
          scope,
          isDesktop: true,
          hasKeyColumn: true,
        }).map((o) => o.value),
      ).toEqual(ALL_OBJECT);
      for (const options of [ARRAY_EXPORT_FORMAT_OPTIONS, OBJECT_EXPORT_FORMAT_OPTIONS]) {
        expect(
          buildExportFormatOptions({ options, scope, isDesktop: true, hasKeyColumn: true }).filter(
            (o) => o.disabled,
          ),
        ).toEqual([]);
      }
    },
  );

  it.each(['current_page', 'selected', 'entire_table', 'filtered_table'] as const)(
    'on desktop disables only SQL UPDATE without a key column, for the %s scope',
    (scope: ExportScope) => {
      for (const options of [ARRAY_EXPORT_FORMAT_OPTIONS, OBJECT_EXPORT_FORMAT_OPTIONS]) {
        const result = buildExportFormatOptions({
          options,
          scope,
          isDesktop: true,
          hasKeyColumn: false,
        });
        expect(result.filter((o) => o.disabled).map((o) => o.value)).toEqual(['sql_update']);
        expect(
          result.find((o) => o.value === 'sql_update')?.disabledReasonKey,
        ).toBe('export.formatDisabledNoKey');
      }
    },
  );

  it('never disables XLSX on desktop, even for the streaming scopes', () => {
    for (const scope of ['entire_table', 'filtered_table'] as const) {
      const result = buildExportFormatOptions({
        options: ARRAY_EXPORT_FORMAT_OPTIONS,
        scope,
        isDesktop: true,
        hasKeyColumn: true,
      });
      expect(result.find((o) => o.value === 'xlsx')?.disabled).toBeFalsy();
    }
  });

  it.each(['current_page', 'selected'] as const)(
    'on web enables every format for the provided %s scope when a key column exists',
    (scope: ExportScope) => {
      for (const options of [ARRAY_EXPORT_FORMAT_OPTIONS, OBJECT_EXPORT_FORMAT_OPTIONS]) {
        expect(
          buildExportFormatOptions({ options, scope, isDesktop: false, hasKeyColumn: true }).filter(
            (o) => o.disabled,
          ),
        ).toEqual([]);
      }
    },
  );

  it.each(['entire_table', 'filtered_table'] as const)(
    'on web disables every non-CSV format with the web reason for the SQL %s scope',
    (scope: ExportScope) => {
      for (const options of [ARRAY_EXPORT_FORMAT_OPTIONS, OBJECT_EXPORT_FORMAT_OPTIONS]) {
        const result = buildExportFormatOptions({
          options,
          scope,
          isDesktop: false,
          hasKeyColumn: true,
        });
        expect(result.filter((o) => !o.disabled).map((o) => o.value)).toEqual(['csv']);
        expect(
          new Set(result.filter((o) => o.disabled).map((o) => o.disabledReasonKey)),
        ).toEqual(new Set(['export.formatDisabledWebSqlScope']));
      }
    },
  );

  it('on a web streaming scope without a key column, SQL UPDATE carries the no-key reason', () => {
    const result = buildExportFormatOptions({
      options: ARRAY_EXPORT_FORMAT_OPTIONS,
      scope: 'entire_table',
      isDesktop: false,
      hasKeyColumn: false,
    });
    expect(result.find((o) => o.value === 'sql_update')?.disabledReasonKey).toBe(
      'export.formatDisabledNoKey',
    );
    expect(result.find((o) => o.value === 'xlsx')?.disabledReasonKey).toBe(
      'export.formatDisabledWebSqlScope',
    );
    expect(
      new Set(result.filter((o) => o.disabled).map((o) => o.disabledReasonKey)),
    ).toEqual(
      new Set(['export.formatDisabledWebSqlScope', 'export.formatDisabledNoKey']),
    );
  });
});
