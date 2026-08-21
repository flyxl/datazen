/**
 * SQL Server driver E2E smoke (optional).
 *
 * Skips unless E2E_SQLSERVER_USER is set and host:port is reachable.
 * See README.md in this directory.
 */
import { createConnection } from 'node:net';
import { expect } from '@wdio/globals';

const HOST = process.env.E2E_SQLSERVER_HOST || '127.0.0.1';
const PORT = Number(process.env.E2E_SQLSERVER_PORT || '1433');

function skipRequested(): boolean {
  return process.env.E2E_SKIP_SQLSERVER === '1';
}

async function sqlServerReachable(timeoutMs = 2000): Promise<boolean> {
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

describe('SQL Server driver smoke', () => {
  before(async function () {
    if (skipRequested()) {
      console.warn('⏩ Skipping SQL Server E2E: E2E_SKIP_SQLSERVER=1');
      this.skip();
    }
    if (!process.env.E2E_SQLSERVER_USER?.trim()) {
      console.warn(
        '⏩ Skipping SQL Server E2E: set E2E_SQLSERVER_USER (see packages/drivers/sqlserver/e2e/README.md)',
      );
      this.skip();
    }
    if (!(await sqlServerReachable())) {
      console.warn(`⏩ Skipping SQL Server E2E: ${HOST}:${PORT} unreachable`);
      this.skip();
    }
  });

  it('placeholder — extend when CI fixture exists', () => {
    expect(process.env.E2E_SQLSERVER_USER).toBeTruthy();
  });
});
