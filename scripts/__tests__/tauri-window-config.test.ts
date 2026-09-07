/** @vitest-environment node */
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

interface WindowConfig {
  label: string;
  dragDropEnabled?: boolean;
  [key: string]: unknown;
}

function readWindows(filename: string): WindowConfig[] {
  const config = JSON.parse(
    readFileSync(new URL(`../../src-tauri/${filename}`, import.meta.url), 'utf8'),
  ) as { app: { windows: WindowConfig[] } };
  return config.app.windows;
}

describe('Tauri platform window replacement', () => {
  const baseWindows = readWindows('tauri.conf.json');

  it('routes main-window drags through HTML5', () => {
    expect(baseWindows.find((window) => window.label === 'main')?.dragDropEnabled).toBe(false);
  });

  it.each([
    ['macos', {}],
    ['windows', { decorations: false }],
  ] as const)(
    '%s retains base settings when its window array replaces the base array',
    (platform, overrides) => {
      // Tauri uses JSON Merge Patch, which replaces the entire windows array.
      // A partial platform window silently restores omitted Tauri defaults.
      const platformWindows = readWindows(`tauri.${platform}.conf.json`);
      expect(platformWindows).toHaveLength(baseWindows.length);
      for (const baseWindow of baseWindows) {
        expect(platformWindows.find((window) => window.label === baseWindow.label)).toMatchObject({
          ...baseWindow,
          ...overrides,
        });
      }
    },
  );
});
