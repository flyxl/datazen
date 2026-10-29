/**
 * The host path picker accessor (form-split F6 prep).
 *
 * The native dialog is a host capability: the desktop binding rides the dialog
 * plugin inside `PlatformServices.pickPath` (the zone-local implementation in
 * `platform/desktop/platformServices.ts`); a web form has no native picker and
 * resolves `null` (cancel semantics — callers disable the browse affordance by
 * capability, not by probing). The F2 re-export shim to the desktop zone is
 * gone; `@datazen/ui`'s `PathInput` keeps receiving this through the `onBrowse`
 * prop, so the design system never learns which form answered.
 */

import { requirePlatformServices, type PickPathInput } from '@datazen/backend-client';

export const pickPath = (options?: PickPathInput): Promise<string | null> =>
  requirePlatformServices().pickPath(options);
