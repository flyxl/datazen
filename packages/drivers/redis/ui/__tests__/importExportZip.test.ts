import { describe, expect, it } from 'vitest';
import {
  MANIFEST_SCHEMA_VERSION,
  base64FromBytes,
  buildDumpManifest,
  packDumpZip,
  parseDumpZip,
  type DumpKeyEntry,
} from '../importExportZip';

describe('importExportZip', () => {
  const sampleEntries: DumpKeyEntry[] = [
    { key: 'a', ttlSeconds: -1, dumpBase64: base64FromBytes(new Uint8Array([0, 1, 2])) },
    { key: 'user:1', ttlSeconds: 3600, dumpBase64: base64FromBytes(new Uint8Array([9, 9, 9])) },
  ];

  it('builds manifest v1 with unique dump files', () => {
    const manifest = buildDumpManifest(0, sampleEntries);
    expect(manifest.schemaVersion).toBe(MANIFEST_SCHEMA_VERSION);
    expect(manifest.dbIndex).toBe(0);
    expect(manifest.keys).toHaveLength(2);
    expect(manifest.keys[0]?.dumpFile).toBe('a.bin');
    expect(manifest.keys[1]?.key).toBe('user:1');
  });

  it('packs and parses dump zip round-trip', () => {
    const zip = packDumpZip(3, sampleEntries);
    const parsed = parseDumpZip(zip);
    expect(parsed.manifest.schemaVersion).toBe(MANIFEST_SCHEMA_VERSION);
    expect(parsed.manifest.dbIndex).toBe(3);
    expect(parsed.restoreEntries).toHaveLength(2);
    expect(parsed.restoreEntries[0]?.key).toBe('a');
    expect(parsed.restoreEntries[0]?.dumpBase64).toBe(sampleEntries[0]?.dumpBase64);
  });
});
