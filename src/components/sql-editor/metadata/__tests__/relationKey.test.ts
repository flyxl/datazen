import { describe, expect, it } from 'vitest';
import type { QualifiedRelationId } from '../../semantic/types';
import {
  buildEditorRelationKey,
  hasNamespace,
  qualifiedNameText,
  relationBaseName,
  relationIdentityKey,
} from '../relationKey';

function ident(namespace: Array<[string, boolean]>, name: [string, boolean]): QualifiedRelationId {
  return {
    namespacePath: namespace.map(([n, q]) => ({ name: n, quoted: q })),
    name: { name: name[0], quoted: name[1] },
  };
}

describe('metadata relationKey', () => {
  it('folds unquoted identifiers for a dialect (postgres → lower)', () => {
    const id = ident([['public', false]], ['USERS', false]);
    expect(relationIdentityKey(id, 'postgresql')).toBe('public.users');
  });

  it('preserves quoted identifier case', () => {
    const id = ident([], ['Users', true]);
    expect(relationIdentityKey(id, 'postgresql')).toBe('Users');
  });

  it('keeps same-name relations in different namespaces distinct', () => {
    const a = ident([['public', false]], ['users', false]);
    const b = ident([['private', false]], ['users', false]);
    expect(relationIdentityKey(a, 'postgresql')).not.toBe(relationIdentityKey(b, 'postgresql'));
  });

  it('builds editor key with dbSessionId prefix', () => {
    const id = ident([['public', false]], ['users', false]);
    expect(buildEditorRelationKey('sess-1', id, 'postgresql')).toBe('sess-1::public.users');
  });

  it('renders qualified name text, preserving quotes', () => {
    expect(qualifiedNameText(ident([['public', false]], ['users', false]), 'postgresql')).toBe(
      'public.users',
    );
    expect(qualifiedNameText(ident([['MySchema', true]], ['Users', true]), 'postgresql')).toBe(
      '"MySchema"."Users"',
    );
  });

  it('returns bare name when there is no namespace', () => {
    expect(qualifiedNameText(ident([], ['users', false]), 'postgresql')).toBe('users');
  });

  it('returns base name normalized to dialect case', () => {
    expect(relationBaseName(ident([['public', false]], ['USERS', false]), 'postgresql')).toBe(
      'users',
    );
    expect(relationBaseName(ident([], ['Users', true]), 'postgresql')).toBe('Users');
  });

  it('detects whether a relation carries an explicit namespace', () => {
    expect(hasNamespace(ident([['public', false]], ['users', false]))).toBe(true);
    expect(hasNamespace(ident([], ['users', false]))).toBe(false);
  });
});
