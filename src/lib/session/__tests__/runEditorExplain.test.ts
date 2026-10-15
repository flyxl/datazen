import { beforeEach, expect, it, vi } from 'vitest';
import { runEditorExplain } from '../runEditorExplain';

const runtime = vi.hoisted(() => ({
  get: vi.fn(),
  execute: vi.fn(),
  refresh: vi.fn(),
  getExecution: vi.fn(),
  readArtifact: vi.fn(),
}));
vi.mock('../queryPanelSessionRoot', () => ({
  editorOwnerForPane: async (key: string) => ({ kind: 'editor', editorSessionId: key }),
  queryPanelExecutionTarget: (connectionId: string, database: string, schema: string) => ({
    connectionId,
    namespace: { database, schema },
  }),
  getQueryPanelSessionRegistry: () => ({ get: runtime.get }),
}));

beforeEach(() => {
  vi.clearAllMocks();
  runtime.get.mockReturnValue({
    execute: runtime.execute,
    refresh: runtime.refresh,
    snapshot: { handle: { dbSessionId: 'owned', runtimeEpoch: 'epoch' } },
    client: {
      backendId: 'desktop',
      getExecution: runtime.getExecution,
      readArtifact: runtime.readArtifact,
    },
  });
  runtime.execute.mockResolvedValue({
    executionId: 'explain-1',
    streamId: 'stream-1',
    state: 'queued',
  });
  runtime.getExecution.mockResolvedValue({
    executionId: 'explain-1',
    state: 'succeeded',
    artifactIds: ['plan-1'],
    runtimeBinding: null,
  });
  runtime.refresh.mockResolvedValue(null);
  runtime.readArtifact.mockResolvedValue({
    artifactId: 'plan-1',
    chunkIndex: '0',
    totalChunks: '1',
    bytes: new TextEncoder().encode(
      JSON.stringify({ output: { kind: 'commandResult', data: { planText: 'Scan owned_temp' } } }),
    ),
  });
});

it('uses the same pane controller and session command with bind values', async () => {
  await expect(
    runEditorExplain('p1::split', 'config', 'SELECT :id', 'A', 's', { id: 73 }),
  ).resolves.toEqual({ planText: 'Scan owned_temp' });
  expect(runtime.get).toHaveBeenCalledWith(
    'p1::split',
    { connectionId: 'config', namespace: { database: 'A', schema: 's' } },
    { kind: 'editor', editorSessionId: 'p1::split' },
  );
  expect(runtime.execute).toHaveBeenCalledExactlyOnceWith({
    command: 'explain',
    input: { sql: 'SELECT :id', params: { id: 73 } },
  });
  expect(runtime.refresh).toHaveBeenCalledOnce();
});

it('recovers a lost artifact read without resubmitting the plan', async () => {
  runtime.readArtifact.mockRejectedValueOnce(new Error('disconnected'));
  await expect(runEditorExplain('p1', 'config', 'SELECT 1', 'A', null)).resolves.toEqual({
    planText: 'Scan owned_temp',
  });
  expect(runtime.execute).toHaveBeenCalledTimes(1);
  expect(runtime.readArtifact).toHaveBeenCalledTimes(2);
});

for (const state of ['failed', 'cancelled'] as const) {
  it(`preserves the ${state} execution outcome instead of reopening a session`, async () => {
    runtime.getExecution.mockResolvedValue({
      executionId: 'explain-1',
      state,
      errorCode: 'resourceLost',
      effectOutcome: 'unknown',
      artifactIds: [],
    });
    await expect(runEditorExplain('p1', 'config', 'SELECT 1', 'A', null)).rejects.toThrow(
      `${state}: resourceLost; effect: unknown`,
    );
    expect(runtime.execute).toHaveBeenCalledTimes(1);
  });
}
