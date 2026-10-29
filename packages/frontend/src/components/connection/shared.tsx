import type { TranslationKey } from '../../locales';

export function newId() {
  return `conn_${Math.random().toString(36).slice(2, 10)}`;
}

export const COLOR_KEYS = [
  { value: '#ef4444', key: 'newConn.colorRed' },
  { value: '#f59e0b', key: 'newConn.colorOrange' },
  { value: '#22c55e', key: 'newConn.colorGreen' },
  { value: '#3b82f6', key: 'newConn.colorBlue' },
  { value: '#8b5cf6', key: 'newConn.colorPurple' },
  { value: '#ec4899', key: 'newConn.colorPink' },
  { value: '#64748b', key: 'newConn.colorGray' },
] as const satisfies readonly { value: string; key: TranslationKey }[];

export { Label } from '@datazen/ui';
