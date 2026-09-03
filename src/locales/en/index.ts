import type { TranslationKey } from '../zh-CN';
/** Full en dictionary (eager + lazy). */
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

const en: Record<TranslationKey, string> = {
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
};

export default en;
