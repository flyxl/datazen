/** Full zh-CN dictionary (eager + lazy) for types, tests, and parity checks. */
import core from './core';
import connection from './connection';
import schema from './schema';
import query from './query';
import settings from './settings';
import chart from './chart';
import backup from './backup';
import ai from './ai';
import sync from './sync';
import transfer from './transfer';
import schemaDiff from './schemaDiff';
import workflows from './workflows';
import dashboard from './dashboard';
import mcp from './mcp';

const zhCN = {
  ...core,
  ...connection,
  ...schema,
  ...query,
  ...settings,
  ...chart,
  ...backup,
  ...ai,
  ...sync,
  ...transfer,
  ...schemaDiff,
  ...workflows,
  ...dashboard,
  ...mcp,
} as const;

export default zhCN;
export type TranslationKey = keyof typeof zhCN;
