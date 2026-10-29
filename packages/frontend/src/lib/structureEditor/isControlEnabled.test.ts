import { describe, it, expect } from 'vitest';
import type { StructureCapabilities } from './types';
import { isControlEnabled } from './isControlEnabled';

describe('isControlEnabled', () => {
  it('returns false when cap is false', () => {
    expect(isControlEnabled({ renameColumn: false } as StructureCapabilities, 'renameColumn')).toBe(false);
  });
  it('returns true when cap is true', () => {
    expect(isControlEnabled({ renameColumn: true } as StructureCapabilities, 'renameColumn')).toBe(true);
  });
});
