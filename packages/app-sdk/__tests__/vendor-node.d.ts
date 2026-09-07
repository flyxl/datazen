/**
 * Minimal ambient typings for node builtins used by test-only helpers
 * (`readFileSync` source-contract checks). Keeps `tsc -p packages/extension-sdk`
 * green without adding a runtime dependency or touching the package tsconfig.
 */
declare module 'node:fs' {
  export function readFileSync(path: string | URL, encoding?: BufferEncoding): string;
}

declare module 'node:path' {
  export function resolve(...segments: string[]): string;
  export function dirname(path: string): string;
}

declare module 'node:url' {
  export function fileURLToPath(url: string | URL): string;
}

type BufferEncoding = 'utf8' | 'utf-8' | 'ascii' | 'latin1' | 'hex' | 'base64';
