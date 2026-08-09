import { describe, expect, it } from 'vitest';
import { parseInfoSections } from '../../../../packages/drivers/redis/ui/infoParse';

describe('parseInfoSections', () => {
  it('splits section headers', () => {
    const raw = '# Server\r\nredis_version:7.0.0\r\n# Memory\r\nused_memory:100\r\n';
    const sections = parseInfoSections(raw);
    expect(sections).toHaveLength(2);
    expect(sections[0].name).toBe('Server');
    expect(sections[0].entries[0]).toEqual({ key: 'redis_version', value: '7.0.0' });
    expect(sections[1].name).toBe('Memory');
    expect(sections[1].entries[0]).toEqual({ key: 'used_memory', value: '100' });
  });

  it('handles LF-only line endings', () => {
    const raw = '# Server\nredis_version:7.0.0\n';
    const sections = parseInfoSections(raw);
    expect(sections).toHaveLength(1);
    expect(sections[0].name).toBe('Server');
    expect(sections[0].entries[0].value).toBe('7.0.0');
  });

  it('returns empty array for blank input', () => {
    expect(parseInfoSections('')).toEqual([]);
    expect(parseInfoSections('   \n\n')).toEqual([]);
  });
});
