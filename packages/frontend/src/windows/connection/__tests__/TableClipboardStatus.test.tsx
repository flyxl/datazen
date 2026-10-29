/**
 * DB-03 §3.4 — the one-line summary under the grid.
 *
 * Two things were promised and neither was rendered: a paste that reverts staged edits says
 * so ("overwrote K pending change(s)"), and a rejected paste names what it dropped. The
 * component had the markup for both; the mistake was in *when* it was allowed to render, and
 * that is only visible by rendering it.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import type { TableClipboardBridge } from '../useTableViewClipboard';
import type { PasteSummary } from '../../../hooks/useGridClipboard';
import { TableClipboardStatus } from '../TableClipboardStatus';
import enQuery from '../../../locales/en/query';

const EMPTY = new Set<string>();

const bridge = (overrides: Partial<TableClipboardBridge> = {}): TableClipboardBridge => ({
  dataTableProps: { clipboard: {} as TableClipboardBridge['dataTableProps']['clipboard'], cellErrors: EMPTY },
  handlers: {} as TableClipboardBridge['handlers'],
  cellErrors: EMPTY,
  problems: [],
  lastPaste: null,
  clear: vi.fn(),
  copySelectedCellsAs: vi.fn(),
  copyFeedback: { copied: false },
  confirmElement: null,
  ...overrides,
});

const summary = (overrides: Partial<PasteSummary> = {}): PasteSummary => ({
  cells: 0,
  staged: 0,
  skipped: 0,
  overwrittenPending: 0,
  rows: 0,
  columns: 0,
  cancelled: false,
  ...overrides,
});

beforeEach(async () => {
  cleanup();
  // The component translates through `useI18n`, so the real English pack has to be
  // registered before it renders — otherwise every assertion would read raw keys.
  const i18n = await import('@datazen/ui');
  i18n.registerTranslations({ en: enQuery as unknown as Record<string, string> });
  i18n.setLocale('en');
});

describe('TableClipboardStatus', () => {
  it('says that a paste reverted staged edits, even though it staged nothing', () => {
    // Pasting the original values back: nothing staged, nothing skipped, but two staged
    // changes are gone. The line used to disappear entirely for this outcome.
    render(
      <TableClipboardStatus
        bridge={bridge({ lastPaste: summary({ overwrittenPending: 2 }) })}
      />,
    );

    expect(screen.getByTestId('table-clipboard-status')).toBeInTheDocument();
    expect(screen.getByTestId('table-clipboard-status').textContent).toContain('2');
  });

  it('shows only the override note when nothing was staged or skipped', () => {
    render(
      <TableClipboardStatus bridge={bridge({ lastPaste: summary({ overwrittenPending: 1 }) })} />,
    );

    // No "0 cell(s) pasted" line: there was no paste to report, only a revert.
    expect(screen.getByTestId('table-clipboard-status').textContent).not.toMatch(/0 /);
  });

  it('stays hidden when the paste changed nothing at all', () => {
    render(<TableClipboardStatus bridge={bridge({ lastPaste: summary() })} />);
    expect(screen.queryByTestId('table-clipboard-status')).not.toBeInTheDocument();
  });

  it('names the dropped columns in the message line', () => {
    render(
      <TableClipboardStatus
        bridge={bridge({
          lastPaste: summary({ cells: 3, skipped: 2 }),
          problems: [
            { code: 'grid.paste.columnMismatch', row: null, column: null, detail: 'city, zip' },
          ],
        })}
      />,
    );

    const text = screen.getByTestId('table-clipboard-status').textContent ?? '';
    expect(text).toContain('city, zip');
    expect(text).toContain('2');
  });
});
