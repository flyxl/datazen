/**
 * Host-owned AI context for a KV panel.
 *
 * Until now a KV panel rendered the toolbar's AI button exactly like a SQL panel,
 * while the assistant only ever received `contextTables` — an entry point that
 * opens but carries no Redis fact is the half-broken state, so this module owns
 * both halves of the fix: the one rule that decides whether the assistant has
 * anything host-owned to say, and the block it says.
 *
 * **Facts only, and only facts the host really owns**: the connection label, the
 * session id, the database the panel is bound to and the key the panel's relay
 * reports as selected. A type / TTL / size / value digest is equally a Redis fact,
 * but those come from `key_object_info`, which the driver's own key-props sidebar
 * fetches inside `packages/drivers/redis/**`; the frozen `KvSlotState` shape carries
 * the scalars the status bar renders, not a key snapshot. Reaching them
 * from here would mean either duplicating a driver command or building a cache in
 * front of the relay, and both are ruled out — that state belongs to the UI
 * segment that owns it. The driver Console buffer is equally off-limits, so it is
 * never injected.
 *
 * The block is a token list (`selected_key=…`), not prose: it is prompt payload,
 * not UI copy, so it must not bend with the interface language and no test may
 * assert English literals through it. The user-facing label lives in
 * i18n (`redis.ai.context.*`) and the value shown next to it is data.
 *
 * The companion subscription lives in `src/hooks/useKvSlotSelectedKey.ts`, which
 * is how a leaf reads the relay without the workspace re-rendering.
 */

/** What the host knows about the panel the assistant is being asked from. */
export interface KvAiFacts {
  connectionName: string;
  dbSessionId: string;
  /** Database the active panel is bound to (`null` while none is resolved). */
  database: string | null;
  /** Key the panel's relay reports as selected (`null` ⇒ none). */
  selectedKey: string | null;
}

/** Ready-to-send context for one message. */
export interface KvAiContext {
  /** The key the block is about; shown next to the label so egress is visible. */
  keyName: string;
  /** Structured fact block appended to the message at send time. */
  block: string;
}

/** The only key scope the host can speak about; an empty label is not a key. */
function selectedKeyNameOf(facts: Pick<KvAiFacts, 'selectedKey'>): string | null {
  const key = facts.selectedKey;
  return key && key.trim().length > 0 ? key : null;
}

/**
 * The binary rule in one place: a KV panel has host-owned AI context only
 * while a key is in scope. Callers must either act on it or hide the affordance —
 * injecting an empty block is the half-broken state this exists to remove.
 */
export function hasKvAiFacts(facts: Pick<KvAiFacts, 'selectedKey'>): boolean {
  return selectedKeyNameOf(facts) !== null;
}

/** The context for these facts, or `null` when there is nothing to attach. */
export function buildKvAiContext(facts: KvAiFacts): KvAiContext | null {
  const keyName = selectedKeyNameOf(facts);
  if (!keyName) return null;
  const lines = [
    '[kv-context]',
    `connection=${facts.connectionName}`,
    `session=${facts.dbSessionId}`,
    `database=${facts.database ?? ''}`,
    `selected_key=${keyName}`,
    '[/kv-context]',
  ];
  return { keyName, block: lines.join('\n') };
}

/**
 * Attach the block to a user message.
 *
 * Sent as part of the message because that is the only host-side path to the
 * model this track may change: `contextTables` / `contextFiles` are relational and
 * file scopes, and inventing a wire field for a KV key would mean a new backend
 * command. The attached block therefore also shows up in the transcript, which is
 * what makes the egress visible to the user.
 */
export function composeKvAiMessage(content: string, context: KvAiContext | null): string {
  return context ? `${content}\n\n${context.block}` : content;
}
