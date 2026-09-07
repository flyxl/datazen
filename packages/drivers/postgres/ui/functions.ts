import type { FunctionEntry } from '@datazen/driver-sdk';

export const postgresqlFunctions: readonly FunctionEntry[] = [
  {
    name: 'DATE_TRUNC',
    description: 'Truncate date to specified precision',
    params: [
      { name: 'precision', type: 'string' },
      { name: 'date', type: 'date' },
    ],
    dialects: ['postgresql', 'postgres', 'pg', 'cockroach'],
  },
  {
    name: 'TO_CHAR',
    description: 'Format timestamp as string',
    params: [
      { name: 'timestamp', type: 'date' },
      { name: 'format', type: 'string' },
    ],
    dialects: ['postgresql', 'postgres', 'pg', 'cockroach'],
  },
  {
    name: 'TO_TIMESTAMP',
    description: 'Convert string to timestamp',
    params: [{ name: 'text', type: 'string' }],
    dialects: ['postgresql', 'postgres', 'pg', 'cockroach'],
  },
  {
    name: 'JSONB_BUILD_OBJECT',
    description: 'Build a JSONB object from key-value pairs',
    params: [
      { name: 'key1', type: 'string' },
      { name: 'value1', type: 'any' },
    ],
    dialects: ['postgresql', 'postgres', 'pg', 'cockroach'],
  },
  {
    name: 'ARRAY_AGG',
    description: 'Aggregate values into an array',
    params: [{ name: 'expr', type: 'any' }],
    dialects: ['postgresql', 'postgres', 'pg', 'cockroach'],
  },
  {
    name: 'STRING_AGG',
    description: 'Concatenate values with separator',
    params: [
      { name: 'expr', type: 'string' },
      { name: 'separator', type: 'string' },
    ],
    dialects: ['postgresql', 'postgres', 'pg', 'cockroach'],
  },
  {
    name: 'GENERATE_SERIES',
    description: 'Generate a series of values',
    params: [
      { name: 'start', type: 'number' },
      { name: 'stop', type: 'number' },
      { name: 'step', type: 'number', optional: true },
    ],
    dialects: ['postgresql', 'postgres', 'pg', 'cockroach'],
  },
  {
    name: 'NOW',
    description: 'Current timestamp',
    params: [],
    dialects: ['postgresql', 'postgres', 'pg', 'cockroach'],
  },
  {
    name: 'AGE',
    description: 'Subtract arguments producing a symbolic result',
    params: [
      { name: 'timestamp', type: 'date' },
      { name: 'timestamp2', type: 'date', optional: true },
    ],
    dialects: ['postgresql', 'postgres', 'pg', 'cockroach'],
  },
  {
    name: 'CONCAT_WS',
    description: 'Concatenate strings with a separator',
    params: [
      { name: 'separator', type: 'string' },
      { name: 'str1', type: 'string' },
      { name: 'str2', type: 'string' },
    ],
    dialects: ['postgresql', 'postgres', 'pg', 'cockroach'],
  },
];
