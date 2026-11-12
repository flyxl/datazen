/**
 * Column presentation entries（issue-32 T09，PRD 验收 2）：
 * 网格表头右键菜单「显示方式…」与 aside 字段菜单同名入口打开同一
 * 面板、编辑同一份列配置；面板保存后 grid cell 与 aside 即时一致；
 * 再从 aside 入口改回原始值，两处同步回落。
 *
 * Mocks  mirror DataTable.presentation.test.tsx（i18n、column resize、
 * virtual table、native context menu）与 T07 的 settingsStore mock
 * （对话框默认开启路径读 store；本文件不点开启，只防模块副作用）。
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup, waitFor, act } from '@testing-library/react';
import { ValuePresentationRegistryImpl, type ColumnSource } from '@datazen/extension-points';
import { DataTable } from '../DataTable';
import { DetailPanel } from '../DetailPanel';
import {
  TIMESTAMP_CONTRIBUTION_ID,
  applyValuePresentationUpdate,
  buildValuePresentationSnapshot,
  parseStoredValuePresentationSettings,
  resetColumnPresentationSession,
  timestampContribution,
  type StoredValuePresentationSettings,
  type ValuePresentationSettingsUpdate,
  type ValuePresentationSnapshot,
} from '../../../lib/valuePresentation/index';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../hooks/useColumnResize', () => ({
  useColumnResize: () => ({
    columnWidths: [160, 200],
    onResizeStart: vi.fn(),
  }),
  adjustWidthsForSort: (widths: number[]) => widths,
}));

vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

const { showNativeContextMenu } = vi.hoisted(() => ({
  showNativeContextMenu: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../../lib/nativeContextMenu', () => ({
  showNativeContextMenu: (...args: unknown[]) => showNativeContextMenu(...args),
}));

const mocks = vi.hoisted(() => ({
  state: {
    valuePresentation: { version: 1, renderers: {} } as StoredValuePresentationSettings,
  },
  updateValuePresentationSettings: vi.fn(async (input: ValuePresentationSettingsUpdate) => {
    // T11：mock 按生产 store 的语义应用 updater；persistedSnapshot 读
    // mocks.state.valuePresentation 即是已持久化的值。
    mocks.state.valuePresentation = applyValuePresentationUpdate(
      mocks.state.valuePresentation,
      input,
    );
  }),
}));

vi.mock('../../../stores/settingsStore', () => {
  const storeState = () => ({
    settings: { valuePresentation: mocks.state.valuePresentation },
    updateValuePresentationSettings: mocks.updateValuePresentationSettings,
  });
  const useSettingsStore = Object.assign(
    (sel: (s: ReturnType<typeof storeState>) => unknown) => sel(storeState()),
    { getState: storeState },
  );
  return { useSettingsStore };
});

type MenuItem = {
  kind: string;
  id?: string;
  label?: string;
  action?: () => void;
  items?: MenuItem[];
};

const SOURCE: Readonly<Omit<ColumnSource, 'column'>> = {
  connectionId: 'conn-1',
  database: 'shop',
  namespace: 'public',
  table: 'orders',
};

const ENVIRONMENT = { locale: 'en-US', localTimeZone: 'Asia/Shanghai' } as const;

const COLS = [
  { id: 'epoch_ms', name: 'epoch_ms', type: 'bigint' },
  { id: 'created_at', name: 'created_at', type: 'timestamp with time zone' },
];

const ROWS: unknown[][] = [[1705316400000, '2026-01-15T14:30:00Z']];

const ROW_RECORD = { epoch_ms: 1705316400000, created_at: '2026-01-15T14:30:00Z' };

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
    overrides: {},
    rules: [],
  };
}

function makeRegistry(): ValuePresentationRegistryImpl {
  const registry = new ValuePresentationRegistryImpl();
  registry.register(timestampContribution);
  return registry;
}

function renderWithSnapshot(
  snapshot: ValuePresentationSnapshot,
  registry: ValuePresentationRegistryImpl,
) {
  return render(
    <>
      <DataTable
        columns={COLS}
        rows={ROWS}
        presentationSnapshot={snapshot}
        presentationSource={SOURCE}
        presentationRegistry={registry}
      />
      <DetailPanel
        open
        columns={COLS}
        row={ROW_RECORD}
        rowIndex={0}
        presentationSnapshot={snapshot}
        presentationSource={SOURCE}
        presentationRegistry={registry}
      />
    </>,
  );
}

function renderGridAndAside() {
  const snapshot = makeSnapshot();
  const registry = makeRegistry();
  const rendered = renderWithSnapshot(snapshot, registry);
  return { ...rendered, snapshot, registry };
}

/**
 * 生产链路里快照来自 settingsStore（useValuePresentationSnapshot）：
 * 面板保存成功后会话条目清除，快照即持久值。本测试用已持久化的
 * stored 值重建快照再渲染，模拟这一链路（T10：保存变异步持久化，
 * 固定快照 prop 不再自动带上覆盖）。
 */
