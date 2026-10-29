/** Full ja dictionary (all domains merged). */
import ai from './ai';
import backup from './backup';
import chart from './chart';
import connection from './connection';
import core from './core';
import dashboard from './dashboard';
import mcp from './mcp';
import onboarding from './onboarding';
import query from './query';
import schema from './schema';
import settings from './settings';
import sync from './sync';
import workflows from './workflows';

const locale = {
  ...ai,
  ...backup,
  ...chart,
  ...connection,
  ...core,
  ...dashboard,
  ...mcp,
  ...onboarding,
  ...query,
  ...schema,
  ...settings,
  ...sync,
  ...workflows,
} as const;

export default locale;
export type TranslationKey = keyof typeof locale;
