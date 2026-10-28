/**
 * Thin re-export shim (form-split F2). The implementation moved to
 * `src/platform/desktop/os.ts`; sanctioned bridge (gate R7), dies in F6.
 */
export { usePlatform, getPlatformSync, type Platform } from '../platform/desktop/os';
