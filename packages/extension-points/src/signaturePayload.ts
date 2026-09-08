/**
 * Canonical payload format shared by sign-ep.mjs and host verification.
 */

export const EP_SIGNATURE_VERSION = 1;
export const EP_SIGNATURE_ALGORITHM = 'Ed25519' as const;

/** Files covered by EP package signatures (relative paths inside the bundle root). */
export const EP_SIGNED_FILE_PATHS = ['manifest.json', 'dist/index.esm.js'] as const;

export type EpSignedFilePath = (typeof EP_SIGNED_FILE_PATHS)[number];

export interface EpFileDigest {
  sha256: string;
}

export interface EpSignatureFile {
  version: typeof EP_SIGNATURE_VERSION;
  algorithm: typeof EP_SIGNATURE_ALGORITHM;
  signedAt: string;
  files: Record<EpSignedFilePath, EpFileDigest>;
  signature: string;
}

/** Build deterministic JSON bytes for Ed25519 signing / verification. */
export function buildSignaturePayload(
  files: Record<string, EpFileDigest>,
  version: number = EP_SIGNATURE_VERSION,
): Uint8Array {
  const sortedFiles: Record<string, EpFileDigest> = {};
  for (const key of Object.keys(files).sort()) {
    sortedFiles[key] = files[key];
  }
  const payload = JSON.stringify({ version, files: sortedFiles });
  return new TextEncoder().encode(payload);
}

export function parseSignatureFile(raw: string): EpSignatureFile {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    throw new Error('Invalid signature.sig: malformed JSON');
  }

  if (!parsed || typeof parsed !== 'object') {
    throw new Error('Invalid signature.sig: expected object');
  }

  const sig = parsed as Partial<EpSignatureFile>;
  if (sig.version !== EP_SIGNATURE_VERSION) {
    throw new Error(`Invalid signature.sig: unsupported version ${String(sig.version)}`);
  }
  if (sig.algorithm !== EP_SIGNATURE_ALGORITHM) {
    throw new Error(`Invalid signature.sig: unsupported algorithm ${String(sig.algorithm)}`);
  }
  if (!sig.files || typeof sig.files !== 'object') {
    throw new Error('Invalid signature.sig: missing files digests');
  }
  if (typeof sig.signature !== 'string' || sig.signature.length === 0) {
    throw new Error('Invalid signature.sig: missing signature');
  }

  for (const path of EP_SIGNED_FILE_PATHS) {
    const digest = sig.files[path as EpSignedFilePath];
    if (!digest || typeof digest.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(digest.sha256)) {
      throw new Error(`Invalid signature.sig: missing or invalid digest for ${path}`);
    }
  }

  return sig as EpSignatureFile;
}
