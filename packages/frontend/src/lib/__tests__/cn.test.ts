import { describe, expect, it } from 'vitest';
import { cn } from '../cn';

describe('cn', () => {
  it('merges tailwind classes with conflict resolution', () => {
    expect(cn('px-2', 'px-4')).toBe('px-4');
    expect(cn('text-red-500', false && 'hidden', 'font-bold')).toBe('text-red-500 font-bold');
  });
});
