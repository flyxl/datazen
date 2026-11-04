/**
 * Web entry (WR-E1).
 *
 * The shared startup chain (marks, CSS, PRO host-module table, E2E store
 * bridge, locale / icon / scroll-lock / PRO init, settings preload, React
 * root) lives in `@datazen/frontend`'s `startSharedApp` — the exact function
 * the desktop entry calls. This file keeps only web-owned steps: the web
 * platform assembly (ports → browser halves, all-throw kernel) and the
 * shared bootstrap call.
 *
 * Routing follows the shared shell's own design: `@datazen/frontend`'s App
 * maps the `?window=` URL parameter onto its window panels
 * (main / onboarding / data-sync / data-transfer / schema-diff / backup —
 * `lib/windowKind.ts`). The browser tab IS the window, so a URL is a route
 * and the "window capability → page/panel" mapping is the established form;
 * no second router is introduced on top of it (WR-E1 ruling, see the
 * assembly header).
 *
 * `onSettingsPreloaded` stays unset: its only consumer today is the desktop
 * startup update check, and `WEB_CAPABILITIES.updater` is false — the
 * settings sections gate on the capability table, not on a probe.
 */
import { startSharedApp } from '@datazen/frontend/app';
import { assembleWebPlatform } from './platform/web/assembly';

assembleWebPlatform();

startSharedApp();
