#!/usr/bin/env node
/**
 * Install the durable MySQL Schema Tree fixture used for manual testing.
 *
 * Reads connection values only from the caller's E2E_MYSQL_* environment;
 * it does not load dotenv files or print connection settings. The E2E runner
 * already exports those variables from its configured environment.
 */
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const host = process.env.E2E_MYSQL_HOST || '127.0.0.1';
const port = process.env.E2E_MYSQL_PORT || '3306';
const user = process.env.E2E_MYSQL_USER || 'root';
const password = process.env.E2E_MYSQL_PASSWORD || '';
const mysqlCli = process.env.MYSQL_CLI || 'mysql';
const sqlPath = fileURLToPath(new URL('./manual-schema-tree.sql', import.meta.url));
const sql = readFileSync(sqlPath, 'utf8');

const result = spawnSync(
  mysqlCli,
  [
    '--protocol=TCP',
    '--host',
    host,
    '--port',
    port,
    '--user',
    user,
    '--default-character-set=utf8mb4',
  ],
  {
    input: sql,
    encoding: 'utf8',
    env: { ...process.env, MYSQL_PWD: password },
  },
);

if (result.error || result.status !== 0) {
  const reason = result.error
    ? 'mysql client could not be started'
    : 'mysql client returned a failure';
  console.error(`[mysql-schema-tree] ${reason}; diagnostic output suppressed`);
  process.exitCode = 1;
} else {
  console.log(
    '[mysql-schema-tree] durable manual fixture is ready in datazen_manual_schema_tree',
  );
}
