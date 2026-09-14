/**
 * Editor Pro Features Screenshot Generator.
 *
 * Captures marketing-quality screenshots of each SQL Editor Pro feature:
 *   1. Statement gutter (multi-statement with run buttons)
 *   2. Paste-as-IN
 *   3. Alt+Enter intentions (quick-fix menu)
 *   4. Transaction controls
 *   5. NL2SQL (AI chat → insert SQL)
 *   6. Autocomplete (table/column completion)
 *   7. Hover tooltip (table schema card)
 *   8. Signature help (function parameter hints)
 *   9. SQL linter feedback
 *  10. Format SQL
 *
 * Outputs to site/assets/screenshots/
 */
import { browser, $ } from '@wdio/globals';
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';

const ROOT = path.resolve(path.dirname(url.fileURLToPath(import.meta.url)), '..', '..');
const OUT = path.join(ROOT, 'site', 'assets', 'screenshots');

async function invoke<T = unknown>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  ) as Promise<T>;
}

/** Get the CodeMirror EditorView from the DOM. */
async function getCmView() {
  return browser.execute(() => {
    const el = document.querySelector('[data-testid="sql-editor"]');
    return (el as any)?.__cmView ?? (el as any)?.cmView?.view ?? null;
  });
}

/** Set editor content via CM dispatch. */
async function setEditorContent(sql: string) {
  await browser.execute((s: string) => {
    const el = document.querySelector('[data-testid="sql-editor"]');
    const view = (el as any)?.__cmView ?? (el as any)?.cmView?.view;
    if (!view) return;
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: s },
    });
  }, sql);
  await browser.pause(300);
}

/** Set cursor position (0-based offset from doc start). Clamps to doc length. */
async function setCursor(pos: number) {
  await browser.execute((p: number) => {
    const el = document.querySelector('[data-testid="sql-editor"]');
    const view = (el as any)?.__cmView ?? (el as any)?.cmView?.view;
    if (!view) return;
    try {
      const clamped = Math.min(p, view.state.doc.length);
      view.dispatch({ selection: { anchor: clamped } });
    } catch {
      /* ignore selection errors */
    }
  }, pos);
  await browser.pause(200);
}

/** Dispatch a keyboard event to the editor. */
async function pressKey(key: string, mods: string[] = []) {
  await browser.execute(
    (k: string, m: string[]) => {
      const el = document.querySelector('[data-testid="sql-editor-content"]');
      if (!el) return;
      const opts: KeyboardEventInit = {
        key: k,
        code: k === ' ' ? 'Space' : `Key${k.toUpperCase()}`,
        bubbles: true,
        cancelable: true,
      };
      if (m.includes('ctrl') || m.includes('meta')) {
        opts.ctrlKey = true;
      }
      if (m.includes('alt')) {
        opts.altKey = true;
      }
      el.dispatchEvent(new KeyboardEvent('keydown', opts));
      el.dispatchEvent(new KeyboardEvent('keyup', opts));
    },
    key,
    mods,
  );
  await browser.pause(500);
}

async function shot(name: string, settleMs = 1200) {
  await browser.pause(settleMs);
  fs.mkdirSync(OUT, { recursive: true });
  const buf = Buffer.from(await browser.takeScreenshot(), 'base64');
  fs.writeFileSync(path.join(OUT, name), buf);
  console.log(`📸 ${name} (${buf.length} bytes)`);
}

async function clickTestId(id: string) {
  const el = await $(`[data-testid="${id}"]`);
  await el.waitForDisplayed({ timeout: 10000 });
  await el.click();
  await browser.pause(300);
}

async function seedAiConfig() {
  const key = process.env.E2E_AI_API_KEY;
  if (!key) {
    console.warn('⚠️  E2E_AI_API_KEY not set — NL2SQL screenshot will show unconfigured state');
    return false;
  }
  await invoke('ai_save_config', {
    config: {
      providerType: process.env.E2E_AI_PROVIDER || 'open_ai',
      endpoint: process.env.E2E_AI_ENDPOINT || '',
      apiKey: key,
      model: process.env.E2E_AI_MODEL || '',
      extra: process.env.E2E_AI_PROTOCOL ? { protocol: process.env.E2E_AI_PROTOCOL } : null,
    },
  });
  return true;
}

