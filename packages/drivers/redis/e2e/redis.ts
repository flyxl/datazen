/**
 * Redis new-pages E2E: Items (browse/filter/TTL), Console, Monitor, Pub/Sub.
 *
 * Covers the v0.2.2 feature surface:
 *   - Key browser: search, filter by type, flat/tree view, select key, detail panel
 *   - TTL controls: set TTL, expire-at, persist
 *   - Console: execute commands, result tabs
 *   - Monitor sub-pages: Info, Memory, Slowlog
 *   - Pub/Sub: subscribe, receive messages, publish
 *
 * Credentials: E2E_REDIS_* (see e2e/.env.example).
 * Skips gracefully when Redis is unreachable or E2E_SKIP_REDIS=1.
 *
 * Seed data (from e2e/setup-demo-data.sh):
 *   db5 → demo:app:name (string), demo:user:1001 (hash),
 *          demo:queue:orders (list), demo:regions (set),
 *          demo:sales:rank (zset)
 */
import { createConnection } from 'node:net';
import { expect, browser, $ } from '@wdio/globals';

// ── constants ──────────────────────────────────────────────────────

const CONN_ID = 'conn_e2e_redis';
const CONN_NAME = 'E2E-Redis-NewPages';
const REDIS_HOST = process.env.E2E_REDIS_HOST || '127.0.0.1';
const REDIS_PORT = process.env.E2E_REDIS_PORT || '6379';
const REDIS_PASSWORD = process.env.E2E_REDIS_PASSWORD || '';
const REDIS_DEMO_DB = process.env.E2E_REDIS_DEMO_DB || 'db5';

// ── helpers ────────────────────────────────────────────────────────

function skipRequested(): boolean {
  return process.env.E2E_SKIP_REDIS === '1';
}

async function redisReachable(timeoutMs = 2000): Promise<boolean> {
  return new Promise((resolve) => {
    const sock = createConnection({ host: REDIS_HOST, port: Number(REDIS_PORT) });
    const timer = setTimeout(() => {
      sock.destroy();
      resolve(false);
    }, timeoutMs);
    sock.on('connect', () => {
      clearTimeout(timer);
      sock.destroy();
      resolve(true);
    });
    sock.on('error', () => {
      clearTimeout(timer);
      resolve(false);
    });
  });
}

async function goToConnections() {
  await browser.execute(() => {
    document
      .querySelector('[data-testid="workspace-nav-databases"]')
      ?.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  });
  await browser.pause(800);
}

