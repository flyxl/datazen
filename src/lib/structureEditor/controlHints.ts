import type { StructureCapabilities, StructureCapabilityFlag } from './types';
import { isControlEnabled } from './isControlEnabled';

/** i18n key shown as title/tooltip when a control is disabled by driver caps. */
export function controlDisabledKey(_control: StructureCapabilityFlag): string {
  return 'structEditor.capDisabled';
}

export function capEnabled(
  caps: StructureCapabilities | null,
  control: StructureCapabilityFlag,
): boolean {
  if (!caps) return false;
  return isControlEnabled(caps, control);
}
