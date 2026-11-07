import { describe, expect, it } from 'vitest';
import { toErrorMessage } from '../errors';

describe('toErrorMessage', () => {
  it('returns message from Error instances', () => {
    expect(toErrorMessage(new Error('boom'))).toBe('boom');
  });

  it('stringifies non-Error values', () => {
    expect(toErrorMessage('plain')).toBe('plain');
    expect(toErrorMessage(42)).toBe('42');
  });
});
