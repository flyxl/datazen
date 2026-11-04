import { strToU8, unzipSync, zipSync } from 'fflate';

export const MANIFEST_SCHEMA_VERSION = 1;
export const MANIFEST_FILE = 'manifest.json';

export interface DumpManifestKey {
  key: string;
  ttlSeconds: number;
  dumpFile: string;
}

export interface DumpManifest {
  schemaVersion: number;
  dbIndex: number;
  keys: DumpManifestKey[];
}

export interface DumpKeyEntry {
  key: string;
  ttlSeconds: number;
  dumpBase64: string;
}

export interface RestoreKeyEntry {
  key: string;
  ttlSeconds: number;
  dumpBase64: string;
}

export function dumpFileNameForKey(key: string, index: number, used: Set<string>): string {
  let base = key
    .split('')
    .map((c) => (/^[a-zA-Z0-9._-]$/.test(c) ? c : '_'))
    .join('');
  if (!base || base.startsWith('.')) {
    base = `key_${index}`;
  }
  if (base.length > 120) {
    base = base.slice(0, 120);
  }
  let candidate = `${base}.bin`;
  if (!used.has(candidate)) {
    used.add(candidate);
    return candidate;
  }
  let n = 2;
  while (used.has(`${base}_${n}.bin`)) {
    n += 1;
  }
  candidate = `${base}_${n}.bin`;
  used.add(candidate);
  return candidate;
}

export function buildDumpManifest(dbIndex: number, entries: DumpKeyEntry[]): DumpManifest {
  const used = new Set<string>();
  const keys = entries.map((entry, index) => ({
    key: entry.key,
    ttlSeconds: entry.ttlSeconds,
    dumpFile: dumpFileNameForKey(entry.key, index, used),
  }));
  return {
    schemaVersion: MANIFEST_SCHEMA_VERSION,
    dbIndex,
    keys,
  };
}

export function packDumpZip(dbIndex: number, entries: DumpKeyEntry[]): Uint8Array {
  const manifest = buildDumpManifest(dbIndex, entries);
  const files: Record<string, Uint8Array> = {
    [MANIFEST_FILE]: strToU8(JSON.stringify(manifest, null, 2)),
  };
  for (const manifestKey of manifest.keys) {
    const entry = entries.find((e) => e.key === manifestKey.key);
    if (!entry) {
      throw new Error(`missing dump entry for key ${manifestKey.key}`);
    }
    files[manifestKey.dumpFile] = bytesFromBase64(entry.dumpBase64);
  }
  return zipSync(files);
}

export function parseDumpZip(bytes: Uint8Array): {
  manifest: DumpManifest;
  restoreEntries: RestoreKeyEntry[];
} {
  const files = unzipSync(bytes);
  const manifestRaw = files[MANIFEST_FILE];
  if (!manifestRaw) {
    throw new Error('manifest.json missing from zip');
  }
  const manifest = JSON.parse(new TextDecoder().decode(manifestRaw)) as DumpManifest;
  if (manifest.schemaVersion !== MANIFEST_SCHEMA_VERSION) {
    throw new Error(`unsupported schemaVersion: ${manifest.schemaVersion}`);
  }
  const restoreEntries: RestoreKeyEntry[] = manifest.keys.map((item) => {
    const dumpBytes = files[item.dumpFile];
    if (!dumpBytes) {
      throw new Error(`missing dump file: ${item.dumpFile}`);
    }
    return {
      key: item.key,
      ttlSeconds: item.ttlSeconds,
      dumpBase64: base64FromBytes(dumpBytes),
    };
  });
  return { manifest, restoreEntries };
}

export function buildJsonExport(entries: DumpKeyEntry[]): string {
  return JSON.stringify(
    entries.map((entry) => ({
      key: entry.key,
      ttlSeconds: entry.ttlSeconds,
      dumpBase64: entry.dumpBase64,
    })),
    null,
    2,
  );
}

export function base64FromBytes(bytes: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 1) {
    binary += String.fromCharCode(bytes[i]!);
  }
  return btoa(binary);
}

export function bytesFromBase64(encoded: string): Uint8Array {
  const binary = atob(encoded.trim());
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

export function zipToBase64(zip: Uint8Array): string {
  return base64FromBytes(zip);
}

export function base64ToZip(encoded: string): Uint8Array {
  return bytesFromBase64(encoded);
}
