/**
 * Lightweight SQL syntax preview for the Settings page.
 *
 * Renders a static HTML snippet colored with the selected preset —
 * no CodeMirror instance needed.
 *
 * Token tagging mirrors the CodeMirror SQL parser:
 *  - Table/column names in dotted expressions → propertyName
 *  - Bare table names, aliases, unqualified identifiers → name
 *  - SQL function names → name (or function(variableName) fallback to propertyName)
 *  - Keywords, strings, numbers, operators, punctuation → as expected
 */
import { useMemo } from 'react';
import { SQL_SYNTAX_PRESETS, type EditorColorContract } from '../lib/themeEditorColors';

interface SqlSyntaxPreviewProps {
  themeId: string;
  dark: boolean;
}

interface TokenDef {
  text: string;
  colorKey:
    | keyof Pick<
        EditorColorContract,
        'comment' | 'keyword' | 'string' | 'number' | 'operator' | 'punctuation'
      >
    | 'typeName'
    | 'propertyName'
    | 'name'
    | 'foreground';
}

/**
 * Sample SQL covering every highlightable token type.
 *
 * Tagging mirrors the CodeMirror SQL parser behaviour:
 * - Dotted member access (`table.column`): left part = name, right part = propertyName
 * - Bare identifiers (aliases, table names in FROM): name
 * - SQL built-in functions (COUNT, SUM, ROUND): name
 * - SELECT / WHERE / AND etc.: keyword
 */
const SAMPLE_TOKENS: TokenDef[] = [
  { text: '-- ', colorKey: 'comment' },
  { text: 'Sample query preview', colorKey: 'comment' },
  { text: '\n', colorKey: 'foreground' },
  { text: 'SELECT', colorKey: 'keyword' },
  { text: ' ', colorKey: 'foreground' },
  { text: 'u', colorKey: 'name' },
  { text: '.', colorKey: 'punctuation' },
  { text: 'name', colorKey: 'propertyName' },
  { text: ',', colorKey: 'punctuation' },
  { text: ' ', colorKey: 'foreground' },
  { text: 'COUNT', colorKey: 'name' },
  { text: '(', colorKey: 'punctuation' },
  { text: 'o', colorKey: 'name' },
  { text: '.', colorKey: 'punctuation' },
  { text: 'id', colorKey: 'propertyName' },
  { text: ')', colorKey: 'punctuation' },
  { text: ' ', colorKey: 'foreground' },
  { text: 'AS', colorKey: 'keyword' },
  { text: ' ', colorKey: 'foreground' },
  { text: 'total', colorKey: 'name' },
  { text: '\n', colorKey: 'foreground' },
  { text: 'FROM', colorKey: 'keyword' },
  { text: ' ', colorKey: 'foreground' },
  { text: 'users', colorKey: 'name' },
  { text: ' ', colorKey: 'foreground' },
  { text: 'u', colorKey: 'name' },
  { text: '\n', colorKey: 'foreground' },
  { text: 'WHERE', colorKey: 'keyword' },
  { text: ' ', colorKey: 'foreground' },
  { text: 'u', colorKey: 'name' },
  { text: '.', colorKey: 'punctuation' },
  { text: 'status', colorKey: 'propertyName' },
  { text: ' ', colorKey: 'foreground' },
  { text: '=', colorKey: 'operator' },
  { text: ' ', colorKey: 'foreground' },
  { text: "'active'", colorKey: 'string' },
  { text: ' ', colorKey: 'foreground' },
  { text: 'AND', colorKey: 'keyword' },
  { text: ' ', colorKey: 'foreground' },
  { text: 'u', colorKey: 'name' },
  { text: '.', colorKey: 'punctuation' },
  { text: 'age', colorKey: 'propertyName' },
  { text: ' ', colorKey: 'foreground' },
  { text: '>', colorKey: 'operator' },
  { text: ' ', colorKey: 'foreground' },
  { text: '18', colorKey: 'number' },
  { text: '\n', colorKey: 'foreground' },
  { text: 'ORDER BY', colorKey: 'keyword' },
  { text: ' ', colorKey: 'foreground' },
  { text: 'total', colorKey: 'name' },
  { text: ' ', colorKey: 'foreground' },
  { text: 'DESC', colorKey: 'keyword' },
  { text: ';', colorKey: 'punctuation' },
];

export function SqlSyntaxPreview({ themeId, dark }: SqlSyntaxPreviewProps) {
  const palette = useMemo(() => {
    const preset = SQL_SYNTAX_PRESETS.find((p) => p.id === themeId) ?? SQL_SYNTAX_PRESETS[0];
    return dark ? preset.dark : preset.light;
  }, [themeId, dark]);

  const bg = palette.background ?? (dark ? '#1e1e1e' : '#ffffff');
  const fg = palette.foreground ?? (dark ? '#d4d4d4' : '#000000');

  return (
    <div
      className="mt-2 overflow-hidden rounded-lg border border-edge text-[12px] leading-[1.7]"
      style={{ backgroundColor: bg }}
    >
      <pre
        className="overflow-x-auto px-4 py-3 font-mono select-text"
        style={{ color: fg, margin: 0 }}
      >
        <code>
          {SAMPLE_TOKENS.map((tok, i) => (
            <span
              key={i}
              style={{
                color:
                  tok.colorKey === 'foreground'
                    ? fg
                    : ((palette as Record<string, string | undefined>)[tok.colorKey] ?? fg),
                fontStyle: tok.colorKey === 'comment' ? 'italic' : undefined,
              }}
            >
              {tok.text}
            </span>
          ))}
        </code>
      </pre>
    </div>
  );
}
