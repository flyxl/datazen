/**
 * Paste/drop/multiple selection compartment factory.
 *
 * §Track S5-A: Paste/Drop/Multiple Selection
 * Pro features (paste-as-IN, drop caret) are supplied via sqlEditorProEP.
 * Community edition does not provide paste-as-IN.
 */
import type { Extension } from '@codemirror/state';
import {
  extensionRegistry,
  sqlEditorProEP,
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
  const pro = extensionRegistry.get(sqlEditorProEP);
  const proPaste = SafeCompartmentWrapper(
    {
      point: sqlEditorProEP as ExtensionPoint<unknown>,
      featureName: 'createPasteExtensions',
    },
    () => pro.createPasteExtensions?.(opts) ?? [],
    [],
  );
  return [...multiCursor, ...proPaste];
}
