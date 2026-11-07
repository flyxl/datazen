/**
 * Host side of the `fold` Pro compartment.
 *
 * Why this lives in its own file rather than in `editorExtensions.ts`: that
 * module is already over the 800-line ceiling, and the compartment registry in
 * `proCompartments.ts` is the sanctioned place to add slots. A dedicated module
 * keeps the fold wiring — one factory, one memo dependency — readable without
 * growing anything that is already at its limit.
 *
 * What the host owns vs. what the extension owns
 * ----------------------------------------------
 * The host owns exactly one decision here: **whether** to ask for folding at
 * all. It reads the `codeFolding` key off the *same* generic settings bag every
 * other privileged key travels on (`readProSettingsBag` →
 * `proSettingFlag`), so folding is switched on and off through the one
 * documented path rather than a bespoke channel of its own.
 *
 * How folding actually works — the `codeFolding` state field, the fold gutter,
 * the fold keymap, and the SQL-specific foldable node — belongs entirely to the
 * privileged extension. The host never imports a fold API.
 */
import type { Extension } from '@codemirror/state';
import { createFoldExtensions, proSettingFlag, type ProSettingsBag } from '../proCompartments';

export interface CreateFoldCompartmentOptions {
  /** The generic privileged settings bag, forwarded verbatim to the extension. */
  proSettings?: ProSettingsBag;
}

/**
 * Read the `codeFolding` flag off the shared bag.
 *
 * `proSettingFlag` treats a missing key as the fallback and **any non-boolean
 * as the fallback too**, so a hand-edited or corrupted settings bag cannot
 * silently switch a feature off that the user believes is on. The fallback is
 * `true`: folding is the common case, and a corrupted bag should not remove it.
 */
export function foldSettingEnabled(proSettings?: ProSettingsBag): boolean {
  return proSettingFlag(proSettings, 'codeFolding', true);
}

/**
 * Build the `fold` compartment payload.
 *
 * There is deliberately **one** flag with **one** value: the `codeFolding` key
 * on the shared bag. The host reads it to decide whether to install the
 * compartment at all, and forwards the same bag to the extension, which reads
 * the same key. An earlier draft let the host override the bag via an `enabled`
 * option — that gave one setting two readers with contradictory instructions
 * (`enabled: true` alongside `codeFolding: false`), and a test caught exactly
 * that. A single source of truth is the whole point of routing the key through
 * the generic bag.
 *
 * Returns `[]` when folding is switched off, which reconfigures the slot to
 * empty rather than unmounting it — see `reconfigureProCompartments`. The slot
 * stays mounted so flipping the setting back on is an ordinary reconfigure, not
 * a remount that would lose the editor's scroll and selection.
 */
export function createFoldCompartmentExtensions(opts?: CreateFoldCompartmentOptions): Extension[] {
  if (!foldSettingEnabled(opts?.proSettings)) return [];
  return createFoldExtensions(opts ? { proSettings: opts.proSettings } : undefined);
}
