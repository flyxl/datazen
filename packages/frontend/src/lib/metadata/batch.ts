/**
 * Metadata batch coalescing, isolated per identity/config/target.
 *
 * Metadata *batches* (e.g. "load columns for these tables") must never mix
 * requests that belong to different connections, config revisions, or
 * targets — a batch keyed without target can silently merge two targets'
 * results. {@link MetadataBatcher} groups queued keys by their full
 * {@link metadataCacheKey} bucket so each identity/config/target gets its
 * own in-flight batch, and deduplicates identical keys inside a batch.
 */

import { metadataBatchBucket, metadataCacheKey, type MetadataIdentityInput } from './identity';

interface PendingBatch {
  bucket: string;
  keys: Set<string>;
  timer: ReturnType<typeof setTimeout> | null;
}

export interface MetadataBatcherOptions {
  debounceMs?: number;
  now?: () => number;
}

export type BatchFlush = (bucket: string, keys: string[]) => void | Promise<void>;

/** Collects per-bucket metadata requests and flushes them as deduped batches. */
export class MetadataBatcher {
  private pending = new Map<string, PendingBatch>();

  constructor(
    private readonly flush: BatchFlush,
    private readonly options: MetadataBatcherOptions = {},
  ) {}

  /** Queue one metadata key for its identity/config/target bucket. */
  enqueue(input: MetadataIdentityInput, key: string): void {
    const bucket = metadataBatchBucket(input);
    let batch = this.pending.get(bucket);
    if (!batch) {
      batch = { bucket, keys: new Set(), timer: null };
      this.pending.set(bucket, batch);
    }
    batch.keys.add(key);
    const debounce = this.options.debounceMs ?? 0;
    if (batch.timer !== null) clearTimeout(batch.timer);
    if (debounce > 0) {
      batch.timer = setTimeout(() => this.flushBucket(bucket), debounce);
    }
  }

  /** Flush one bucket immediately (or all when bucket is omitted). */
  async flushNow(bucket?: string): Promise<void> {
    if (bucket !== undefined) {
      await this.flushBucket(bucket);
      return;
    }
    for (const b of [...this.pending.keys()]) await this.flushBucket(b);
  }

  /** Drop pending requests for every bucket matching the invalidation rule. */
  invalidate(matches: (bucket: string) => boolean): void {
    for (const [bucket, batch] of [...this.pending.entries()]) {
      if (!matches(bucket)) continue;
      if (batch.timer !== null) clearTimeout(batch.timer);
      this.pending.delete(bucket);
    }
  }

  /** Number of pending buckets (observability / tests). */
  get size(): number {
    return this.pending.size;
  }

  private async flushBucket(bucket: string): Promise<void> {
    const batch = this.pending.get(bucket);
    if (!batch) return;
    if (batch.timer !== null) clearTimeout(batch.timer);
    this.pending.delete(bucket);
    await this.flush(bucket, [...batch.keys]);
  }
}

export { metadataCacheKey };
