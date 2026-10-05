import type {
  ArtifactChunk, BackendClient, ConnectionEvent, EventEnvelope, ExecutionReceipt,
  ExecutionView, RuntimeResultBinding, SessionHandle, StatementResultSource,
} from '@datazen/backend-client';
import { counterValue, sameHandle, wireCounter } from './counters';

export interface PublishedResultChunk {
  chunk: ArtifactChunk;
  source: StatementResultSource | null;
}

/** Result state survives editor context changes; consumption has its own lifetime. */
export class ExecutionProjection {
  private sequence: bigint | null = null;
  private iterator: AsyncIterator<EventEnvelope<ConnectionEvent>> | null = null;
  private generation = 0;
  private reads = new Map<string, Promise<void>>();
  private chunksByKey = new Map<string, PublishedResultChunk>();
  private listeners = new Set<() => void>();
  view: ExecutionView | null = null;
  error: unknown = null;

  constructor(
    readonly client: BackendClient,
    readonly receipt: ExecutionReceipt,
    readonly originalHandle: SessionHandle | null,
    private readonly onSession?: (event: Extract<ConnectionEvent, { kind: 'sessionChanged' }>) => void,
  ) {}

  get chunks(): readonly PublishedResultChunk[] {
    return [...this.chunksByKey.values()].sort((a, b) => {
      if (a.chunk.artifactId !== b.chunk.artifactId) return a.chunk.artifactId.localeCompare(b.chunk.artifactId);
      const x = counterValue(a.chunk.chunkIndex), y = counterValue(b.chunk.chunkIndex);
      return x < y ? -1 : x > y ? 1 : 0;
    });
  }
  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  }
  private publish(): void { for (const listener of this.listeners) listener(); }
  private key(artifactId: string, index: bigint): string {
    return JSON.stringify([this.client.backendId, this.receipt.executionId, artifactId, index.toString()]);
  }
  writableBinding(currentHandle: SessionHandle | null): RuntimeResultBinding | null {
    const binding = this.view?.runtimeBinding;
    return binding && binding.executionId === this.receipt.executionId &&
      sameHandle(binding.handle, this.originalHandle) && sameHandle(binding.handle, currentHandle)
      ? binding : null;
  }
  invalidateRuntimeBinding(): void {
    if (this.view) this.view = { ...this.view, runtimeBinding: null };
    this.publish();
  }
  private async read(artifactId: string, index: bigint, source: StatementResultSource | null): Promise<void> {
    const key = this.key(artifactId, index);
    if (this.chunksByKey.has(key)) return;
    const existing = this.reads.get(key);
    if (existing) return existing;
    const read = (async () => {
      const chunk = await this.client.readArtifact({ artifactId: artifactId as ArtifactChunk['artifactId'], chunkIndex: wireCounter(index) });
      if (chunk.artifactId !== artifactId || counterValue(chunk.chunkIndex) !== index) {
        throw new Error('Artifact read returned a different chunk');
      }
      this.chunksByKey.set(key, { chunk, source });
      this.publish();
    })().finally(() => { this.reads.delete(key); });
    this.reads.set(key, read);
    return read;
  }

  async recover(): Promise<void> {
    const view = await this.client.getExecution(this.receipt.executionId);
    if (view.executionId !== this.receipt.executionId) throw new Error('Execution identity mismatch');
    this.view = view;
    this.publish();
    for (const artifactId of view.artifactIds) {
      // Publication grows while writing: fetch fresh metadata even when chunk
      // zero was already displayed before a disconnect.
      const first = await this.client.readArtifact({ artifactId, chunkIndex: wireCounter(0n) });
      if (first.artifactId !== artifactId) throw new Error('Artifact identity mismatch');
      const count = first?.totalChunks ?? first?.publishedChunkCount;
      if (count === undefined || count === null) throw new Error('Artifact publication metadata unavailable');
      if (counterValue(count) > 0n) {
        const key = this.key(artifactId, 0n);
        if (!this.chunksByKey.has(key)) this.chunksByKey.set(key, { chunk: first, source: null });
      }
      for (let index = 1n; index < counterValue(count); index++) await this.read(artifactId, index, null);
    }
    this.error = null;
    this.publish();
  }

  async accept(event: EventEnvelope<ConnectionEvent>): Promise<void> {
    if (event.streamId !== this.receipt.streamId) return;
    if (event.executionId && event.executionId !== this.receipt.executionId) return;
    const next = counterValue(event.sequence);
    if (this.sequence !== null && next <= this.sequence) return;
    if (this.sequence !== null && next !== this.sequence + 1n) await this.recover();
    const payload = event.payload;
    if (payload.kind === 'streamResetRequired') {
      this.sequence = null;
      await this.recover();
      return;
    }
    if (payload.kind === 'sessionChanged') {
      if (sameHandle(payload.session.handle, this.originalHandle) &&
          sameHandle(event.sessionHandle, this.originalHandle)) this.onSession?.(payload);
    } else if (payload.kind === 'executionChanged') {
      if (payload.execution.executionId !== this.receipt.executionId) return;
      this.view = payload.execution;
      this.publish();
    } else if (payload.kind === 'resultChunk') {
      if (payload.source.executionId !== this.receipt.executionId) return;
      await this.read(payload.artifactId, counterValue(payload.chunkIndex), payload.source);
    }
    // Advance only after reads succeed, so failures remain recoverable.
    this.sequence = next;
  }

  async consume(): Promise<void> {
    await this.unsubscribe();
    const generation = ++this.generation;
    const stream = this.client.subscribeEvents({
      streamId: this.receipt.streamId, afterSequence: this.sequence === null ? null : wireCounter(this.sequence),
    });
    const iterator = stream[Symbol.asyncIterator]();
    this.iterator = iterator;
    try {
      while (generation === this.generation) {
        const item = await iterator.next();
        if (item.done || generation !== this.generation) break;
        await this.accept(item.value);
      }
    } catch (error) {
      if (generation === this.generation) { this.error = error; this.publish(); }
    } finally {
      if (this.iterator === iterator) this.iterator = null;
      await iterator.return?.();
    }
  }
  async unsubscribe(): Promise<void> {
    ++this.generation;
    const iterator = this.iterator;
    this.iterator = null;
    await iterator?.return?.();
  }
  dispose(): Promise<void> {
    this.listeners.clear();
    return this.unsubscribe();
  }
}
