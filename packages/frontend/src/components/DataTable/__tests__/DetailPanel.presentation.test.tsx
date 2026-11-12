/**
 * DetailPanel presentation wiring (issue-32 T05, design §7).
 *
 * The aside builds its own plans from the same (columns, source, snapshot)
 * triple as the grid, so the same input reads the same text. The display
 * path consults the plan; the editor still initialises from the raw value
 * and NULL still renders `NULL`. No snapshot ⇒ the old formatCell text
 * (default-off).
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

function makeSnapshot(): ValuePresentationSnapshot {
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
      [columnOverrideKey({ ...SOURCE, column: 'epoch_ms' })]: {
        mode: 'custom',
        rendererId: TIMESTAMP_CONTRIBUTION_ID,
        options: {
          interpretation: { kind: 'unix', unit: 'milliseconds' },
          format: 'iso-tz',
          displayTimeZone: { kind: 'iana', name: 'Asia/Shanghai' },
        },
      },
    },
    rules: [],
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

/**
 * BUG-005: editable fields now start in display state — click the display
 * container (in column order) to open the editor for that field.
 */
function openEditor(container: HTMLElement, index = 0): void {
  const displays = container.querySelectorAll('aside [role="button"]');
  fireEvent.click(displays[index]);
}

function asidePres(container: HTMLElement): string[] {
  return Array.from(container.querySelectorAll('aside pre')).map((pre) => pre.textContent ?? '');
}

describe('DetailPanel presentation wiring (issue-32 T05)', () => {
  it('shows the PRD anchor as Shanghai iso-tz in the aside', () => {
    const { container } = render(
      <DetailPanel
        open
        columns={COLS}
        row={ROW}
        rowIndex={0}
        presentationSnapshot={makeSnapshot()}
        presentationSource={SOURCE}
        presentationRegistry={makeRegistry()}
      />,
    );
    expect(asidePres(container)).toContain('2026-01-15T22:30:00+08:00');
  });

  it('shows the BIGINT Unix column with milliseconds intact (BUG-003)', () => {
    const { container } = render(
      <DetailPanel
        open
        columns={COLS}
        row={ROW}
        rowIndex={0}
        presentationSnapshot={makeSnapshot()}
        presentationSource={SOURCE}
        presentationRegistry={makeRegistry()}
      />,
    );
    const texts = asidePres(container);
    expect(texts.some((text) => text.includes('.123') && text.includes('19:00'))).toBe(true);
  });

  it('leaves a plain number column untouched', () => {
    const { container } = render(
      <DetailPanel
        open
        columns={COLS}
        row={ROW}
        rowIndex={0}
        presentationSnapshot={makeSnapshot()}
        presentationSource={SOURCE}
        presentationRegistry={makeRegistry()}
      />,
    );
    expect(asidePres(container)).toContain('42');
  });

  it('shows the old formatCell text with no snapshot (default-off)', () => {
    const { container } = render(<DetailPanel open columns={COLS} row={ROW} rowIndex={0} />);
    expect(asidePres(container)).toContain('2026-01-15T14:30:00Z');
    expect(asidePres(container).some((text) => text.includes('22:30'))).toBe(false);
    expect(asidePres(container)).toContain('1705316400123');
  });

  it('initialises the editor from the raw value, never the presented text', () => {
    const onFieldEdit = vi.fn();
    const { container } = render(
      <DetailPanel
        open
        editable
        columns={[{ id: 'created_at', name: 'created_at', type: 'timestamp with time zone' }]}
        row={{ created_at: '2026-01-15T14:30:00Z' }}
        rowIndex={0}
        onFieldEdit={onFieldEdit}
        presentationSnapshot={makeSnapshot()}
        presentationSource={SOURCE}
        presentationRegistry={makeRegistry()}
      />,
    );
    // BUG-005: the editable aside now shows the presented text by default…
    expect(asidePres(container)).toContain('2026-01-15T22:30:00+08:00');
    expect(fieldInputs(container)).toHaveLength(0);
    // …and only the editor, opened from the display, holds the raw value.
    openEditor(container);
    const input = fieldInputs(container)[0];
    expect(input).toHaveValue('2026-01-15T14:30:00Z');
    expect(input).not.toHaveValue('2026-01-15T22:30:00+08:00');
  });

  it('NULL still renders NULL even with a snapshot', () => {
    const { container } = render(
      <DetailPanel
        open
        columns={COLS}
        row={{ ...ROW, created_at: null }}
        rowIndex={0}
        presentationSnapshot={makeSnapshot()}
        presentationSource={SOURCE}
        presentationRegistry={makeRegistry()}
      />,
    );
    expect(asidePres(container)).toContain('NULL');
    expect(asidePres(container).some((text) => text.includes('22:30'))).toBe(false);
  });

  it('commits a plain text edit on blur (pre-existing path)', () => {
    const onFieldEdit = vi.fn();
    const { container } = render(
      <DetailPanel
        open
        editable
        columns={[{ id: 'name', name: 'name', type: 'varchar' }]}
        row={{ name: 'Bob' }}
        rowIndex={0}
        onFieldEdit={onFieldEdit}
      />,
    );
    // BUG-005: the editor opens from the display container, not on mount.
    openEditor(container);
    const input = fieldInputs(container)[0];
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: 'Carol' } });
    fireEvent.blur(input);
    expect(onFieldEdit).toHaveBeenCalledWith(0, 'name', 'Carol');
  });

  it('commits a plain text edit on Enter (pre-existing path)', () => {
    const onFieldEdit = vi.fn();
    const { container } = render(
      <DetailPanel
        open
        editable
        columns={[{ id: 'name', name: 'name', type: 'varchar' }]}
        row={{ name: 'Bob' }}
        rowIndex={0}
        onFieldEdit={onFieldEdit}
      />,
    );
    // BUG-005: the editor opens from the display container, not on mount.
    openEditor(container);
    const input = fieldInputs(container)[0];
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: 'Dave' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onFieldEdit).toHaveBeenCalledWith(0, 'name', 'Dave');
  });

  it('filters fields by the column search (pre-existing path)', () => {
    const { container, queryByText, getByText } = render(
      <DetailPanel open columns={COLS} row={ROW} rowIndex={0} />,
    );
    const search = container.querySelector('input')!;
    fireEvent.change(search, { target: { value: 'created' } });
    expect(getByText('created_at')).toBeInTheDocument();
    expect(queryByText('qty')).toBeNull();
    expect(queryByText('name')).toBeNull();
  });
});
