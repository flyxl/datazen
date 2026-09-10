import { browser, $ } from '@wdio/globals';
import { invokeBackend } from '../helpers.js';

describe('DBG-EXPORT', () => {
  it('debug export button', async () => {
    await browser.pause(3000);
    const state = await browser.execute(() => {
      const btns = Array.from(document.querySelectorAll('[data-testid="data-table-export"]'));
      return btns.map((b) => ({
        disabled: (b as HTMLButtonElement).disabled,
        title: b.getAttribute('title'),
        text: (b.textContent || '').trim(),
        rect: (() => {
          const r = b.getBoundingClientRect();
          return { w: r.width, h: r.height, x: r.x, y: r.y };
        })(),
      }));
    });
    console.log('EXPORT_BTNS=' + JSON.stringify(state));
    // try native click
    await browser.execute(() => {
      const b = document.querySelector('[data-testid="data-table-export"]') as HTMLElement | null;
      b?.click();
    });
    await browser.pause(1500);
    const dialog = await browser.execute(() => {
      const d = document.querySelector('[data-testid="data-export-dialog"]');
      return d ? { exists: true, text: (d.textContent || '').slice(0, 40) } : { exists: false };
    });
    console.log('DIALOG=' + JSON.stringify(dialog));
  });
});
