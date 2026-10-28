/**
 * Thin re-export shim (form-split F2). The transport implementation moved to
 * `src/platform/desktop/tauriBackendTransport.ts`; the dialog/clipboard port
 * implementation lives in `src/platform/desktop/platformServices.ts`.
 * Sanctioned bridge (gate R7), dies in F6.
 */
export {
  bindDesktopBackend,
  createDesktopBackendTransport,
  DESKTOP_BACKEND_ID,
} from './desktop/tauriBackendTransport';
export { createDesktopPlatformServices } from './desktop/platformServices';
