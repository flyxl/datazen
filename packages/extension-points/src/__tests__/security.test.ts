import { describe, it, expect, beforeAll } from 'vitest';
import {
  buildSignaturePayload,
  EP_SIGNATURE_ALGORITHM,
  EP_SIGNATURE_VERSION,
  parseSignatureFile,
} from '../signaturePayload';
import {
  checkEngineCompatibility,
  EXTENSION_POINTS_VERSION,
  normalizeTrustedPublicKey,
  OFFICIAL_EP_PUBLIC_KEY_SPKI_B64,
  parseTrustedPublicKeys,
  readAllowUnverifiedFromEnv,
  sha256Hex,
  UNVERIFIED_EXTENSION_LABEL,
  verifyExtensionPackage,
  verifySignatureWithPublicKey,
  type ExtensionManifest,
} from '../security';

/** Test private key (PKCS8 DER base64) — pairs with OFFICIAL_EP_PUBLIC_KEY_SPKI_B64 */
const TEST_EP_PRIVATE_KEY_PKCS8_B64 =
  'MC4CAQAwBQYDK2VwBCIEIPEWEScCcGuvAK1PLiblsaf/hx6x2/oVqUovRkBU/X0N';

const SAMPLE_MANIFEST: ExtensionManifest = {
  id: '@datazen/extension-demo',
  version: '0.0.1',
  main: 'dist/index.esm.js',
  engines: {
    datazen: '>=0.1.2',
    extensionPointsVersion: EXTENSION_POINTS_VERSION,
  },
};

const SAMPLE_BUNDLE = `export function activate() { return 'demo'; }\n`;

function decodeBase64(value: string): Uint8Array {
  return Uint8Array.from(atob(value), (c) => c.charCodeAt(0));
}

async function importTestPrivateKey(): Promise<CryptoKey> {
  const der = decodeBase64(TEST_EP_PRIVATE_KEY_PKCS8_B64);
  return crypto.subtle.importKey('pkcs8', der, { name: 'Ed25519' }, false, ['sign']);
}

async function signFiles(files: Record<string, { sha256: string }>): Promise<string> {
  const payload = buildSignaturePayload(files);
  const privateKey = await importTestPrivateKey();
  const signature = await crypto.subtle.sign('Ed25519', privateKey, payload);
  return btoa(String.fromCharCode(...new Uint8Array(signature)));
}

async function buildSignedPackage(
  manifestContent: string,
  bundleContent: string,
): Promise<string> {
  const files = {
    'manifest.json': { sha256: await sha256Hex(manifestContent) },
    'dist/index.esm.js': { sha256: await sha256Hex(bundleContent) },
  };
  const signature = await signFiles(files);
  return JSON.stringify(
    {
      version: EP_SIGNATURE_VERSION,
      algorithm: EP_SIGNATURE_ALGORITHM,
      signedAt: new Date().toISOString(),
      files,
      signature,
    },
    null,
    2,
  );
}

