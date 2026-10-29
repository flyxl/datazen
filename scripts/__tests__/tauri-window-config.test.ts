/** @vitest-environment node */
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

interface WindowConfig {
  label: string;
  [key: string]: unknown;
}

function readAppConfig(filename: string): {
  app: { windows: WindowConfig[]; security: { csp: Record<string, string> } };
} {
  return JSON.parse(
    readFileSync(new URL(`../../apps/desktop/src-tauri/${filename}`, import.meta.url), 'utf8'),
  ) as { app: { windows: WindowConfig[]; security: { csp: Record<string, string> } } };
}

function readWindows(filename: string): WindowConfig[] {
  return readAppConfig(filename).app.windows;
}

describe('Tauri platform window replacement', () => {
  // The main window is created programmatically by Rust at startup
  // (see `create_main_window` / `create_onboarding_window` in commands/window.rs)
  // so all config files define an empty windows array. Window *settings* the
  // config cannot express anymore — notably `dragDropEnabled: false`, which
  // HTML5 drag & drop depends on — are pinned by the Rust guard
  // `every_window_builder_disables_native_drag_drop` in commands/window.rs.

  it('has no statically defined windows (created programmatically)', () => {
    const baseWindows = readWindows('tauri.conf.json');
    expect(baseWindows).toHaveLength(0);
  });

  it('limits renderer connections to local Tauri IPC and asset origins', () => {
    const connectSrc = readAppConfig('tauri.conf.json').app.security.csp['connect-src'];
    expect(connectSrc).toContain('ipc:');
    expect(connectSrc).toContain('http://ipc.localhost');
    expect(connectSrc).toContain('asset://localhost');
    expect(connectSrc.split(/\s+/)).not.toContain('https:');
    expect(connectSrc.split(/\s+/)).not.toContain('http:');
  });

  it.each([['macos'], ['windows']] as const)(
    '%s config defines an empty windows array (merged into base)',
    (platform) => {
      const platformWindows = readWindows(`tauri.${platform}.conf.json`);
      expect(platformWindows).toHaveLength(0);
    },
  );
});
