/**
 * DetailPanel display state (issue-32 BUG-005) and grid/aside source
 * parity (issue-32 BUG-006).
 *
 * BUG-005: the editable drawer used to render `InlineFieldEditor` outright,
 * so the T05 presented text in the `<pre>` branch was unreachable in the
 * production combination (editable table panel + snapshot) and Escape could
 * never "return to display". The display state is now the default; the
 * editor opens from it (click / Enter / Space) and commit or Escape returns
 * to it.
 *
 * BUG-006: the grid passed `database: ''` where the aside passed
 * `database: undefined` for the same absence, so one override hit aside-side
 * and missed grid-side. TableView now normalises `''` to `undefined` in the
 * source assembly only.
 */
import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { ValuePresentationRegistryImpl, type ColumnSource } from '@datazen/extension-points';
import { DetailPanel } from '../DetailPanel';
import type { ColumnDef } from '../TableHeader';
import {
  TIMESTAMP_CONTRIBUTION_ID,
  timestampContribution,
  columnOverrideKey,
  type ValuePresentationSnapshot,
} from '../../../lib/valuePresentation/index';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

const SOURCE: Readonly<Omit<ColumnSource, 'column'>> = {
  connectionId: 'conn-1',
  database: 'shop',
  namespace: 'public',
  table: 'orders',
};

const ENVIRONMENT = { locale: 'en-GB', localTimeZone: 'Asia/Shanghai' } as const;

function makeSnapshot(
  source: Readonly<Omit<ColumnSource, 'column'>> = SOURCE,
): ValuePresentationSnapshot {
  return {
    revision: 1,
    environment: ENVIRONMENT,
    configs: {
      [TIMESTAMP_CONTRIBUTION_ID]: {
        configVersion: 1,
        config: {
          interpretation: { kind: 'native' },
          format: 'iso-tz',
          displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
        },
      },
    },
    enabledRendererIds: [TIMESTAMP_CONTRIBUTION_ID],
    overrides: {
      [columnOverrideKey({ ...source, column: 'epoch_ms' })]: {
        mode: 'custom',
        rendererId: TIMESTAMP_CONTRIBUTION_ID,
        options: {
          interpretation: { kind: 'unix', unit: 'milliseconds' },
          format: 'iso-tz',
          displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
        },
      },
    },
  };
}

function makeRegistry(): ValuePresentationRegistryImpl {
  const registry = new ValuePresentationRegistryImpl();
  registry.register(timestampContribution);
  return registry;
}

const COLS: ColumnDef[] = [
  { id: 'created_at', name: 'created_at', type: 'timestamp with time zone' },
  { id: 'epoch_ms', name: 'epoch_ms', type: 'bigint' },
  { id: 'qty', name: 'qty', type: 'integer' },
  { id: 'name', name: 'name', type: 'varchar' },
];

const ROW = {
  created_at: '2026-01-15T14:30:00Z',
  epoch_ms: 1705316400123,
  qty: 42,
  name: 'Alice',
};

/** Field editors sit below the column-search input. */
function fieldInputs(container: HTMLElement): HTMLInputElement[] {
  return Array.from(container.querySelectorAll('input')).slice(1) as HTMLInputElement[];
}

function asidePres(container: HTMLElement): string[] {
  return Array.from(container.querySelectorAll('aside pre')).map((pre) => pre.textContent ?? '');
}

/** Display containers, in column order. */
function displayContainers(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll('aside [role="button"]'));
}

function renderEditable(
  over: Partial<Parameters<typeof DetailPanel>[0]> = {},
  row: Record<string, unknown> = ROW,
  columns: ColumnDef[] = COLS,
) {
  const onFieldEdit = vi.fn();
  const rendered = render(
    <DetailPanel
      open
      editable
      columns={columns}
      row={row}
      rowIndex={0}
      onFieldEdit={onFieldEdit}
      presentationSnapshot={makeSnapshot()}
      presentationSource={SOURCE}
      presentationRegistry={makeRegistry()}
      {...over}
    />,
  );
  return { ...rendered, onFieldEdit };
}

