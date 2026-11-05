import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, waitFor } from '@testing-library/react';
import { MemorySection } from '../MemorySection';
import type { AiMemoryEntry, AiSettingsConfig } from '../../../types';

const commands = vi.hoisted(() => ({
  memoryList: vi.fn(),
  memorySearch: vi.fn(),
  memoryStats: vi.fn(),
  memoryCreate: vi.fn(),
  memoryUpdate: vi.fn(),
  memoryConfirm: vi.fn(),
  memoryReject: vi.fn(),
  memoryDelete: vi.fn(),
  memoryClear: vi.fn(),
  memoryExport: vi.fn(),
}));

const store = vi.hoisted(() => ({
  saveSettingsConfig: vi.fn().mockResolvedValue(true),
  connections: [] as { id: string; name: string }[],
}));

vi.mock('../../../commands/ai', () => ({ aiCommands: commands }));
vi.mock('../../../stores/aiStore', () => ({
  useAiStore: (selector: (state: typeof store) => unknown) => selector(store),
}));
vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: (selector: (state: typeof store) => unknown) => selector(store),
}));
vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, params?: { count?: number }) =>
      params?.count === undefined ? key : `${key}:${params.count}`,
  }),
}));

const baseMemory = {
  enabled: false,
  autoLearn: false,
  cloudPolicy: 'normal_only' as const,
  privacyAcknowledged: false,
};

const config: AiSettingsConfig = {
  activeProfileId: '',
  profiles: [],
  globalInstructions: '',
  promptTemplates: [],
  memory: baseMemory,
};

function memoryEntry(overrides: Partial<AiMemoryEntry>): AiMemoryEntry {
  return {
    id: 'm1',
    scopeType: 'user',
    scopeId: 'me',
    connectionId: null,
    databaseName: null,
    schemaName: null,
    tableName: null,
    columnName: null,
    kind: 'preference',
    subject: 'Timezone preference',
    content: 'Reports use UTC',
    structuredJson: null,
    source: 'explicit',
    sourceRef: null,
    confidence: 0.8,
    sensitivity: 'normal',
    status: 'candidate',
    pinned: false,
    useCount: 0,
    lastUsedAt: null,
    expiresAt: null,
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    ...overrides,
  };
}

const entries = [
  memoryEntry({ id: 'm1', subject: 'Timezone preference', content: 'Reports use UTC' }),
  memoryEntry({
    id: 'm2',
    subject: 'Order relationship',
    content: 'orders.customer_id -> customers.id',
    kind: 'relationship',
    confidence: 0.9,
  }),
  memoryEntry({ id: 'm3', subject: 'Default limit', status: 'active' }),
];

beforeEach(() => {
  vi.clearAllMocks();
  store.saveSettingsConfig.mockResolvedValue(true);
  commands.memoryList.mockResolvedValue({ entries, total: entries.length, limit: 200, offset: 0 });
  commands.memorySearch.mockResolvedValue({
    entries,
    total: entries.length,
    limit: 200,
    offset: 0,
  });
  commands.memoryStats.mockResolvedValue({
    total: entries.length,
    byStatus: { candidate: 2, active: 1 },
    byKind: { preference: 2, relationship: 1 },
    byScope: { user: 3 },
  });
  commands.memoryConfirm.mockResolvedValue(entries[0]);
  commands.memoryReject.mockResolvedValue(entries[0]);
  commands.memoryUpdate.mockResolvedValue(entries[0]);
  commands.memoryDelete.mockResolvedValue(true);
  commands.memoryExport.mockResolvedValue('[]');
});

afterEach(cleanup);