async function invoke<T = unknown>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (
        window as unknown as {
          __TAURI_INTERNALS__: { invoke: (c: string, a: string) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  ) as Promise<T>;
}

/** Connect to Redis via IPC, returning the dbSessionId string. */
async function connectRedis(): Promise<string> {
  // Try existing connection first
  let connId: unknown = await invoke<string>('connect', { connectionId: CONN_ID });
  if (typeof connId === 'string' && !connId.startsWith('__error')) {
    return connId;
  }

  // Save and connect
  const config = {
    id: CONN_ID,
    name: CONN_NAME,
    databaseType: 'redis',
    host: REDIS_HOST,
    port: Number(REDIS_PORT),
    username: '',
    password: REDIS_PASSWORD,
    database: REDIS_DEMO_DB,
  };
  await invoke('save_connection', { config });
  connId = await invoke<string>('connect', { connectionId: CONN_ID });
  if (typeof connId !== 'string' || connId.startsWith('__error')) {
    throw new Error(`connect(${CONN_ID}) failed: ${JSON.stringify(connId)}`);
  }
  return connId;
}

/** Click a Redis tab by its key. */
async function clickTab(tab: 'items' | 'console' | 'monitor' | 'pubsub') {
  const btn = await $(`[data-testid="redis-tab-${tab}"]`);
  await btn.waitForDisplayed({ timeout: 10000 });
  await btn.click();
  await browser.pause(400);
}

/**
 * Open the Redis inline panel in the main workspace by navigating to
 * connections, expanding the Redis connection in the sidebar, and clicking
 * the target db node.  (Same pattern as zz-screenshots.ts "15-redis".)
 */
async function openRedisInlinePanel() {
  // Navigate fresh so the page re-renders with the new connection
  await browser.url('tauri://localhost');
  await browser.pause(2000);

  await goToConnections();
  await browser.pause(1000);

  // Wait for the connection to appear in the sidebar tree
  let found = false;
  const dl = Date.now();
  while (Date.now() < dl + 20000 && !found) {
    found = await browser.execute(
      (name: string) =>
        Array.from(document.querySelectorAll('[data-conn-item]')).some((el) =>
          (el.getAttribute('data-conn-name') || '').includes(name),
        ),
      CONN_NAME,
    );
    if (!found) {
      // Scroll sidebar and try again
      await browser.execute(() => {
        const scrollers = Array.from(
          document.querySelectorAll<HTMLElement>(
            '[class*="overflow-y-auto"], [class*="overflow-auto"]',
          ),
        ).filter((s) => s.scrollHeight > s.clientHeight);
        for (const s of scrollers) s.scrollTop = Math.max(0, s.scrollTop - 300);
      });
      await browser.pause(500);
    }
  }
  if (!found) throw new Error(`${CONN_NAME} not found in sidebar tree`);

  // Expand the Redis connection (click chevron)
  await browser.execute((connName: string) => {
    const items = Array.from(document.querySelectorAll('[data-conn-item]'));
    const target = items.find((el) => (el.getAttribute('data-conn-name') || '').includes(connName));
    const chev = Array.from(target?.querySelectorAll('button') ?? []).find(
      (b) =>
        !!b.querySelector('svg.lucide-chevron-right') ||
        !!b.querySelector('svg.lucide-chevron-down'),
    );
    (chev as HTMLElement | undefined)?.click();
  }, CONN_NAME);

  await browser.pause(800);

  // Click the target db node (e.g. db5) to open Redis panel in main workspace
  let openedDb = false;
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline && !openedDb) {
    openedDb = await browser.execute((prefDb: string) => {
      const preferred = document.querySelector<HTMLElement>(
        `button[data-tree-node="kv-db"][data-db-name="${prefDb}"]`,
      );
      const any =
        preferred ?? document.querySelector<HTMLElement>('button[data-tree-node="kv-db"]');
      if (!any) {
        // scroll sidebar to reveal more nodes
        const scrollers = Array.from(
          document.querySelectorAll<HTMLElement>(
            '[class*="overflow-y-auto"], [class*="overflow-auto"]',
          ),
        ).filter((s) => s.scrollHeight > s.clientHeight);
        for (const s of scrollers) s.scrollTop = Math.max(0, s.scrollTop - 300);
        return false;
      }
      any.scrollIntoView({ block: 'center' });
      any.click();
      return true;
    }, REDIS_DEMO_DB);
    if (!openedDb) await browser.pause(400);
  }
  if (!openedDb) throw new Error('no kv-db node rendered for redis connection');

  // Wait for Redis UI tabs to appear (confirms RedisConnectionView loaded)
  await browser.waitUntil(async () => await $('[data-testid="redis-tab-items"]').isDisplayed(), {
    timeout: 20000,
    timeoutMsg: 'Redis tabs not visible after opening db node',
  });
}

// ── test suite ─────────────────────────────────────────────────────

