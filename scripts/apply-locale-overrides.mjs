#!/usr/bin/env node
/** Apply manual translation fixes after machine translation. */
import { readFileSync, writeFileSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');
const DATA = resolve(ROOT, 'scripts/locale-data');

const OVERRIDES = {
  de: {
    'common.ok': 'Okay',
    'common.cancel': 'Abbrechen',
    'settings.langEn': 'English',
    'settings.langZh': '简体中文',
  },
  es: {
    'common.ok': 'Aceptar',
    'common.cancel': 'Cancelar',
    'settings.langEn': 'English',
    'settings.langZh': '简体中文',
  },
  fr: {
    'common.ok': 'Valider',
    'common.cancel': 'Annuler',
    'settings.langEn': 'English',
    'settings.langZh': '简体中文',
  },
  ja: {
    'common.ok': '了解',
    'common.cancel': 'キャンセル',
    'settings.langEn': 'English',
    'settings.langZh': '简体中文',
  },
  ko: {
    'common.ok': '확인',
    'common.cancel': '취소',
    'settings.langEn': 'English',
    'settings.langZh': '简体中文',
  },
  'pt-BR': {
    'common.ok': 'Confirmar',
    'common.cancel': 'Cancelar',
    'settings.langEn': 'English',
    'settings.langZh': '简体中文',
  },
  ru: {
    'common.ok': 'ОК',
    'common.cancel': 'Отмена',
    'settings.langEn': 'English',
    'settings.langZh': '简体中文',
  },
};

for (const [locale, fixes] of Object.entries(OVERRIDES)) {
  const path = resolve(DATA, `${locale}.json`);
  const data = JSON.parse(readFileSync(path, 'utf-8'));
  Object.assign(data, fixes);
  writeFileSync(path, JSON.stringify(data, null, 2) + '\n');
  console.log(`[locale-overrides] patched ${locale} (${Object.keys(fixes).length} keys)`);
}