describe('MemorySection', () => {
  it('is disabled by default and persists the first-use acknowledgement', async () => {
    const { getByTestId, queryByTestId } = render(<MemorySection config={config} />);
    expect(getByTestId('ai-memory-enabled')).toHaveAttribute('aria-checked', 'false');
    fireEvent.click(getByTestId('ai-memory-enabled'));
    expect(getByTestId('ai-memory-first-use')).toBeInTheDocument();
    await waitFor(() => expect(queryByTestId('ai-memory-list')).not.toBeNull());

    fireEvent.click(getByTestId('ai-memory-first-use-confirm'));
    await waitFor(() =>
      expect(store.saveSettingsConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          memory: expect.objectContaining({ enabled: true, privacyAcknowledged: true }),
        }),
      ),
    );
  });

  it('confirms only the candidates in the current filtered result and shows the count', async () => {
    const { getByTestId, getByText } = render(
      <MemorySection config={{ ...config, memory: { ...baseMemory, enabled: true } }} />,
    );
    await waitFor(() => expect(getByTestId('ai-memory-row-m1')).toBeInTheDocument());
    expect(getByText('settings.ai.memory.confirmAll:2')).toBeInTheDocument();
    fireEvent.click(getByTestId('ai-memory-confirm-all'));
    await waitFor(() => {
      expect(commands.memoryConfirm).toHaveBeenCalledTimes(2);
      expect(commands.memoryConfirm).toHaveBeenCalledWith('m1');
      expect(commands.memoryConfirm).toHaveBeenCalledWith('m2');
    });
  });

  it('persists auto-learn independently of the master switch', async () => {
    const { getByTestId } = render(
      <MemorySection config={{ ...config, memory: { ...baseMemory, enabled: true } }} />,
    );
    await waitFor(() => expect(getByTestId('ai-memory-row-m1')).toBeInTheDocument());
    fireEvent.click(getByTestId('ai-memory-auto-learn'));
    await waitFor(() =>
      expect(store.saveSettingsConfig).toHaveBeenCalledWith(
        expect.objectContaining({ memory: expect.objectContaining({ autoLearn: true }) }),
      ),
    );
  });

  it('confirm all applies only to the current filtered result', async () => {
    commands.memoryList.mockImplementation(async (params: { kind?: string; status?: string }) => ({
      entries: entries.filter(
        (entry) =>
          (!params.kind || entry.kind === params.kind) &&
          (!params.status || entry.status === params.status),
      ),
      total: 1,
      limit: 200,
      offset: 0,
    }));
    const { getByTestId, getByText } = render(
      <MemorySection config={{ ...config, memory: { ...baseMemory, enabled: true } }} />,
    );
    await waitFor(() => expect(getByTestId('ai-memory-row-m2')).toBeInTheDocument());
    fireEvent.change(getByTestId('ai-memory-filter-kind'), {
      target: { value: 'relationship' },
    });
    await waitFor(() => expect(getByTestId('ai-memory-row-m2')).toBeInTheDocument());
    await waitFor(() => expect(getByText('settings.ai.memory.confirmAll:1')).toBeInTheDocument());
    fireEvent.click(getByTestId('ai-memory-confirm-all'));
    await waitFor(() => {
      expect(commands.memoryConfirm).toHaveBeenCalledTimes(1);
      expect(commands.memoryConfirm).toHaveBeenCalledWith('m2');
    });
  });

  it('confirms, rejects, edits, and merges candidates', async () => {
    const { getByTestId } = render(
      <MemorySection config={{ ...config, memory: { ...baseMemory, enabled: true } }} />,
    );
    await waitFor(() => expect(getByTestId('ai-memory-row-m1')).toBeInTheDocument());

    fireEvent.click(getByTestId('ai-memory-confirm-m1'));
    await waitFor(() => expect(commands.memoryConfirm).toHaveBeenCalledWith('m1'));
    fireEvent.click(getByTestId('ai-memory-reject-m2'));
    await waitFor(() => expect(commands.memoryReject).toHaveBeenCalledWith('m2'));

    fireEvent.click(getByTestId('ai-memory-edit-m1'));
    fireEvent.change(getByTestId('ai-memory-edit-subject'), {
      target: { value: 'Updated timezone' },
    });
    fireEvent.click(getByTestId('ai-memory-edit-save'));
    await waitFor(() =>
      expect(commands.memoryUpdate).toHaveBeenCalledWith(
        'm1',
        expect.objectContaining({ subject: 'Updated timezone' }),
      ),
    );

    fireEvent.click(getByTestId('ai-memory-select-m1'));
    fireEvent.click(getByTestId('ai-memory-select-m2'));
    fireEvent.click(getByTestId('ai-memory-merge-selected'));
    await waitFor(() => {
      expect(commands.memoryUpdate).toHaveBeenCalledWith(
        'm1',
        expect.objectContaining({
          status: 'active',
          content: expect.stringContaining('orders.customer_id'),
        }),
      );
      expect(commands.memoryDelete).toHaveBeenCalledWith('m2');
    });
    expect(getByTestId('ai-memory-candidates-title')).toHaveTextContent(
      'settings.ai.memory.candidatesTitle',
    );
  });

  it('exports JSON and Markdown and deletes immediately', async () => {
    const { getByTestId, queryByTestId } = render(
      <MemorySection config={{ ...config, memory: { ...baseMemory, enabled: true } }} />,
    );
    await waitFor(() => expect(getByTestId('ai-memory-row-m1')).toBeInTheDocument());

    fireEvent.click(getByTestId('ai-memory-export-json'));
    await waitFor(() => expect(commands.memoryExport).toHaveBeenCalledWith({ format: 'json' }));
    fireEvent.click(getByTestId('ai-memory-export-markdown'));
    await waitFor(() => expect(commands.memoryExport).toHaveBeenCalledWith({ format: 'markdown' }));

    fireEvent.click(getByTestId('ai-memory-delete-m1'));
    await waitFor(() => expect(commands.memoryDelete).toHaveBeenCalledWith('m1'));
    await waitFor(() => expect(queryByTestId('ai-memory-row-m1')).toBeNull());
  });
});
