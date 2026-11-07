import { describe, expect, it } from 'vitest';
import { importFileDisplayName, importFilePasswordPolicy } from '../importConnectionFile';

describe('importConnectionFile', () => {
  it('requires password for encrypted connection extensions', () => {
    expect(importFilePasswordPolicy('/tmp/a.datazenconnection')).toBe('required');
    expect(importFilePasswordPolicy('C:\\share\\b.tableplusconnection')).toBe('required');
  });

  it('treats competitor plain formats as optional password', () => {
    expect(importFilePasswordPolicy('/tmp/data-sources.json')).toBe('optional');
    expect(importFilePasswordPolicy('/tmp/export.ncx')).toBe('optional');
  });

  it('extracts basename for display', () => {
    expect(importFileDisplayName('/tmp/foo/datazen-connections.datazenconnection')).toBe(
      'datazen-connections.datazenconnection',
    );
  });
});
