/**
 * Identifier normalization for foreign-key prediction.
 *
 * Conventions differ far more than the textbook `{table}_id` pattern, so names
 * are folded before comparison: camelCase is split, case is folded, and a table
 * name is tried both as written and singularized.
 *
 * Two deliberate limits, both consequences of "a wrong match is worse than no
 * match":
 *
 * - Singularization is **conservative and never a gate**. `people → person` and
 *   `address → address` are handled; anything not recognized is left alone. A
 *   missed match costs a manual click, an invented one costs a wrong query.
 * - Abbreviation guessing is not attempted at all. `usr_id` might mean `user_id`
 *   or `usage_id`, and there is no evidence to choose.
 */

/** Plural forms whose singular cannot be derived by a rule. */
const IRREGULAR_SINGULARS: Record<string, string> = {
  people: 'person',
  children: 'child',
  men: 'man',
  women: 'woman',
  teeth: 'tooth',
  feet: 'foot',
  geese: 'goose',
  mice: 'mouse',
  oxen: 'ox',
  indices: 'index',
  matrices: 'matrix',
  vertices: 'vertex',
  criteria: 'criterion',
  phenomena: 'phenomenon',
  data: 'data',
};

/** Words that end in `s` but are not plural. */
const SINGULAR_LOOKING = new Set([
  'address',
  'status',
  'business',
  'access',
  'class',
  'process',
  'analysis',
  'series',
  'species',
  'news',
  'boss',
  'alias',
  'alias_',
  'gas',
  'lens',
  'atlas',
  'campus',
  'corpus',
  'census',
  'plus',
  'bonus',
  'virus',
  'cactus',
  'focus',
  'radius',
  'surplus',
  'apparatus',
  'status_',
]);

/**
 * Split an identifier into lowercase words.
 *
 * `UserID` → `['user','id']`, `user_id` → `['user','id']`, `userID` → same.
 * Runs of capitals are treated as one acronym (`userID` → `user` + `id`,
 * `HTTPServer` → `http` + `server`).
 */
export function splitIdentifierWords(value: string): string[] {
  return (
    value
      // camelCase boundary: lower/digit followed by upper
      .replace(/([a-z0-9])([A-Z])/g, '$1\u0000$2')
      // acronym boundary: upper followed by upper+lower
      .replace(/([A-Z]+)([A-Z][a-z])/g, '$1\u0000$2')
      .split('\u0000')
      .flatMap((chunk) => chunk.split(/[^A-Za-z0-9]+/))
      .filter((word) => word.length > 0)
      .map((word) => word.toLowerCase())
  );
}

/** Canonical comparison form: words joined by `_`. */
export function normalizeIdentifier(value: string): string {
  return splitIdentifierWords(value).join('_');
}

/**
 * Singularize the last word of a table name.
 *
 * Only the last word is touched (`order_items` → `order_item`), and words that
 * merely look plural are left alone. Returns the input unchanged when unsure.
 */
export function singularize(tableName: string): string {
  const words = splitIdentifierWords(tableName);
  if (words.length === 0) return tableName;

  const last = words[words.length - 1]!;
  const irregular = IRREGULAR_SINGULARS[last];
  if (irregular) {
    words[words.length - 1] = irregular;
    return words.join('_');
  }
  if (SINGULAR_LOOKING.has(last)) return words.join('_');
  if (last.endsWith('ies') && last.length > 4) {
    words[words.length - 1] = `${last.slice(0, -3)}y`;
    return words.join('_');
  }
  if (
    last.endsWith('ses') ||
    last.endsWith('xes') ||
    last.endsWith('zes') ||
    last.endsWith('ches') ||
    last.endsWith('shes')
  ) {
    words[words.length - 1] = last.slice(0, -2);
    return words.join('_');
  }
  if (last.endsWith('s') && !last.endsWith('ss') && last.length > 3) {
    words[words.length - 1] = last.slice(0, -1);
    return words.join('_');
  }
  return words.join('_');
}

/**
 * Column names built from a table name and one of its key columns.
 *
 * A relationship's source column conventionally names its target:
 * `users` + `id` → `users_id` / `user_id`. Both the table name and its singular
 * are offered, so either naming style is recognized.
 */
export function qualifiedKeyNameForms(tableName: string, keyColumn: string): string[] {
  const forms = new Set<string>();
  for (const table of [tableName, singularize(tableName)]) {
    const normalizedTable = normalizeIdentifier(table);
    const normalizedKey = normalizeIdentifier(keyColumn);
    if (!normalizedTable || !normalizedKey) continue;
    forms.add(`${normalizedTable}_${normalizedKey}`);
  }
  return [...forms];
}

/**
 * Key column names that carry no domain meaning.
 *
 * `id` is excluded from the same-name pattern: every table's surrogate key is
 * called `id`, so matching on it would relate every table to every other.
 */
export function isGenericKeyName(keyColumn: string): boolean {
  const normalized = normalizeIdentifier(keyColumn);
  return normalized === 'id' || normalized === 'uuid' || normalized === 'pk';
}

/** Type families that must agree for a relationship to be plausible. */
export type TypeFamily =
  | 'numeric'
  | 'string'
  | 'uuid'
  | 'temporal'
  | 'boolean'
  | 'binary'
  | 'json'
  | 'other';

const TYPE_FAMILY_PATTERNS: readonly [RegExp, TypeFamily][] = [
  [/^(uuid|guid)$/, 'uuid'],
  [
    /^(int|int2|int4|int8|integer|bigint|smallint|tinyint|mediumint|serial|bigserial|smallserial|number|numeric|decimal|dec|real|double|double precision|float|float4|float8|money|unsigned big int)$/,
    'numeric',
  ],
  [/^(bool|boolean|bit)$/, 'boolean'],
  [
    /^(date|time|timetz|timestamp|timestamptz|datetime|datetime2|smalldatetime|interval|year)$/,
    'temporal',
  ],
  [/^(json|jsonb)$/, 'json'],
  [/^(bytea|blob|binary|varbinary|image|raw)$/, 'binary'],
  [
    /^(char|character|nchar|varchar|nvarchar|varchar2|nvarchar2|text|ntext|tinytext|mediumtext|longtext|clob|nclob|citext|string|enum|set)$/,
    'string',
  ],
];

/**
 * Reduce a driver-reported type string to a comparison family.
 *
 * Lengths, precision and unsigned flags are dropped (`varchar(32)` → `string`),
 * and every numeric type shares one family so a `bigint` key referencing an
 * `integer` key still matches — a common, intentional schema shape.
 */
export function typeFamily(dataType: string): TypeFamily {
  const normalized = dataType
    .trim()
    .toLowerCase()
    .replace(/\(.*\)$/, '')
    .replace(/\s+/g, ' ')
    .trim();
  const unsigned = normalized.replace(/^unsigned /, '').replace(/ unsigned$/, '');
  for (const [pattern, family] of TYPE_FAMILY_PATTERNS) {
    if (pattern.test(unsigned)) return family;
  }
  return 'other';
}

/** Canonical form used for the exact-type bonus. */
export function normalizeDataType(dataType: string): string {
  return dataType
    .trim()
    .toLowerCase()
    .replace(/\(.*\)$/, '')
    .replace(/\s+/g, ' ')
    .replace(/^unsigned /, '')
    .replace(/ unsigned$/, '')
    .trim();
}
