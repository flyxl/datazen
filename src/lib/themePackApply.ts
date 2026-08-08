import { themeCommands } from '../commands/theme';
import { emitCrossWindow } from './crossWindowBus';
import { bootstrapDefaultIconResolver } from './bootstrapIconResolver';
import { UI_ICON_IDS } from './iconIds';
import { createIconResolver, setActiveIconResolver, type IconSourceMap } from './iconResolver';
import { getDbIcon, getDriverIconMap } from './databaseTypes';
import { buildHostLucideById } from './hostLucideMap';
import type { DatabaseType } from '../types';
import {
  parsePackEditorOverlay,
  setPackEditorColorOverlay,
} from './themeEditorColors';
import { setChartPaletteOverride } from './chart/colors';

export const THEME_PACK_STYLE_ID = 'datazen-theme-pack';

const ICON_EXTENSIONS = ['.svg', '.webp', '.png'] as const;
const FONT_URL_RE = /url\s*\(\s*(['"]?)([^'")]+)\1\s*\)/gi;

const MIME_BY_EXT: Record<string, string> = {
  '.svg': 'image/svg+xml',
  '.webp': 'image/webp',
  '.png': 'image/png',
  '.woff2': 'font/woff2',
  '.woff': 'font/woff',
};

let activeBlobUrls: string[] = [];

function trackBlobUrl(url: string): string {
  activeBlobUrls.push(url);
  return url;
}

function revokeBlobUrls(): void {
  for (const url of activeBlobUrls) {
    URL.revokeObjectURL(url);
  }
  activeBlobUrls = [];
}

function bytesToBlobUrl(bytes: number[], mimeType: string): string {
  const blob = new Blob([new Uint8Array(bytes)], { type: mimeType });
  return trackBlobUrl(URL.createObjectURL(blob));
}

function mimeForPath(path: string): string {
  const dot = path.lastIndexOf('.');
  const ext = dot >= 0 ? path.slice(dot).toLowerCase() : '';
  return MIME_BY_EXT[ext] ?? 'application/octet-stream';
}

function decodeUtf8(bytes: number[]): string {
  return new TextDecoder().decode(new Uint8Array(bytes));
}

export function injectThemePackCss(css: string): void {
  let el = document.getElementById(THEME_PACK_STYLE_ID);
  if (!el) {
    el = document.createElement('style');
    el.id = THEME_PACK_STYLE_ID;
    document.head.appendChild(el);
  }
  el.textContent = css;
}

export function clearThemePackDom(): void {
  document.getElementById(THEME_PACK_STYLE_ID)?.remove();
}

export function syncWebviewBackgroundFromTokens(): void {
  const surface = getComputedStyle(document.documentElement).getPropertyValue('--c-surface').trim();
  if (surface) {
    document.documentElement.style.backgroundColor = surface;
    return;
  }
  const isDark = document.documentElement.classList.contains('dark');
  document.documentElement.style.backgroundColor = isDark ? '#0f172a' : '#ffffff';
}

function notifyThemePackChanged(): void {
  document.dispatchEvent(new CustomEvent('datazen:theme-pack-changed'));
}

async function readPackFile(packId: string, relativePath: string): Promise<number[] | null> {
  try {
    return await themeCommands.readThemePackFile(packId, relativePath);
  } catch {
    return null;
  }
}

export async function rewriteFontUrls(css: string, packId: string): Promise<string> {
  const replacements: { start: number; end: number; url: string }[] = [];
  let match: RegExpExecArray | null;
  const re = new RegExp(FONT_URL_RE.source, 'gi');
  while ((match = re.exec(css)) !== null) {
    const rawUrl = match[2].trim();
    if (/^https?:/i.test(rawUrl)) {
      throw new Error(`Remote font URL not allowed: ${rawUrl}`);
    }
    if (rawUrl.startsWith('blob:') || rawUrl.startsWith('data:')) {
      continue;
    }
    const relPath = rawUrl.replace(/^\.\//, '');
    const bytes = await readPackFile(packId, relPath);
    if (!bytes) {
      throw new Error(`Font file not found in pack: ${relPath}`);
    }
    const blobUrl = bytesToBlobUrl(bytes, mimeForPath(relPath));
    replacements.push({ start: match.index, end: match.index + match[0].length, url: blobUrl });
  }
  if (replacements.length === 0) return css;
  let result = css;
  for (let i = replacements.length - 1; i >= 0; i--) {
    const { start, end, url } = replacements[i];
    result = `${result.slice(0, start)}url("${url}")${result.slice(end)}`;
  }
  return result;
}

async function probePackIcon(packId: string, semanticId: string): Promise<string | null> {
  for (const ext of ICON_EXTENSIONS) {
    const relPath = `icons/${semanticId}${ext}`;
    const bytes = await readPackFile(packId, relPath);
    if (bytes && bytes.length > 0) {
      return bytesToBlobUrl(bytes, mimeForPath(relPath));
    }
  }
  return null;
}

async function loadPackIcons(packId: string): Promise<IconSourceMap> {
  const ids = [...UI_ICON_IDS, ...Object.keys(getDriverIconMap())];
  const map: IconSourceMap = {};
  await Promise.all(
    ids.map(async (id) => {
      const url = await probePackIcon(packId, id);
      if (url) map[id] = url;
    }),
  );
  return map;
}

function installIconResolver(packIcons: IconSourceMap): void {
  setActiveIconResolver(
    createIconResolver({
      packIcons,
      driverIcons: getDriverIconMap(),
      lucideById: buildHostLucideById(),
      placeholderForDb: (dbType) => {
        const { label, bg } = getDbIcon(dbType as DatabaseType);
        return { label, bgClass: bg };
      },
    }),
  );
}

function parseChartsJson(bytes: number[]): Record<string, string[]> | null {
  try {
    const json = JSON.parse(decodeUtf8(bytes)) as unknown;
    if (!json || typeof json !== 'object') return null;
    const result: Record<string, string[]> = {};
    for (const [key, value] of Object.entries(json as Record<string, unknown>)) {
      if (Array.isArray(value) && value.every((entry) => typeof entry === 'string')) {
        result[key] = value;
      }
    }
    return Object.keys(result).length > 0 ? result : null;
  } catch {
    return null;
  }
}

function resetPackState(): void {
  clearThemePackDom();
  revokeBlobUrls();
  setPackEditorColorOverlay(null);
  setChartPaletteOverride(null);
  bootstrapDefaultIconResolver();
}

export function clearThemePack(): void {
  resetPackState();
  notifyThemePackChanged();
}

export async function applyThemePack(packId: string | null): Promise<void> {
  resetPackState();

  if (!packId) {
    syncWebviewBackgroundFromTokens();
    notifyThemePackChanged();
    void emitCrossWindow('datazen:theme-pack-changed', packId);
    return;
  }

  try {
    const tokensBytes = await readPackFile(packId, 'tokens.css');
    if (!tokensBytes) {
      throw new Error('tokens.css missing');
    }

    let css = await rewriteFontUrls(decodeUtf8(tokensBytes), packId);

    const fontsBytes = await readPackFile(packId, 'fonts.css');
    if (fontsBytes) {
      const fontsCss = await rewriteFontUrls(decodeUtf8(fontsBytes), packId);
      css = `${css}\n${fontsCss}`;
    }

    injectThemePackCss(css);

    const packIcons = await loadPackIcons(packId);
    installIconResolver(packIcons);

    const editorBytes = await readPackFile(packId, 'editor.json');
    if (editorBytes) {
      try {
        const overlay = parsePackEditorOverlay(JSON.parse(decodeUtf8(editorBytes)) as unknown);
        setPackEditorColorOverlay(overlay);
      } catch (err) {
        console.warn('[theme] failed to parse editor.json', err);
      }
    }

    const chartsBytes = await readPackFile(packId, 'charts.json');
    if (chartsBytes) {
      setChartPaletteOverride(parseChartsJson(chartsBytes));
    }

    syncWebviewBackgroundFromTokens();
    notifyThemePackChanged();
  } catch (err) {
    console.warn('[theme] failed to apply pack', packId, err);
    resetPackState();
    syncWebviewBackgroundFromTokens();
    notifyThemePackChanged();
  }
  void emitCrossWindow('datazen:theme-pack-changed', packId);
}
