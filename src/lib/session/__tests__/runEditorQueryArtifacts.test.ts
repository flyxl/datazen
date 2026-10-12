import { beforeEach, expect, it, vi } from 'vitest';
import type { QueryStreamEvent } from '../../../types';
import { runEditorQuery } from '../runEditorQuery';

const runtime = vi.hoisted(() => ({ getExecution: vi.fn(), readArtifact: vi.fn(), refresh: vi.fn(), execute: vi.fn() }));
vi.mock('../../../stores/settingsStore', () => ({ useSettingsStore: { getState: () => ({ settings: { limitSelectResults: false } }) } }));
vi.mock('../queryPanelSessionRoot', () => ({
  editorOwnerForPane: async () => ({ kind: 'editor' }),
  queryPanelExecutionTarget: () => ({}),
  getQueryPanelSessionRegistry: () => ({ get: () => ({ execute: runtime.execute, refresh: runtime.refresh, snapshot: null,
    client: { getExecution: runtime.getExecution, readArtifact: runtime.readArtifact } }) }),
}));

beforeEach(() => {
  vi.clearAllMocks();
  runtime.execute.mockResolvedValue({ executionId: 'execution-1', streamId: 'stream-1', state: 'queued' });
  runtime.refresh.mockResolvedValue(null);
  runtime.readArtifact.mockResolvedValue({ artifactId: 'artifact-1', chunkIndex: '0', totalChunks: '1',
    bytes: new TextEncoder().encode(JSON.stringify({ output: { kind: 'rows', statementIndex: 0, rows: [[1]] } })) });
});

for (const state of ['succeeded', 'failed', 'cancelled'] as const) {
  it(`preserves authoritative artifact references before ${state} completion`, async () => {
    runtime.getExecution.mockResolvedValue({ executionId: 'execution-1', state,
      artifactIds: ['artifact-1'], resultCompleteness: state === 'succeeded' ? 'complete' : 'truncated',
      provenance: null, errorCode: state === 'cancelled' ? 'cancelled' : 'sqlError', effectOutcome: 'unknown' });
    const events: QueryStreamEvent[] = [];
    const run = runEditorQuery('pane', 'connection', 'SELECT 1', 'db', null, undefined, (event) => events.push(event));
    if (state === 'succeeded') await expect(run).resolves.toBeUndefined();
    else await expect(run).rejects.toThrow(state);
    const published = events.find((event) => event.type === 'publishedArtifacts');
    expect(published).toEqual({ type: 'publishedArtifacts', artifacts: { executionId: 'execution-1', artifactIds: ['artifact-1'],
      resultCompleteness: state === 'succeeded' ? 'complete' : 'truncated' } });
    expect(events[0]).toEqual({ type: 'executionStarted', executionId: 'execution-1' });
    expect(events.some((event) => event.type === 'done')).toBe(state === 'succeeded');
  });
}
