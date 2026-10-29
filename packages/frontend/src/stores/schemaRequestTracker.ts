/** Request identity and session lifecycle barriers; no UI state lives here. */
export class SchemaRequestTracker {
  private sessions = new Map<string, Map<string, object>>();

  capture(dbSessionId: string): () => boolean {
    let session = this.sessions.get(dbSessionId);
    if (!session) {
      session = new Map();
      this.sessions.set(dbSessionId, session);
    }
    return () => this.sessions.get(dbSessionId) === session;
  }

  begin(dbSessionId: string, slot: string): () => boolean {
    const alive = this.capture(dbSessionId);
    const session = this.sessions.get(dbSessionId);
    const request = {};
    session?.set(slot, request);
    return () => alive() && session?.get(slot) === request;
  }

  captureSlot(dbSessionId: string, slot: string): () => boolean {
    const alive = this.capture(dbSessionId);
    const session = this.sessions.get(dbSessionId);
    if (!session?.has(slot)) return this.begin(dbSessionId, slot);
    const request = session.get(slot);
    return () => alive() && session.get(slot) === request;
  }

  invalidate(dbSessionId: string): void {
    this.sessions.delete(dbSessionId);
  }

  reset(): void {
    this.sessions.clear();
  }
}
