/** Full ja dictionary (all domains merged). */
import ai from './ai';
import backup from './backup';
import chart from './chart';
import connection from './connection';
import core from './core';
import dashboard from './dashboard';
import mcp from './mcp';
import query from './query';
import schema from './schema';
import schemaDiff from './schemaDiff';
import settings from './settings';
import sync from './sync';
import transfer from './transfer';
import workflows from './workflows';

const locale = {
  ...ai,
  ...backup,
  ...chart,
  ...connection,
  ...core,
  ...dashboard,
  ...mcp,
  ...query,
  ...schema,
  ...schemaDiff,
  ...settings,
  ...sync,
  ...transfer,
  ...workflows,
} as const;

export default locale;
export type TranslationKey = keyof typeof locale;
