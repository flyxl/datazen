/**
 * Host EP extension security gate — signature verification, engine compatibility,
 * and graded trust policies (official / enterprise / developer / local dev).
 */

import {
  buildSignaturePayload,
  EP_SIGNED_FILE_PATHS,
  parseSignatureFile,
  type EpSignatureFile,
} from './signaturePayload';

/** Current host extension-points contract version (Wave 1 baseline). */
export const EXTENSION_POINTS_VERSION = '1.0.0';

/**
 * Built-in official Ed25519 public key (SPKI DER, base64).
 * Pairs with the test private key used by `scripts/sign-ep.mjs` when no env key is set.
 */
export const OFFICIAL_EP_PUBLIC_KEY_SPKI_B64 =
  'MCowBQYDK2VwAyEAXM/L6lIBBr45uHMVQ373GubRZrzwMld3MElCY0GstxU=';

/** Label shown for extensions loaded without cryptographic verification. */
export const UNVERIFIED_EXTENSION_LABEL = 'Unverified / 未经验证';

export type ExtensionTrustSource = 'official' | 'enterprise' | 'local-dev' | 'unverified';

export interface ExtensionManifestEngines {
  datazen?: string;
  extensionPointsVersion?: string;
}

export interface ExtensionManifest {
  id: string;
  version: string;
  main?: string;
  engines?: ExtensionManifestEngines;
}

export type ExtensionSourceKind = 'dzx' | 'module' | 'local-link';

export interface ExtensionPackageFiles {
  manifestContent: string;
  bundleContent: string;
  signatureContent?: string | null;
}

export interface ExtensionSecurityConfig {
  /** Settings: allowUnsignedExtensions */
  allowUnsignedExtensions?: boolean;
  /** Env: DATAZEN_ALLOW_UNVERIFIED_EP=1 */
  allowUnverifiedEnv?: boolean;
  /** Enterprise trusted public keys (SPKI DER base64), e.g. from `{appData}/trusted-keys/*.pub` */
  trustedPublicKeys?: readonly string[];
  /** Override host contract version (defaults to EXTENSION_POINTS_VERSION). */
  hostExtensionPointsVersion?: string;
}

export interface ExtensionVerificationSuccess {
  ok: true;
  trustSource: ExtensionTrustSource;
  /** Present when trustSource is `unverified`. */
  unverifiedLabel?: typeof UNVERIFIED_EXTENSION_LABEL;
  signature?: EpSignatureFile;
}

export interface ExtensionVerificationFailure {
  ok: false;
  reason: string;
  code:
    | 'engine-incompatible'
    | 'missing-signature'
    | 'invalid-signature'
    | 'tampered'
    | 'unsigned-rejected';
}

export type ExtensionVerificationResult = ExtensionVerificationSuccess | ExtensionVerificationFailure;

export interface VerifyExtensionOptions extends ExtensionSecurityConfig {
  manifest: ExtensionManifest;
  files: ExtensionPackageFiles;
  sourceKind?: ExtensionSourceKind;
}

function isDeveloperMode(config: ExtensionSecurityConfig): boolean {
  return Boolean(config.allowUnsignedExtensions || config.allowUnverifiedEnv);
}

/** SHA-256 hex digest of UTF-8 text content. */
export async function sha256Hex(content: string): Promise<string> {
  const data = new TextEncoder().encode(content);
  const digest = await crypto.subtle.digest('SHA-256', data);
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

async function importEd25519PublicKey(spkiBase64: string): Promise<CryptoKey> {
  const binary = Uint8Array.from(atob(spkiBase64), (c) => c.charCodeAt(0));
  return crypto.subtle.importKey('spki', binary, { name: 'Ed25519' }, false, ['verify']);
}

function decodeBase64(value: string): Uint8Array {
  return Uint8Array.from(atob(value), (c) => c.charCodeAt(0));
}

/** Verify Ed25519 signature over the canonical payload for the given digest map. */
export async function verifySignatureWithPublicKey(
  files: Record<string, { sha256: string }>,
  signatureBase64: string,
  publicKeySpkiBase64: string,
): Promise<boolean> {
  const payload = buildSignaturePayload(files);
  const signature = decodeBase64(signatureBase64);
  const publicKey = await importEd25519PublicKey(publicKeySpkiBase64);
  return crypto.subtle.verify(
    'Ed25519',
    publicKey,
    new Uint8Array(signature),
    new Uint8Array(payload),
  );
}

/** Check manifest engines.extensionPointsVersion against the host contract. */
export function checkEngineCompatibility(
  manifest: ExtensionManifest,
  hostVersion: string = EXTENSION_POINTS_VERSION,
): { compatible: true } | { compatible: false; reason: string } {
  const required = manifest.engines?.extensionPointsVersion;
  if (!required) {
    return {
      compatible: false,
      reason: 'manifest.json missing engines.extensionPointsVersion',
    };
  }
  if (required !== hostVersion) {
    return {
      compatible: false,
      reason: `extensionPointsVersion mismatch: manifest requires ${required}, host provides ${hostVersion}`,
    };
  }
  return { compatible: true };
}

async function computeActualDigests(files: ExtensionPackageFiles): Promise<Record<string, { sha256: string }>> {
  return {
    'manifest.json': { sha256: await sha256Hex(files.manifestContent) },
    'dist/index.esm.js': { sha256: await sha256Hex(files.bundleContent) },
  };
}

function digestsMatch(
  expected: Record<string, { sha256: string }>,
  actual: Record<string, { sha256: string }>,
): boolean {
  for (const path of EP_SIGNED_FILE_PATHS) {
    if (expected[path]?.sha256 !== actual[path]?.sha256) {
      return false;
    }
  }
  return true;
}

async function tryVerifyWithKeys(
  sigFile: EpSignatureFile,
  publicKeys: readonly string[],
): Promise<ExtensionTrustSource | null> {
  const [officialKey, ...enterpriseKeys] = publicKeys;
  if (officialKey) {
    const ok = await verifySignatureWithPublicKey(sigFile.files, sigFile.signature, officialKey);
    if (ok) return 'official';
  }
  for (const key of enterpriseKeys) {
    const ok = await verifySignatureWithPublicKey(sigFile.files, sigFile.signature, key);
    if (ok) return 'enterprise';
  }
  return null;
}

/** Parse `.pub` file content into SPKI DER base64 (supports raw base64 or PEM wrapper). */
export function normalizeTrustedPublicKey(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed.includes('BEGIN')) {
    return trimmed.replace(/\s+/g, '');
  }
  const body = trimmed
    .replace(/-----BEGIN PUBLIC KEY-----/g, '')
    .replace(/-----END PUBLIC KEY-----/g, '')
    .replace(/\s+/g, '');
  return body;
}

