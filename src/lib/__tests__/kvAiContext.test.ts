import { describe, expect, it } from 'vitest';
import { buildKvAiContext, composeKvAiMessage, hasKvAiFacts, type KvAiFacts } from '../kvAiContext';

/**
 * The host may only inject facts it really owns, and the affordance must
 * disappear together with the facts. These cases pin the binary rule
 * ("no key in scope ⇒ no context ⇒ no AI button") in one place; the component
 * tests below it then only check that the two call sites act on it.
 */
const FACTS: KvAiFacts = {
  connectionName: 'KV Local',
  dbSessionId: 'sess-1',
  database: 'db7',
  selectedKey: 'user:42',
};

describe('hasKvAiFacts (empty-state rule)', () => {
  it('reports no facts while no key is in scope', () => {
    expect(hasKvAiFacts({ selectedKey: null })).toBe(false);
  });

  it('treats a blank key as no key', () => {
    // A tree that publishes '' must not resurrect the button with an empty block.
    expect(hasKvAiFacts({ selectedKey: '' })).toBe(false);
    expect(hasKvAiFacts({ selectedKey: '   ' })).toBe(false);
  });

  it('reports facts for a real key even when nothing else resolved', () => {
    // The key alone is already a fact worth sending; `database` may still be null.
    expect(hasKvAiFacts({ selectedKey: 'user:42' })).toBe(true);
  });
});

describe('buildKvAiContext (host-owned facts only)', () => {
  it('returns null instead of an empty block', () => {
    // The half-broken state is banned: this is the branch that hides the button.
    expect(buildKvAiContext({ ...FACTS, selectedKey: null })).toBeNull();
    expect(buildKvAiContext({ ...FACTS, selectedKey: '  ' })).toBeNull();
  });

  it('carries exactly the four facts the host owns', () => {
    const ctx = buildKvAiContext(FACTS);
    expect(ctx?.keyName).toBe('user:42');
    expect(ctx?.block.split('\n')).toEqual([
      '[kv-context]',
      'connection=KV Local',
      'session=sess-1',
      'database=db7',
      'selected_key=user:42',
      '[/kv-context]',
    ]);
  });

  it('keeps an unresolved database as an empty token rather than a guess', () => {
    const ctx = buildKvAiContext({ ...FACTS, database: null });
    expect(ctx?.block).toContain('database=\n');
  });

  it('never mentions the driver console buffer', () => {
    // The console buffer is deliberately not injected, and this track may not
    // start a cache to obtain it: whatever is not in `KvAiFacts` cannot reach the
    // prompt.
    const ctx = buildKvAiContext(FACTS);
    expect(ctx?.block).not.toMatch(/console|history|buffer/i);
  });

  it('is a stable value for stable facts (memo input, not a getter)', () => {
    // The drawer memoises on the scalar it subscribed to; two identical fact sets
    // must produce byte-identical blocks so no downstream effect re-fires.
    const a = buildKvAiContext(FACTS);
    const b = buildKvAiContext({ ...FACTS });
    expect(a?.block).toBe(b?.block);
  });
});

describe('composeKvAiMessage', () => {
  it('is the identity for a panel without KV context', () => {
    expect(composeKvAiMessage('why is this key big?', null)).toBe('why is this key big?');
  });

  it('appends the block to the user question', () => {
    const ctx = buildKvAiContext(FACTS);
    const composed = composeKvAiMessage('why is this key big?', ctx ?? null);
    expect(composed.startsWith('why is this key big?\n\n')).toBe(true);
    expect(composed).toContain('selected_key=user:42');
  });
});
