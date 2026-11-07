import { describe, expect, it } from 'vitest';
import { metadataSessionKey, type MetadataSessionIdentity } from '../../metadata';
import {
  editorSessionCacheKey,
  registerEditorSessionConfigResolver,
  resolveEditorSessionConfig,
  UNBOUND_SESSION_CONFIG,
} from '../sessionConfig';
import type { EditorSessionConfig } from '../types';

describe('editorSessionCacheKey', () => {
  it('addresses a partition by identity, configuration and session', () => {
    const key = editorSessionCacheKey('s1', {
      connectionId: 'conn-1',
      configRevision: 7,
    });
    expect(key).toContain('conn-1');
    expect(key).toContain('7');
    expect(key).toContain('s1');
  });

  it('is derived from the same session prefix the per-relation key uses', () => {
    // A partition address and a relation cache entry must never disagree about
    // which configuration they belong to, so both come from one derivation.
    expect(editorSessionCacheKey('s1', { connectionId: 'conn-1', configRevision: 3 })).toBe(
      metadataSessionKey({
        // Branded ids: the test names a concrete connection/session, so the
        // brands are asserted here once instead of at every call site.
        connectionId: 'conn-1' as MetadataSessionIdentity['connectionId'],
        configRevision: 3 as MetadataSessionIdentity['configRevision'],
        dbSessionId: 's1' as MetadataSessionIdentity['dbSessionId'],
      }),
    );
  });

  it('separates two configuration revisions of the same session', () => {
    const before = editorSessionCacheKey('s1', { connectionId: 'conn-1', configRevision: 1 });
    const after = editorSessionCacheKey('s1', { connectionId: 'conn-1', configRevision: 2 });
    expect(before).not.toBe(after);
  });

  it('separates two connection configs running under one session id', () => {
    const a = editorSessionCacheKey('s1', { connectionId: 'conn-a', configRevision: 0 });
    const b = editorSessionCacheKey('s1', { connectionId: 'conn-b', configRevision: 0 });
    expect(a).not.toBe(b);
  });

  it('keeps the unbound partition distinct from every bound one', () => {
    const unbound = editorSessionCacheKey('s1');
    expect(unbound).not.toBe(
      editorSessionCacheKey('s1', { connectionId: '', configRevision: 1 }),
    );
    expect(unbound).not.toBe(editorSessionCacheKey('s1', { connectionId: 'conn-1', configRevision: 0 }));
  });
});

describe('resolveEditorSessionConfig', () => {
  it('reports the unpublished partition when no host is registered', () => {
    expect(resolveEditorSessionConfig('s1')).toEqual(UNBOUND_SESSION_CONFIG);
  });

  it('uses the registered resolver', () => {
    const unregister = registerEditorSessionConfigResolver(() => ({
      connectionId: 'conn-9',
      configRevision: 4,
    }));
    try {
      expect(resolveEditorSessionConfig('s1')).toEqual({
        connectionId: 'conn-9',
        configRevision: 4,
      });
      expect(editorSessionCacheKey('s1')).toBe(
        editorSessionCacheKey('s1', { connectionId: 'conn-9', configRevision: 4 }),
      );
    } finally {
      unregister();
    }
    expect(resolveEditorSessionConfig('s1')).toEqual(UNBOUND_SESSION_CONFIG);
  });

  it('treats a resolver that knows nothing as unbound, not as a fallback mode', () => {
    const unregister = registerEditorSessionConfigResolver(
      (dbSessionId) =>
        dbSessionId === 'known' ? { connectionId: 'conn-1', configRevision: 2 } : null,
    );
    try {
      expect(resolveEditorSessionConfig('known')).toEqual({
        connectionId: 'conn-1',
        configRevision: 2,
      });
      // Unknown sessions land in the unbound partition, which is its own
      // address — never in `known`'s.
      expect(editorSessionCacheKey('other')).not.toBe(editorSessionCacheKey('known'));
      expect(resolveEditorSessionConfig('other')).toEqual(UNBOUND_SESSION_CONFIG);
    } finally {
      unregister();
    }
  });

  it('unregisters in LIFO order and leaves a newer resolver alone', () => {
    const first = { connectionId: 'conn-1', configRevision: 1 } satisfies EditorSessionConfig;
    const second = { connectionId: 'conn-2', configRevision: 1 } satisfies EditorSessionConfig;
    const unregisterFirst = registerEditorSessionConfigResolver(() => first);
    const unregisterSecond = registerEditorSessionConfigResolver(() => second);
    expect(resolveEditorSessionConfig('s1')).toEqual(second);
    // Stale unregister: the newer resolver owns the slot, so tearing it down
    // here would strand the host's live binding.
    unregisterFirst();
    expect(resolveEditorSessionConfig('s1')).toEqual(second);
    unregisterSecond();
    expect(resolveEditorSessionConfig('s1')).toEqual(first);
    unregisterFirst();
    expect(resolveEditorSessionConfig('s1')).toEqual(UNBOUND_SESSION_CONFIG);
  });
});