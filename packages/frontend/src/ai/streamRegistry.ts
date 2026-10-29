/**
 * Streaming lifecycle state machine for AI chat runs (P6 §14.4).
 *
 * One entry per `requestId`:
 *
 *     starting ──first chunk──▶ streaming ──cancel marked──▶ cancelling
 *         │                         │                            │
 *         └───────────terminal chunk / error / timeout───────────┘
 *                                   ▼
 *                              terminal(done | cancelled | error)
 *
 * Two invariants the UI depends on:
 *
 * 1. **Generations are monotonic per conversation.** `begin()` bumps the
 *    conversation generation, so a chunk from a superseded run can never be
 *    mistaken for the current run's output.
 * 2. **Late chunks are dropped, not merged.** After a generation change or
 *    after a cancel has been issued, only the terminal chunk is still
 *    accepted; every non-terminal late chunk is reported with a drop reason
 *    and never touches the rendered text.
 *
 * The module is pure TypeScript: no React, no Tauri, no timers. The 8s cancel
 * fallback lives in `sessionCancel`.
 */

export type StreamPhase = 'starting' | 'streaming' | 'cancelling' | 'terminal';

export type StreamTerminalKind = 'done' | 'cancelled' | 'error';

/** Why a chunk was not applied. */
export type StreamDropReason =
  | 'unknown-request'
  | 'stale-generation'
  | 'cancel-issued'
  | 'terminal';

export interface StreamEntry {
  readonly requestId: string;
  readonly conversationId: string;
  readonly generation: number;
  phase: StreamPhase;
  terminalKind: StreamTerminalKind | null;
  /** Wall-clock ms when the cancel was marked, or `null`. */
  cancelIssuedAtMs: number | null;
}

export interface ApplyChunkInput {
  /** Terminal marker of the backend chunk. */
  done?: boolean;
  /** Terminal marker: `done && cancelled` means the run ended by cancel. */
  cancelled?: boolean;
  /** Backend error carried inline; treated as a terminal `error`. */
  error?: string | null;
}

export interface ApplyChunkResult {
  accepted: boolean;
  dropReason?: StreamDropReason;
  /** Phase after the call (or current phase when dropped). `null` = unknown request. */
  phase: StreamPhase | null;
  generation: number | null;
  terminalKind: StreamTerminalKind | null;
}

export type StreamRegistryEvent =
  | {
      type: 'started' | 'streaming' | 'cancelling';
      entry: Readonly<StreamEntry>;
    }
  | {
      type: 'settled';
      entry: Readonly<StreamEntry>;
      terminalKind: StreamTerminalKind;
    };

export type StreamRegistryListener = (event: StreamRegistryEvent) => void;

export interface StreamRegistryOptions {
  now?: () => number;
}

function snapshot(entry: StreamEntry): Readonly<StreamEntry> {
  return { ...entry };
}

export class StreamRegistry {
  private readonly entries = new Map<string, StreamEntry>();
  private readonly generations = new Map<string, number>();
  private readonly currentRequest = new Map<string, string>();
  private readonly listeners = new Set<StreamRegistryListener>();
  private readonly now: () => number;

  constructor(options: StreamRegistryOptions = {}) {
    this.now = options.now ?? (() => Date.now());
  }

  /**
   * Start a new generation for `conversationId`.
   *
   * A previous non-terminal run of the same conversation is superseded and
   * settled as `cancelled`; its late chunks are dropped as
   * `stale-generation`. Calling `begin` twice for the still-live request is a
   * no-op so a duplicated send cannot silently change the active generation.
   */
  begin(conversationId: string, requestId: string): Readonly<StreamEntry> {
    const existing = this.entries.get(requestId);
    if (existing && existing.phase !== 'terminal') return snapshot(existing);

    const previousRequest = this.currentRequest.get(conversationId);
    if (previousRequest && previousRequest !== requestId) {
      const previous = this.entries.get(previousRequest);
      if (previous && previous.phase !== 'terminal') this.settle(previousRequest, 'cancelled');
    }

    const generation = (this.generations.get(conversationId) ?? 0) + 1;
    this.generations.set(conversationId, generation);
    this.currentRequest.set(conversationId, requestId);

    const entry: StreamEntry = {
      requestId,
      conversationId,
      generation,
      phase: 'starting',
      terminalKind: null,
      cancelIssuedAtMs: null,
    };
    this.entries.set(requestId, entry);
    this.emit({ type: 'started', entry: snapshot(entry) });
    return snapshot(entry);
  }

  has(requestId: string): boolean {
    return this.entries.has(requestId);
  }

  get(requestId: string): Readonly<StreamEntry> | undefined {
    const entry = this.entries.get(requestId);
    return entry ? snapshot(entry) : undefined;
  }

