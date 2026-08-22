/**
 * F9 Host E2E: runtime UI plugins — sample plugin fixture journeys (PRD §7/§8).
 *
 * Fixture: `e2e/fixtures/sample-plugin/` (id `datazen.sample`, zero-build
 * static package with one workspace page + one theme contribution).
 *
 * Journeys:
 *   J1 install via management page dialog (typed package path → review → confirm)
 *   J2 open tab from Workspace navigator + bridge round-trip inside the iframe
 *      (context.getConnections, storage.set/get, dark state, token count)
 *   J3 tab independence: connection/workspace modes keep separate state;
 *      closing all workspace tabs restores the default card view
 *   J5 Settings → 外观 shows the plugin theme card and applies it persistently
 *   J4 disable removes tab + navigator entry; uninstall (with confirm) removes card
 *      (executed last because it tears the plugin down)
 */
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { browser, $, $$, expect } from '@wdio/globals';
import { invokeBackend } from '../helpers/data-dashboard.js';
import { openSettingsInMainWindow } from '../helpers.js';

const PLUGIN_ID = 'datazen.sample';
const PAGE_KEY = `${PLUGIN_ID}:hello`;
const EXPECTED_PACK_ID = `plugin:${PLUGIN_ID}:sample-light`;
const THEME_ID = 'sample-light';

const THIS_DIR = path.dirname(fileURLToPath(import.meta.url));
/** Absolute path typed into the install dialog's PathInput. */
const FIXTURE_DIR = path.resolve(THIS_DIR, '..', 'fixtures', 'sample-plugin');

interface PluginSummaryRow {
  id: string;
  enabled?: boolean;
}

interface PersistedSettings {
  theme: { mode?: string; packId?: string | null };
}

async function removeSamplePluginViaIpc() {
  try {
    await invokeBackend('remove_plugin', { id: PLUGIN_ID });
  } catch {
    /* not installed yet */
  }
}

async function resetThemePackId() {
  const settings = await invokeBackend<PersistedSettings>('get_settings');
  if (settings.theme.packId) {
    await invokeBackend('save_settings', {
      settings: { ...settings, theme: { ...settings.theme, packId: null } },
    });
  }
}

async function openPluginsPage() {
  const nav = await $('[data-testid="workspace-nav-plugins"]');
  await nav.waitForDisplayed({ timeout: 10000 });
  await nav.click();
  await $('[data-testid="plugin-management-page"]').waitForDisplayed({ timeout: 10000 });
}

async function openWorkspaceMode() {
  const nav = await $('[data-testid="workspace-nav-workspace-pages"]');
  await nav.waitForDisplayed({ timeout: 10000 });
  await nav.click();
  await $('[data-testid="workspace-navigator"]').waitForDisplayed({ timeout: 10000 });
}

async function sampleCard() {
  return $(`[data-testid="plugin-card"][data-plugin-id="${PLUGIN_ID}"]`);
}

async function waitForSampleCard(timeout = 15000) {
  const card = await sampleCard();
  await card.waitForDisplayed({ timeout, timeoutMsg: 'sample plugin card not visible' });
  return card;
}

async function openSampleTabFromNavigator() {
  const item = await $(`[data-testid="workspace-nav-item"][data-page-key="${PAGE_KEY}"]`);
  await item.waitForDisplayed({ timeout: 10000 });
  await item.click();
  await $('[data-testid="workspace-tabbar"]').waitForDisplayed({ timeout: 10000 });
  const iframe = await $('[data-testid="plugin-iframe"]');
  await iframe.waitForExist({ timeout: 15000 });
  return iframe;
}

async function textOfTestId(testId: string): Promise<string> {
  const el = await $(`[data-testid="${testId}"]`);
  if (!(await el.isExisting())) return '';
  return el.getText();
}

/**
 * Switch into the plugin iframe, run `assertions`, then always switch back to
 * the top document (the shell keeps only one frame level).
 */
async function insidePluginFrame(assertions: () => Promise<void>) {
  const iframe = await openSampleTabFromNavigator();
  await browser.switchToFrame(iframe);
  try {
    await assertions();
  } finally {
    await browser.switchToParentFrame();
  }
}

