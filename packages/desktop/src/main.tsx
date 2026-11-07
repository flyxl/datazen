/**
 * Desktop entry (form-split F6 / host-boundary follow-up F2).
 *
 * The shared startup chain (marks, CSS, PRO host-module table, E2E store
 * bridge, locale / icon / scroll-lock / PRO init, settings preload, React
 * root) lives in `@datazen/frontend`'s `startSharedApp`. This file keeps
 * only desktop-owned steps: the Tauri event compat patch, the platform
 * assembly and the sub-window show handshake.
 */
import { installTauriEventUnlistenRaceWorkaround } from './platform/desktop/tauriEventCompat';
import { assembleDesktopPlatform, showSubWindowWhenReady } from './shell/desktop/assembly';
import { startSharedApp } from '@datazen/frontend/app';

installTauriEventUnlistenRaceWorkaround();

startSharedApp({
  // Backend binding + platform port injection + chrome registration; runs
  // after the shared module init and before the settings preload — the same
  // slot it occupied in the pre-F2 main.tsx bootstrap.
  configureHost: assembleDesktopPlatform,
});

showSubWindowWhenReady();
