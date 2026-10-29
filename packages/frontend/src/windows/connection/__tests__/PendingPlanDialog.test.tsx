import { describe, expect, it, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/react';
import { PendingPlanDialog } from '../PendingPlanDialog';
import type { RowChangePlan } from '../../../lib/tableChanges';

/**
 * The description of the plan preview.
 *
 * It used to render every count **twice** — the raw number next to the
 * interpolated wording — which read "1 1 insert(s)" and "0 0 updates". The
 * wording carries the number itself (`{count} insert(s)`), so the count is
 * rendered once. This case pins the resulting text verbatim, so the doubled
 * form cannot come back unnoticed.
 *
 * The real i18n hook is used on purpose: the text is part of the contract.
 */

const CONTEXT = {
  connectionId: 'connection-1',
  dbSessionId: 'session-1',
  driverType: 'postgres',
  database: 'app',
  schema: null,
  table: 'users',
};

const PLAN: RowChangePlan = {
  planId: 'plan-1',
  fingerprint: 'fingerprint-1',
  table: CONTEXT,
  updates: [],
  deletes: [],
  inserts: [
    {
      rowIdentity: {},
      originalValues: {},
      currentValues: { code: 'e2e' },
      changedColumns: ['code'],
      sqlTemplate: 'INSERT INTO "users" ("code") VALUES ($1)',
      parameterSummary: ['INSERT code="e2e"'],
    },
  ],
  warnings: [],
};

afterEach(cleanup);

/**
 * The text the dialog describes itself with (its `aria-describedby` target).
 *
 * The dialog renders in a portal, so the lookup goes through the document.
 */
function descriptionOf(): string | null {
  const describedBy = document
    .querySelector('[role="dialog"]')
    ?.getAttribute('aria-describedby');
  if (!describedBy) return null;
  return document.getElementById(describedBy)?.textContent ?? null;
}

describe('PendingPlanDialog', () => {
  it('renders every count once', () => {
    render(<PendingPlanDialog plan={PLAN} open onClose={() => {}} />);

    // "1 1 insert(s)" and "0 0 updates" are what this used to say.
    expect(descriptionOf()).toBe('users · 1 insert(s) · 0 updates · 0 deletes');
  });

  it('renders the counts of every kind of statement', () => {
    const plan: RowChangePlan = {
      ...PLAN,
      updates: [
        {
          rowIdentity: { id: 1 },
          originalValues: { name: 'Alice' },
          currentValues: { name: 'Bob' },
          changedColumns: ['name'],
          sqlTemplate: 'UPDATE "users" SET "name" = $1 WHERE "id" = 1',
          parameterSummary: ['SET name="Bob"'],
        },
      ],
      deletes: [
        {
          rowIdentity: { id: 2 },
          originalValues: {},
          currentValues: {},
          changedColumns: [],
          sqlTemplate: 'DELETE FROM "users" WHERE "id" = 2',
          parameterSummary: [],
        },
      ],
    };
    render(<PendingPlanDialog plan={plan} open onClose={() => {}} />);

    expect(descriptionOf()).toBe('users · 1 insert(s) · 1 updates · 1 deletes');
  });
});
