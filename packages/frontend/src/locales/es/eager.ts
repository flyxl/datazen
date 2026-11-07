/** Eager domain packs for es (always in main chunk). */
import core from './core';
import connection from './connection';
import schema from './schema';
import query from './query';
import settings from './settings';
import chart from './chart';
import backup from './backup';
import ai from './ai';
import onboarding from './onboarding';

const eager = {
  ...core,
  ...connection,
  ...schema,
  ...query,
  ...settings,
  ...chart,
  ...backup,
  ...ai,
  ...onboarding,
} as const;

export default eager;
