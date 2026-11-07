import type { FunctionEntry } from '@datazen/driver-sdk';

export const mysqlFunctions: readonly FunctionEntry[] = [
  {
    name: 'IF',
    description: 'Return expr2 if expr1 is TRUE, else expr3',
    params: [
      { name: 'expr1', type: 'any' },
      { name: 'expr2', type: 'any' },
      { name: 'expr3', type: 'any' },
    ],
    dialects: ['mysql', 'mariadb', 'tidb'],
  },
  {
    name: 'DATE_ADD',
    description: 'Add an interval to a date/datetime',
    params: [
      { name: 'date', type: 'date' },
      { name: 'interval', type: 'interval' },
    ],
    dialects: ['mysql', 'mariadb', 'tidb'],
  },
  {
    name: 'DATE_FORMAT',
    description: 'Format date as string',
    params: [
      { name: 'date', type: 'date' },
      { name: 'format', type: 'string' },
    ],
    dialects: ['mysql', 'mariadb', 'tidb'],
  },
  {
    name: 'IFNULL',
    description: 'Return alternative if expression is NULL',
    params: [
      { name: 'expr', type: 'any' },
      { name: 'alt', type: 'any' },
    ],
    dialects: ['mysql', 'mariadb', 'tidb'],
  },
  {
    name: 'DATEDIFF',
    description: 'Difference between two dates',
    params: [
      { name: 'expr1', type: 'date' },
      { name: 'expr2', type: 'date' },
    ],
    dialects: ['mysql', 'mariadb', 'tidb'],
  },
  {
    name: 'GROUP_CONCAT',
    description: 'Concatenate grouped values',
    params: [{ name: 'expr', type: 'any' }],
    dialects: ['mysql', 'mariadb', 'tidb'],
  },
  {
    name: 'JSON_EXTRACT',
    description: 'Extract value from JSON document',
    params: [
      { name: 'json_doc', type: 'string' },
      { name: 'path', type: 'string' },
    ],
    dialects: ['mysql', 'mariadb', 'tidb'],
  },
  {
    name: 'NOW',
    description: 'Current timestamp',
    params: [],
    dialects: ['mysql', 'mariadb', 'tidb'],
  },
  {
    name: 'CONCAT_WS',
    description: 'Concatenate strings with a separator',
    params: [
      { name: 'separator', type: 'string' },
      { name: 'str1', type: 'string' },
      { name: 'str2', type: 'string' },
    ],
    dialects: ['mysql', 'mariadb', 'tidb'],
  },
];
