import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
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

/** Host dark defaults (One Dark–ish); used when CSS vars are unset. */
export const DEFAULT_EDITOR_COLORS: EditorColorContract = {
  keyword: '#c678dd',
  string: '#98c379',
  number: '#d19a66',
  comment: '#5c6370',
  operator: '#56b6c2',
  punctuation: '#abb2bf',
  foreground: '#f1f5f9',
  background: '#0f172a',
  selection: 'rgba(59,130,246,0.25)',
  cursor: '#f1f5f9',
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

export function readEditorColorsFromElement(el: Element = document.documentElement): EditorColorContract {
  const style = getComputedStyle(el);
  const colors = readEditorColors((name) => style.getPropertyValue(name));
  if (!packEditorOverlay) return colors;
  return { ...colors, ...packEditorOverlay };
}

export function editorColorsFromJson(json: unknown, base: EditorColorContract): EditorColorContract {
  if (!json || typeof json !== 'object') return { ...base };
  const next = { ...base };
  for (const [key, value] of Object.entries(json as Record<string, unknown>)) {
    if (EDITOR_JSON_KEYS.has(key) && typeof value === 'string' && value.trim()) {
      next[key as keyof EditorColorContract] = value.trim();
    }
  }
  return next;
}

/** Tags not covered by --cm-* vars; kept as theme-mode-specific accents. */
const EXTENDED_DARK = {
  typeName: '#e5c07b',
  propertyName: '#61afef',
  variableName: '#e06c75',
  name: '#abb2bf',
};

const EXTENDED_LIGHT = {
  typeName: '#b45309',
  propertyName: '#2563eb',
  variableName: '#dc2626',
  name: '#0f172a',
};

export function buildEditorHighlightStyle(colors: EditorColorContract, dark: boolean): HighlightStyle {
  const ext = dark ? EXTENDED_DARK : EXTENDED_LIGHT;
  return HighlightStyle.define([
    { tag: tags.keyword, color: colors.keyword },
    { tag: tags.operatorKeyword, color: colors.keyword },
    { tag: tags.typeName, color: ext.typeName },
    { tag: tags.string, color: colors.string },
    { tag: tags.number, color: colors.number },
    { tag: tags.bool, color: colors.number },
    { tag: tags.null, color: colors.number },
    { tag: tags.comment, color: colors.comment, fontStyle: 'italic' },
    { tag: tags.punctuation, color: colors.punctuation },
    { tag: tags.bracket, color: colors.punctuation },
    { tag: tags.operator, color: colors.operator },
    { tag: tags.propertyName, color: ext.propertyName },
    { tag: tags.function(tags.variableName), color: ext.propertyName },
    { tag: tags.variableName, color: ext.variableName },
    { tag: tags.name, color: ext.name },
  ]);
}

export function editorSyntaxHighlighting(colors: EditorColorContract, dark: boolean) {
  return syntaxHighlighting(buildEditorHighlightStyle(colors, dark));
}
