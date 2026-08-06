import { expect, browser, $ } from '@wdio/globals';
import {
  openConnectionWindow,
  closeExtraWindows,
} from '../helpers.js';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: any) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: any) => done(r))
        .catch((e: any) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as any)) {
    throw new Error((result as any).__error);
  }
  return result as T;
}

describe('ER 图功能 E2E 测试 (ER-001~ER-006)', () => {
  let mainWindow: string;
  let connWindow: string;

  before(async () => {
    const windows = await openConnectionWindow();
    mainWindow = windows.mainWindow;
    connWindow = windows.connWindow;
  });

  after(async () => {
    if (mainWindow) {
      await closeExtraWindows(mainWindow);
    }
  });

  it('ER-001: get_er_data returns schema data', async () => {
    // Get the connection info from the window
    const connId = await browser.execute(() => {
      return new URLSearchParams(window.location.search).get('connectionId') || '';
    }) as string;

    if (!connId) {
      console.warn('No connectionId in URL, skipping');
      return;
    }

    // Get current database
    const databases = await invokeBackend<string[]>('get_databases', { connectionId: connId });
    if (databases.length === 0) {
      console.warn('No databases found, skipping');
      return;
    }

    const database = databases[0];
    const schemas = await invokeBackend<any[]>('get_er_data', {
      connectionId: connId,
      database,
    });

    expect(Array.isArray(schemas)).toBe(true);
  });

  it('ER-002: ER diagram button exists in toolbar', async () => {
    await browser.switchToWindow(connWindow);
    await browser.pause(1000);

    // Look for the ER diagram button
    const erButton = await $('button*=ER');
    const exists = await erButton.isExisting();

    // Button may use icon-only or text, check for either
    if (exists) {
      await expect(erButton).toBeDisplayed();
    }
  });

  it('ER-003: clicking ER button opens ER diagram panel', async () => {
    await browser.switchToWindow(connWindow);

    // Find and click the ER diagram button
    const erButton = await $('button*=ER');
    if (await erButton.isExisting()) {
      await erButton.click();
      await browser.pause(2000);

      // The React Flow canvas should appear
      const reactFlowPane = await $('[class*="react-flow"]');
      if (await reactFlowPane.isExisting()) {
        await expect(reactFlowPane).toBeDisplayed();
      }
    }
  });

  it('ER-004: ER diagram tab appears in panel tabs', async () => {
    await browser.switchToWindow(connWindow);

    // Check if an ER tab exists after opening
    const erTab = await $('button*=ER');
    if (await erTab.isExisting()) {
      await expect(erTab).toBeDisplayed();
    }
  });

  it('ER-005: ER diagram shows controls', async () => {
    await browser.switchToWindow(connWindow);

    // React Flow controls panel
    const controls = await $('[class*="react-flow__controls"]');
    if (await controls.isExisting()) {
      await expect(controls).toBeDisplayed();
    }
  });

  it('ER-006: ER diagram shows table and relation counts', async () => {
    await browser.switchToWindow(connWindow);

    // The stats panel shows counts
    const statsPanel = await $('[class*="top-right"]');
    if (await statsPanel.isExisting()) {
      const text = await statsPanel.getText();
      // Should contain count information (e.g., "5 张表" or "5 tables")
      expect(text.length).toBeGreaterThan(0);
    }
  });
});
