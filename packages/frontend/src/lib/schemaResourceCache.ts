/** TTL cache with a request-identity barrier across invalidation. */
export class SchemaResourceCache<T> {
  private values = new Map<string, { value: T; at: number }>();
  private errors = new Map<string, { error: unknown; at: number }>();
  private inflight = new Map<string, Promise<T>>();

  keys(): string[] {
    return [...new Set([...this.values.keys(), ...this.errors.keys(), ...this.inflight.keys()])];
  }

  invalidate(matches: (key: string) => boolean): void {
    for (const key of this.keys()) {
      if (!matches(key)) continue;
      this.values.delete(key);
      this.errors.delete(key);
      // Detach old requests; their completion cannot overwrite new requests.
      this.inflight.delete(key);
    }
  }

  async get(key: string, load: () => Promise<T>): Promise<T> {
    const cached = this.values.get(key);
    if (cached && Date.now() - cached.at < 60_000) return cached.value;
    const failed = this.errors.get(key);
    if (failed && Date.now() - failed.at < 10_000) throw failed.error;
    const existing = this.inflight.get(key);
    if (existing) return existing;

    const pending = load()
      .then((value) => {
        if (this.inflight.get(key) === pending) {
          this.values.set(key, { value, at: Date.now() });
          this.errors.delete(key);
        }
        return value;
      })
      .catch((error: unknown) => {
        if (this.inflight.get(key) === pending) {
          this.errors.set(key, { error, at: Date.now() });
        }
        throw error;
      })
      .finally(() => {
        if (this.inflight.get(key) === pending) this.inflight.delete(key);
      });
    this.inflight.set(key, pending);
    return pending;
  }
}
