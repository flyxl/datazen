/**
 * ClickHouse driver E2E smoke (optional).
 *
 * Skips unless E2E_CLICKHOUSE_HOST is set or default localhost is reachable.
 * See README.md in this directory.
 */
import { createConnection } from 'node:net';
import { expect } from '@wdio/globals';

const HOST = process.env.E2E_CLICKHOUSE_HOST || '127.0.0.1';
const PORT = Number(process.env.E2E_CLICKHOUSE_PORT || '8123');

function skipRequested(): boolean {
  return process.env.E2E_SKIP_CLICKHOUSE === '1';
}

async function clickhouseReachable(timeoutMs = 2000): Promise<boolean> {
  return new Promise((resolve) => {
    const sock = createConnection({ host: HOST, port: PORT });
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

describe('ClickHouse driver smoke', () => {
  before(async function () {
    if (skipRequested()) {
      console.warn('⏩ Skipping ClickHouse E2E: E2E_SKIP_CLICKHOUSE=1');
      this.skip();
    }
    if (!(await clickhouseReachable())) {
      console.warn(`⏩ Skipping ClickHouse E2E: ${HOST}:${PORT} unreachable`);
      this.skip();
    }
  });

  it('placeholder — extend when CI fixture exists', () => {
    expect(HOST).toBeTruthy();
  });
});
