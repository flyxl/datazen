import type { FunctionEntry } from '@datazen/driver-sdk';

export const sqliteFunctions: readonly FunctionEntry[] = [
  {
    name: 'IFNULL',
    description: 'Return alternative if expression is NULL',
    params: [
      { name: 'expr', type: 'any' },
      { name: 'alt', type: 'any' },
    ],
    dialects: ['sqlite'],
  },
  {
    name: 'DATETIME',
    description: 'Format date as string',
    params: [
      { name: 'date', type: 'date' },
      { name: 'modifier', type: 'string', optional: true },
    ],
    dialects: ['sqlite'],
  },
  {
    name: 'PRINTF',
    description: 'Format string with printf-style arguments',
    params: [
      { name: 'format', type: 'string' },
      { name: 'value', type: 'any' },
    ],
    dialects: ['sqlite'],
  },
  {
    name: 'JSON_EXTRACT',
    description: 'Extract value from JSON document',
    params: [
      { name: 'json_doc', type: 'string' },
      { name: 'path', type: 'string' },
    ],
    dialects: ['sqlite'],
  },
  {
    name: 'TYPEOF',
    description: 'Return the type of an expression',
    params: [{ name: 'expr', type: 'any' }],
    dialects: ['sqlite'],
  },
  {
    name: 'HEX',
    description: 'Convert to hexadecimal string',
    params: [{ name: 'value', type: 'any' }],
    dialects: ['sqlite'],
  },
];
