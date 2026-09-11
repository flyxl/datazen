import { HighlightStyle, syntaxHighlighting, syntaxTree } from '@codemirror/language';
import type { EditorState } from '@codemirror/state';
import {
  Decoration,
  EditorView,
  ViewPlugin,
  type DecorationSet,
  type ViewUpdate,
} from '@codemirror/view';
import { tags } from '@lezer/highlight';

export interface EditorColorContract {
  keyword: string;
  string: string;
  number: string;
  comment: string;
  operator: string;
  punctuation: string;
  foreground: string;
  background: string;
  selection: string;
  cursor: string;
}

const CM_VARS: Record<keyof EditorColorContract, string> = {
  keyword: '--cm-keyword',
  string: '--cm-string',
  number: '--cm-number',
  comment: '--cm-comment',
  operator: '--cm-operator',
  punctuation: '--cm-punctuation',
  foreground: '--cm-foreground',
  background: '--cm-background',
  selection: '--cm-selection',
  cursor: '--cm-cursor',
};

/** Host dark defaults (demo startup-journey palette); used when CSS vars are unset. */
export const DEFAULT_EDITOR_COLORS: EditorColorContract = {
  keyword: '#93c5fd',
  string: '#6ee7b7',
  number: '#fbbf24',
  comment: '#5f6879',
  operator: '#67e8f9',
  punctuation: '#8b94a7',
  foreground: '#e6eaf2',
  background: '#0b0e14',
  selection: 'rgba(79, 195, 247, 0.25)',
  cursor: '#e6eaf2',
};

const EDITOR_JSON_KEYS = new Set<string>(Object.keys(CM_VARS));

let packEditorOverlay: Partial<EditorColorContract> | null = null;

export function setPackEditorColorOverlay(overlay: Partial<EditorColorContract> | null): void {
  packEditorOverlay = overlay;
}

export function parsePackEditorOverlay(json: unknown): Partial<EditorColorContract> | null {
  if (!json || typeof json !== 'object') return null;
  const overlay: Partial<EditorColorContract> = {};
  let hasAny = false;
  for (const [key, value] of Object.entries(json as Record<string, unknown>)) {
    if (EDITOR_JSON_KEYS.has(key) && typeof value === 'string' && value.trim()) {
      overlay[key as keyof EditorColorContract] = value.trim();
      hasAny = true;
    }
  }
  return hasAny ? overlay : null;
}

function normalizeColor(value: string, fallback: string): string {
  const trimmed = value.trim();
  return trimmed || fallback;
}

export function readEditorColors(getVar: (name: string) => string): EditorColorContract {
  const colors = {} as EditorColorContract;
  for (const key of Object.keys(CM_VARS) as (keyof EditorColorContract)[]) {
    colors[key] = normalizeColor(getVar(CM_VARS[key]), DEFAULT_EDITOR_COLORS[key]);
  }
  return colors;
}

export function readEditorColorsFromElement(
  el: Element = document.documentElement,
): EditorColorContract {
  const style = getComputedStyle(el);
  const colors = readEditorColors((name) => style.getPropertyValue(name));
  if (!packEditorOverlay) return colors;
  return { ...colors, ...packEditorOverlay };
}

export function editorColorsFromJson(
  json: unknown,
  base: EditorColorContract,
): EditorColorContract {
  if (!json || typeof json !== 'object') return { ...base };
  const next = { ...base };
  for (const [key, value] of Object.entries(json as Record<string, unknown>)) {
    if (EDITOR_JSON_KEYS.has(key) && typeof value === 'string' && value.trim()) {
      next[key as keyof EditorColorContract] = value.trim();
    }
  }
  return next;
}

/* -------------------------------------------------------------------------- */
/*  SQL Syntax Theme Presets (mirroring VSCode built-in themes)                */
/* -------------------------------------------------------------------------- */

/** Extended highlight colors beyond the CSS variable contract. */
interface ExtendedHighlightColors {
  typeName: string;
  propertyName: string;
  variableName: string;
  name: string;
}