/** Load enterprise trusted keys from pre-read `{appData}/trusted-keys/*.pub` entries. */
export function parseTrustedPublicKeys(entries: readonly { content: string }[]): string[] {
  const seen = new Set<string>();
  const keys: string[] = [];
  for (const entry of entries) {
    try {
      const normalized = normalizeTrustedPublicKey(entry.content);
      if (normalized.length > 0 && !seen.has(normalized)) {
        seen.add(normalized);
        keys.push(normalized);
      }
    } catch {
      // skip malformed key files
    }
  }
  return keys;
}

function buildPublicKeyList(config: ExtensionSecurityConfig): string[] {
  const keys: string[] = [OFFICIAL_EP_PUBLIC_KEY_SPKI_B64];
  if (config.trustedPublicKeys) {
    for (const key of config.trustedPublicKeys) {
      const normalized = normalizeTrustedPublicKey(key);
      if (!keys.includes(normalized)) {
        keys.push(normalized);
      }
    }
  }
  return keys;
}

/**
 * Full EP package verification gate.
 *
 * Trust precedence:
 * 1. Local directory link (`sourceKind: 'local-link'`) — bypass production signature gate.
 * 2. Engine compatibility — always enforced unless bypassed by local-dev.
 * 3. Signed packages — official or enterprise key must verify AND digests must match content.
 * 4. Unsigned — allowed only in developer mode, tagged as unverified.
 */
export async function verifyExtensionPackage(
  options: VerifyExtensionOptions,
): Promise<ExtensionVerificationResult> {
  const hostVersion = options.hostExtensionPointsVersion ?? EXTENSION_POINTS_VERSION;

  if (options.sourceKind === 'local-link') {
    return { ok: true, trustSource: 'local-dev' };
  }

  const engineCheck = checkEngineCompatibility(options.manifest, hostVersion);
  if (!engineCheck.compatible) {
    return {
      ok: false,
      code: 'engine-incompatible',
      reason: engineCheck.reason,
    };
  }

  const signatureRaw = options.files.signatureContent?.trim();
  if (!signatureRaw) {
    if (isDeveloperMode(options)) {
      return {
        ok: true,
        trustSource: 'unverified',
        unverifiedLabel: UNVERIFIED_EXTENSION_LABEL,
      };
    }
    return {
      ok: false,
      code: 'unsigned-rejected',
      reason: 'Extension is not signed; enable Developer Mode to load unverified extensions',
    };
  }

  let sigFile: EpSignatureFile;
  try {
    sigFile = parseSignatureFile(signatureRaw);
  } catch (err) {
    return {
      ok: false,
      code: 'invalid-signature',
      reason: err instanceof Error ? err.message : 'Invalid signature.sig',
    };
  }

  const actualDigests = await computeActualDigests(options.files);
  if (!digestsMatch(sigFile.files, actualDigests)) {
    return {
      ok: false,
      code: 'tampered',
      reason: 'Extension package digest mismatch — manifest or bundle was tampered with',
    };
  }

  const publicKeys = buildPublicKeyList(options);
  const trustSource = await tryVerifyWithKeys(sigFile, publicKeys);
  if (!trustSource) {
    return {
      ok: false,
      code: 'invalid-signature',
      reason: 'signature.sig was not signed by an official or trusted enterprise key',
    };
  }

  return { ok: true, trustSource, signature: sigFile };
}

/** Returns true when env var DATAZEN_ALLOW_UNVERIFIED_EP equals `1`. */
export function readAllowUnverifiedFromEnv(env: Record<string, string | undefined>): boolean {
  return env.DATAZEN_ALLOW_UNVERIFIED_EP === '1';
}
