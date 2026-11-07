/// <reference types="vite/client" />

// Form-split F2 (ruling #5): the Tauri CLI injects TAURI_ENV_* into the vite
// dev/build process; `envPrefix` in vite.config.ts exposes them here. They are
// the compile-time form determination — plain `vite dev` leaves them undefined.
interface ImportMetaEnv {
  readonly TAURI_ENV_PLATFORM?: string;
  readonly TAURI_ENV_ARCH?: string;
  readonly TAURI_ENV_FAMILY?: string;
  readonly TAURI_ENV_PLATFORM_VERSION?: string;
  readonly TAURI_ENV_DEBUG?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
