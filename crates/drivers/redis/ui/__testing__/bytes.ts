/**
 * Test-support byte fixtures.
 *
 * Lives outside `__tests__/` on purpose: the driver vitest config collects
 * *every* `.ts`/`.tsx` under `__tests__/` as a test file, so a shared helper
 * placed there would be run as an empty suite.
 *
 * `bytesToBase64` used to live in `valueView/codecs.ts` next to its inverse
 * `base64ToBytes`, but nothing on the production path encodes bytes — the UI
 * only ever *decodes* a stored base64 blob — so it is a fixture builder, and
 * belongs with the other fixtures.
 */
export function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  // eslint-disable-next-line no-restricted-globals
  return btoa(binary);
}

/** UTF-8 text → base64, the shape every string-value mock payload wants. */
export function textToBase64(text: string): string {
  return bytesToBase64(new TextEncoder().encode(text));
}
