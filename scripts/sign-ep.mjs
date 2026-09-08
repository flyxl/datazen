#!/usr/bin/env node
/**
 * Sign a DataZen Host EP extension bundle (manifest.json + dist/index.esm.js).
 *
 * Produces `signature.sig` beside manifest.json using Ed25519 over a canonical
 * SHA-256 digest payload. Private key from DATAZEN_EP_PRIVATE_KEY (SPKI/PKCS8
 * DER base64 or PEM) or the built-in test key for local development.
 *
 * Usage:
 *   node scripts/sign-ep.mjs --dir packages/pro-extensions/sql-editor-pro
 *   DATAZEN_EP_PRIVATE_KEY=<pkcs8-b64> node scripts/sign-ep.mjs --dir ./my-ep
 */
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

const EP_SIGNATURE_VERSION = 1;
const EP_SIGNATURE_ALGORITHM = 'Ed25519';
const EP_SIGNED_FILE_PATHS = ['manifest.json', 'dist/index.esm.js'];

/** Built-in test private key (PKCS8 DER base64) — pairs with OFFICIAL_EP_PUBLIC_KEY in security.ts */
const TEST_EP_PRIVATE_KEY_PKCS8_B64 =
  'MC4CAQAwBQYDK2VwBCIEIPEWEScCcGuvAK1PLiblsaf/hx6x2/oVqUovRkBU/X0N';

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`);
  if (i >= 0 && process.argv[i + 1]) return process.argv[i + 1];
  return fallback;
}

function usage() {
  console.error(`Usage: node scripts/sign-ep.mjs --dir <extension-root>

Options:
  --dir              Extension package root containing manifest.json and dist/index.esm.js
  --private-key-env  Env var holding PKCS8 private key (default: DATAZEN_EP_PRIVATE_KEY)
  --out              Output path for signature.sig (default: <dir>/signature.sig)
`);
  process.exit(1);
}

function normalizePrivateKey(raw) {
  const trimmed = raw.trim();
  if (trimmed.includes('BEGIN')) {
    return crypto.createPrivateKey(trimmed);
  }
  const der = Buffer.from(trimmed.replace(/\s+/g, ''), 'base64');
  return crypto.createPrivateKey({ key: der, format: 'der', type: 'pkcs8' });
}

function resolvePrivateKey(envName) {
  const fromEnv = process.env[envName];
  if (fromEnv) {
    return normalizePrivateKey(fromEnv);
  }
  const der = Buffer.from(TEST_EP_PRIVATE_KEY_PKCS8_B64, 'base64');
  return crypto.createPrivateKey({ key: der, format: 'der', type: 'pkcs8' });
}

function sha256Hex(content) {
  return crypto.createHash('sha256').update(content, 'utf8').digest('hex');
}

function buildSignaturePayload(files, version = EP_SIGNATURE_VERSION) {
  const sortedFiles = {};
  for (const key of Object.keys(files).sort()) {
    sortedFiles[key] = files[key];
  }
  return Buffer.from(JSON.stringify({ version, files: sortedFiles }), 'utf8');
}

function readRequiredFile(root, relativePath) {
  const full = path.join(root, relativePath);
  if (!fs.existsSync(full)) {
    throw new Error(`Missing required file: ${relativePath} (expected at ${full})`);
  }
  return fs.readFileSync(full, 'utf8');
}

function signExtensionDir(dir, privateKey, outPath) {
  const root = path.resolve(dir);
  const files = {};

  for (const relativePath of EP_SIGNED_FILE_PATHS) {
    const content = readRequiredFile(root, relativePath);
    files[relativePath] = { sha256: sha256Hex(content) };
  }

  const payload = buildSignaturePayload(files);
  const signature = crypto.sign(null, payload, privateKey);
  const signedAt = new Date().toISOString();

  const sigFile = {
    version: EP_SIGNATURE_VERSION,
    algorithm: EP_SIGNATURE_ALGORITHM,
    signedAt,
    files,
    signature: signature.toString('base64'),
  };

  fs.writeFileSync(outPath, `${JSON.stringify(sigFile, null, 2)}\n`, 'utf8');
  console.log(`Wrote ${outPath}`);
  console.log(`  algorithm: ${EP_SIGNATURE_ALGORITHM}`);
  console.log(`  signedAt:  ${signedAt}`);
  for (const [filePath, digest] of Object.entries(files)) {
    console.log(`  ${filePath}: sha256=${digest.sha256.slice(0, 16)}…`);
  }
}

const dir = arg('dir');
if (!dir) usage();

const envName = arg('private-key-env', 'DATAZEN_EP_PRIVATE_KEY');
const outPath = path.resolve(arg('out', path.join(path.resolve(dir), 'signature.sig')));

try {
  const privateKey = resolvePrivateKey(envName);
  signExtensionDir(dir, privateKey, outPath);
} catch (err) {
  console.error(`sign-ep failed: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
}
