import type { StructureCapabilities, StructureCapabilityFlag } from './types';

export function isControlEnabled(
  caps: StructureCapabilities,
  control: StructureCapabilityFlag,
): boolean {
  return caps[control] === true;
}
