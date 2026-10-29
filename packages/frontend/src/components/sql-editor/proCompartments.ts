/**
 * Privileged-extension (EP) plumbing for the SQL editor: the compartment
 * registry, the atomic batch reconfiguration, the generic settings bag, and the
 * two generic hooks (`createExtraExtensions` / `createExtraKeymap`).
 *
 * Why this module exists
 * ----------------------
 * The editor used to carry a hard-coded six-slot compartment set and rewire it
 * with six independent `view.dispatch` calls, one per slot. That has two costs:
 *
 *  1. **Non-atomic.** Each dispatch is a separate transaction, so an observer
 *     (an update listener, a decoration pass, a key handler) can observe a
 *     frame in which some Pro compartments are configured and others are not.
 *  2. **Closed set.** A privileged extension could not add a capability of its
 *     own without a host release, because the payload shape was a six-key
 *     literal type.
 *
 * `ProCompartmentPayload` is now an open `Record<string, Extension[]>` that is
 * traversed rather than destructured, so an extension that registers its own
 * compartment id before mount gets a first-class, independently reconfigurable
 * slot — and every reconfiguration lands in exactly one transaction.
 *
 * Multi-instance note: the registry is a module-level singleton and the
 * `Compartment` instances are shared by every editor instance, but
 * CodeMirror keeps a *per-state* `compartment → content` map, so a
 * reconfigure dispatched to one view can never reach another. That invariant is
 * pinned by a dedicated test rather than assumed.
 */
import {
  Compartment,
  Prec,
  Transaction,
  type Extension,
  type StateEffect,
} from '@codemirror/state';
import { keymap, type EditorView, type KeyBinding } from '@codemirror/view';
import {
  SafeCompartmentWrapper,
  extensionRegistry,
  sqlEditorEnhancedEP,
  type ExtensionPoint,
  type SqlEditorEnhancedOptions,
} from '@datazen/extension-points';

/* -------------------------------------------------------------------------- */
/*  Compartment ids                                                            */
/* -------------------------------------------------------------------------- */

/** Slot hosting `createExtraKeymap` output. */
export const KEYMAP_COMPARTMENT_ID = 'keymap';
/** Generic slot hosting `createExtraExtensions` output (and any overflow). */
export const EXTRA_COMPARTMENT_ID = 'extra';
/**
 * Slot hosting `createFoldExtensions` output (code folding).
 *
 * A dedicated slot rather than a corner of `extra`, because folding carries
 * its own keymap: it needs to sit at a known position in the mount order (see
 * {@link BASE_PRO_COMPARTMENT_IDS}) and to be reconfigurable on its own without
 * disturbing the generic bucket's contents.
 */
export const FOLD_COMPARTMENT_ID = 'fold';

/**
 * Priority-ordered ids every editor instance mounts out of the box.
 *
 * The order matters: it is the order the compartments appear in the editor
 * state, which is the order their extension precedence is resolved in.
 */
export const BASE_PRO_COMPARTMENT_IDS = [
  'statement',
  'completion',
  'intention',
  'hover',
  'paste',
  'linter',
  FOLD_COMPARTMENT_ID,
  KEYMAP_COMPARTMENT_ID,
  EXTRA_COMPARTMENT_ID,
] as const;

export type BaseProCompartmentId = (typeof BASE_PRO_COMPARTMENT_IDS)[number];

/* -------------------------------------------------------------------------- */
/*  Extensible registry                                                         */
/* -------------------------------------------------------------------------- */

const compartmentRegistry = new Map<string, Compartment>();

/**
 * Live view of the registered compartments, keyed by id.
 *
 * A plain object rather than a frozen literal so that `ensureProCompartment`
 * can grow it at runtime while `compartments.statement` keeps working as a
 * property access for existing call sites.
 */
export const compartments: Record<string, Compartment> = {};

/**
 * Register a compartment id (idempotent) and return its `Compartment`.
 *
 * Call this **before** the `EditorView` is created. CodeMirror cannot
 * retro-fit a brand-new `Compartment` into an existing state — reconfiguring a
 * compartment that was never mounted throws — so an id discovered for the
 * first time after mount is routed into {@link EXTRA_COMPARTMENT_ID} instead.
 */
export function ensureProCompartment(id: string): Compartment {
  const existing = compartmentRegistry.get(id);
  if (existing) return existing;
  const created = new Compartment();
  compartmentRegistry.set(id, created);
  compartments[id] = created;
  return created;
}

