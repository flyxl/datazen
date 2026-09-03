/** Eager domain packs for fr (always in main chunk). */
import core from './core';
import connection from './connection';
import schema from './schema';
import query from './query';
import settings from './settings';
import chart from './chart';
import backup from './backup';
import ai from './ai';

const eager = {
  ...core,
  ...connection,
  ...schema,
  ...query,
  ...settings,
  ...chart,
  ...backup,
  ...ai,
} as const;

export default eager;
