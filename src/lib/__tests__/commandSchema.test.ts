import { describe, expect, it } from 'vitest';
import type { DriverCommandDefinition } from '../../types';
import {
  allPrivileges,
  fieldPlaceholder,
  hasCommand,
  hasSchemaField,
  privilegeGroups,
  schemaProperties,
} from '../commandSchema';

const pgCreateDatabase: DriverCommandDefinition = {
  id: 'create_database',
  name: 'Create Database',
  inputSchema: {
    type: 'object',
    properties: {
      name: { type: 'string' },
      encoding: { type: 'string', examples: ['UTF8'] },
      owner: { type: 'string', examples: ['postgres'] },
    },
    required: ['name'],
  },
  permissions: [],
};

const pgGrant: DriverCommandDefinition = {
  id: 'grant_privileges',
  name: 'Grant Privileges',
  inputSchema: {
    type: 'object',
    properties: {
      username: { type: 'string' },
      privileges: { type: 'array' },
      grantOption: { type: 'boolean' },
    },
    required: ['username', 'privileges'],
    'x-datazen': {
      privilegeGroups: [
        { label: 'Table', privileges: ['SELECT', 'INSERT'] },
        { label: 'Database', privileges: ['CONNECT'] },
      ],
    },
  },
  permissions: [],
};

describe('commandSchema', () => {
  it('lists schema properties from command definitions', () => {
    const fields = schemaProperties(pgCreateDatabase).map((field) => field.name);
    expect(fields).toEqual(['name', 'encoding', 'owner']);
    expect(hasSchemaField(pgCreateDatabase, 'owner')).toBe(true);
    expect(hasSchemaField(pgCreateDatabase, 'missing')).toBe(false);
  });

  it('reads placeholders from schema examples', () => {
    const encoding = schemaProperties(pgCreateDatabase).find((field) => field.name === 'encoding');
    expect(fieldPlaceholder(encoding!)).toBe('UTF8');
  });

  it('reads privilege groups from x-datazen extension', () => {
    expect(privilegeGroups(pgGrant)).toEqual([
      { label: 'Table', privileges: ['SELECT', 'INSERT'] },
      { label: 'Database', privileges: ['CONNECT'] },
    ]);
    expect(allPrivileges(pgGrant)).toEqual(['SELECT', 'INSERT', 'CONNECT']);
  });

  it('detects supported commands', () => {
    expect(hasCommand([pgCreateDatabase, pgGrant], 'drop_user')).toBe(false);
    expect(hasCommand([pgCreateDatabase, pgGrant], 'create_database')).toBe(true);
  });
});