/** The compartment registered for `id`, or `undefined` when it is unknown. */
export function getProCompartment(id: string): Compartment | undefined {
  return compartmentRegistry.get(id);
}

/** Registered ids in registration order (base ids first). */
export function registeredProCompartmentIds(): string[] {
  return [...compartmentRegistry.keys()];
}

for (const id of BASE_PRO_COMPARTMENT_IDS) ensureProCompartment(id);

/* -------------------------------------------------------------------------- */
/*  Payload                                                                     */
/* -------------------------------------------------------------------------- */

/**
 * Per-compartment extension bags, keyed by compartment id.
 *
 * Open by design: an extension that called {@link ensureProCompartment} gets a
 * key here without this type growing a member.
 */
export type ProCompartmentPayload = Record<string, Extension[]>;

/**
 * Extension list to install into `state.extensions` at editor mount time.
 *
 * Registers every payload id first, so the returned list covers both the base
 * slots and any extension-owned slot.
 */
export function mountProCompartments(payload: ProCompartmentPayload): Extension[] {
  return Object.keys(payload).map((id) => ensureProCompartment(id).of(payload[id]));
}

/**
 * Atomically reconfigure every Pro-driven compartment on a live view.
 *
 * All applicable slots are reconfigured by a **single** transaction, so no
 * intermediate frame exists in which some Pro compartments are configured and
 * others are not.
 *
 * Ids whose compartment is not mounted in this view (an extension that only
 * announced its slot after the view was built) are folded into
 * {@link EXTRA_COMPARTMENT_ID} when that slot is mounted, and otherwise skipped
 * with a warning. Either way this degrades instead of throwing, because a
 * rejected reconfigure would leave the editor with half-applied extensions.
 */
export function reconfigureProCompartments(view: EditorView, payload: ProCompartmentPayload): void {
  const direct: StateEffect<unknown>[] = [];
  const directIds: string[] = [];
  const overflow: string[] = [];

  for (const id of Object.keys(payload)) {
    const compartment = getProCompartment(id);
    // `Compartment.get` returns `undefined` when the compartment is not part
    // of this view's configuration — the only reliable presence check CM6 offers.
    if (compartment && compartment.get(view.state) !== undefined) {
      directIds.push(id);
    } else {
      overflow.push(id);
    }
  }

  for (const id of directIds) {
    direct.push(getProCompartment(id)!.reconfigure(payload[id]));
  }

  const extraCompartment = getProCompartment(EXTRA_COMPARTMENT_ID);
  if (overflow.length > 0) {
    if (extraCompartment && extraCompartment.get(view.state) !== undefined) {
      // `overflow` cannot contain `extra` itself, and that is a property of
      // the classification above rather than a runtime possibility: reaching
      // this branch *means* `extra` is mounted in this view, and a mounted
      // `extra` was filed under `directIds` by the same predicate. So `extra`
      // needs no per-id guard in the merge below — there is no "fold `extra`
      // into `extra`" state to exclude. A guard here would be dead code
      // (ep-hooks-settings-BUG-003).
      const merged: Extension[] = [...(payload[EXTRA_COMPARTMENT_ID] ?? [])];
      for (const id of overflow) {
        merged.push(...payload[id]);
      }
      direct.push(extraCompartment.reconfigure(merged));
    } else {
      console.warn(
        `[reconfigureProCompartments] no mounted slot for compartment(s) ${overflow.join(', ')} — ` +
          'register them with ensureProCompartment() before the editor mounts.',
      );
    }
  }

  if (direct.length === 0) return;

  view.dispatch({
    effects: direct,
    annotations: Transaction.addToHistory.of(false),
  });
}

/* -------------------------------------------------------------------------- */
/*  Generic settings bag (G3)                                                   */
/* -------------------------------------------------------------------------- */

/** Privileged extension settings, as persisted under `driverSettings[extensionId]`. */
export type ProSettingsBag = Readonly<Record<string, unknown>>;

/** Settings namespaces owned by the SQL editor privileged EP. */
export const PRO_SETTINGS_EXTENSION_IDS = ['sql-editor-enhanced', 'sql-editor-pro'] as const;

/**
 * Stable empty bag.
 *
 * A fresh `{}` per render would give `useMemo`/selector a new identity on every
 * store read and spin the reconfigure effect forever.
 */
const EMPTY_PRO_SETTINGS: ProSettingsBag = Object.freeze({});