  conversationGeneration(conversationId: string): number {
    return this.generations.get(conversationId) ?? 0;
  }

  /**
   * Apply one stream chunk.
   *
   * Terminal chunks are accepted at most once. Non-terminal chunks are only
   * accepted while the entry belongs to the conversation's current generation
   * and no cancel has been issued.
   */
  applyChunk(requestId: string, chunk: ApplyChunkInput): ApplyChunkResult {
    const gate = this.gate(requestId);
    if (gate.drop) return gate.drop;
    const entry = gate.entry;

    if (chunk.error) return this.settleEntry(entry, 'error');

    if (chunk.done) {
      return this.settleEntry(entry, chunk.cancelled ? 'cancelled' : 'done');
    }

    // A cancel was marked: everything except the terminal chunk is late.
    if (entry.phase === 'cancelling') {
      return this.drop(entry, 'cancel-issued');
    }

    if (entry.phase === 'starting') {
      entry.phase = 'streaming';
      this.emit({ type: 'streaming', entry: snapshot(entry) });
    }
    return this.accepted(entry);
  }

  /** Settle on an out-of-band error event. Idempotent. */
  applyError(requestId: string): ApplyChunkResult {
    const gate = this.gate(requestId);
    if (gate.drop) return gate.drop;
    return this.settleEntry(gate.entry, 'error');
  }

  /**
   * Mark a run as cancel-requested. Returns `true` when the phase changed (or
   * was already `cancelling`); `false` for stale/terminal/unknown requests.
   */
  markCancelling(requestId: string, atMs: number = this.now()): boolean {
    const entry = this.entries.get(requestId);
    if (!entry) return false;
    if (entry.phase === 'terminal') return false;
    if (this.generations.get(entry.conversationId) !== entry.generation) return false;
    if (entry.phase === 'cancelling') return true;
    entry.phase = 'cancelling';
    entry.cancelIssuedAtMs = atMs;
    this.emit({ type: 'cancelling', entry: snapshot(entry) });
    return true;
  }

  /**
   * Force a terminal state (used by the 8s cancel fallback). Idempotent:
   * returns `false` if the entry is already terminal or unknown.
   */
  settle(requestId: string, kind: StreamTerminalKind): boolean {
    const entry = this.entries.get(requestId);
    if (!entry || entry.phase === 'terminal') return false;
    this.terminalize(entry, kind);
    return true;
  }

  subscribe(listener: StreamRegistryListener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private gate(requestId: string): { entry: StreamEntry; drop?: undefined } | { entry: null; drop: ApplyChunkResult } {
    const entry = this.entries.get(requestId);
    if (!entry) {
      return {
        entry: null,
        drop: {
          accepted: false,
          dropReason: 'unknown-request',
          phase: null,
          generation: null,
          terminalKind: null,
        },
      };
    }
    if (this.generations.get(entry.conversationId) !== entry.generation) {
      return {
        entry: null,
        drop: {
          accepted: false,
          dropReason: 'stale-generation',
          phase: entry.phase,
          generation: entry.generation,
          terminalKind: entry.terminalKind,
        },
      };
    }
    if (entry.phase === 'terminal') {
      return {
        entry: null,
        drop: {
          accepted: false,
          dropReason: 'terminal',
          phase: entry.phase,
          generation: entry.generation,
          terminalKind: entry.terminalKind,
        },
      };
    }
    return { entry };
  }

  private accepted(entry: StreamEntry): ApplyChunkResult {
    return {
      accepted: true,
      phase: entry.phase,
      generation: entry.generation,
      terminalKind: entry.terminalKind,
    };
  }

  private drop(entry: StreamEntry, reason: StreamDropReason): ApplyChunkResult {
    return {
      accepted: false,
      dropReason: reason,
      phase: entry.phase,
      generation: entry.generation,
      terminalKind: entry.terminalKind,
    };
  }

  private settleEntry(entry: StreamEntry, kind: StreamTerminalKind): ApplyChunkResult {
    if (entry.phase === 'terminal') return this.drop(entry, 'terminal');
    this.terminalize(entry, kind);
    return {
      accepted: true,
      phase: entry.phase,
      generation: entry.generation,
      terminalKind: entry.terminalKind,
    };
  }

  private terminalize(entry: StreamEntry, kind: StreamTerminalKind): void {
    entry.phase = 'terminal';
    entry.terminalKind = kind;
    this.emit({ type: 'settled', entry: snapshot(entry), terminalKind: kind });
  }

  private emit(event: StreamRegistryEvent): void {
    for (const listener of this.listeners) listener(event);
  }
}