describe('UI plugins (F9: sample plugin + bridge + appearance)', () => {
  before(async () => {
    // Clean slate: drop any leftover install and theme selection.
    await browser.url('tauri://localhost');
    await browser.pause(1500);
    await removeSamplePluginViaIpc();
    await resetThemePackId();
  });

  // ── J1: install through the management page dialog ──────────────────

  it('J1-001: installs the fixture directory via the two-step dialog and shows the card', async () => {
    await openPluginsPage();

    const emptyState = await $('[data-testid="plugin-page-empty"]');
    if (await emptyState.isExisting()) {
      await expect(emptyState).toBeDisplayed(); // sanity: starts without our plugin
    }

    await $('[data-testid="plugin-install-button"]').click();
    const dialog = await $('[role="dialog"]');
    await dialog.waitForDisplayed({ timeout: 10000 });

    const pathInput = await $('input[placeholder="/path/to/plugin.zip"]');
    await pathInput.waitForDisplayed({ timeout: 5000 });
    await pathInput.setValue(FIXTURE_DIR);

    // Step 1 → 2: validate-only inspect; review shows name/version/permissions.
    await $('[data-testid="plugin-install-next"]').click();
    await $('[data-testid="plugin-install-review"]').waitForDisplayed({ timeout: 15000 });
    const review = await $('[data-testid="plugin-install-review"]').getText();
    expect(review).toContain('Sample Hello');
    expect(review).toContain('1.0.0');

    const badges = await $$('[data-testid="plugin-install-permissions"] [title]');
    expect(badges.length).toBe(3); // context:connections / command:invoke / storage:local

    await $('[data-testid="plugin-install-confirm"]').click();
    await browser.waitUntil(async () => !(await dialog.isExisting()), {
      timeout: 15000,
      timeoutMsg: 'install dialog did not close',
    });

    const card = await waitForSampleCard();
    await expect(card).toBeDisplayed();

    // Permission badges on the card.
    const cardText = await card.getText();
    expect(cardText).toContain('context:connections');
    expect(cardText).toContain('command:invoke');
    expect(cardText).toContain('storage:local');

    // Enabled by default after install.
    const toggle = await card.$('[data-testid="plugin-toggle"]');
    expect(await toggle.getAttribute('aria-checked')).toBe('true');
  });

  it('J1-002: list_plugins reports the installed plugin as enabled', async () => {
    const plugins = await invokeBackend<PluginSummaryRow[]>('list_plugins');
    const row = plugins.find((p) => p.id === PLUGIN_ID);
    expect(row).toBeDefined();
    expect(row?.enabled).toBe(true);
  });

  // ── J2: workspace entry + bridge round-trip inside the iframe ───────

  it('J2-001: workspace navigator lists the page and opens a tab', async () => {
    await openWorkspaceMode();

    const item = await $(`[data-testid="workspace-nav-item"][data-page-key="${PAGE_KEY}"]`);
    await item.waitForDisplayed({ timeout: 10000 });

    await item.click();
    await $('[data-testid="workspace-tabbar"]').waitForDisplayed({ timeout: 10000 });
    const tab = await $('[data-testid="workspace-tab"]');
    await expect(tab).toBeDisplayed();
  });

  it('J2-002: bridge handshake completes and renders host context in the iframe', async () => {
    await openWorkspaceMode();
    await insidePluginFrame(async () => {
      // Handshake: plugin.ready → host.ready.
      await browser.waitUntil(async () => (await textOfTestId('bridge-status')) === 'ready', {
        timeout: 20000,
        timeoutMsg: `bridge-status never became "ready" (last: ${await textOfTestId('bridge-status')})`,
      });

      // Dark state and token snapshot arrived with host.ready.
      const darkState = await textOfTestId('dark-state');
      expect(['dark', 'light']).toContain(darkState);
      expect(Number(await textOfTestId('token-count'))).toBeGreaterThan(0);
    });
  });

  it('J2-003: storage set/get round-trips through the RPC bridge', async () => {
    await openWorkspaceMode();
    await insidePluginFrame(async () => {
      await browser.waitUntil(async () => (await textOfTestId('storage-roundtrip')) !== '-', {
        timeout: 20000,
        timeoutMsg: `storage round-trip never settled (last: ${await textOfTestId('storage-roundtrip')})`,
      });
      expect(await textOfTestId('storage-roundtrip')).toBe('ok');
    });
  });

  it('J2-004: context.getConnections count matches the persisted connections', async () => {
    const conns = await invokeBackend<{ id: string }[]>('get_connections');
    expect(conns.length).toBeGreaterThanOrEqual(1); // wdio.conf seeds 本地 PostgreSQL

    await openWorkspaceMode();
    await insidePluginFrame(async () => {
      await browser.waitUntil(
        async () => Number(await textOfTestId('conn-count')) === conns.length,
        {
          timeout: 20000,
          timeoutMsg: `conn-count (${await textOfTestId('conn-count')}) did not match get_connections (${conns.length})`,
        },
      );
    });
  });

  // ── J3: independent tab systems ─────────────────────────────────────

  it('J3-001: switching to connections mode and back preserves the workspace tab', async () => {
    await openWorkspaceMode();
    await openSampleTabFromNavigator();

    await $('[data-testid="workspace-nav-connections"]').click();
    await browser.pause(600);
    // Connections mode replaces the whole workspace layout.
    expect(await $('[data-testid="workspace-navigator"]').isExisting()).toBe(false);

    await openWorkspaceMode();
    // The tab survived the round-trip; its iframe instance is still mounted.
    await $('[data-testid="workspace-tabbar"]').waitForDisplayed({ timeout: 10000 });
    expect((await $$('[data-testid="workspace-tab"]')).length).toBe(1);
    await expect(await $('[data-testid="plugin-page-shell"]')).toBeDisplayed();
  });

  it('J3-002: closing all workspace tabs restores the default cards view', async () => {
    while (await $('[data-testid="workspace-tab-close"]').isExisting()) {
      await (await $('[data-testid="workspace-tab-close"]')).click();
      await browser.pause(400);
    }

    expect(await $('[data-testid="workspace-tabbar"]').isExisting()).toBe(false);
    expect(await $('[data-testid="plugin-page-shell"]').isExisting()).toBe(false);
    const cards = await $('[data-testid="workspace-default-cards"]');
    await cards.waitForDisplayed({ timeout: 10000 });
    const body = await $('body').getText();
    expect(body).toContain('Sample Hello'); // page card offered again
  });

  // ── J5: Settings → 外观 applies the plugin theme persistently ───────

  it('J5-001: appearance section lists Sample Light and applying persists plugin:<id>:<theme>', async () => {
    await openSettingsInMainWindow('appearance');

    const section = await $('[data-testid="appearance-section"]');
    await section.waitForDisplayed({ timeout: 10000 });

    const card = await $(`[data-testid="appearance-theme-card"][data-theme-id="${THEME_ID}"]`);
    await card.waitForDisplayed({ timeout: 10000 });

    await card.click();
    await browser.waitUntil(async () => (await card.getAttribute('aria-pressed')) === 'true', {
      timeout: 10000,
      timeoutMsg: 'theme card did not become active',
    });
    await expect(await $('[data-testid="appearance-current-badge"]')).toBeDisplayed();

    // Durable value lives in settings (not localStorage): plugin:{pluginId}:{themeId}.
    const settings = await invokeBackend<PersistedSettings>('get_settings');
    expect(settings.theme.packId).toBe(EXPECTED_PACK_ID);
  });

  // ── J4: disable → tab/nav removed; uninstall (confirm) → card gone ──

  it('J4-001: disabling the plugin closes its tab and removes the navigator entry', async () => {
    // Open a fresh tab so we can watch it being torn down.
    await openWorkspaceMode();
    await openSampleTabFromNavigator();

    await openPluginsPage();
    const card = await waitForSampleCard();
    const toggle = await card.$('[data-testid="plugin-toggle"]');
    await toggle.click();
    await browser.waitUntil(async () => (await toggle.getAttribute('aria-checked')) === 'false', {
      timeout: 15000,
      timeoutMsg: 'plugin toggle did not flip to disabled',
    });

    await openWorkspaceMode();
    await $('[data-testid="workspace-navigator"]').waitForDisplayed({ timeout: 10000 });
    expect(await $('[data-testid="workspace-nav-item"]').isExisting()).toBe(false);
    expect(await $('[data-testid="workspace-tabbar"]').isExisting()).toBe(false);
    expect(await $('[data-testid="plugin-page-shell"]').isExisting()).toBe(false);
  });

  it('J4-002: uninstalling asks for confirmation and removes the management card', async () => {
    await openPluginsPage();
    const card = await waitForSampleCard();
    await (await card.$('[data-testid="plugin-uninstall"]')).click();

    const confirm = await $('[data-testid="confirm-dialog-ok"]');
    await confirm.waitForDisplayed({ timeout: 10000 });
    await confirm.click();

    const cardAfter = await sampleCard();
    await browser.waitUntil(async () => !(await cardAfter.isExisting()), {
      timeout: 15000,
      timeoutMsg: 'plugin card still present after uninstall',
    });

    const plugins = await invokeBackend<PluginSummaryRow[]>('list_plugins');
    expect(plugins.find((p) => p.id === PLUGIN_ID)).toBeUndefined();
  });

  // ── cleanup ─────────────────────────────────────────────────────────

  after(async () => {
    try {
      await removeSamplePluginViaIpc();
    } catch {
      /* ignore */
    }
    try {
      await resetThemePackId();
    } catch {
      /* ignore */
    }
    await browser.url('tauri://localhost');
    await browser.pause(800);
  });
});