/**
 * Read the privileged settings bag for a given extension id.
 *
 * The bag is passed wholesale into every compartment factory and is a
 * dependency of each of them, so **any** key an extension declares in its
 * `settingsContributions` can now influence the editor. Previously the editor
 * read five hard-coded keys and ignored the rest.
 */
export function readProSettingsBag(driverSettings?: Record<string, unknown>): ProSettingsBag {
  for (const id of PRO_SETTINGS_EXTENSION_IDS) {
    const bag = driverSettings?.[id];
    if (bag && typeof bag === 'object' && !Array.isArray(bag)) return bag as ProSettingsBag;
  }
  return EMPTY_PRO_SETTINGS;
}

/** Read a boolean setting, falling back when the key is absent or non-boolean. */
export function proSettingFlag(
  bag: ProSettingsBag | undefined,
  key: string,
  fallback = true,
): boolean {
  const value = bag?.[key];
  return typeof value === 'boolean' ? value : fallback;
}

/* -------------------------------------------------------------------------- */
/*  Generic hooks                                                               */
/* -------------------------------------------------------------------------- */

const enhancedSafe = (featureName: string) => ({
  point: sqlEditorEnhancedEP as ExtensionPoint<unknown>,
  featureName,
});

/**
 * `createExtraExtensions` → the generic `extra` compartment.
 *
 * Circuit-broken like every other privileged factory: a throwing extension
 * unregisters the EP and degrades to `[]` instead of taking the editor down.
 */
export function createProExtraExtensions(opts?: SqlEditorEnhancedOptions): Extension[] {
  const enhanced = extensionRegistry.get(sqlEditorEnhancedEP);
  return SafeCompartmentWrapper(
    enhancedSafe('createExtraExtensions'),
    () => enhanced.createExtraExtensions?.(opts) ?? [],
    [],
  );
}

/**
 * `createFoldExtensions` → the `fold` compartment.
 *
 * Optional hook, absent-means-empty like every other contract 1.1.0 hook: an
 * extension that predates code folding simply does not implement it and the
 * slot reconfigures to `[]`. Adding an *optional* member did not require an
 * `EXTENSION_POINTS_VERSION` bump — that guard is exact string equality
 * (`security.ts:136-151`), so bumping it would reject every EP whose manifest
 * still declares 1.1.0. Both directions of the drift degrade rather than
 * break: a newer extension against an older host lands in
 * {@link EXTRA_COMPARTMENT_ID} via {@link reconfigureProCompartments}'s
 * overflow path.
 */
export function createFoldExtensions(opts?: SqlEditorEnhancedOptions): Extension[] {
  const enhanced = extensionRegistry.get(sqlEditorEnhancedEP);
  return SafeCompartmentWrapper(
    enhancedSafe('createFoldExtensions'),
    () => enhanced.createFoldExtensions?.(opts) ?? [],
    [],
  );
}

/**
 * `createExtraKeymap` → a `Prec.highest` keymap.
 *
 * `Prec.highest` is load-bearing, not decoration. `createBaseEditorExtensions`
 * registers `keymap.of([...defaultKeymap, ...])` *before* any Pro compartment,
 * and several `defaultKeymap` commands — `copyLineUp`/`copyLineDown`
 * (`Shift-Alt-ArrowUp`/`Down`), `moveLineUp`/`moveLineDown`
 * (`Alt-ArrowUp`/`Down`), `addCursorAbove`/`addCursorBelow`
 * (`Mod-Alt-ArrowUp`/`Down`) — return `true` whenever they do anything at all.
 * CodeMirror stops at the first handler that returns `true`, so an extension
 * binding one of those chords at default precedence is never even consulted:
 * the keystroke is consumed by the host command and the extension looks broken.
 * Raising precedence is the only thing that makes this hook usable.
 *
 * The corollary is checked too: a hook handler that returns `false` still hands
 * the chord on to `defaultKeymap`. `Prec.highest` widens reach, it does not
 * make the hook greedy.
 */
export function createProKeymapExtension(opts?: SqlEditorEnhancedOptions): Extension {
  const enhanced = extensionRegistry.get(sqlEditorEnhancedEP);
  const bindings: KeyBinding[] = SafeCompartmentWrapper(
    enhancedSafe('createExtraKeymap'),
    () => enhanced.createExtraKeymap?.(opts) ?? [],
    [],
  );
  return Prec.highest(keymap.of(bindings));
}
