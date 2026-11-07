import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

// Form-split F6 prep: the SKU-channel reasoning is shared and pure — these are
// the lib-side tests. The "nothing may reach the plugin" flow half of the
// regression guard lives in `src/platform/__tests__/updaterFlow.test.ts`.

// The form branch is the compile-time IS_DESKTOP_BUILD constant (shared
// `platform/env`); a getter-export mock replaces the old toggle.
const envState = vi.hoisted(() => ({ desktop: false }));
vi.mock('../../platform/env', () => ({
  get IS_DESKTOP_BUILD() {
    return envState.desktop;
  },
}));

// A private / local SKU: it names itself but publishes no updater channel, which
// is what a build that passed no --variant ends up as.
vi.mock('../../extensions/generated', () => ({
  DATAZEN_VARIANT: 'custom',
  DATAZEN_UPDATER_CHANNEL: false,
}));

import {
  currentVariant,
  getUpdateChannel,
  hasUpdaterChannel,
  isUpdaterSupported,
  maybeCheckOnStartup,
} from '../updater';

describe('updater on a SKU with no published channel', () => {
  beforeEach(() => {
    envState.desktop = true;
    vi.clearAllMocks();
  });

  afterEach(() => {
    envState.desktop = false;
  });

  it('is a desktop build, so it shows the manual download card', () => {
    expect(currentVariant()).toBe('custom');
    expect(hasUpdaterChannel()).toBe(false);
    expect(getUpdateChannel()).toBe('manual');
  });

  it('never runs the in-app updater', () => {
    expect(isUpdaterSupported()).toBe(false);
  });

  it('stays inert even when the startup setting is on', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    await maybeCheckOnStartup(true);
    // Not even a warning: there is nothing to report, the SKU simply has no channel.
    expect(warn).not.toHaveBeenCalled();
    warn.mockRestore();
  });

  it('logs why it stays inert', async () => {
    const info = vi.spyOn(console, 'info').mockImplementation(() => {});
    await maybeCheckOnStartup(true);
    expect(info).toHaveBeenCalledWith(
      '[updater] startup check skipped: setting=true channel=manual variant=custom',
    );
    info.mockRestore();
  });
});
