import { pluginCommands } from '../commands/plugins';
import { themeCommands } from '../commands/theme';
import { emitCrossWindow } from './crossWindowBus';
import { bootstrapDefaultIconResolver } from './bootstrapIconResolver';
import { UI_ICON_IDS } from './iconIds';
import { createIconResolver, setActiveIconResolver, type IconSourceMap } from './iconResolver';
import { getDbIcon, getDriverIconMap } from './databaseTypes';
import { buildHostLucideById } from './hostLucideMap';
import type { DatabaseType } from '../types';
import { parsePackEditorOverlay, setPackEditorColorOverlay } from './themeEditorColors';
import { setChartPaletteOverride } from './chart/colors';
import {
  DEFAULT_SURFACE_DARK,
  DEFAULT_SURFACE_LIGHT,
  cssColorToHex,
  persistSurfaceBackground,
} from './surfaceBgCache';

export const THEME_PACK_STYLE_ID = 'datazen-theme-pack';

/**
 * Prefix persisted in `settings.theme.packId` for themes contributed by UI
 * plugins: `plugin:{pluginId}:{themeId}`. Plugin ids (`<publisher>.<name>`)
 * and theme ids never contain colons, so the first colon splits reliably.
 */
export const PLUGIN_THEME_PACK_PREFIX = 'plugin:';

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
  const isDark = document.documentElement.classList.contains('dark');
  const fallback = isDark ? DEFAULT_SURFACE_DARK : DEFAULT_SURFACE_LIGHT;
  const surface = getComputedStyle(document.documentElement).getPropertyValue('--c-surface').trim();
  document.documentElement.style.backgroundColor = surface || fallback;
  const computed = getComputedStyle(document.documentElement).backgroundColor;
  const hex = cssColorToHex(computed) ?? cssColorToHex(surface) ?? fallback;
  document.documentElement.style.backgroundColor = hex;
  persistSurfaceBackground(isDark, hex);
  syncNativeWindowBackground(hex);
}

function syncNativeWindowBackground(hex: string): void {
  if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) return;
  void import('@tauri-apps/api/window')
    .then(({ getCurrentWindow }) => getCurrentWindow().setBackgroundColor(hex))
    .catch(() => {
      // Missing ACL / platform without setBackgroundColor — HTML cache still applies.
    });
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

async function readPluginFileOrNull(
  pluginId: string,
  relativePath: string,
): Promise<number[] | null> {
  try {
    return await pluginCommands.readPluginFile(pluginId, relativePath);
  } catch {
    return null;
  }
}

type PackFileReader = (relativePath: string) => Promise<number[] | null>;

/**
 * Lexically joins an asset reference against the directory of the css file
 * it appears in (`''` = plugin/pack root). Rejects traversal outside the root.
 */
function joinRelativePath(baseDir: string, ref: string): string {
  const segments = `${ref.startsWith('/') ? '' : baseDir ? `${baseDir}/` : ''}${ref}`.split('/');
  const stack: string[] = [];
  for (const segment of segments) {
    if (!segment || segment === '.') continue;
    if (segment === '..') {
      stack.pop();
      continue;
    }
    stack.push(segment);
  }
  return stack.join('/');
}

/**
 * Rewrites every `url(...)` reference to a local pack file into a blob URL.
 * Remote http(s) URLs are rejected; blob:/data: URLs are kept as-is.
 */