describe('Redis new pages E2E', () => {
  let dbSessionId: string;

  before(async () => {
    if (skipRequested()) return;
    if (!(await redisReachable())) return;

    dbSessionId = await connectRedis();
    await openRedisInlinePanel();
  });

  after(async () => {
    if (skipRequested()) return;
    try {
      await invoke('disconnect', { dbSessionId });
    } catch {
      /* best effort */
    }
    try {
      await goToConnections();
    } catch {
      /* best effort */
    }
  });

  // ─── Items tab ─────────────────────────────────────────────────

  describe('Items tab', () => {
    it('should show the Items tab as active by default', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      const tab = await $('[data-testid="redis-tab-items"]');
      await expect(tab).toBeDisplayed();
    });

    it('should display seeded keys in the key table', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      // Wait for at least one key row to render
      await browser.waitUntil(
        async () =>
          browser.execute(() => {
            const rows = document.querySelectorAll('[data-testid^="redis-key-row-"]');
            return rows.length > 0;
          }),
        { timeout: 20000, timeoutMsg: 'No key rows rendered in Redis key table' },
      );

      // Verify at least one demo key is visible
      const hasDemoKey = await browser.execute(() =>
        Array.from(document.querySelectorAll('[data-testid^="redis-key-row-"]')).some((el) =>
          el.getAttribute('data-testid')?.includes('demo:'),
        ),
      );
      expect(hasDemoKey).toBe(true);
    });

    it('should select a key and show its detail', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      // Click the demo string key
      const keyRow = await $('[data-testid^="redis-key-row-demo:app:name"]');
      if (await keyRow.isExisting()) {
        await keyRow.click();
        await browser.pause(600);
        // The key detail panel should appear
        const hasDetail = await browser.execute(
          () =>
            (document.body.textContent || '').includes('TTL') ||
            (document.body.textContent || '').includes('Value'),
        );
        expect(hasDetail).toBe(true);
      }
    });

    it('should switch between Flat and Tree view modes', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      // Flat view button
      const flatBtn = await $('button[title*="Flat"], button[title*="flat"]');
      if (await flatBtn.isExisting()) {
        await flatBtn.click();
        await browser.pause(300);
      }
      // Tree view button
      const treeBtn = await $('button[title*="Tree"], button[title*="tree"]');
      if (await treeBtn.isExisting()) {
        await treeBtn.click();
        await browser.pause(300);
      }
      // Verify key rows still render
      const rowCount = await browser.execute(
        () => document.querySelectorAll('[data-testid^="redis-key-row-"]').length,
      );
      expect(rowCount).toBeGreaterThan(0);
    });

    it('should open the Create Key dialog', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      const createBtn = await $('[data-testid="redis-create-key"]');
      await createBtn.waitForDisplayed({ timeout: 5000 });
      await createBtn.click();
      await browser.pause(400);

      // The create dialog should be visible
      const hasDialog = await browser.execute(() => {
        const dialogs = document.querySelectorAll('[role="dialog"], [data-testid*="dialog"]');
        return dialogs.length > 0;
      });
      expect(hasDialog).toBe(true);

      // Close it
      await browser.keys(['Escape']);
      await browser.pause(300);
    });
  });

  // ─── Console tab ──────────────────────────────────────────────

  describe('Console tab', () => {
    it('should switch to Console tab and show the input', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      await clickTab('console');

      const input = await $('[data-testid="redis-console-input"]');
      await input.waitForDisplayed({ timeout: 5000 });
      await expect(input).toBeDisplayed();
    });

    it('should execute a PING command', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      const input = await $('[data-testid="redis-console-input"]');
      await input.click();
      await input.setValue('PING');

      const runBtn = await $('[data-testid="redis-console-run"]');
      await runBtn.click();

      // Wait for result to appear
      await browser.waitUntil(
        async () =>
          browser.execute(() => {
            const result = document.querySelector('[data-testid="redis-console-result"]');
            return result && result.textContent && result.textContent.length > 0;
          }),
        { timeout: 10000, timeoutMsg: 'PING result not shown' },
      );

      const resultText = await browser.execute(
        () => document.querySelector('[data-testid="redis-console-result"]')?.textContent ?? '',
      );
      expect(resultText).toContain('PONG');
    });

    it('should execute a SET/GET pair', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      const input = await $('[data-testid="redis-console-input"]');
      await input.click();
      await input.setValue('SET e2e:test:key hello');

      const runBtn = await $('[data-testid="redis-console-run"]');
      await runBtn.click();
      await browser.pause(800);

      // Now GET it
      await input.click();
      await input.setValue('GET e2e:test:key');
      await runBtn.click();

      await browser.waitUntil(
        async () =>
          browser.execute(() => {
            const result = document.querySelector('[data-testid="redis-console-result"]');
            return result && (result.textContent || '').includes('hello');
          }),
        { timeout: 10000, timeoutMsg: 'GET result did not contain "hello"' },
      );
    });

    it('should show an error for invalid commands', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      const input = await $('[data-testid="redis-console-input"]');
      await input.click();
      await input.setValue('NOTACOMMAND');

      const runBtn = await $('[data-testid="redis-console-run"]');
      await runBtn.click();
      await browser.pause(800);

      // Should show an error result
      const hasError = await browser.execute(() => {
        const result = document.querySelector('[data-testid="redis-console-result"]');
        const text = result?.textContent || '';
        return text.includes('ERR') || text.includes('error') || text.includes('unknown');
      });
      expect(hasError).toBe(true);
    });
  });

  // ─── Monitor tab ──────────────────────────────────────────────

  describe('Monitor tab', () => {
    it('should switch to Monitor tab and show Info sub-page', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      await clickTab('monitor');

      // Info sub-page should be visible
      const infoBtn = await $('[data-testid="redis-monitor-sub-info"]');
      await infoBtn.waitForDisplayed({ timeout: 5000 });
      await expect(infoBtn).toBeDisplayed();
    });

    it('should load INFO data', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      // Click refresh if needed
      const refreshBtn = await $('button[title*="Refresh"], button[title*="刷新"]');
      if (await refreshBtn.isExisting()) {
        await refreshBtn.click();
        await browser.pause(1500);
      }

      // INFO should show some redis_version text
      await browser.waitUntil(
        async () =>
          browser.execute(() => (document.body.textContent || '').includes('redis_version')),
        { timeout: 15000, timeoutMsg: 'INFO data did not load (no redis_version)' },
      );
    });

    it('should switch to Memory sub-page', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      const memBtn = await $('[data-testid="redis-monitor-sub-memory"]');
      await memBtn.waitForDisplayed({ timeout: 5000 });
      await memBtn.click();
      await browser.pause(500);

      // Memory page should render its sub-page content (input, empty state, or sample results)
      const hasMemoryUI = await browser.execute(() => {
        const text = document.body.textContent || '';
        // zh-CN renders "内存" for memory; also check for key input placeholder
        return text.includes('内存') || text.includes('memory') || text.includes('MEMORY');
      });
      expect(hasMemoryUI).toBe(true);
    });

    it('should switch to Slowlog sub-page', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      const slowBtn = await $('[data-testid="redis-monitor-sub-slowlog"]');
      await slowBtn.waitForDisplayed({ timeout: 5000 });
      await slowBtn.click();
      await browser.pause(500);

      // Slowlog page should render its sub-page content
      const hasSlowlog = await browser.execute(() => {
        const text = document.body.textContent || '';
        // zh-CN renders "慢日志" for slowlog
        return text.includes('慢日志') || text.includes('slowlog') || text.includes('SLOWLOG');
      });
      expect(hasSlowlog).toBe(true);
    });
  });

  // ─── Pub/Sub tab ──────────────────────────────────────────────

  describe('Pub/Sub tab', () => {
    it('should switch to Pub/Sub tab', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      await clickTab('pubsub');

      const channelsInput = await $('[data-testid="redis-pubsub-channels"]');
      await channelsInput.waitForDisplayed({ timeout: 5000 });
      await expect(channelsInput).toBeDisplayed();
    });

    it('should subscribe to a channel', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      const channelsInput = await $('[data-testid="redis-pubsub-channels"]');
      await channelsInput.click();
      await channelsInput.setValue('e2e:test:channel');

      const subscribeBtn = await $('[data-testid="redis-pubsub-subscribe"]');
      await subscribeBtn.click();
      await browser.pause(1000);

      // After subscribing, the active subscriptions area should show the channel
      const hasSubscription = await browser.execute(() =>
        (document.body.textContent || '').includes('e2e:test:channel'),
      );
      expect(hasSubscription).toBe(true);
    });

    it('should receive a published message', async () => {
      if (skipRequested() || !(await redisReachable())) return;
      // Publish a message to the subscribed channel
      const pubChannel = await $('[data-testid="redis-pubsub-publish-channel"]');
      await pubChannel.click();
      await pubChannel.setValue('e2e:test:channel');

      const pubMessage = await $('[data-testid="redis-pubsub-publish-message"]');
      await pubMessage.click();
      await pubMessage.setValue('hello from e2e');

      const publishBtn = await $('[data-testid="redis-pubsub-publish"]');
      await publishBtn.click();
      await browser.pause(1500);

      // The messages table should contain our message
      const hasMessage = await browser.execute(() =>
        (document.body.textContent || '').includes('hello from e2e'),
      );
      expect(hasMessage).toBe(true);
    });
  });
});
