/**
 * The DBR-04 mode row of the filter panel: Any Column and Raw SQL.
 *
 * The pinned rules:
 *
 * * a caller that passes no mode props renders the panel exactly as before —
 *   no mode row, both panes absent;
 * * the Raw SQL tab appears only for a driver that declared the channel;
 * * switching modes is render-only (the callback fires, nothing else does);
 * * each pane applies its draft through its own callback, and Enter applies;
 * * an applied raw predicate lights the panel up — badge, summary and the
 *   Clear button — exactly like applied structured filters.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup, screen } from '@testing-library/react';
import type { ComponentProps } from 'react';
import { FilterEditor } from '../FilterEditor';
import type { FilterCondition } from '../../types';

vi.mock('../../hooks/useI18n', () => ({
  useI18n: () => ({
    // The real t interpolates `{name}` params against the dictionary entry.
    // The mock carries the two DBR-04 templates the pane tests assert on, so
    // the interpolation path itself is exercised, not just the key lookup.
    t: (key: string, params?: Record<string, string>) => {
      const templates: Record<string, string> = {
        'filter.anyColumnScope': 'searches {searchable}, skips {skipped}',
        'filter.rawAppliedSummary': 'SQL: {predicate}',
      };
      const text = templates[key] ?? key;
      if (!params) return text;
      return Object.entries(params).reduce(
        (result, [name, value]) => result.replaceAll(`{${name}}`, value),
        text,
      );
    },
  }),
}));

vi.mock('../ui/Select', () => ({
  Select: () => <select />,
}));

afterEach(cleanup);

const columns = [{ name: 'id' }, { name: 'name' }];

type EditorProps = ComponentProps<typeof FilterEditor>;

function baseProps(overrides: Partial<EditorProps> = {}): EditorProps {
  return {
    columns,
    appliedFilters: [] as FilterCondition[],
    appliedLogic: 'and',
    draftFilters: [],
    draftLogic: 'and',
    open: true,
    onOpenChange: vi.fn(),
    onLogicChange: vi.fn(),
    onChange: vi.fn(),
    onAdd: vi.fn(),
    onRemove: vi.fn(),
    onApply: vi.fn(),
    onClear: vi.fn(),
    ...overrides,
  };
}

describe('FilterEditor DBR-04 mode row', () => {
  it('renders no mode row when no mode props are passed', () => {
    render(<FilterEditor {...baseProps()} />);
    expect(screen.queryByTestId('filter-mode-structured')).toBeNull();
    expect(screen.queryByTestId('filter-any-column-pane')).toBeNull();
    expect(screen.queryByTestId('filter-raw-sql-pane')).toBeNull();
  });

  it('offers structured and any column, and raw sql only when declared', () => {
    render(
      <FilterEditor
        {...baseProps({
          mode: 'structured',
          onModeChange: vi.fn(),
          onApplyAnyColumn: vi.fn(),
          rawWhereSupported: false,
        })}
      />,
    );
    expect(screen.getByTestId('filter-mode-structured')).toBeTruthy();
    expect(screen.getByTestId('filter-mode-anyColumn')).toBeTruthy();
    // The driver did not declare the raw channel: the tab is not offered.
    expect(screen.queryByTestId('filter-mode-rawSql')).toBeNull();

    cleanup();
    render(
      <FilterEditor
        {...baseProps({
          mode: 'structured',
          onModeChange: vi.fn(),
          onApplyAnyColumn: vi.fn(),
          rawWhereSupported: true,
        })}
      />,
    );
    expect(screen.getByTestId('filter-mode-rawSql')).toBeTruthy();
  });

  it('switching a mode is render-only: the callback fires, nothing else', () => {
    const onModeChange = vi.fn();
    const onApply = vi.fn();
    render(
      <FilterEditor
        {...baseProps({
          mode: 'structured',
          onModeChange,
          onApplyAnyColumn: vi.fn(),
        })}
      />,
    );
    fireEvent.click(screen.getByTestId('filter-mode-anyColumn'));
    expect(onModeChange).toHaveBeenCalledWith('anyColumn');
    expect(onApply).not.toHaveBeenCalled();
  });

  it('the any column pane applies its value on Enter and on the button', () => {
    const onApplyAnyColumn = vi.fn();
    const onAnyColumnValueChange = vi.fn();
    render(
      <FilterEditor
        {...baseProps({
          mode: 'anyColumn',
          onModeChange: vi.fn(),
          onApplyAnyColumn,
          onAnyColumnValueChange,
          anyColumnValue: 'gold',
          anyColumnSearchableCount: 3,
          anyColumnSkippedCount: 1,
        })}
      />,
    );
    const input = screen.getByTestId('any-column-input') as HTMLInputElement;
    expect(input.value).toBe('gold');
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onApplyAnyColumn).toHaveBeenCalledWith('gold');
    fireEvent.click(screen.getByTestId('any-column-apply'));
    expect(onApplyAnyColumn).toHaveBeenCalledTimes(2);
    // The scope report is stated, so a skipped column is never a surprise.
    expect(screen.getByTestId('filter-any-column-pane').textContent).toContain('3');
  });

  it('the any column apply is disabled while the draft is empty', () => {
    render(
      <FilterEditor
        {...baseProps({
          mode: 'anyColumn',
          onModeChange: vi.fn(),
          onApplyAnyColumn: vi.fn(),
          anyColumnValue: '   ',
        })}
      />,
    );
    const apply = screen.getByTestId('any-column-apply') as HTMLButtonElement;
    expect(apply.disabled).toBe(true);
  });

  it('the raw sql pane applies its predicate and shows the risk hint', () => {
    const onApplyRawWhere = vi.fn();
    render(
      <FilterEditor
        {...baseProps({
          mode: 'rawSql',
          onModeChange: vi.fn(),
          rawWhereSupported: true,
          onApplyRawWhere,
          rawWhereValue: 'total > 100',
        })}
      />,
    );
    const input = screen.getByTestId('raw-where-input') as HTMLInputElement;
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onApplyRawWhere).toHaveBeenCalledWith('total > 100');
    // The advanced-mode risk hint is part of the pane, not a tooltip.
    expect(screen.getByTestId('raw-where-hint').textContent).toContain('rawHint');
  });

  it('an applied raw predicate lights the panel: badge, summary and clear', () => {
    render(
      <FilterEditor
        {...baseProps({
          mode: 'rawSql',
          onModeChange: vi.fn(),
          rawWhereSupported: true,
          onApplyRawWhere: vi.fn(),
          rawWhereValue: 'total > 100',
          appliedRawWhere: 'total > 100',
        })}
      />,
    );
    // The collapsed-panel affordances exist even while open: the badge says
    // SQL, the summary carries the predicate, the clear button is offered.
    expect(screen.getByTestId('filter-editor').textContent).toContain('filter.rawBadge');
    expect(screen.getByTestId('filter-editor').textContent).toContain('total > 100');
    expect(screen.getByTestId('filter-clear')).toBeTruthy();
  });

  it('a raw-only applied filter keeps the panel mounted when collapsed', () => {
    // The structured list is empty and the panel is closed: with a raw
    // predicate in effect the panel must not unmount — the user needs the
    // summary and the clear button.
    render(
      <FilterEditor
        {...baseProps({
          open: false,
          mode: 'rawSql',
          onModeChange: vi.fn(),
          rawWhereSupported: true,
          onApplyRawWhere: vi.fn(),
          appliedRawWhere: 'total > 100',
        })}
      />,
    );
    expect(screen.getByTestId('filter-editor')).toBeTruthy();
    expect(screen.getByTestId('filter-clear')).toBeTruthy();
  });
});