async function rewriteCssUrls(css: string, readFile: PackFileReader): Promise<string> {
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
    const bytes = await readFile(relPath);
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

export async function rewriteFontUrls(css: string, packId: string): Promise<string> {
  return rewriteCssUrls(css, (relPath) => readPackFile(packId, relPath));
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

export type ApplyThemePackResult = { ok: true } | { ok: false; error: string };

export function encodePluginThemePackId(pluginId: string, themeId: string): string {
  return `${PLUGIN_THEME_PACK_PREFIX}${pluginId}:${themeId}`;
}

export function parsePluginThemePackId(
  packId: string | null | undefined,
): { pluginId: string; themeId: string } | null {
  if (!packId || !packId.startsWith(PLUGIN_THEME_PACK_PREFIX)) return null;
  const rest = packId.slice(PLUGIN_THEME_PACK_PREFIX.length);
  const sep = rest.indexOf(':');
  if (sep <= 0 || sep === rest.length - 1) return null;
  return { pluginId: rest.slice(0, sep), themeId: rest.slice(sep + 1) };
}

/** Reference to a theme contributed by an installed UI plugin. */
export interface PluginThemeRef {
  pluginId: string;
  themeId: string;
  /** Display name, accepted for caller convenience (not used for lookup). */
  name?: string;
}

async function resolvePluginTokensPath(pluginId: string, themeId: string): Promise<string> {
  const manifest = await pluginCommands.getPluginManifest(pluginId);
  const theme = manifest.contributes.themes.find((th) => th.id === themeId);
  if (!theme) {
    throw new Error(`Theme "${themeId}" not found in plugin "${pluginId}"`);
  }
  // tokensCss is relative to the plugin root (e.g. "themes/midnight-blue/tokens.css").
  return theme.tokensCss;
}

async function applyPluginThemePackId(
  packId: string,
  { broadcast = true }: { broadcast?: boolean } = {},
): Promise<ApplyThemePackResult> {
  const parsed = parsePluginThemePackId(packId);
  if (!parsed) {
    return { ok: false, error: `invalid plugin theme id: ${packId}` };
  }
  const { pluginId, themeId } = parsed;

  resetPackState();
  try {
    const tokensPath = await resolvePluginTokensPath(pluginId, themeId);
    const tokensBytes = await readPluginFileOrNull(pluginId, tokensPath);
    if (!tokensBytes) {
      throw new Error('tokens.css missing');
    }

    // Relative url(...) references resolve against the tokens.css directory.
    const baseDir = tokensPath.includes('/')
      ? tokensPath.slice(0, tokensPath.lastIndexOf('/'))
      : '';
    const css = await rewriteCssUrls(decodeUtf8(tokensBytes), (relPath) =>
      readPluginFileOrNull(pluginId, joinRelativePath(baseDir, relPath)),
    );
    injectThemePackCss(css);

    syncWebviewBackgroundFromTokens();
    notifyThemePackChanged();
    if (broadcast) void emitCrossWindow('datazen:theme-pack-changed', packId);
    return { ok: true };
  } catch (err) {
    console.warn('[theme] failed to apply plugin theme', packId, err);
    resetPackState();
    syncWebviewBackgroundFromTokens();
    notifyThemePackChanged();
    if (broadcast) void emitCrossWindow('datazen:theme-pack-changed', null);
    const message = err instanceof Error ? err.message : String(err);
    return { ok: false, error: message };
  }
}

/**
 * Apply a theme contributed by an enabled UI plugin: reads `tokens.css` (and
 * any local assets referenced by its `url(...)`s) as bytes through
 * `read_plugin_file` and injects them into the same style element used by
 * legacy `{appData}/themes/` packs.
 */
export async function applyPluginTheme(
  theme: PluginThemeRef,
  options: { broadcast?: boolean } = {},
): Promise<ApplyThemePackResult> {
  return applyPluginThemePackId(encodePluginThemePackId(theme.pluginId, theme.themeId), options);
}

export async function applyThemePack(
  packId: string | null,
  { broadcast = true }: { broadcast?: boolean } = {},
): Promise<ApplyThemePackResult> {
  if (packId && parsePluginThemePackId(packId)) {
    return applyPluginThemePackId(packId, { broadcast });
  }

  resetPackState();

  if (!packId) {
    syncWebviewBackgroundFromTokens();
    notifyThemePackChanged();
    if (broadcast) void emitCrossWindow('datazen:theme-pack-changed', packId);
    return { ok: true };
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
    if (broadcast) void emitCrossWindow('datazen:theme-pack-changed', packId);
    return { ok: true };
  } catch (err) {
    console.warn('[theme] failed to apply pack', packId, err);
    resetPackState();
    syncWebviewBackgroundFromTokens();
    notifyThemePackChanged();
    if (broadcast) void emitCrossWindow('datazen:theme-pack-changed', null);
    const message = err instanceof Error ? err.message : String(err);
    return { ok: false, error: message };
  }
}
