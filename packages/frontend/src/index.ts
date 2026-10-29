/**
 * @datazen/frontend — package entry (form-split F6).
 *
 * The desktop app consumes the shared frontend through deep subpaths
 * (`@datazen/frontend/src/...`) so the assembly root can reach stores, libs
 * and components directly. This entry re-exports the shared platform surface
 * for consumers that prefer the bare specifier, and exists so the package
 * manifest's `main`/`types`/`exports` "." entry points at a real file.
 */
export { DESKTOP_CAPABILITIES, WEB_CAPABILITIES } from './platform/capabilities';
export { IS_DESKTOP_BUILD } from './platform/env';
