import { browser, $ } from '@wdio/globals';
import { connectSeededPgInWorkspace } from '../helpers.js';

describe('debug', () => {
  it('find all testids', async () => {
    await connectSeededPgInWorkspace();
    await browser.pause(1000);
    const qa = await $('[data-testid="home-quick-new-query"]');
    if (await qa.isExisting()) await qa.click();
    await browser.pause(3000);

    const info = await browser.execute(() => {
      const allTestIds = Array.from(document.querySelectorAll('[data-testid]')).map((el) => ({
        testId: el.getAttribute('data-testid'),
        tag: el.tagName.toLowerCase(),
        text: el.textContent?.trim().substring(0, 60),
      }));
      return allTestIds;
    });
    console.log('DEBUG testids:', JSON.stringify(info, null, 2));
  });
});
