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
import workflows from './workflows';
import dashboard from './dashboard';
import mcp from './mcp';
import onboarding from './onboarding';

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
  ...workflows,
  ...dashboard,
  ...mcp,
  ...onboarding,
};

export default en;
