import { beforeEach, describe, expect, it, vi } from 'vitest';
import { runPredefinedEdit } from '../runPredefinedEdit';

describe('runPredefinedEdit', () => {
  beforeEach(() => {
    Object.defineProperty(document, 'execCommand', {
      configurable: true,
      writable: true,
      value: vi.fn().mockReturnValue(true),
    });
  });

  it('maps predefined items to execCommand', () => {
    const spy = document.execCommand as unknown as ReturnType<typeof vi.fn>;
    runPredefinedEdit('Copy');
    runPredefinedEdit('Cut');
    runPredefinedEdit('Paste');
    runPredefinedEdit('SelectAll');
    runPredefinedEdit('Undo');
    runPredefinedEdit('Redo');
    expect(spy.mock.calls.map((c) => c[0])).toEqual([
      'copy',
      'cut',
      'paste',
      'selectAll',
      'undo',
      'redo',
    ]);
  });

  it('ignores separators', () => {
    const spy = document.execCommand as unknown as ReturnType<typeof vi.fn>;
    runPredefinedEdit('Separator');
    expect(spy).not.toHaveBeenCalled();
  });
});
