/**
 * Thin re-export shim (form-split F2). The implementation moved to
 * `src/platform/desktop/eventChannel.ts` (the desktop `EventChannel` port);
 * sanctioned bridge (gate R7), dies in F6.
 */
export { emitCrossWindow, listenCrossWindow } from '../platform/desktop/eventChannel';
