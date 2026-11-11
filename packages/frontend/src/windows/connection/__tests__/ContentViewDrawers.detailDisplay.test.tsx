/**
 * ContentViewDrawers with the real DetailPanel (issue-32 BUG-005).
 *
 * The T05 tester asked for this mount: the production combination is a
 * table panel rendered through `ContentViewDrawers`, which always passes
 * `editable` + `onFieldEdit` for table/view panels — the combination the
 * DetailPanel-only tests never exercised, which is how BUG-005 shipped.
 * The drawer reads its rows from `useTableDataStore`, so the slice is
 * seeded directly; no IPC runs for a table panel (`useResultEditability`
 * gets a null key).
 */
import { describe, expect, it, vi, afterEach, beforeEach } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/react';
import { valuePresentationRegistry } from '@datazen/extension-points';
import { ContentViewDrawers } from '../ContentViewDrawers';
import { useTableDataStore } from '../../../stores/tableDataStore';
import { emptyTableState } from '../../../stores/tableData/connectionState';
import type { Panel } from '../../../stores/panelStore';
import {
  TIMESTAMP_CONTRIBUTION_ID,
  timestampContribution,
  columnOverrideKey,
  type ValuePresentationSnapshot,
} from '../../../lib/valuePresentation/index';

vi.mock('../../../hooks/useResizable', () => ({
  useResizable: () => ({ size: 320, handleRef: { current: null }, startResize: vi.fn() }),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

const PANEL = {
  id: 'panel-table-1',
  type: 'table',
  connectionId: 'conn-1',
  dbSessionId: 'sess-1',
  connectionName: 'App',
  databaseType: 'postgresql',
  database: 'shop',
  tableSchema: 'public',
  tableName: 'orders',
  subTab: 'data',
} as unknown as Panel;

const SOURCE = {
  connectionId: 'conn-1',
  database: 'shop',
  namespace: 'public',
  table: 'orders',
} as const;

function makeSnapshot(): ValuePresentationSnapshot {
  return {
    revision: 1,
    environment: { locale: 'en-GB', localTimeZone: 'Asia/Shanghai' },
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
  };
}

beforeEach(() => {
  // ContentViewDrawers passes no registry, so the aside reads the global
  // one — the production app registers the contribution at startup (T06
  // owns the threading); the test registers it here and unregisters after.
  valuePresentationRegistry.register(timestampContribution);
  useTableDataStore.setState({
    byPanel: new Map([
      [
        PANEL.id,
        {
          ...emptyTableState(null),
          columns: [
            { name: 'created_at', dataType: 'timestamp with time zone', nullable: true },
            { name: 'epoch_ms', dataType: 'bigint', nullable: true },
            { name: 'qty', dataType: 'integer', nullable: true },
          ],
          rows: [{ created_at: '2026-01-15T14:30:00Z', epoch_ms: 1705316400123, qty: 42 }],
          totalRows: 1,
          detailRowIndex: 0,
          selectedRows: new Set([0]),
        },
      ],
    ]),
  });
});

afterEach(() => {
  cleanup();
  valuePresentationRegistry.unregister(TIMESTAMP_CONTRIBUTION_ID);
  useTableDataStore.setState({ byPanel: new Map() });
});

function renderDrawers() {
  return render(
    <ContentViewDrawers
      activePanel={PANEL}
      detailOpen
      aiChatOpen={false}
      detailPanelApplicable
      dbSessionId="sess-1"
      connectionName="App"
      currentDatabase="shop"
      databaseType="postgresql"
      onCloseDetail={() => undefined}
      pendingDraftRequest={null}
      onDraftConsumed={() => undefined}
      presentationSnapshot={makeSnapshot()}
    />,
  );
}

function asidePres(container: HTMLElement): string[] {
  return Array.from(container.querySelectorAll('aside pre')).map((pre) => pre.textContent ?? '');
}

describe('ContentViewDrawers detail drawer (issue-32 BUG-005)', () => {
  it('shows presented text in the aside for a table panel with a snapshot', () => {
    const { container } = renderDrawers();
    expect(asidePres(container)).toContain('2026-01-15T22:30:00+08:00');
    expect(asidePres(container).some((t) => t.includes('.123') && t.includes('19:00'))).toBe(true);
    expect(asidePres(container)).toContain('42');
    // No field editor is mounted by default — only the search input.
    expect(container.querySelectorAll('aside input')).toHaveLength(1);
    expect(asidePres(container)).not.toContain('2026-01-15T14:30:00Z');
    expect(asidePres(container)).not.toContain('1705316400123');
  });

  it('click opens the editor from the raw value and Escape returns to display', () => {
    const { container } = renderDrawers();
    const displays = container.querySelectorAll('aside [role="button"]');
    fireEvent.click(displays[0]);
    const input = Array.from(container.querySelectorAll('aside input')).slice(
      1,
    )[0] as HTMLInputElement;
    expect(input).toHaveValue('2026-01-15T14:30:00Z');
    fireEvent.change(input, { target: { value: 'temp' } });
    fireEvent.keyDown(input, { key: 'Escape' });
    expect(container.querySelectorAll('aside input')).toHaveLength(1);
    expect(asidePres(container)).toContain('2026-01-15T22:30:00+08:00');
  });
});
