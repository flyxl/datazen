import { describe, expect, it } from 'vitest';

import {
  ARRAY_EXPORT_FORMAT_OPTIONS,
  formatOptionsForScope,
  OBJECT_EXPORT_FORMAT_OPTIONS,
} from '../exportDialogShared';
import type { ExportScope } from '../exportData';

/**
 * Task 6 (plan §2.8): the format list a scope may offer depends on the
 * form. Desktop serves every format for every scope; web has no SQL path,
 * so entire/filtered scopes offer CSV only, while the provided scopes
 * format in JS and keep every format.
 */

const ALL_ARRAY = ['csv', 'tsv', 'json', 'markdown', 'xlsx', 'sql_insert', 'sql_update'];
const ALL_OBJECT = ['csv', 'json', 'markdown', 'xlsx', 'sql_insert', 'sql_update'];

describe('formatOptionsForScope', () => {
  it.each(['current_page', 'selected', 'entire_table', 'filtered_table'] as const)(
    'on desktop offers every format for the %s scope',
    (scope: ExportScope) => {
      expect(
        formatOptionsForScope(ARRAY_EXPORT_FORMAT_OPTIONS, scope, true).map((o) => o.value),
      ).toEqual(ALL_ARRAY);
      expect(
        formatOptionsForScope(OBJECT_EXPORT_FORMAT_OPTIONS, scope, true).map((o) => o.value),
      ).toEqual(ALL_OBJECT);
    },
  );

  it.each(['current_page', 'selected'] as const)(
    'on web offers every format for the provided %s scope',
    (scope: ExportScope) => {
      expect(
        formatOptionsForScope(ARRAY_EXPORT_FORMAT_OPTIONS, scope, false).map((o) => o.value),
      ).toEqual(ALL_ARRAY);
      expect(
        formatOptionsForScope(OBJECT_EXPORT_FORMAT_OPTIONS, scope, false).map((o) => o.value),
      ).toEqual(ALL_OBJECT);
    },
  );

  it.each(['entire_table', 'filtered_table'] as const)(
    'on web offers CSV only for the SQL %s scope',
    (scope: ExportScope) => {
      expect(
        formatOptionsForScope(ARRAY_EXPORT_FORMAT_OPTIONS, scope, false).map((o) => o.value),
      ).toEqual(['csv']);
      expect(
        formatOptionsForScope(OBJECT_EXPORT_FORMAT_OPTIONS, scope, false).map((o) => o.value),
      ).toEqual(['csv']);
    },
  );
});
