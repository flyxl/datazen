#!/usr/bin/env node
/**
 * sign-ep.mjs — sign privileged extension package artifacts (manifest.json + dist/index.esm.js).
 *
 * Produces `signature.sig` beside manifest.json using Ed25519 over a canonical
 * SHA-256 digest payload. Compatible with packages/extension-points/src/security.ts.
 *
 * Environment:
 *   DATAZEN_EP_PRIVATE_KEY or DATAZEN_EP_SIGNING_PRIVATE_KEY — PKCS#8 PEM or base64
 *
 * Usage:
 *   node scripts/sign-ep.mjs --dir packages/pro-extensions/sql-editor-pro
 *   node scripts/sign-ep.mjs --dir=packages/pro-extensions/sql-editor-pro [--out=path]
 */

import crypto, {
  createHash,
  createPrivateKey,
  createPublicKey,
  sign,
} from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const EP_SIGNATURE_VERSION = 1;
export const EP_SIGNATURE_ALGORITHM = 'Ed25519';
export const DEFAULT_SIGNED_FILES = ['manifest.json', 'dist/index.esm.js'];
export const EP_SIGNED_FILE_PATHS = DEFAULT_SIGNED_FILES;

/** Built-in test private key (PKCS8 DER base64) — pairs with OFFICIAL_EP_PUBLIC_KEY in security.ts */
export const TEST_EP_PRIVATE_KEY_PKCS8_B64 =
  'MC4CAQAwBQYDK2VwBCIEIPEWEScCcGuvAK1PLiblsaf/hx6x2/oVqUovRkBU/X0N';

export function sha256Hex(content) {
  return createHash('sha256').update(content).digest('hex');
}

export function sha256File(filePath) {
  return sha256Hex(fs.readFileSync(filePath));
}

export function parsePrivateKeyMaterial(raw) {
  const trimmed = raw.trim();
  if (trimmed.includes('BEGIN')) {
    return createPrivateKey(trimmed);
  }
  const b64 = trimmed.replace(/\s+/g, '');
  const buf = Buffer.from(b64, 'base64');
  if (buf.length === 32) {
    // raw 32-byte Ed25519 seed
    return createPrivateKey({
      key: Buffer.concat([
        Buffer.from('302e020100300506032b657004220420', 'hex'),
        buf,
      ]),
      format: 'der',
      type: 'pkcs8',
    });
  }
  return createPrivateKey({ key: buf, format: 'der', type: 'pkcs8' });
}

export function resolveSigningPrivateKey(env = process.env) {
  const envVal =
    env.DATAZEN_EP_PRIVATE_KEY?.trim() ||
    env.DATAZEN_EP_SIGNING_PRIVATE_KEY?.trim();
  if (envVal) {
    return parsePrivateKeyMaterial(envVal);
  }
  return parsePrivateKeyMaterial(TEST_EP_PRIVATE_KEY_PKCS8_B64);
}

export function buildSignaturePayload(files, version = EP_SIGNATURE_VERSION) {
  const sortedFiles = {};
  for (const key of Object.keys(files).sort()) {
    sortedFiles[key] = files[key];
  }
  return Buffer.from(JSON.stringify({ version, files: sortedFiles }), 'utf8');
}

/**
 * @param {{
 *   packageDir: string,
 *   files?: string[],
 *   privateKey?: import('crypto').KeyObject,
 *   signedAt?: string,
 *   outPath?: string,
 * }} opts
 */
export function signEpPackage(opts) {
  const packageDir = path.resolve(opts.packageDir);
  const filesList = opts.files ?? DEFAULT_SIGNED_FILES;
  const privateKey = opts.privateKey ?? resolveSigningPrivateKey();
  const signedAt = opts.signedAt ?? new Date().toISOString();

  const files = {};
  for (const rel of filesList) {
    const abs = path.join(packageDir, rel);
    if (!fs.existsSync(abs)) {
      throw new Error(`[sign-ep] missing signed artifact: ${rel} (expected at ${abs})`);
    }
    files[rel] = { sha256: sha256File(abs) };
  }

  const payloadBuf = buildSignaturePayload(files);
  const signature = crypto.sign(null, payloadBuf, privateKey).toString('base64');

  const sigDoc = {
    version: EP_SIGNATURE_VERSION,
    algorithm: EP_SIGNATURE_ALGORITHM,
    signedAt,
    files,
    signature,
  };

  const outPath = opts.outPath ?? path.join(packageDir, 'signature.sig');
  fs.writeFileSync(outPath, `${JSON.stringify(sigDoc, null, 2)}\n`, 'utf8');
  return { outPath, sigDoc };
}

export function parseSignArgs(argv = process.argv.slice(2)) {
  let packageDir = null;
  let outPath = null;
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--dir' && argv[i + 1]) {
      packageDir = argv[++i];
    } else if (arg.startsWith('--dir=')) {
      packageDir = arg.slice('--dir='.length);
    } else if (arg === '--out' && argv[i + 1]) {
      outPath = argv[++i];
    } else if (arg.startsWith('--out=')) {
      outPath = arg.slice('--out='.length);
    }
  }
  return { packageDir, outPath };
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);
if (isMain) {
  const { packageDir, outPath } = parseSignArgs();
  if (!packageDir) {
    console.error('Usage: node scripts/sign-ep.mjs --dir=<package-root> [--out=<signature.sig>]');
    process.exit(1);
  }
  try {
    const result = signEpPackage({ packageDir, outPath: outPath ?? undefined });
    console.log(`[sign-ep] wrote ${result.outPath}`);
  } catch (err) {
    console.error(`sign-ep failed: ${err instanceof Error ? err.message : String(err)}`);
    process.exit(1);
  }
}