export interface SqlSyntaxPreset {
  /** Unique identifier (matches AppSettings.sqlSyntaxTheme). */
  id: string;
  /** User-facing label. */
  label: string;
  /** Dark mode colors. */
  dark: Partial<EditorColorContract> & Partial<ExtendedHighlightColors>;
  /** Light mode colors. */
  light: Partial<EditorColorContract> & Partial<ExtendedHighlightColors>;
}

/** @see https://code.visualstudio.com/docs/getstarted/themes */
export const SQL_SYNTAX_PRESETS: SqlSyntaxPreset[] = [
  {
    id: 'default',
    label: 'Default (DataZen)',
    dark: {
      keyword: '#93c5fd',
      string: '#6ee7b7',
      number: '#fbbf24',
      comment: '#9ca3af',
      operator: '#67e8f9',
      punctuation: '#d1d5db',
      typeName: '#e5c07b',
      propertyName: '#61afef',
      variableName: '#e06c75',
      name: '#abb2bf',
    },
    light: {
      keyword: '#2563eb',
      string: '#16a34a',
      number: '#d97706',
      comment: '#6b7280',
      operator: '#0891b2',
      punctuation: '#374151',
      typeName: '#b45309',
      propertyName: '#2563eb',
      variableName: '#dc2626',
      name: '#111827',
    },
  },
  {
    id: 'vscode-dark',
    label: 'Visual Studio Dark+',
    dark: {
      // From VSCode dark_vs.json + dark_plus.json
      keyword: '#569cd6',
      string: '#ce9178',
      number: '#b5cea8',
      comment: '#6A9955',
      operator: '#d4d4d4',
      punctuation: '#808080',
      typeName: '#4EC9B0',
      propertyName: '#9cdcfe',
      variableName: '#9cdcfe',
      name: '#d4d4d4',
      foreground: '#d4d4d4',
      selection: 'rgba(38, 79, 120, 0.5)',
    },
    light: {
      // From VSCode light_vs.json + light_plus.json
      keyword: '#0000ff',
      string: '#a31515',
      number: '#098658',
      comment: '#008000',
      operator: '#000000',
      punctuation: '#000000',
      typeName: '#267f99',
      propertyName: '#001080',
      variableName: '#001080',
      name: '#000000',
      foreground: '#000000',
      selection: 'rgba(0, 0, 0, 0.14)',
    },
  },
  {
    id: 'monokai',
    label: 'Monokai',
    dark: {
      keyword: '#f92672',
      string: '#e6db74',
      number: '#ae81ff',
      comment: '#75715e',
      operator: '#f92672',
      punctuation: '#f8f8f2',
      typeName: '#66d9ef',
      propertyName: '#a6e22e',
      variableName: '#f8f8f2',
      name: '#f8f8f2',
      foreground: '#f8f8f2',
      background: '#272822',
      selection: 'rgba(102, 102, 102, 0.5)',
    },
    light: {
      keyword: '#e3116c',
      string: '#e6db74',
      number: '#ae81ff',
      comment: '#75715e',
      operator: '#e3116c',
      punctuation: '#333333',
      typeName: '#2b908f',
      propertyName: '#a6e22e',
      variableName: '#333333',
      name: '#333333',
      foreground: '#333333',
      background: '#fff',
      selection: 'rgba(181, 137, 0, 0.2)',
    },
  },
  {
    id: 'solarized-dark',
    label: 'Solarized Dark',
    dark: {
      keyword: '#859900',
      string: '#2aa198',
      number: '#d33682',
      comment: '#586e75',
      operator: '#839496',
      punctuation: '#839496',
      typeName: '#b58900',
      propertyName: '#268bd2',
      variableName: '#839496',
      name: '#839496',
      foreground: '#839496',
      background: '#002b36',
      selection: 'rgba(147, 161, 161, 0.3)',
    },
    light: {
      keyword: '#859900',
      string: '#2aa198',
      number: '#d33682',
      comment: '#93a1a1',
      operator: '#657b83',
      punctuation: '#657b83',
      typeName: '#b58900',
      propertyName: '#268bd2',
      variableName: '#657b83',
      name: '#657b83',
      foreground: '#657b83',
      background: '#fdf6e3',
      selection: 'rgba(147, 161, 161, 0.2)',
    },
  },
  {
    id: 'solarized-light',
    label: 'Solarized Light',
    dark: {
      keyword: '#859900',
      string: '#2aa198',
      number: '#d33682',
      comment: '#586e75',
      operator: '#839496',
      punctuation: '#839496',
      typeName: '#b58900',
      propertyName: '#268bd2',
      variableName: '#839496',
      name: '#839496',
      foreground: '#839496',
      background: '#002b36',
      selection: 'rgba(147, 161, 161, 0.3)',
    },
    light: {
      keyword: '#859900',
      string: '#2aa198',
      number: '#d33682',
      comment: '#93a1a1',
      operator: '#657b83',
      punctuation: '#657b83',
      typeName: '#b58900',
      propertyName: '#268bd2',
      variableName: '#657b83',
      name: '#657b83',
      foreground: '#657b83',
      background: '#fdf6e3',
      selection: 'rgba(147, 161, 161, 0.2)',
    },
  },
  {
    id: 'dracula',
    label: 'Dracula',
    dark: {
      keyword: '#ff79c6',
      string: '#f1fa8c',
      number: '#bd93f9',
      comment: '#6272a4',
      operator: '#ff79c6',
      punctuation: '#f8f8f2',
      typeName: '#8be9fd',
      propertyName: '#50fa7b',
      variableName: '#f8f8f2',
      name: '#f8f8f2',
      foreground: '#f8f8f2',
      background: '#282a36',
      selection: 'rgba(68, 71, 90, 0.6)',
    },
    light: {
      keyword: '#e3116c',
      string: '#c18e27',
      number: '#7c3aed',
      comment: '#6272a4',
      operator: '#e3116c',
      punctuation: '#343746',
      typeName: '#0d93b3',
      propertyName: '#067d17',
      variableName: '#343746',
      name: '#343746',
      foreground: '#343746',
      background: '#f8f8f2',
      selection: 'rgba(68, 71, 90, 0.2)',
    },
  },
  {
    id: 'github-dark',
    label: 'GitHub Dark',
    dark: {
      keyword: '#ff7b72',
      string: '#a5d6ff',
      number: '#79c0ff',
      comment: '#8b949e',
      operator: '#ff7b72',
      punctuation: '#c9d1d9',
      typeName: '#ffa657',
      propertyName: '#d2a8ff',
      variableName: '#c9d1d9',
      name: '#c9d1d9',
      foreground: '#c9d1d9',
      background: '#0d1117',
      selection: 'rgba(56, 139, 253, 0.4)',
    },
    light: {
      keyword: '#cf222e',
      string: '#0a3069',
      number: '#0550ae',
      comment: '#6e7781',
      operator: '#cf222e',
      punctuation: '#24292f',
      typeName: '#953800',
      propertyName: '#8250df',
      variableName: '#24292f',
      name: '#24292f',
      foreground: '#24292f',
      background: '#ffffff',
      selection: 'rgba(56, 139, 253, 0.2)',
    },
  },
];

