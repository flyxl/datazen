/**
 * Paste/drop/multiple selection compartment factory.
 *
 * §Track S5-A: Paste/Drop/Multiple Selection
 * Enhanced features (paste-as-IN, drop caret) are supplied via sqlEditorEnhancedEP.
 * Community edition does not provide paste-as-IN.
 */
import type { Extension } from '@codemirror/state';
import {
  extensionRegistry,
  sqlEditorEnhancedEP,
  SafeCompartmentWrapper,
  type ExtensionPoint,
} from '@datazen/extension-points';
import type { DroppedTablePayload } from '../contracts';
import { createMultipleSelectionsExtension } from './multipleSelections';

export interface PasteCompartmentOptions {
  connectionId?: string;
  onDrop?: (payload: DroppedTablePayload, pos: number | null) => void;
  onDropError?: (message: string) => void;
}

export function createPasteExtensions(opts: PasteCompartmentOptions): Extension[] {
  const multiCursor = createMultipleSelectionsExtension();
  const enhanced = extensionRegistry.get(sqlEditorEnhancedEP);
  const enhancedPaste = SafeCompartmentWrapper(
    {
      point: sqlEditorEnhancedEP as ExtensionPoint<unknown>,
      featureName: 'createPasteExtensions',
    },
    () => enhanced.createPasteExtensions?.(opts) ?? [],
    [],
  );
  return [...multiCursor, ...enhancedPaste];
}