describe('EP security gate (security.test.ts)', () => {
  let manifestContent: string;
  let signatureContent: string;

  beforeAll(async () => {
    manifestContent = JSON.stringify(SAMPLE_MANIFEST, null, 2);
    signatureContent = await buildSignedPackage(manifestContent, SAMPLE_BUNDLE);
  });

  it('accepts a valid official signature', async () => {
    const result = await verifyExtensionPackage({
      manifest: SAMPLE_MANIFEST,
      files: {
        manifestContent,
        bundleContent: SAMPLE_BUNDLE,
        signatureContent,
      },
    });

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.trustSource).toBe('official');
      expect(result.unverifiedLabel).toBeUndefined();
    }
  });

  it('rejects tampered manifest digest mismatch', async () => {
    const tamperedManifest = manifestContent.replace('"0.0.1"', '"9.9.9"');
    const result = await verifyExtensionPackage({
      manifest: { ...SAMPLE_MANIFEST, version: '9.9.9' },
      files: {
        manifestContent: tamperedManifest,
        bundleContent: SAMPLE_BUNDLE,
        signatureContent,
      },
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.code).toBe('tampered');
    }
  });

  it('rejects tampered bundle with valid signature file digests', async () => {
    const result = await verifyExtensionPackage({
      manifest: SAMPLE_MANIFEST,
      files: {
        manifestContent,
        bundleContent: `${SAMPLE_BUNDLE}// tampered`,
        signatureContent,
      },
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.code).toBe('tampered');
    }
  });

  it('rejects invalid signature even when developer mode is enabled', async () => {
    const parsed = JSON.parse(signatureContent) as { signature: string };
    const sigBytes = decodeBase64(parsed.signature);
    sigBytes[0] ^= 0xff;
    parsed.signature = btoa(String.fromCharCode(...sigBytes));
    const badSig = JSON.stringify(parsed);
    const result = await verifyExtensionPackage({
      manifest: SAMPLE_MANIFEST,
      files: {
        manifestContent,
        bundleContent: SAMPLE_BUNDLE,
        signatureContent: badSig,
      },
      allowUnsignedExtensions: true,
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(['invalid-signature', 'tampered']).toContain(result.code);
    }
  });

  it('allows unsigned extensions in developer mode with unverified label', async () => {
    const result = await verifyExtensionPackage({
      manifest: SAMPLE_MANIFEST,
      files: {
        manifestContent,
        bundleContent: SAMPLE_BUNDLE,
        signatureContent: null,
      },
      allowUnsignedExtensions: true,
    });

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.trustSource).toBe('unverified');
      expect(result.unverifiedLabel).toBe(UNVERIFIED_EXTENSION_LABEL);
    }
  });

  it('rejects unsigned extensions when developer mode is off', async () => {
    const result = await verifyExtensionPackage({
      manifest: SAMPLE_MANIFEST,
      files: {
        manifestContent,
        bundleContent: SAMPLE_BUNDLE,
      },
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.code).toBe('unsigned-rejected');
    }
  });

  it('accepts extensions signed by enterprise trusted public keys', async () => {
    const enterprise = await crypto.subtle.generateKey('Ed25519', true, ['sign', 'verify']);
    const spki = await crypto.subtle.exportKey('spki', enterprise.publicKey);
    const enterprisePubB64 = btoa(String.fromCharCode(...new Uint8Array(spki)));

    const files = {
      'manifest.json': { sha256: await sha256Hex(manifestContent) },
      'dist/index.esm.js': { sha256: await sha256Hex(SAMPLE_BUNDLE) },
    };
    const payload = buildSignaturePayload(files);
    const signature = await crypto.subtle.sign('Ed25519', enterprise.privateKey, payload);
    const enterpriseSig = JSON.stringify({
      version: EP_SIGNATURE_VERSION,
      algorithm: EP_SIGNATURE_ALGORITHM,
      signedAt: new Date().toISOString(),
      files,
      signature: btoa(String.fromCharCode(...new Uint8Array(signature))),
    });

    const result = await verifyExtensionPackage({
      manifest: SAMPLE_MANIFEST,
      files: {
        manifestContent,
        bundleContent: SAMPLE_BUNDLE,
        signatureContent: enterpriseSig,
      },
      trustedPublicKeys: [enterprisePubB64],
    });

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.trustSource).toBe('enterprise');
    }
  });

  it('rejects engine version mismatch with safe fallback', async () => {
    const incompatibleManifest: ExtensionManifest = {
      ...SAMPLE_MANIFEST,
      engines: {
        ...SAMPLE_MANIFEST.engines,
        extensionPointsVersion: '99.0.0',
      },
    };
    const incompatibleContent = JSON.stringify(incompatibleManifest, null, 2);

    const result = await verifyExtensionPackage({
      manifest: incompatibleManifest,
      files: {
        manifestContent: incompatibleContent,
        bundleContent: SAMPLE_BUNDLE,
        signatureContent,
      },
      allowUnsignedExtensions: true,
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.code).toBe('engine-incompatible');
    }
  });

  it('bypasses signature gate for local directory linking', async () => {
    const result = await verifyExtensionPackage({
      manifest: {
        ...SAMPLE_MANIFEST,
        engines: { extensionPointsVersion: '99.0.0' },
      },
      files: {
        manifestContent: '{}',
        bundleContent: '',
      },
      sourceKind: 'local-link',
    });

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.trustSource).toBe('local-dev');
    }
  });

  it('verifySignatureWithPublicKey validates official key round-trip', async () => {
    const files = {
      'manifest.json': { sha256: await sha256Hex('{}') },
      'dist/index.esm.js': { sha256: await sha256Hex('export {}') },
    };
    const signature = await signFiles(files);
    const ok = await verifySignatureWithPublicKey(
      files,
      signature,
      OFFICIAL_EP_PUBLIC_KEY_SPKI_B64,
    );
    expect(ok).toBe(true);
  });

  it('checkEngineCompatibility enforces exact contract version', () => {
    expect(checkEngineCompatibility(SAMPLE_MANIFEST).compatible).toBe(true);
    expect(
      checkEngineCompatibility({
        ...SAMPLE_MANIFEST,
        engines: { extensionPointsVersion: '2.0.0' },
      }).compatible,
    ).toBe(false);
  });

  it('parseTrustedPublicKeys normalizes PEM and raw base64 entries', () => {
    const pem = `-----BEGIN PUBLIC KEY-----\n${OFFICIAL_EP_PUBLIC_KEY_SPKI_B64}\n-----END PUBLIC KEY-----`;
    const keys = parseTrustedPublicKeys([
      { content: OFFICIAL_EP_PUBLIC_KEY_SPKI_B64 },
      { content: pem },
    ]);
    expect(keys).toHaveLength(1);
    expect(keys[0]).toBe(OFFICIAL_EP_PUBLIC_KEY_SPKI_B64);
  });

  it('normalizeTrustedPublicKey strips PEM wrappers', () => {
    const pem = `-----BEGIN PUBLIC KEY-----\n${OFFICIAL_EP_PUBLIC_KEY_SPKI_B64}\n-----END PUBLIC KEY-----`;
    expect(normalizeTrustedPublicKey(pem)).toBe(OFFICIAL_EP_PUBLIC_KEY_SPKI_B64);
  });

  it('readAllowUnverifiedFromEnv reads DATAZEN_ALLOW_UNVERIFIED_EP', () => {
    expect(readAllowUnverifiedFromEnv({})).toBe(false);
    expect(readAllowUnverifiedFromEnv({ DATAZEN_ALLOW_UNVERIFIED_EP: '1' })).toBe(true);
    expect(readAllowUnverifiedFromEnv({ DATAZEN_ALLOW_UNVERIFIED_EP: '0' })).toBe(false);
  });

  it('allows unsigned via DATAZEN_ALLOW_UNVERIFIED_EP env flag', async () => {
    const result = await verifyExtensionPackage({
      manifest: SAMPLE_MANIFEST,
      files: {
        manifestContent,
        bundleContent: SAMPLE_BUNDLE,
      },
      allowUnverifiedEnv: true,
    });

    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.trustSource).toBe('unverified');
    }
  });

  it('parseSignatureFile rejects malformed and incomplete signature files', () => {
    expect(() => parseSignatureFile('not-json')).toThrow(/malformed JSON/);
    expect(() => parseSignatureFile('null')).toThrow(/expected object/);
    expect(() =>
      parseSignatureFile(JSON.stringify({ version: 99, algorithm: 'Ed25519', files: {}, signature: 'x' })),
    ).toThrow(/unsupported version/);
    expect(() =>
      parseSignatureFile(JSON.stringify({ version: 1, algorithm: 'RSA', files: {}, signature: 'x' })),
    ).toThrow(/unsupported algorithm/);
    expect(() =>
      parseSignatureFile(JSON.stringify({ version: 1, algorithm: 'Ed25519', signature: 'x' })),
    ).toThrow(/missing files/);
    expect(() =>
      parseSignatureFile(
        JSON.stringify({
          version: 1,
          algorithm: 'Ed25519',
          files: { 'manifest.json': { sha256: 'abc' } },
          signature: '',
        }),
      ),
    ).toThrow(/missing signature/);
  });

  it('checkEngineCompatibility rejects manifest without extensionPointsVersion', () => {
    const result = checkEngineCompatibility({ id: 'x', version: '1.0.0' });
    expect(result.compatible).toBe(false);
    if (!result.compatible) {
      expect(result.reason).toContain('missing engines.extensionPointsVersion');
    }
  });

  it('verifyExtensionPackage rejects unknown signer without developer bypass', async () => {
    const foreign = await crypto.subtle.generateKey('Ed25519', true, ['sign', 'verify']);
    const files = {
      'manifest.json': { sha256: await sha256Hex(manifestContent) },
      'dist/index.esm.js': { sha256: await sha256Hex(SAMPLE_BUNDLE) },
    };
    const payload = buildSignaturePayload(files);
    const signature = await crypto.subtle.sign('Ed25519', foreign.privateKey, payload);
    const foreignSig = JSON.stringify({
      version: EP_SIGNATURE_VERSION,
      algorithm: EP_SIGNATURE_ALGORITHM,
      signedAt: new Date().toISOString(),
      files,
      signature: btoa(String.fromCharCode(...new Uint8Array(signature))),
    });

    const result = await verifyExtensionPackage({
      manifest: SAMPLE_MANIFEST,
      files: {
        manifestContent,
        bundleContent: SAMPLE_BUNDLE,
        signatureContent: foreignSig,
      },
    });

    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.code).toBe('invalid-signature');
    }
  });
});
