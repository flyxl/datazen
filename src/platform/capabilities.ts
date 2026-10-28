/**
 * Static capability tables (form-split F5, plan §3.3 / §5.5).
 *
 * Capabilities are a **compile-time property of the form factor**, not a
 * runtime probe: each assembly binds its table alongside the ports, and
 * consumers branch on the table — never on a form-detection check. The
 * mechanism deliberately mirrors `DATAZEN_UPDATER_CHANNEL`'s compile-time
 * constant precedent: a plain module constant, generated or hand-written,
 * with no runtime environment sniffing anywhere.
 */

import type { PlatformCapabilities } from './contracts';
import { ALL_SETTINGS_SECTIONS } from '../windows/settings/settingsSections';
import type { SettingsSection } from '../windows/settings/settingsSections';

/** Sections a form without a host can serve (the shared contract only). */
const WEB_SETTINGS_SECTIONS: ReadonlySet<SettingsSection> = new Set<SettingsSection>([
  'general',
  'appearance',
  'dataBrowsing',
  'editor',
  'behavior',
]);

/** The desktop form's table: everything the host supplies. */
export const DESKTOP_CAPABILITIES: PlatformCapabilities = {
  settingsSections: ALL_SETTINGS_SECTIONS,
  updater: true,
  versionSource: 'host-app',
};

/** The web form's table: no host, no native updater, version from the build. */
export const WEB_CAPABILITIES: PlatformCapabilities = {
  settingsSections: WEB_SETTINGS_SECTIONS,
  updater: false,
  versionSource: 'build-constant',
};