describe('DetailPanel display state (issue-32 BUG-005)', () => {
  it('shows presented text by default in the editable production combination', () => {
    const { container } = renderEditable();
    // The grid shows these; the aside must read the same.
    expect(asidePres(container)).toContain('2026-01-15T22:30:00+08:00');
    expect(asidePres(container).some((t) => t.includes('.123') && t.includes('19:00'))).toBe(true);
    expect(asidePres(container)).toContain('42');
    // No editor is mounted yet — the raw values are not inputs.
    expect(fieldInputs(container)).toHaveLength(0);
    expect(container.querySelectorAll('aside textarea')).toHaveLength(0);
    expect(asidePres(container)).not.toContain('2026-01-15T14:30:00Z');
    expect(asidePres(container)).not.toContain('1705316400123');
  });

  it('clicking the display opens the editor initialised from the raw value', () => {
    const { container } = renderEditable();
    fireEvent.click(displayContainers(container)[0]);
    const input = fieldInputs(container)[0];
    expect(input).toHaveValue('2026-01-15T14:30:00Z');
    expect(input).not.toHaveValue('2026-01-15T22:30:00+08:00');
    // One click lands straight in a typeable editor (BUG-005 autoFocus).
    expect(document.activeElement).toBe(input);
  });

  it('Enter on the display container opens the editor', () => {
    const { container } = renderEditable();
    fireEvent.keyDown(displayContainers(container)[3], { key: 'Enter' });
    expect(fieldInputs(container)[0]).toHaveValue('Alice');
  });

  it('Escape returns to the presented text without calling onFieldEdit', () => {
    const { container, onFieldEdit } = renderEditable();
    fireEvent.click(displayContainers(container)[3]);
    const input = fieldInputs(container)[0];
    fireEvent.change(input, { target: { value: 'temp' } });
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(onFieldEdit).not.toHaveBeenCalled();
    expect(fieldInputs(container)).toHaveLength(0);
    expect(asidePres(container)).toContain('Alice');
  });

  it('blur after a change commits and returns to the display state', () => {
    const { container, onFieldEdit } = renderEditable();
    fireEvent.click(displayContainers(container)[3]);
    const input = fieldInputs(container)[0];
    fireEvent.change(input, { target: { value: 'Carol' } });
    fireEvent.blur(input);
    expect(onFieldEdit).toHaveBeenCalledWith(0, 'name', 'Carol');
    expect(fieldInputs(container)).toHaveLength(0);
    // The row prop is unchanged, so the display is back to the old value.
    expect(asidePres(container)).toContain('Alice');
  });

  it('Enter after a change commits and returns to the display state', () => {
    const { container, onFieldEdit } = renderEditable();
    fireEvent.click(displayContainers(container)[3]);
    const input = fieldInputs(container)[0];
    fireEvent.change(input, { target: { value: 'Dave' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onFieldEdit).toHaveBeenCalledWith(0, 'name', 'Dave');
    expect(fieldInputs(container)).toHaveLength(0);
    expect(asidePres(container)).toContain('Alice');
  });

  it('NULL shows NULL in the editable display state', () => {
    const { container } = renderEditable({}, { ...ROW, created_at: null });
    expect(asidePres(container)).toContain('NULL');
    expect(fieldInputs(container)).toHaveLength(0);
  });

  it('a blob column shows the summary in the editable display state', () => {
    const blobValue = 'x'.repeat(200);
    const { container } = renderEditable({}, { avatar: blobValue }, [
      { id: 'avatar', name: 'avatar', type: 'bytea' },
    ]);
    // The same summary as the read-only blob branch — never the long raw
    // string the editor would initialise from.
    expect(container.querySelector('aside')).toHaveTextContent('detail.blobData');
    expect(container.querySelector('aside')).toHaveTextContent('200');
    expect(asidePres(container)).toHaveLength(0);
    expect(container.querySelector('aside')?.textContent).not.toContain(blobValue);
    expect(fieldInputs(container)).toHaveLength(0);
  });
});

describe('grid/aside source parity (issue-32 BUG-006)', () => {
  /**
   * Mirrors the `presentationSource` assembly in TableView.tsx (BUG-006):
   * `database === '' ? undefined : database` — the `database` prop itself
   * stays a plain string, only the source normalises the absence.
   */
  function buildGridSource(database: string): Readonly<Omit<ColumnSource, 'column'>> {
    const connectionId = 'conn-1';
    return {
      connectionId,
      database: database === '' ? undefined : database,
      namespace: 'public',
      table: 'orders',
    };
  }

  it('a missing database hits the same override grid-side and aside-side', () => {
    const asideSource: Readonly<Omit<ColumnSource, 'column'>> = {
      connectionId: 'conn-1',
      database: undefined,
      namespace: 'public',
      table: 'orders',
    };
    // The override key is identical — the collision BUG-004 guarded
    // against is not reintroduced by the normalisation.
    expect(columnOverrideKey({ ...buildGridSource(''), column: 'epoch_ms' })).toBe(
      columnOverrideKey({ ...asideSource, column: 'epoch_ms' }),
    );
    // And the grid-assembled source hits the aside-keyed override through
    // the real plan path: the presented text appears in the aside.
    const { container } = render(
      <DetailPanel
        open
        editable
        columns={COLS}
        row={ROW}
        rowIndex={0}
        onFieldEdit={vi.fn()}
        presentationSnapshot={makeSnapshot(asideSource)}
        presentationSource={buildGridSource('')}
        presentationRegistry={makeRegistry()}
      />,
    );
    expect(asidePres(container).some((t) => t.includes('.123') && t.includes('19:00'))).toBe(true);
  });

  it('a real database value is unaffected by the normalisation', () => {
    expect(columnOverrideKey({ ...buildGridSource('shop'), column: 'epoch_ms' })).not.toBe(
      columnOverrideKey({ ...buildGridSource(''), column: 'epoch_ms' }),
    );
    const { container } = render(
      <DetailPanel
        open
        editable
        columns={COLS}
        row={ROW}
        rowIndex={0}
        onFieldEdit={vi.fn()}
        presentationSnapshot={makeSnapshot()}
        presentationSource={buildGridSource('shop')}
        presentationRegistry={makeRegistry()}
      />,
    );
    expect(asidePres(container).some((t) => t.includes('.123') && t.includes('19:00'))).toBe(true);
    // The empty-database source does not hit the 'shop'-keyed override.
    const { container: emptyContainer } = render(
      <DetailPanel
        open
        editable
        columns={COLS}
        row={ROW}
        rowIndex={0}
        onFieldEdit={vi.fn()}
        presentationSnapshot={makeSnapshot()}
        presentationSource={buildGridSource('')}
        presentationRegistry={makeRegistry()}
      />,
    );
    expect(asidePres(emptyContainer).some((t) => t.includes('.123') && t.includes('19:00'))).toBe(
      false,
    );
  });
});