export type SqlSyntaxThemeId =
  | 'default'
  | 'vscode-dark'
  | 'monokai'
  | 'solarized-dark'
  | 'solarized-light'
  | 'dracula'
  | 'github-dark';

export function isSqlSyntaxThemeId(v: unknown): v is SqlSyntaxThemeId {
  return typeof v === 'string' && SQL_SYNTAX_PRESETS.some((p) => p.id === v);
}

function lookupPreset(id: string): SqlSyntaxPreset {
  return SQL_SYNTAX_PRESETS.find((p) => p.id === id) ?? SQL_SYNTAX_PRESETS[0];
}

/**
 * Merge a SQL syntax preset on top of the base editor colors.
 * Returns full colors ready for `buildEditorHighlightStyle`.
 */
export function applySqlSyntaxPreset(
  base: EditorColorContract,
  themeId: string | undefined,
  dark: boolean,
): EditorColorContract & ExtendedHighlightColors {
  if (!themeId || themeId === 'default') {
    return {
      ...base,
      ...(dark
        ? { typeName: '#e5c07b', propertyName: '#61afef', variableName: '#e06c75', name: '#abb2bf' }
        : {
            typeName: '#b45309',
            propertyName: '#2563eb',
            variableName: '#dc2626',
            name: '#111827',
          }),
    };
  }

  const preset = lookupPreset(themeId);
  const palette = dark ? preset.dark : preset.light;

  const ext: ExtendedHighlightColors = {
    typeName: palette.typeName ?? (dark ? '#e5c07b' : '#b45309'),
    propertyName: palette.propertyName ?? (dark ? '#61afef' : '#2563eb'),
    variableName: palette.variableName ?? (dark ? '#e06c75' : '#dc2626'),
    name: palette.name ?? (dark ? '#abb2bf' : '#111827'),
  };

  return { ...base, ...palette, ...ext };
}

