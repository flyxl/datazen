#!/usr/bin/env node
/**
 * Build zh-TW.ts from zh-CN.ts using OpenCC s2t + manual fixes.
 */
import { readFileSync, writeFileSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';
import * as OpenCC from 'opencc-js';

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(__dirname, '..');

function parseLocaleTs(source) {
  const map = new Map();
  const re = /'((?:\\'|[^'])*)':\s*'((?:\\'|[^'])*)'/g;
  let m;
  while ((m = re.exec(source)) !== null) {
    map.set(m[1].replace(/\\'/g, "'"), m[2].replace(/\\'/g, "'").replace(/\\n/g, '\n'));
  }
  return map;
}

function escapeTs(value) {
  return value.replace(/\\/g, '\\\\').replace(/'/g, "\\'").replace(/\n/g, '\\n');
}

const MANUAL = {
  'settings.language': '語言',
  'common.ok': '確定',
  'common.cancel': '取消',
  'common.rows': '列',
  'common.columns': '欄',
  'menu.settings': '設定',
  'menu.ctxAddFavorite': '加入收藏',
  'action.exportConfig': '匯出應用資料…',
  'action.importConfig': '匯入應用資料…',
  'appData.exportSuccess':
    '應用資料已成功匯出。ZIP 中不包含加密金鑰 — 若要在其他機器解密已儲存的密碼，請另行備份金鑰。',
  'appData.importConfirmTitle': '匯入應用資料',
  'appData.importConfirmMessage':
    '此操作將覆寫所有應用資料（現有日誌檔案會保留）。備份不包含加密金鑰 — 密碼僅在本機（或相同金鑰的機器）還原後可用。是否繼續？',
  'appData.backupKeyTitle': '備份加密金鑰',
  'appData.backupKeyMessage':
    '是否現在另存加密金鑰？請妥善保管；在其他機器還原應用資料後，需要此金鑰才能解密已儲存的密碼。',
  'appData.backupKeySaved': '加密金鑰已儲存。',
  'appData.backupKeyFailed': '無法儲存加密金鑰。',
  'connShare.exportTitle': '匯出連線',
  'connShare.importTitle': '匯入連線',
  'connShare.exportSuccess': '已匯出 {count} 個連線',
  'connShare.importSuccess': '新增 {imported}、更新 {overwritten}、新增 {groupsAdded} 個群組',
  'query.cancelled': '查詢已取消',
  'settings.langZh': '简体中文',
  'settings.langEn': 'English',
  'win.settings': '設定 - DataZen',
};

const converter = OpenCC.Converter({ from: 'cn', to: 'tw' });
const zhCN = parseLocaleTs(readFileSync(resolve(ROOT, 'src/locales/zh-CN.ts'), 'utf-8'));
const en = parseLocaleTs(readFileSync(resolve(ROOT, 'src/locales/en.ts'), 'utf-8'));

const lines = [];
for (const [key, cnVal] of zhCN) {
  let val = MANUAL[key] ?? converter(cnVal);
  // Fill any gap from en via opencc won't work - use zh-CN
  if (!val && en.has(key)) val = en.get(key);
  lines.push(`  '${key}': '${escapeTs(val)}',`);
}

const out = `const translations = {\n${lines.join('\n')}\n} as const;\n\nexport type TranslationKey = keyof typeof translations;\nexport default translations;\n`;
writeFileSync(resolve(ROOT, 'src/locales/zh-TW.ts'), out, 'utf-8');
console.log(`[build-zh-tw] wrote zh-TW.ts (${zhCN.size} keys)`);