function persistedSnapshot(
  registry: ValuePresentationRegistryImpl,
  revision: number,
): ValuePresentationSnapshot {
  return buildValuePresentationSnapshot(
    parseStoredValuePresentationSettings(mocks.state.valuePresentation),
    ENVIRONMENT,
    registry,
    revision,
  );
}

async function openHeaderMenu(container: HTMLElement): Promise<MenuItem[]> {
  const header = container.querySelector('[data-col-header="epoch_ms"]')!;
  fireEvent.contextMenu(header, { clientX: 10, clientY: 10 });
  await waitFor(() => expect(showNativeContextMenu).toHaveBeenCalled());
  return showNativeContextMenu.mock.calls[0]![0] as MenuItem[];
}

function cellText(container: HTMLElement, columnName: string): string {
  return container.querySelector(`[data-dt-col="${columnName}"]`)?.textContent ?? '';
}

beforeEach(() => {
  vi.clearAllMocks();
  resetColumnPresentationSession();
});

afterEach(cleanup);

describe('column presentation entries (issue-32 T09)', () => {
  it('the header menu offers "Display…" for the right-clicked column', async () => {
    const { container } = renderGridAndAside();
    const items = await openHeaderMenu(container);
    expect(items.some((item) => item.id === 'column-presentation')).toBe(true);
  });

  it('grid entry and aside entry open the same panel and apply to both places', async () => {
    const { container, rerender, registry } = renderGridAndAside();
    // Before any override the BIGINT column shows its raw digits in both places.
    expect(cellText(container, 'epoch_ms')).toBe('1705316400000');
    expect(screen.getAllByText('1705316400000')).toHaveLength(2);

    // Entry 1: grid header menu → Display… → Time (Unix ms) → Save.
    const items = await openHeaderMenu(container);
    const entry = items.find((item) => item.id === 'column-presentation');
    expect(entry?.action).toBeDefined();
    act(() => {
      entry?.action?.();
    });
    expect(screen.getByTestId('column-presentation-dialog-body')).toBeTruthy();
    expect(screen.getByTestId('column-presentation-original').textContent).toBe('1705316400000');
    fireEvent.click(screen.getByTestId('column-presentation-mode-custom'));
    expect(screen.getByTestId('column-presentation-preview-result').textContent).toBe(
      '2024-01-15T19:00:00+08:00',
    );
    fireEvent.click(screen.getByTestId('column-presentation-save'));

    // 保存是异步持久化：等写盘完成、面板关闭后，用已持久化的值重建
    // 快照再渲染——grid cell 与 aside field 读同一份持久覆盖。
    await waitFor(() => {
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledTimes(1);
    });
    await waitFor(() => {
      expect(screen.queryByTestId('column-presentation-dialog-body')).toBeNull();
    });
    const persisted = persistedSnapshot(registry, 2);
    rerender(
      <>
        <DataTable
          columns={COLS}
          rows={ROWS}
          presentationSnapshot={persisted}
          presentationSource={SOURCE}
          presentationRegistry={registry}
        />
        <DetailPanel
          open
          columns={COLS}
          row={ROW_RECORD}
          rowIndex={0}
          presentationSnapshot={persisted}
          presentationSource={SOURCE}
          presentationRegistry={registry}
        />
      </>,
    );
    expect(cellText(container, 'epoch_ms')).toBe('2024-01-15T19:00:00+08:00');
    expect(screen.getAllByText('2024-01-15T19:00:00+08:00')).toHaveLength(2);

    // Entry 2: the aside field menu opens the same panel for the same column.
    fireEvent.click(screen.getByTestId('detail-field-presentation-epoch_ms'));
    expect(screen.getByTestId('column-presentation-dialog-body')).toBeTruthy();
    expect(
      (screen.getByTestId('column-presentation-mode-custom') as HTMLInputElement).checked,
    ).toBe(true);
    // Switch the same column back to raw: both places fall back to the raw value.
    fireEvent.click(screen.getByTestId('column-presentation-mode-raw'));
    fireEvent.click(screen.getByTestId('column-presentation-save'));
    await waitFor(() => {
      expect(mocks.updateValuePresentationSettings).toHaveBeenCalledTimes(2);
    });
    await waitFor(() => {
      expect(screen.queryByTestId('column-presentation-dialog-body')).toBeNull();
    });
    const persistedAfterRaw = persistedSnapshot(registry, 3);
    rerender(
      <>
        <DataTable
          columns={COLS}
          rows={ROWS}
          presentationSnapshot={persistedAfterRaw}
          presentationSource={SOURCE}
          presentationRegistry={registry}
        />
        <DetailPanel
          open
          columns={COLS}
          row={ROW_RECORD}
          rowIndex={0}
          presentationSnapshot={persistedAfterRaw}
          presentationSource={SOURCE}
          presentationRegistry={registry}
        />
      </>,
    );
    expect(cellText(container, 'epoch_ms')).toBe('1705316400000');
    expect(screen.getAllByText('1705316400000')).toHaveLength(2);
  });
});
