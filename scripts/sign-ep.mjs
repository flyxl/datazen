#!/usr/bin/env node
/**
 * sign-ep.mjs — sign privileged extension package artifacts.
 *
 * Computes SHA-256 digests for manifest.json and dist/index.esm.js, then signs
 * a canonical payload with Ed25519. Writes signature.sig next to the package root.
 *
 * Environment:
 *   DATAZEN_EP_SIGNING_PRIVATE_KEY — PEM (PKCS#8) or base64 raw 32-byte seed
 *
 * Usage:
 *   node scripts/sign-ep.mjs --dir=packages/pro-extensions/sql-editor-pro
 *   node scripts/sign-ep.mjs --dir=/path/to/staged/ep [--out=/path/signature.sig]
 */

import {
  createHash,
  createPrivateKey,
  createPublicKey,
  generateKeyPairSync,
  sign,
} from 'crypto';
import { existsSync, readFileSync, writeFileSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
export const ROOT = resolve(__dirname, '..');

/** Files included in the signed payload (relative to package root). */
export const DEFAULT_SIGNED_FILES = ['manifest.json', 'dist/index.esm.js'];

export function sha256Hex(content) {
  return createHash('sha256').update(content).digest('hex');
}

export function sha256File(filePath) {
  return sha256Hex(readFileSync(filePath));
}

export function parsePrivateKeyMaterial(raw) {
  const trimmed = raw.trim();
  if (trimmed.includes('BEGIN')) {
    return createPrivateKey(trimmed);
  }
  const seed = Buffer.from(trimmed, 'base64');
  if (seed.length !== 32) {
    throw new Error(
      '[sign-ep] DATAZEN_EP_SIGNING_PRIVATE_KEY must be PEM or base64-encoded 32-byte Ed25519 seed',
    );
  }
  return createPrivateKey({
    key: Buffer.concat([
      Buffer.from('302e020100300506032b657004220420', 'hex'),
      seed,
    ]),
    format: 'der',
    type: 'pkcs8',
  });
}

export function resolveSigningPrivateKey(env = process.env) {
  const fromEnv = env.DATAZEN_EP_SIGNING_PRIVATE_KEY?.trim();
  if (fromEnv) {
    return parsePrivateKeyMaterial(fromEnv);
  }
  const { privateKey } = generateKeyPairSync('ed25519');
  return privateKey;
}

export function buildSignaturePayload(fileHashes) {
  const sorted = Object.keys(fileHashes)
    .sort()
    .reduce((acc, key) => {
      acc[key] = fileHashes[key];
      return acc;
    }, {});
  return JSON.stringify({
    algorithm: 'Ed25519',
    hashAlgorithm: 'SHA-256',
    files: sorted,
  });
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
  const packageDir = resolve(opts.packageDir);
  const files = opts.files ?? DEFAULT_SIGNED_FILES;
  const privateKey = opts.privateKey ?? resolveSigningPrivateKey();
  const signedAt = opts.signedAt ?? new Date().toISOString();

  const fileHashes = {};
  for (const rel of files) {
    const abs = join(packageDir, rel);
    if (!existsSync(abs)) {
      throw new Error(`[sign-ep] missing signed artifact: ${rel} (expected at ${abs})`);
    }
    fileHashes[rel] = sha256File(abs);
  }

  const payload = buildSignaturePayload(fileHashes);
  const signature = sign(null, Buffer.from(payload, 'utf8'), privateKey).toString('base64');
  const publicKey = createPublicKey(privateKey);
  const publicKeyDer = publicKey.export({ type: 'spki', format: 'der' });
  const publicKeyBase64 = Buffer.from(publicKeyDer).subarray(12).toString('base64');

  const sigDoc = {
    algorithm: 'Ed25519',
    hashAlgorithm: 'SHA-256',
    signedAt,
    publicKey: publicKeyBase64,
    files: fileHashes,
    payload,
    signature,
  };

  const outPath = opts.outPath ?? join(packageDir, 'signature.sig');
  writeFileSync(outPath, `${JSON.stringify(sigDoc, null, 2)}\n`, 'utf8');
  return { outPath, sigDoc };
}

export function parseSignArgs(argv = process.argv.slice(2)) {
  let packageDir = null;
  let outPath = null;
  for (const arg of argv) {
    if (arg.startsWith('--dir=')) {
      packageDir = arg.slice('--dir='.length);
    } else if (arg.startsWith('--out=')) {
      outPath = arg.slice('--out='.length);
    }
  }
  return { packageDir, outPath };
}

const isMain = process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1]);
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
    console.error(err);
    process.exit(1);
  }
}