/* -------------------------------------------------------------------------- */
/*  HighlightStyle                                                            */
/* -------------------------------------------------------------------------- */

export function buildEditorHighlightStyle(
  colors: EditorColorContract & Partial<ExtendedHighlightColors>,
  dark: boolean,
): HighlightStyle {
  return HighlightStyle.define([
    { tag: tags.keyword, color: colors.keyword },
    { tag: tags.operatorKeyword, color: colors.keyword },
    { tag: tags.typeName, color: colors.typeName ?? (dark ? '#e5c07b' : '#b45309') },
    { tag: tags.string, color: colors.string },
    { tag: tags.number, color: colors.number },
    { tag: tags.bool, color: colors.number },
    { tag: tags.null, color: colors.number },
    { tag: tags.comment, color: colors.comment, fontStyle: 'italic' },
    { tag: tags.punctuation, color: colors.punctuation },
    { tag: tags.bracket, color: colors.punctuation },
    { tag: tags.operator, color: colors.operator },
    { tag: tags.propertyName, color: colors.propertyName ?? (dark ? '#61afef' : '#2563eb') },
    {
      tag: tags.function(tags.variableName),
      color: colors.propertyName ?? (dark ? '#61afef' : '#2563eb'),
    },
    { tag: tags.variableName, color: colors.variableName ?? (dark ? '#e06c75' : '#dc2626') },
    { tag: tags.name, color: colors.name ?? (dark ? '#abb2bf' : '#111827') },
  ]);
}

export function editorSyntaxHighlighting(
  colors: EditorColorContract & Partial<ExtendedHighlightColors>,
  dark: boolean,
) {
  return syntaxHighlighting(buildEditorHighlightStyle(colors, dark));
}

/**
 * CodeMirror's SQL grammar tags every identifier as `name`, including the
 * right-hand side of a dotted reference (`table.column`). Add the
 * context-sensitive property-name mark that the settings preview models.
 */
const sqlPropertyNameMark = Decoration.mark({ class: 'cm-sql-property-name' });

function findSqlPropertyNames(state: EditorState): DecorationSet {
  const ranges: ReturnType<typeof sqlPropertyNameMark.range>[] = [];
  syntaxTree(state).iterate({
    enter: (node) => {
      if (
        (node.name === 'Identifier' || node.name === 'QuotedIdentifier') &&
        node.from > 0 &&
        state.sliceDoc(0, node.from).trimEnd().endsWith('.')
      ) {
        ranges.push(sqlPropertyNameMark.range(node.from, node.to));
      }
    },
  });
  return Decoration.set(ranges, true);
}

/** Mark dotted SQL property names without changing the base syntax grammar. */
export function sqlPropertyNameHighlighting() {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;

      constructor(view: EditorView) {
        this.decorations = findSqlPropertyNames(view.state);
      }

      update(update: ViewUpdate) {
        if (update.docChanged || update.viewportChanged) {
          this.decorations = findSqlPropertyNames(update.state);
        }
      }
    },
    { decorations: (value) => value.decorations },
  );
}
