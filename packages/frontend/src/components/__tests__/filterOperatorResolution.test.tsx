/**
 * Round-2 verification locks (independently written, not part of the track).
 *
 * Three properties that the fix for FF-D1 / FF-D3 / FF-D4 depends on, and
 * that nothing in the shipped suite currently pins:
 *
 * 1. **The component does not judge emptiness.** `FilterEditor` treats an
 *    empty `supportedOperators` list as "this driver serves nothing" and
 *    offers only the operator the condition already holds. If the panel
 *    silently fell back to the shared set instead, the end-to-end test
 *    (`TableView.filterOperators.test.tsx`) would stop catching a `TableView`
 *    that bypasses the resolver — the panel would mask the defect again.
 *    This test is the lock on that teeth.
 * 2. **One value parser.** The collapsed summary and the expanded editor read
 *    the same value shape, including the comma separated form of an older
 *    draft.
 * 3. **An unknown operator degrades.** A condition persisted by another build
 *    must not throw while the panel renders.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/react';
import { FilterEditor } from '../FilterEditor';
import { conditionSummary } from '../filter/filterLabels';
import { valueInputFor } from '../../lib/filterOperators';
import type { FilterCondition, FilterOperator } from '../../types';

vi.mock('../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../ui/Select', () => ({
  Select: ({
    value,
    options,
    onChange,
  }: {
    value: string;
    options: { value: string; label: string }[];
    onChange: (v: string) => void;
  }) => (
    <select value={value} onChange={(e) => onChange(e.target.value)}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  ),
}));

const t = (key: string) => key;

afterEach(cleanup);

function renderEditor(props: {
  columns?: { name: string; type?: string }[];
  draftFilters: FilterCondition[];
  supportedOperators?: readonly FilterOperator[];
}) {
  return render(
    <FilterEditor
      columns={props.columns ?? [{ name: 'name', type: 'text' }]}
      supportedOperators={props.supportedOperators}
      appliedFilters={[]}
      appliedLogic="and"
      draftFilters={props.draftFilters}
      draftLogic="and"
      open
      onOpenChange={vi.fn()}
      onLogicChange={vi.fn()}
      onChange={vi.fn()}
      onAdd={vi.fn()}
      onRemove={vi.fn()}
      onApply={vi.fn()}
      onClear={vi.fn()}
    />,
  );
}

/** The values of the operator dropdown of the expanded condition. */
function operatorOptions(): string[] {
  const selects = screen.getAllByRole('combobox');
  const operatorSelect = selects[1] as HTMLSelectElement;
  return Array.from(operatorSelect.options).map((option) => option.value);
}

describe('the panel does not judge emptiness itself', () => {
  it('offers only the condition’s own operator when the list is empty', () => {
    renderEditor({
      supportedOperators: [],
      draftFilters: [{ column: 'name', operator: 'eq', value: '' }],
    });
    // The panel does NOT fall back to the shared set: an empty list means the
    // caller resolved "this driver serves nothing". Falling back here would
    // mask a `TableView` that bypasses the resolver.
    expect(operatorOptions()).toEqual(['eq']);
  });

  it('offers the shared set when the list is not passed at all', () => {
    renderEditor({ draftFilters: [{ column: 'name', operator: 'eq', value: '' }] });
    expect(operatorOptions().length).toBeGreaterThan(1);
  });
});

describe('one value parser for the summary and the editor', () => {
  it('reads the comma separated form of a range in the collapsed summary', () => {
    expect(
      conditionSummary({ column: 'score', operator: 'between', value: '10,20' }, t),
    ).toBe('score filter.between 10, 20');
    expect(
      conditionSummary({ column: 'score', operator: 'between', value: ['10', '20'] }, t),
    ).toBe('score filter.between 10, 20');
  });

  it('reads the comma separated form of a token list in the collapsed summary', () => {
    expect(
      conditionSummary({ column: 'name', operator: 'in', value: 'paid, open' }, t),
    ).toBe('name filter.in paid, open');
    expect(
      conditionSummary({ column: 'name', operator: 'in', value: ['paid', 'open'] }, t),
    ).toBe('name filter.in paid, open');
  });
});

describe('an unknown operator degrades instead of throwing', () => {
  it('maps to the single value control', () => {
    const legacy = 'legacyOperator' as unknown as FilterOperator;
    expect(() => valueInputFor(legacy)).not.toThrow();
    expect(valueInputFor(legacy)).toBe('single');
  });

  it('renders a condition that names one', () => {
    const legacy = {
      column: 'name',
      operator: 'legacyOperator' as unknown as FilterOperator,
      value: 'alice',
    };
    expect(() => renderEditor({ draftFilters: [legacy] })).not.toThrow();
    expect(conditionSummary(legacy, t)).toBe('name filter.legacyOperator alice');
  });
});
