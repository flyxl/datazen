/**
 * Shared probes for the converged "copy to clipboard" confirmation
 * (`useCopyFeedback`).
 *
 * Every migrated copy affordance inherits the same three guarantees, and none
 * of them is observable from the DOM alone once the component is gone:
 *
 *  1. **The feedback window is the site-specific duration.** 1.2s for the
 *     Redis key header row, 1.5s for the AI/error affordances, 2s for the
 *     history/sync/settings ones. A test that only asserts "it eventually
 *     reverts" cannot tell a real parameter from a copy-pasted literal, so
 *     `expectWindowEdges` asserts the window edge-precisely.
 *  2. **Unmounting inside the window clears the timer.** React 18 dropped the
 *     setState-after-unmount warning, so a leaked timer is silent. The evidence
 *     is `spyOnWindowTimers()`: name the handle the click armed, then prove
 *     that exact handle reached `clearTimeout` at unmount.
 *  3. **A late rejection is bound to the request that started it.** A failure
 *     from click #1 must not erase click #2's confirmation, nor kill click
 *     #2's timer. `installControlledClipboard()` hands out one deferred
 *     promise per call so a test can settle them out of order.
 *
 * No test may assert on source shape (a `clearTimeout` call site, an import
 * list): every probe here is a runtime observation.
 */
import { act } from '@testing-library/react';
import { vi } from 'vitest';

interface Deferred {
  resolve: () => void;
  reject: (reason: Error) => void;
}

export interface ControlledClipboard {
  /** The `navigator.clipboard.writeText` mock, in call order. */
  readonly writeText: ReturnType<typeof vi.fn>;
  /** Settle the `index`-th (0-based) outstanding write. */
  settle: (index: number, outcome?: 'resolve' | 'reject') => Promise<void>;
  /** How many writes have been requested and not yet settled. */
  outstanding: () => number;
}

/**
 * Replace `navigator.clipboard` with one deferred promise per `writeText`
 * call, so a test can leave a write hanging (mid-flight) and decide later —
 * or never — whether it resolves or rejects.
 */
export function installControlledClipboard(): ControlledClipboard {
  const deferreds: Deferred[] = [];
  const settled = new Set<number>();
  const writeText = vi.fn(
    (_text: string) =>
      new Promise<void>((resolve, reject) => {
        deferreds.push({
          resolve: () => resolve(),
          reject: (reason) => reject(reason),
        });
      }),
  );
  Object.defineProperty(navigator, 'clipboard', {
    configurable: true,
    value: { writeText },
  });
  return {
    writeText,
    outstanding: () => deferreds.length - settled.size,
    async settle(index, outcome = 'resolve') {
      const target = deferreds[index];
      if (!target) {
        throw new Error(`no writeText call #${index} to settle`);
      }
      if (settled.has(index)) {
        throw new Error(`writeText call #${index} was already settled`);
      }
      settled.add(index);
      if (outcome === 'resolve') target.resolve();
      else target.reject(new Error('clipboard write denied'));
      // Let the hook's rejection handler and the React commit that follows
      // it flush before the caller asserts on the DOM.
      await act(async () => {
        await Promise.resolve();
      });
    },
  };
}

/** Install a clipboard whose writes always resolve, for non-race tests. */
export function installResolvedClipboard(): ReturnType<typeof vi.fn> {
  const writeText = vi.fn(() => Promise.resolve());
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
  return writeText;
}

export interface TimerWatch {
  /** The numeric handle of the most recently armed `window.setTimeout`. */
  lastArmedHandle: () => number;
  /** Every handle passed to `window.clearTimeout` since the watch began. */
  clearedHandles: () => number[];
}

/**
 * Watch the window timer API so a test can name the exact handle an
 * interaction armed, then ask whether *that* handle was cleared.
 *
 * Comparing raw `getTimerCount()` values across an unmount is not sound here:
 * mounting a component can queue timers of its own (scheduler, observers), and
 * unmount legitimately clears those too, so the count drops for reasons that
 * have nothing to do with the copy window. Correlating the cleared handle with
 * the armed one is unambiguous, and it is still a runtime observation of the
 * host timer API rather than an inspection of the component's source.
 */
export function spyOnWindowTimers(): TimerWatch {
  const setTimeoutSpy = vi.spyOn(window, 'setTimeout');
  const clearTimeoutSpy = vi.spyOn(window, 'clearTimeout');
  return {
    lastArmedHandle: () => setTimeoutSpy.mock.results.at(-1)?.value as number,
    clearedHandles: () => clearTimeoutSpy.mock.calls.map((call) => call[0] as number),
  };
}

/** Advance fake timers by `ms` and flush the resulting React work. */
export function advanceBy(ms: number): void {
  act(() => {
    vi.advanceTimersByTime(ms);
  });
}
