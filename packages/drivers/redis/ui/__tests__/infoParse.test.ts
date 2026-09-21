import { describe, expect, it } from 'vitest';
import { parseInfoSections, filterInfoSections } from '../observe/infoParse';

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

describe('filterInfoSections', () => {
  const sample = parseInfoSections(
    '# Server\r\nredis_version:7.0.0\r\nos:Linux\r\n# Memory\r\nused_memory:100\r\nused_memory_rss:200\r\n# Clients\r\nconnected_clients:5\r\n',
  );

  it('returns all entries when no search query', () => {
    const result = filterInfoSections(sample);
    expect(result.totalEntries).toBe(5);
    expect(result.matchedEntries).toBe(5);
    expect(result.sections).toHaveLength(3);
  });

  it('filters by key name (case-insensitive)', () => {
    const result = filterInfoSections(sample, 'version');
    expect(result.matchedEntries).toBe(1);
    expect(result.sections).toHaveLength(1);
    expect(result.sections[0].name).toBe('Server');
    expect(result.sections[0].entries[0].key).toBe('redis_version');
  });

  it('filters by value (case-insensitive)', () => {
    const result = filterInfoSections(sample, 'linux');
    expect(result.matchedEntries).toBe(1);
    expect(result.sections[0].entries[0].value).toBe('Linux');
  });

  it('filters across multiple sections', () => {
    const result = filterInfoSections(sample, 'memory');
    expect(result.matchedEntries).toBe(2);
    expect(result.sections).toHaveLength(1);
    expect(result.sections[0].name).toBe('Memory');
  });

  it('returns empty when nothing matches', () => {
    const result = filterInfoSections(sample, 'zzzznotfound');
    expect(result.matchedEntries).toBe(0);
    expect(result.sections).toHaveLength(0);
  });

  it('ignores leading/trailing whitespace in query', () => {
    const result = filterInfoSections(sample, '  linux  ');
    expect(result.matchedEntries).toBe(1);
  });
});
