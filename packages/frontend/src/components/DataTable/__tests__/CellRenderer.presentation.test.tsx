/**
 * CellRenderer presentation wiring (issue-32 T05, design §7).
 *
 * The plan runs after the editing / NULL branches and before the family
 * branches: a `text` result wins (so a BIGINT with an explicit Unix plan
 * renders as a timestamp, not a number); `unchanged` — or no plan at all —
 * falls through to the original branches (default-off).
 *
 * Editing always shows the editor on the raw value, never the presented
 * text; NULL always renders `NULL`, even with a plan.
 */
import { describe, expect, it, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/react';
import { CellRenderer } from '../CellRenderer';
import type { ColumnPresentationPlan } from '../../../lib/valuePresentation/presentationPlan';
import type {
  PresentationColumnContext,
  PresentationResult,
  PresentationValue,
} from '@datazen/extension-points';

const NOOP = () => {};

const ANCHOR = '2026-01-15T14:30:00Z';
const PRESENTED = '2026-01-15T22:30:00+08:00';

const COLUMN_CONTEXT: PresentationColumnContext = {
  columnName: 'created_at',
  semanticKind: 'instant',
};

function makePlan(
  present: (value: PresentationValue) => PresentationResult,
): ColumnPresentationPlan {
  return {
    columnName: 'created_at',
    column: COLUMN_CONTEXT,
    status: 'ready',
    rendererId: 'datazen.timestamp',
    present,
  };
}

const textPlan = (text: string): ColumnPresentationPlan =>
  makePlan(() => ({ kind: 'text', text, interpretation: 'native' }));

const unchangedPlan: ColumnPresentationPlan = {
  columnName: 'created_at',
  column: COLUMN_CONTEXT,
  status: 'disabled',
  rendererId: 'datazen.timestamp',
  reason: 'disabled',
  present: () => ({ kind: 'unchanged', reason: 'disabled' }),
};

describe('CellRenderer presentation plan (issue-32 T05)', () => {
  it('renders the presented text with the raw value as the tooltip', () => {
    const { container } = render(
      <CellRenderer
        columnName="created_at"
        dataType="timestamp with time zone"
        value={ANCHOR}
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
        presentationPlan={textPlan(PRESENTED)}
      />,
    );
    const span = container.querySelector('span')!;
    expect(span.textContent).toBe(PRESENTED);
    expect(span.getAttribute('title')).toBe(ANCHOR);
    // The datetime branch's own style.
    expect(span.className).toContain('font-mono');
    expect(span.className).toContain('text-xs');
    expect(span.className).toContain('text-dt-datetime');
  });

  it('falls through to the datetime branch when the plan returns unchanged', () => {
    const { container } = render(
      <CellRenderer
        columnName="created_at"
        dataType="timestamp with time zone"
        value={ANCHOR}
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
        presentationPlan={unchangedPlan}
      />,
    );
    const span = container.querySelector('span')!;
    // The old formatTimestamp text (UTC normalisation), not the presented one.
    expect(span.textContent).toBe('2026-01-15T14:30:00.000Z');
    expect(span.textContent).not.toContain('22:30');
  });

  it('renders the old datetime text when no plan is given', () => {
    const { container } = render(
      <CellRenderer
        columnName="created_at"
        dataType="timestamp with time zone"
        value={ANCHOR}
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
      />,
    );
    expect(container.querySelector('span')!.textContent).toBe('2026-01-15T14:30:00.000Z');
  });

  it('lets a text plan win over the number branch (BIGINT Unix, BUG-003)', () => {
    const { container } = render(
      <CellRenderer
        columnName="epoch_ms"
        dataType="bigint"
        value={1705316400123}
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
        presentationPlan={textPlan('2024-01-15T19:00:00.123+08:00')}
      />,
    );
    const span = container.querySelector('span')!;
    expect(span.textContent).toBe('2024-01-15T19:00:00.123+08:00');
    expect(span.textContent).not.toContain('1705316400123');
  });

  it('editing still shows the editor on the raw value, never the presented text', () => {
    const { container } = render(
      <CellRenderer
        columnName="created_at"
        dataType="timestamp with time zone"
        value={ANCHOR}
        isEditing
        onCommit={NOOP}
        onCancel={NOOP}
        presentationPlan={textPlan(PRESENTED)}
      />,
    );
    expect(container.textContent).not.toContain(PRESENTED);
    // The editor is present (an input/textarea/select), initialised from
    // the raw value — the presented text never reaches it.
    expect(container.querySelector('input, textarea, select')).not.toBeNull();
  });

  it('NULL still renders NULL even with a plan', () => {
    const { container } = render(
      <CellRenderer
        columnName="created_at"
        dataType="timestamp with time zone"
        value={null}
        isEditing={false}
        onCommit={NOOP}
        onCancel={NOOP}
        presentationPlan={textPlan(PRESENTED)}
      />,
    );
    const span = container.querySelector('span')!;
    expect(span.textContent).toBe('NULL');
    expect(span.className).toContain('text-dt-null');
  });

  it('loads enum candidates through the per-column loader (pre-existing path)', () => {
    const loadCandidates = vi.fn(async () => ['open', 'closed']);
    const { container } = render(
      <CellRenderer
        columnName="status"
        dataType="varchar"
        value="open"
        isEditing
        onCommit={NOOP}
        onCancel={NOOP}
        loadCandidates={loadCandidates}
        editorKindOverride="enum"
      />,
    );
    fireEvent.click(container.querySelector('[data-testid="cell-editor-load-candidates"]')!);
    expect(loadCandidates).toHaveBeenCalledWith('status');
  });
});
