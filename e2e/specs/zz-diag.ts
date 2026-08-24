/** Diagnostic: dump startup UI state + screenshots to /tmp. */
import { browser, $ } from '@wdio/globals';
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';

const ROOT = path.resolve(path.dirname(url.fileURLToPath(import.meta.url)), '..', '..');

describe('diag startup state', () => {
  it('dumps state', async () => {
    await browser.pause(2000);
    const invoke = (cmd: string, args: Record<string, unknown> = {}) =>
      browser.executeAsync(
        (c: string, a: string, done: (r: unknown) => void) => {
          (window as any).__TAURI_INTERNALS__
            .invoke(c, JSON.parse(a))
            .then((r: unknown) => done(r))
            .catch((e: unknown) => done({ __error: String(e) }));
        },
        cmd,
        JSON.stringify(args),
      );

    const conns = await invoke('get_connections');
    console.log(
      'CONNS:',
      JSON.stringify(conns, null, 2).replace(/"password":"[^"]*"/g, '"password":"***"'),
    );

    await browser.execute(() => {
      document
        .querySelector('[data-testid="workspace-nav-connections"]')
        ?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });
    await browser.pause(1500);
    await browser.saveScreenshot('/tmp/diag-conn.png');

    const info = await browser.execute(() => ({
      connItems: document.querySelectorAll('[data-conn-item]').length,
      items: Array.from(document.querySelectorAll('[data-conn-item]')).map((el) =>
        (el.textContent || '').trim().slice(0, 30),
      ),
      hasNewConn: (document.body.textContent || '').includes('新建连接'),
    }));
    console.log('UI:', JSON.stringify(info, null, 2));

    // workflows on disk
    const home = process.env.HOME || '';
    const candidates = [
      path.join(home, 'Library/Application Support/com.datazen.app/workflows'),
      path.join(home, 'Library/Application Support/com.datazen.dev/workflows'),
      path.join(home, 'Library/Application Support/datazen/workflows'),
    ];
    for (const dir of candidates) {
      if (fs.existsSync(dir)) {
        console.log('WFDIR:', dir);
        for (const f of fs.readdirSync(dir)) console.log('  WF:', f);
      }
    }
  });
});
