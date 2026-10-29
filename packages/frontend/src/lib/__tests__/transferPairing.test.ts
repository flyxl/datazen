import { describe, expect, it } from 'vitest';
import { resolveTransferPairing } from '../transferPairing';

describe('Data Transfer adapter availability', () => {
  it('rejects Redis before preview or execution', () => {
    const pair = resolveTransferPairing('redis', 'redis');
    expect(pair.supported).toBe(false);
    expect(pair.path).toBe('unsupported');
    expect(pair.reason).toContain('source and target adapters');
  });

  it('preserves supported SQL paths', () => {
    expect(resolveTransferPairing('postgresql', 'mysql').path).toBe('ir');
    expect(resolveTransferPairing('mysql', 'mariadb').path).toBe('direct');
    expect(resolveTransferPairing('mongodb', 'mongodb').path).toBe('direct');
  });
});
