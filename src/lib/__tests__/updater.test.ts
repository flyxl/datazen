import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Form-split F6 prep: channel/variant/manifest reasoning is shared and pure —
// these are the lib-side tests. The plugin-updater FLOWS moved to
// `src/platform/__tests__/updaterFlow.test.ts` (the desktop zone owns them);
// nothing here imports the zone or the plugins.

// The form branch is the compile-time IS_DESKTOP_BUILD constant (shared
// `platform/env`, pure `import.meta.env`); a getter-export mock replaces the
// old __TAURI_INTERNALS__ toggle.
const envState = vi.hoisted(() => ({ desktop: false }));
vi.mock('../../platform/env', () => ({
  get IS_DESKTOP_BUILD() {
    return envState.desktop;
  },
}));

// The SKU is a build-time constant from gitignored codegen. These suites stand in
// for "this artifact was built as the Basic SKU, which publishes a channel" — the
// gating of every other SKU is covered in updaterVariant.test.ts.
vi.mock('../../extensions/generated', () => ({
  DATAZEN_VARIANT: 'basic',
  DATAZEN_UPDATER_CHANNEL: true,
}));

import {
  currentVariant,
  getUpdateChannel,
  hasUpdaterChannel,
  isUpdaterSupported,
  manifestBelongsToBuild,
  maybeCheckOnStartup,
  readManifestVariant,
} from '../updater';

describe('updater without Tauri', () => {
  it('isUpdaterSupported returns false', () => {
    expect(isUpdaterSupported()).toBe(false);
  });

  it('reports no update channel for a non-desktop build', () => {
    expect(getUpdateChannel()).toBe('none');
  });

  it('maybeCheckOnStartup is no-op', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const info = vi.spyOn(console, 'info').mockImplementation(() => {});
    await maybeCheckOnStartup(true);
    expect(warn).not.toHaveBeenCalled();
    // Browser dev: no channel at all, so nothing to check.
    expect(info).toHaveBeenCalledWith(
      '[updater] startup check skipped: setting=true channel=none variant=basic',
    );
    warn.mockRestore();
    info.mockRestore();
  });
});

describe('updater with Tauri', () => {
  beforeEach(() => {
    envState.desktop = true;
    vi.clearAllMocks();
  });

  afterEach(() => {
    envState.desktop = false;
  });

  it('isUpdaterSupported returns true', () => {
    expect(isUpdaterSupported()).toBe(true);
  });

  it('exposes the build SKU and its channel', () => {
    expect(currentVariant()).toBe('basic');
    expect(hasUpdaterChannel()).toBe(true);
    expect(getUpdateChannel()).toBe('auto');
  });

  it('maybeCheckOnStartup skips when disabled', async () => {
    const info = vi.spyOn(console, 'info').mockImplementation(() => {});
    await maybeCheckOnStartup(false);
    // The decision is logged so "why is this build not checking for updates?"
    // can be answered from the console.
    expect(info).toHaveBeenCalledWith(
      '[updater] startup check skipped: setting=false channel=auto variant=basic',
    );
    info.mockRestore();
  });
});

describe('updater refuses a manifest belonging to another SKU', () => {
  beforeEach(() => {
    envState.desktop = true;
    vi.clearAllMocks();
  });

  afterEach(() => {
    envState.desktop = false;
  });

  it('reads the manifest variant only when it is a usable string', () => {
    expect(readManifestVariant({ variant: 'all' })).toBe('all');
    expect(readManifestVariant({})).toBeNull();
    expect(readManifestVariant({ variant: 7 })).toBeNull();
    expect(readManifestVariant({ variant: '   ' })).toBeNull();
    expect(readManifestVariant(null)).toBeNull();
    expect(readManifestVariant('all')).toBeNull();
  });

  it('accepts a manifest only when it names this build', () => {
    // Basic inherits its manifest name from earlier releases, so manifests
    // published before the field existed must stay acceptable — rejecting them
    // would strand every already-installed Basic build.
    expect(manifestBelongsToBuild({}, 'basic')).toBe(true);
    expect(manifestBelongsToBuild({ variant: 'basic' }, 'basic')).toBe(true);
    expect(manifestBelongsToBuild({ variant: 'all' }, 'basic')).toBe(false);

    // Variant manifests exist only because of per-SKU channels, so a missing
    // field there can only mean the manifest belongs to another SKU.
    expect(manifestBelongsToBuild({}, 'all')).toBe(false);
    expect(manifestBelongsToBuild({ variant: 'all' }, 'all')).toBe(true);
    expect(manifestBelongsToBuild({ variant: '-all' }, 'all')).toBe(true);
    expect(manifestBelongsToBuild({ variant: ' AKULAKU ' }, 'akulaku')).toBe(true);
    expect(manifestBelongsToBuild({ variant: 'akulaku' }, 'all')).toBe(false);

    // This predicate only compares SKUs; `custom` never gets this far because
    // `getUpdateChannel()` returns 'manual' for it.
    expect(manifestBelongsToBuild({}, 'custom')).toBe(false);
  });
});
