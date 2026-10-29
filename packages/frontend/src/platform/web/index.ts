/**
 * `platform/web` — the web form zone (form-split F5 stub).
 *
 * Today it holds the compile-time same-shape stub (`./stub`); the future web
 * assembly root grows here beside it, mirroring `platform/desktop`.
 */

export {
  assembleWebPlatformStub,
  createWebBackendTransportStub,
  createWebEventChannel,
  createWebPlatformServices,
  createWebWindowRouter,
} from './stub';
