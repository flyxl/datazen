import { createIconResolver, setActiveIconResolver } from './iconResolver';
import { getDbIcon, getDriverIconMap } from './databaseTypes';
import { buildHostLucideById } from './hostLucideMap';
import type { DatabaseType } from '../types';

/** Seed the module-level icon resolver with built-in driver SVGs (no theme pack). */
export function bootstrapDefaultIconResolver(): void {
  setActiveIconResolver(
    createIconResolver({
      packIcons: {},
      driverIcons: getDriverIconMap(),
      lucideById: buildHostLucideById(),
      placeholderForDb: (dbType) => {
        const { label, bg } = getDbIcon(dbType as DatabaseType);
        return { label, bgClass: bg };
      },
    }),
  );
}
