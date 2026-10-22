/**
 * The **generation** guard on the verdict cache.
 *
 * A verdict is about the rows it was made for. When the result is replaced
 * while its verdict request is still in flight, the answer that arrives
 * afterwards is about rows that are no longer on screen — it must not land in
 * the cache.
 *
 * The store answers that with a generation counter: every request takes a
 * number, and an answer whose number is no longer the current one is
 * dropped. This file reproduces that scenario directly against the store.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { resultEditKey, usePanelResultEdit } from '../panelResultEdit';

const { resolveResultOrigin } = vi.hoisted(() => ({ resolveResultOrigin: vi.fn() }));

vi.mock('../../commands/query', () => ({
  queryCommands: {
    resolveResultOrigin: (...args: unknown[]) => resolveResultOrigin(...args),
  },
}));

const KEY = resultEditKey('sess-1', 0);

const REQUEST = {
  dbSessionId: 'sess-1',
  database: 'app',
  schema: 'public',
  sql: 'SELECT id, total FROM orders',
  resultColumns: ['id', 'total'],
  table: 'orders',
};

const VERDICT = {
  writable: false,
  identityColumns: [],
  editableColumns: [],
  table: null,
  reason: 'multiTable' as const,
};

beforeEach(() => {
  resolveResultOrigin.mockReset();
  usePanelResultEdit.setState({ verdicts: {}, resolving: {}, errors: {}, generations: {} });
});

afterEach(() => {
  vi.clearAllMocks();
});

describe('a verdict that outlives its rows', () => {
  it('is dropped when the result was replaced while the request was in flight', async () => {
    let release: (value: unknown) => void = () => undefined;
    resolveResultOrigin.mockReturnValue(
      new Promise((resolve) => {
        release = resolve;
      }),
    );

    const pending = usePanelResultEdit.getState().resolveForResult(KEY, REQUEST);
    // The request is on its way.
    expect(usePanelResultEdit.getState().resolving[KEY]).toBe(true);

    // The result is replaced: the verdict of the old rows is dropped, and
    // everything still in flight becomes stale.
    usePanelResultEdit.getState().clearForResult(KEY);
    expect(usePanelResultEdit.getState().verdicts[KEY]).toBeUndefined();

    // The old answer arrives late.
    release(VERDICT);
    await pending;

    // It must not have landed: it is about rows that are no longer on screen.
    expect(usePanelResultEdit.getState().verdicts[KEY]).toBeUndefined();
    expect(usePanelResultEdit.getState().errors[KEY]).toBeUndefined();
  });

  it('is dropped when a newer request for the same result has started', async () => {
    let releaseFirst: (value: unknown) => void = () => undefined;
    resolveResultOrigin.mockReturnValueOnce(
      new Promise((resolve) => {
        releaseFirst = resolve;
      }),
    );

    const first = usePanelResultEdit.getState().resolveForResult(KEY, REQUEST);
    // The result is replaced while the first request is in flight: its answer
    // is about rows that are gone.
    usePanelResultEdit.getState().clearForResult(KEY);
    // The new result asks for its own verdict.
    resolveResultOrigin.mockResolvedValueOnce({ ...VERDICT, reason: 'view' });
    const second = usePanelResultEdit.getState().resolveForResult(KEY, REQUEST);
    await second;

    // The newer answer is the one that counts.
    expect(usePanelResultEdit.getState().verdicts[KEY]?.reason).toBe('view');

    // The older answer arrives afterwards and must not overwrite it.
    releaseFirst(VERDICT);
    await first;
    expect(usePanelResultEdit.getState().verdicts[KEY]?.reason).toBe('view');
  });

  it('does not send a second request while one is still in flight', async () => {
    let release: (value: unknown) => void = () => undefined;
    resolveResultOrigin.mockReturnValue(
      new Promise((resolve) => {
        release = resolve;
      }),
    );

    const first = usePanelResultEdit.getState().resolveForResult(KEY, REQUEST);
    // A second surface asks for the same verdict: it waits for the first
    // answer instead of sending a second request.
    await usePanelResultEdit.getState().resolveForResult(KEY, REQUEST);
    expect(resolveResultOrigin).toHaveBeenCalledTimes(1);

    release(VERDICT);
    await first;
    expect(usePanelResultEdit.getState().verdicts[KEY]?.reason).toBe('multiTable');
  });

  it('keeps the answer of a request that was never superseded', async () => {
    resolveResultOrigin.mockResolvedValue(VERDICT);

    await usePanelResultEdit.getState().resolveForResult(KEY, REQUEST);

    expect(usePanelResultEdit.getState().verdicts[KEY]?.reason).toBe('multiTable');
  });
});