async function waitForResults(timeoutMs = 30000) {
  const startSeq = await browser.execute(() => {
    const el = document.querySelector('[data-execution-seq]');
    return el ? parseInt(el.getAttribute('data-execution-seq') || '0', 10) : 0;
  });
  await browser.waitUntil(
    async () => {
      const cur = await browser.execute(() => {
        const el = document.querySelector('[data-execution-seq]');
        return el ? parseInt(el.getAttribute('data-execution-seq') || '0', 10) : 0;
      });
      if (cur > startSeq) {
        return browser.execute(() => {
          const t = document.body.textContent || '';
          return /\d+\s*(行|rows?|records?)\b/i.test(t);
        });
      }
      return false;
    },
    { timeout: timeoutMs, timeoutMsg: `Results did not appear within ${timeoutMs}ms` },
  );
}

describe('Editor Pro Feature Screenshots', () => {
  it('captures all Pro features', async function () {
    this.timeout(600000);

    // ── Setup ──
    await browser.url('tauri://localhost/window.html?window=onboarding');
    await $('[data-testid="onboarding-wizard"]').waitForDisplayed({ timeout: 30000 });
    const aiReady = await seedAiConfig();
    await browser.execute(() => {
      const el = document.documentElement;
      el.style.width = '2560px';
      el.style.height = '1648px';
    });
    // Set Tauri window size
    await invoke('set_size', {
      kind: 'main',
      value: { Logical: { width: 2560, height: 1648 } },
    }).catch(() => {});

    // Complete wizard
    await $('[data-testid="onboarding-entry-sample"]').waitForDisplayed({ timeout: 15000 });
    await browser.pause(1500);
    await $('[data-testid="onboarding-entry-sample"]').click();
    await $('[data-testid="onboarding-step-s1-sample"]').waitForDisplayed({ timeout: 15000 });
    await browser.pause(2000);
    await browser.waitUntil(
      async () =>
        (await $('[data-testid="onboarding-sample-path"]').isExisting()) ||
        (await $('[data-testid="onboarding-sample-error"]').isExisting()),
      { timeout: 30000, timeoutMsg: 'Sample data seeding timeout' },
    );
    await browser.pause(1000);
    const continueBtn = $('[data-testid="onboarding-continue"]');
    await browser.waitUntil(async () => await continueBtn.isEnabled(), { timeout: 10000 });
    await continueBtn.click();
    await $('[data-testid="onboarding-step-s2-ai"]').waitForDisplayed({ timeout: 15000 });
    await browser.pause(1500);
    await $('[data-testid="onboarding-skip"]').click();
    await $('[data-testid="onboarding-step-s3"]').waitForDisplayed({ timeout: 15000 });
    await browser.pause(1000);

    // Switch to main window
    const wizardHandle = await browser.getWindowHandle();
    await $('[data-testid="onboard-open-datazen"]').click();
    await browser.waitUntil(
      async () => {
        const handles = await browser.getWindowHandles().catch(() => [] as string[]);
        const next = handles.find((h) => h !== wizardHandle);
        if (!next) return false;
        await browser.switchToWindow(next).catch(() => {});
        return browser
          .execute(() => !!document.querySelector('[data-testid="workspace-nav-databases"]'))
          .catch(() => false);
      },
      { timeout: 30000, timeoutMsg: 'Main window did not appear' },
    );
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 15000 });
    await browser.pause(1500);

    // Ensure connected + open query tab
    await browser.execute(() => {
      const nav =
        document.querySelector('[data-testid="connection-navigator-aside"]') ??
        Array.from(document.querySelectorAll('aside')).find((a) =>
          a.querySelector('[data-conn-item]'),
        );
      const items = nav?.querySelectorAll('[data-conn-item]') ?? [];
      for (const item of items) {
        if (item.textContent?.includes('Sample Playground')) {
          (item as HTMLElement).dispatchEvent(
            new MouseEvent('dblclick', { bubbles: true, cancelable: true }),
          );
          return;
        }
      }
    });
    // Open query tab
    {
      const deadline = Date.now() + 60_000;
      let opened = false;
      while (Date.now() < deadline && !opened) {
        try {
          let btn = await $('[data-testid="conn-toolbar-new-query"]');
          if (!(await btn.isExisting()) || !(await btn.isDisplayed().catch(() => false))) {
            btn = await $('[data-testid="home-quick-new-query"]');
          }
          if (!(await btn.isExisting()) || !(await btn.isDisplayed().catch(() => false))) {
            await browser.pause(300);
            continue;
          }
          await btn.waitForClickable({ timeout: Math.max(1000, deadline - Date.now()) });
          await btn.click();
          await browser.pause(250);
          opened = await $('[data-testid="editor-execute-button"]')
            .isExisting()
            .catch(() => false);
        } catch {
          await browser.pause(300);
        }
      }
    }
    await $('[data-testid="editor-execute-button"]').waitForDisplayed({ timeout: 20000 });
    await browser.pause(1500);

    // ════════════════════════════════════════════════════════════════
    // 1. Statement Gutter — multi-statement with per-statement run
    // ════════════════════════════════════════════════════════════════
    await setEditorContent(
      `SELECT region, SUM(amount) AS total
FROM demo_sales
GROUP BY region;

SELECT quarter, COUNT(*) AS cnt
FROM demo_sales
GROUP BY quarter;

SELECT * FROM demo_sales LIMIT 5;`,
    );
    await browser.pause(800);
    await shot('pro-01-statement-gutter.png');

    // ════════════════════════════════════════════════════════════════
    // 2. Autocomplete — type table prefix
    // ════════════════════════════════════════════════════════════════
    await setEditorContent('SELECT d');
    await setCursor(7);
    await browser.pause(300);
    // Trigger autocomplete via Ctrl+Space
    await pressKey(' ', ['ctrl']);
    await browser
      .waitUntil(
        async () => {
          return browser.execute(() => {
            return !!document.querySelector('.cm-tooltip-autocomplete');
          });
        },
        { timeout: 8000, timeoutMsg: 'Autocomplete tooltip did not appear' },
      )
      .catch(() => {});
    await browser.pause(600);
    await shot('pro-02-autocomplete.png');

    // Close autocomplete by pressing Escape
    await browser.execute(() => {
      const el = document.querySelector('[data-testid="sql-editor-content"]');
      el?.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
      );
    });
    await browser.pause(500);

    // ════════════════════════════════════════════════════════════════
    // 3. Hover Tooltip — table schema card
    // ════════════════════════════════════════════════════════════════
    await setEditorContent('SELECT * FROM demo_sales;');
    await browser.pause(500);
    // Move cursor to "demo_sales" token and hover
    await browser.execute(() => {
      const content = document.querySelector('[data-testid="sql-editor-content"]');
      if (!content) return;
      // Find the "demo_sales" text node position
      const cm = document.querySelector('.cm-content');
      if (!cm) return;
      // Dispatch mousemove over the "demo_sales" token area
      const rect = cm.getBoundingClientRect();
      const ev = new MouseEvent('mousemove', {
        bubbles: true,
        clientX: rect.left + 120,
        clientY: rect.top + 5,
      });
      cm.dispatchEvent(ev);
    });
    // Wait for hover tooltip
    await browser
      .waitUntil(
        async () => {
          return browser.execute(() => {
            const tooltips = document.querySelectorAll('.cm-tooltip');
            for (const t of tooltips) {
              if (t.textContent?.includes('demo_sales') || t.textContent?.includes('region')) {
                return true;
              }
            }
            return false;
          });
        },
        { timeout: 8000, timeoutMsg: 'Hover tooltip did not appear' },
      )
      .catch(() => {});
    await browser.pause(800);
    await shot('pro-03-hover-tooltip.png');

    // ════════════════════════════════════════════════════════════════
    // 4. Signature Help — function with params
    // ════════════════════════════════════════════════════════════════
    await setEditorContent('SELECT COALESCE(');
    await setCursor(18);
    await browser.pause(300);
    // Type a character to trigger signature
    await browser.execute(() => {
      const el = document.querySelector('[data-testid="sql-editor-content"]');
      el?.dispatchEvent(
        new KeyboardEvent('keydown', {
          key: 'a',
          code: 'KeyA',
          bubbles: true,
          cancelable: true,
        }),
      );
    });
    await browser
      .waitUntil(
        async () => {
          return browser.execute(() => {
            const sig = document.querySelector('.cm-tooltip-signature');
            return !!sig && (sig.textContent || '').length > 0;
          });
        },
        { timeout: 8000, timeoutMsg: 'Signature help did not appear' },
      )
      .catch(() => {});
    await browser.pause(800);
    await shot('pro-04-signature-help.png');

    // ════════════════════════════════════════════════════════════════
    // 5. SQL Linter — syntax error feedback
    // ════════════════════════════════════════════════════════════════
    await setEditorContent('SELECT * FORM demo_sales;');
    await browser.pause(1500);
    await shot('pro-05-linter.png');

    // ════════════════════════════════════════════════════════════════
    // 6. Alt+Enter Intentions — quick fix menu
    // ════════════════════════════════════════════════════════════════
    // Position cursor on the error token "FORM"
    await setCursor(11);
    await browser.pause(300);
    await pressKey('Enter', ['alt']);
    await browser
      .waitUntil(
        async () => {
          return browser.execute(() => {
            // Check for intention/code-action menu
            const menus = document.querySelectorAll(
              '.cm-panels, .cm-tooltip-autocomplete, [class*="intention"], [class*="code-action"]',
            );
            for (const m of menus) {
              if (
                (m.textContent || '').includes('修复') ||
                (m.textContent || '').includes('Fix') ||
                (m.textContent || '').includes('SELECT')
              ) {
                return true;
              }
            }
            return false;
          });
        },
        { timeout: 8000, timeoutMsg: 'Intentions menu did not appear' },
      )
      .catch(() => {});
    await browser.pause(800);
    await shot('pro-06-intentions.png');

    // Close any open menus
    await browser.execute(() => {
      document
        .querySelector('[data-testid="sql-editor-content"]')
        ?.dispatchEvent(
          new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
        );
    });
    await browser.pause(500);

    // ════════════════════════════════════════════════════════════════
    // 7. Transaction Controls — visible in toolbar
    // ════════════════════════════════════════════════════════════════
    await setEditorContent('SELECT 1;');
    await browser.pause(500);
    // Look for transaction mode toggle (auto-commit / manual)
    await browser.execute(() => {
      const btns = document.querySelectorAll('button');
      for (const b of btns) {
        const t = b.textContent || '';
        if (/事务|Transaction|Auto.?Commit|手动|Manual/i.test(t)) {
          b.setAttribute('data-e2e-tx-btn', 'true');
        }
      }
    });
    const txBtn = await $('[data-e2e-tx-btn]');
    if (await txBtn.isExisting().catch(() => false)) {
      await txBtn.click();
      await browser.pause(600);
    }
    await shot('pro-07-transaction-controls.png');

    // ════════════════════════════════════════════════════════════════
    // 8. Paste-as-IN — paste comma-separated values
    // ════════════════════════════════════════════════════════════════
    await setEditorContent('SELECT * FROM demo_sales WHERE region IN ');
    await setCursor(44);
    // Simulate paste of comma-separated values
    await browser.execute(() => {
      const el = document.querySelector('[data-testid="sql-editor-content"]');
      if (!el) return;
      // Create a paste event with CSV data
      const dt = new DataTransfer();
      dt.setData('text/plain', 'East,West,North');
      const pasteEvent = new ClipboardEvent('paste', {
        bubbles: true,
        cancelable: true,
        clipboardData: dt,
      });
      el.dispatchEvent(pasteEvent);
    });
    await browser
      .waitUntil(
        async () => {
          return browser.execute(() => {
            // Check for paste-as-IN dialog/tooltip
            const dialogs = document.querySelectorAll(
              '[role="dialog"], .cm-tooltip, [class*="paste"]',
            );
            for (const d of dialogs) {
              const t = d.textContent || '';
              if (t.includes('IN') || t.includes('粘贴') || t.includes('Paste')) return true;
            }
            return false;
          });
        },
        { timeout: 6000, timeoutMsg: 'Paste-as-IN dialog did not appear' },
      )
      .catch(() => {});
    await browser.pause(800);
    await shot('pro-08-paste-as-in.png');

    // ════════════════════════════════════════════════════════════════
    // 9. Format SQL — formatted output
    // ════════════════════════════════════════════════════════════════
    await setEditorContent(
      'select a.region,a.amount,b.quarter from demo_sales a join demo_sales b on a.region=b.region where a.amount>100 order by a.amount desc;',
    );
    await browser.pause(500);
    // Click format button if available
    await browser.execute(() => {
      const btns = document.querySelectorAll('button');
      for (const b of btns) {
        const label = b.getAttribute('aria-label') || b.textContent || '';
        if (/格式化|Format|format/i.test(label)) {
          b.setAttribute('data-e2e-format-btn', 'true');
        }
      }
    });
    const fmtBtn = await $('[data-e2e-format-btn]');
    if (await fmtBtn.isExisting().catch(() => false)) {
      await fmtBtn.click();
      await browser.pause(800);
    }
    await shot('pro-09-format-sql.png');

    // ════════════════════════════════════════════════════════════════
    // 10. NL2SQL — AI chat natural language to SQL
    // ════════════════════════════════════════════════════════════════
    if (aiReady) {
      await clickTestId('conn-toolbar-ai');
      await browser.pause(1000);
      // Type NL2SQL prompt
      await browser.execute((q: string) => {
        const field = document.querySelector('[data-testid="ai-input-field"] textarea');
        const ta = field as HTMLTextAreaElement | null;
        if (ta) {
          const nativeSetter = Object.getOwnPropertyDescriptor(
            window.HTMLTextAreaElement.prototype,
            'value',
          )?.set;
          nativeSetter?.call(ta, q);
          ta.dispatchEvent(new Event('input', { bubbles: true }));
        }
      }, '查询每个地区的销售总额，按金额降序排列');
      await browser.pause(500);
      // Submit
      await browser.execute(() => {
        const field = document.querySelector('[data-testid="ai-input-field"] textarea');
        if (field) {
          field.dispatchEvent(
            new KeyboardEvent('keydown', {
              key: 'Enter',
              bubbles: true,
              cancelable: true,
            }),
          );
        }
      });
      // Wait for response with code block
      await browser.waitUntil(
        async () => {
          return browser.execute(() => {
            return (
              document.querySelectorAll('[data-testid="ai-code-block"]').length > 0 ||
              document.querySelectorAll('.bg-surface-alt pre').length > 0
            );
          });
        },
        { timeout: 120000, timeoutMsg: 'NL2SQL response did not appear' },
      );
      await browser.pause(2000);
      await shot('pro-10-nl2sql.png');
      // Close AI drawer
      await clickTestId('conn-toolbar-ai').catch(() => {});
      await browser.pause(500);
    } else {
      // Show AI not configured state
      await clickTestId('conn-toolbar-ai');
      await browser.pause(1000);
      await shot('pro-10-nl2sql.png');
      await clickTestId('conn-toolbar-ai').catch(() => {});
      await browser.pause(500);
    }

    // ── Final: clean editor state ──
    await setEditorContent(
      'SELECT region, SUM(amount) AS total\nFROM demo_sales\nGROUP BY region\nORDER BY total DESC;',
    );
    await browser.pause(500);

    console.log('✅ All Editor Pro screenshots captured');
  });
});
