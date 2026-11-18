/**
 * valuePresentationRules — 全局规则模型、受限正则与匹配（issue-32 T08，设计 §5，PRD §3.1/§4）。
 *
 * 纯逻辑模块：无 React / DOM / Store / IPC 依赖。规则是时间展示器的
 * 全局配置层（表列覆盖 > 全局规则 > 全局默认，PRD §4），顺序即优先级，
 * 首条命中冻结整列解释（命中后不因某一行解析失败回落到低优先级规则）。
 *
 * 裁决（任务规格未明之处，按此实现并在测试中锁定）：
 *
 * 1. 存储版本：保持 version 1 + 可选 `rules` 字段，不升 version 2。
 *    规则是加法性可选字段；升版会迫使无迁移的 v2 走「未知版本 → 空
 *    快照」分支，反而丢配置。旧 payload 无 rules → 默认空列表；未知
 *    顶层版本仍原样保留、不解析 rules、不写回默认值（见 settings.ts）。
 * 2. 规则至少含一个匹配条件：空条件规则会匹配所有列，与全局默认
 *    重叠且极易误伤，故验证时拒绝（`match` 至少一个字段）。
 * 3. 匹配条件全部为 AND；连接 / 数据库 / namespace 精确匹配，
 *    表名 / 列名 / 数据库类型名支持精确或受限正则。表级条件
 *    （连接、数据库、namespace、表）要求来源字段存在：缺失来源
 *    不能满足表级条件（不能从 SQL 别名猜表来源，PRD §3.2）。
 *    无来源列（查询结果）仍可用仅依赖列名 / 类型名的规则。
 * 4. 受限正则（设计 §5 硬要求）：见 validateRestrictedPattern。
 *    只接受经手写解析器验证的表达式子集；flags 只允许 '' 与 'i'；
 *    pattern 长度上限 128；规则数上限 20（MAX_* 常量，见下）。
 *    通过验证的 pattern 才交给 RegExp，且只在 prepare/计划阶段
 *    编译一次、不逐行编译、不在渲染线程执行任意用户正则。
 * 5. 文本输入空格处理沿用 T07 裁决：首尾 trim 后接受；名内空白
 *    （规则名、精确匹配值、标识符类条件）非法；正则 source 同样
 *    trim（行首行尾空白在正则里无实际用途，属输入噪声）。
 * 6. 规则名在列表内唯一（大小写敏感）：命中来源提示（PRD §3.3
 *    「配置来源：规则名称」）按名显示，重名会让来源含糊，故
 *    编辑器层拒绝重名（validateValuePresentationRule 本身不判重，
 *    重名检查在编辑器/面板层做，见 validateRuleNameUniqueness）。
 */

import { validateTimestampOptions, type TimestampOptions } from './timestampConfig';

/** 规则数上限（设计 §5：规则数量与输入长度同时设限）。 */
export const MAX_VALUE_PRESENTATION_RULES = 20;

/** 单个 pattern（精确值或正则 source）长度上限（字符数）。 */
export const MAX_PATTERN_LENGTH = 128;

/** 规则名长度上限（字符数）。 */
export const MAX_RULE_NAME_LENGTH = 64;

/** 标识符类精确值（连接 / 数据库 / namespace / 精确匹配值）长度上限。 */
export const MAX_MATCH_VALUE_LENGTH = 128;

/** 正则重复量 `{n,m}` 的 m 上限（限制重复展开规模）。 */
const MAX_REPETITION_COUNT = 100;

// ---------------------------------------------------------------------------
// 模型
// ---------------------------------------------------------------------------

/**
 * 名称模式：精确值（全等匹配）或受限正则（见 validateRestrictedPattern）。
 * 只保存 source 与受限 flags，绝不持久化 RegExp（设计 §5）。
 */
export type ValuePresentationNamePattern =
  | { readonly kind: 'exact'; readonly value: string }
  | { readonly kind: 'regex'; readonly source: string; readonly flags?: string };

/** 一条规则的匹配条件：全部可选，出现者为 AND；至少出现一个。 */
export interface ValuePresentationRuleMatch {
  readonly connectionId?: string;
  readonly database?: string;
  readonly namespace?: string;
  readonly table?: ValuePresentationNamePattern;
  readonly column?: ValuePresentationNamePattern;
  readonly databaseTypeName?: ValuePresentationNamePattern;
}

/** 一条全局规则：名称、启用开关、匹配条件、时间选项（复用 T02）。 */
export interface ValuePresentationRule {
  readonly name: string;
  readonly enabled: boolean;
  readonly match: ValuePresentationRuleMatch;
  readonly options: TimestampOptions;
}

/** 规则匹配的列上下文：表级字段缺失时表级条件不满足。 */
export interface ValuePresentationRuleContext {
  readonly connectionId?: string;
  readonly database?: string;
  readonly namespace?: string;
  readonly table?: string;
  readonly column: string;
  readonly databaseTypeName?: string;
}

/** 路径化验证问题（与 T02 的 TimestampOptionsProblem 同形）。 */
export interface ValuePresentationRuleProblem {
  path: string;
  message: string;
}

export type ValuePresentationRuleValidation =
  | { ok: true; rule: ValuePresentationRule }
  | { ok: false; problems: ReadonlyArray<ValuePresentationRuleProblem> };

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/** 标识符类文本：trim 后非空、无内空白、长度受限。 */
function readIdentifierValue(
  value: unknown,
  path: string,
  problems: ValuePresentationRuleProblem[],
): string | undefined {
  if (typeof value !== 'string') {
    problems.push({ path, message: 'expected a string' });
    return undefined;
  }
  const trimmed = value.trim();
  if (trimmed === '') {
    problems.push({ path, message: 'must not be empty' });
    return undefined;
  }
  if (/\s/.test(trimmed)) {
    problems.push({ path, message: 'must not contain whitespace' });
    return undefined;
  }
  if (trimmed.length > MAX_MATCH_VALUE_LENGTH) {
    problems.push({
      path,
      message: `must be at most ${MAX_MATCH_VALUE_LENGTH} characters`,
    });
    return undefined;
  }
  return trimmed;
}

// ---------------------------------------------------------------------------
// 受限正则：手写解析器验证的表达式子集
// ---------------------------------------------------------------------------

/**
 * 受限正则子集（设计 §5：只接受可保证复杂度的表达式子集；复杂或
 * 不支持的表达式在保存时拒绝）：
 *
 * 允许：字面量、`.`、`^`、`$`、字符类 `[...]`、分组 `(...)` /
 * `(?:...)`、顶层或组内交替 `|`、量词 `*` `+` `?` `{n}` `{n,}`
 * `{n,m}`（m ≤ 100）及惰性后缀 `?`、转义 `\d \D \w \W \s \S \b \B
 * \t \n \r \f \v \0` 与标点转义。
 *
 * 拒绝（回溯风险或不可判定复杂度）：
 * - 反向引用（`\1`…`\9`、`\k<…>`）、环视（`(?=`、`(?!`、`(?<=`、
 *   `(?<!`）及其他 `(?…` 结构；
 * - `\p{…}`、`\u…`、`\x…` 等非常用转义；
 * - 量词套量词（`(a+)+`、`a**`、`a+*`）；
 * - 被量化的分组内含交替（`(a|ab)+` 这类分区回溯可指数爆炸；
 *   未量化的交替如 PRD 例 `^(created_at|updated_at)$` 不受影响）；
 * - 被量化的分组内含可空分支（`(a|)+`、`(^a|)+` 等）；
 * - 未闭合的分组 / 字符类、行尾反斜杠、非法的 `{n,m}`。
 *
 * 匹配语义：RegExp.test 子串匹配，不隐式加锚点——写
 * `^(created_at|updated_at)$` 得全名匹配，写 `created` 得前缀/
 * 子串匹配，与用户对正则的直觉一致。
 *
 * 纵深防御：匹配只发生在列计划构建时（每列每规则一次），不逐行；
 * pattern 长度同时受限（≤128），即使漏网的病态表达式也只影响
 * 计划构建、不进入渲染循环。
 */
export type RestrictedPatternValidation = { ok: true } | { ok: false; reason: string };

interface PatternNodeFlags {
  /** 节点内是否出现过量词。 */
  hasQuantifier: boolean;
  /** 节点内是否出现过交替。 */
  hasAlternation: boolean;
  /** 节点是否可匹配空串。 */
  nullable: boolean;
}

const EMPTY_FLAGS: PatternNodeFlags = {
  hasQuantifier: false,
  hasAlternation: false,
  nullable: true,
};

function mergeSequenceFlags(terms: ReadonlyArray<PatternNodeFlags>): PatternNodeFlags {
  return {
    hasQuantifier: terms.some((t) => t.hasQuantifier),
    hasAlternation: terms.some((t) => t.hasAlternation),
    nullable: terms.every((t) => t.nullable),
  };
}

class RestrictedPatternParser {
  private pos = 0;

  constructor(
    private readonly source: string,
    private readonly fail: (reason: string) => never,
  ) {}

  parse(): PatternNodeFlags {
    const flags = this.parseAlternation(null);
    if (this.pos !== this.source.length) {
      this.fail(`unexpected character "${this.source[this.pos]}" at offset ${this.pos}`);
    }
    return flags;
  }

  /** 交替：branch ('|' branch)*；end 为 ')' 或 null（串尾）。 */
  private parseAlternation(end: ')' | null): PatternNodeFlags {
    const branches: PatternNodeFlags[] = [this.parseSequence(end)];
    while (this.peek() === '|') {
      this.pos += 1;
      branches.push(this.parseSequence(end));
    }
    if (end === ')' && this.peek() !== ')') {
      this.fail('unclosed group');
    }
    return {
      hasQuantifier: branches.some((b) => b.hasQuantifier),
      hasAlternation: branches.length > 1 || branches.some((b) => b.hasAlternation),
      nullable: branches.some((b) => b.nullable),
    };
  }

  /** 序列：term*，遇 '|' / end / 串尾停。 */
  private parseSequence(end: ')' | null): PatternNodeFlags {
    const terms: PatternNodeFlags[] = [];
    for (;;) {
      const ch = this.peek();
      if (ch === undefined || ch === '|' || ch === end) break;
      if (ch === ')') {
        this.fail('unmatched closing parenthesis');
      }
      terms.push(this.parseTerm());
    }
    return terms.length === 0 ? EMPTY_FLAGS : mergeSequenceFlags(terms);
  }

  /** term：atom + 可选量词。 */
  private parseTerm(): PatternNodeFlags {
    const atom = this.parseAtom();
    const quantifier = this.parseQuantifier();
    if (quantifier === null) return atom;
    // 量词套量词 / 量化交替组 / 量化可空组一律拒绝（见文件头）。
    if (atom.hasQuantifier) {
      this.fail('nested quantifier is not allowed');
    }
    if (atom.hasAlternation) {
      this.fail('a quantified group must not contain alternation');
    }
    if (atom.nullable) {
      this.fail('a quantified group must not match the empty string');
    }
    return {
      hasQuantifier: true,
      hasAlternation: atom.hasAlternation,
      nullable: atom.nullable || quantifier.min === 0,
    };
  }

  /**
   * 量词：`*` `+` `?` `{n}` `{n,}` `{n,m}`，可带惰性后缀 `?`。
   * 返回 null 表示无量词。`{` 后不是合法重复量时不消费（交由
   * parseAtom 按字面量处理）。
   */
  private parseQuantifier(): { min: number } | null {
    const ch = this.peek();
    if (ch === '*' || ch === '+' || ch === '?') {
      this.pos += 1;
      this.consumeLazyMarker();
      return { min: ch === '*' || ch === '?' ? 0 : 1 };
    }
    if (ch === '{') {
      const match = /^\{(\d+)(,(\d*))?\}/.exec(this.source.slice(this.pos));
      if (match === null) return null;
      const min = Number(match[1]);
      const maxText = match[3];
      const max = maxText === undefined || maxText === '' ? undefined : Number(maxText);
      if (min > MAX_REPETITION_COUNT || (max !== undefined && max > MAX_REPETITION_COUNT)) {
        this.fail(`repetition count must be at most ${MAX_REPETITION_COUNT}`);
      }
      if (max !== undefined && min > max) {
        this.fail('repetition minimum must not exceed its maximum');
      }
      this.pos += match[0].length;
      this.consumeLazyMarker();
      return { min };
    }
    return null;
  }

  /** 惰性后缀 `?`：量词后恰一个 `?` 属惰性标记，不是第二个量词。 */
  private consumeLazyMarker(): void {
    if (this.peek() === '?') this.pos += 1;
  }

  /** 原子：分组 / 字符类 / 锚点 / 转义 / 字面量。 */
  private parseAtom(): PatternNodeFlags {
    const ch = this.peek() ?? '';
    if (ch === '(') {
      this.pos += 1;
      if (this.peek() === '?') {
        // 只接受非捕获组 `(?:`；环视与其他 `(?…` 一律拒绝。
        if (this.source[this.pos + 1] === ':') {
          this.pos += 2;
        } else {
          this.fail('lookaround and other (?...) groups are not allowed');
        }
      }
      const inner = this.parseAlternation(')');
      this.pos += 1; // 消费 ')'
      return inner;
    }
    if (ch === '[') {
      this.parseCharacterClass();
      return { hasQuantifier: false, hasAlternation: false, nullable: false };
    }
    if (ch === '^' || ch === '$') {
      this.pos += 1;
      return { hasQuantifier: false, hasAlternation: false, nullable: true };
    }
    if (ch === '\\') {
      this.parseEscape();
      return { hasQuantifier: false, hasAlternation: false, nullable: false };
    }
    if (ch === '{') {
      // `{n,m}` 形文本在 term 起始处永远是无前置原子的量词，JS 本身
      // 拒绝编译（`a{2}{3}` 亦然），这里同样拒绝；`{,m}` 形不是合法
      // 重复量，恒按字面量。
      if (/^\{\d+(,\d*)?\}/.test(this.source.slice(this.pos))) {
        this.fail('repetition without a preceding atom');
      }
      const loose = /^\{\d*(,\d*)?\}/.exec(this.source.slice(this.pos));
      if (loose !== null) {
        this.pos += loose[0].length;
      } else {
        this.pos += 1;
      }
      return { hasQuantifier: false, hasAlternation: false, nullable: false };
    }
    this.pos += 1;
    return { hasQuantifier: false, hasAlternation: false, nullable: false };
  }

  /** 字符类：`[` … `]`，支持 `]` 打头字面量、`^` 否定、转义与区间。 */
  private parseCharacterClass(): void {
    this.pos += 1; // 消费 '['
    if (this.peek() === '^') this.pos += 1;
    // ']' 紧跟 '['（或 '[^'）时是字面量，不是闭合。
    if (this.peek() === ']') this.pos += 1;
    for (;;) {
      const ch = this.peek();
      if (ch === undefined) {
        this.fail('unclosed character class');
      }
      if (ch === ']') {
        this.pos += 1;
        return;
      }
      if (ch === '\\') {
        this.parseEscape();
      } else {
        this.pos += 1;
      }
    }
  }

  /** 转义：只接受白名单；反向引用与非常用转义拒绝。 */
  private parseEscape(): void {
    this.pos += 1; // 消费 '\'
    const esc = this.peek();
    if (esc === undefined) {
      this.fail('trailing backslash');
    }
    if (esc >= '0' && esc <= '9') {
      this.fail('backreferences are not allowed');
    }
    if (esc === 'k') {
      this.fail('named backreferences are not allowed');
    }
    if (esc === 'p' || esc === 'u' || esc === 'x') {
      this.fail(`unsupported escape "\\${esc}"`);
    }
    const allowed = 'dDwWsSbBtTrRfFv0';
    if (allowed.includes(esc)) {
      this.pos += 1;
      return;
    }
    // 其余标点转义（如 \. \\ \- \/）是字面量，接受；字母转义拒绝。
    if (/[A-Za-z]/.test(esc)) {
      this.fail(`unsupported escape "\\${esc}"`);
    }
    this.pos += 1;
  }

  private peek(): string | undefined {
    return this.pos < this.source.length ? this.source[this.pos] : undefined;
  }
}

/** 验证一条正则 source 与 flags 是否属于受限子集（不抛异常）。 */
export function validateRestrictedPattern(
  source: string,
  flags?: string,
): RestrictedPatternValidation {
  if (flags !== undefined && flags !== '' && flags !== 'i') {
    return { ok: false, reason: 'only the "i" flag is allowed' };
  }
  if (source === '') {
    return { ok: false, reason: 'pattern must not be empty' };
  }
  if (source.length > MAX_PATTERN_LENGTH) {
    return {
      ok: false,
      reason: `pattern must be at most ${MAX_PATTERN_LENGTH} characters`,
    };
  }
  let reason: string | undefined;
  const fail = (message: string): never => {
    reason = message;
    throw new Error(message);
  };
  try {
    new RestrictedPatternParser(source, fail).parse();
  } catch {
    return { ok: false, reason: reason ?? 'invalid pattern' };
  }
  // 解析通过的表达式一定能编译；防御性再编译一次，失败即拒绝。
  try {
    new RegExp(source, flags ?? '');
  } catch {
    return { ok: false, reason: 'pattern cannot be compiled' };
  }
  return { ok: true };
}

// ---------------------------------------------------------------------------
// 名称模式匹配（只在列计划构建时调用，不逐行）
// ---------------------------------------------------------------------------

/** 匹配一个名称模式：精确值全等；正则用 RegExp.test 子串匹配。 */
export function matchNamePattern(
  pattern: Readonly<ValuePresentationNamePattern>,
  value: string,
): boolean {
  if (pattern.kind === 'exact') return pattern.value === value;
  try {
    return new RegExp(pattern.source, pattern.flags ?? '').test(value);
  } catch {
    // 已验证的 pattern 不应编译失败；防御性按不匹配处理。
    return false;
  }
}

/** 名称模式的人类可读描述（规则列表摘要用）：精确值原文，正则 /source/flags。 */
export function describeNamePattern(pattern: Readonly<ValuePresentationNamePattern>): string {
  return pattern.kind === 'exact' ? pattern.value : `/${pattern.source}/${pattern.flags ?? ''}`;
}

// ---------------------------------------------------------------------------
// 规则验证
// ---------------------------------------------------------------------------

function readNamePattern(
  value: unknown,
  path: string,
  problems: ValuePresentationRuleProblem[],
): ValuePresentationNamePattern | undefined {
  if (!isPlainObject(value)) {
    problems.push({ path, message: 'expected an object' });
    return undefined;
  }
  const before = problems.length;
  let pattern: ValuePresentationNamePattern | undefined;
  if (value.kind === 'exact') {
    const exact = readIdentifierValue(value.value, `${path}.value`, problems);
    if (exact !== undefined) pattern = { kind: 'exact', value: exact };
    for (const key of Object.keys(value)) {
      if (key !== 'kind' && key !== 'value') {
        problems.push({ path: `${path}.${key}`, message: 'unknown field' });
      }
    }
  } else if (value.kind === 'regex') {
    if (typeof value.source !== 'string') {
      problems.push({ path: `${path}.source`, message: 'expected a string' });
    } else {
      const source = value.source.trim();
      const flags =
        value.flags === undefined
          ? undefined
          : typeof value.flags === 'string'
            ? value.flags
            : null;
      if (flags === null) {
        problems.push({ path: `${path}.flags`, message: 'expected a string' });
      } else {
        const checked = validateRestrictedPattern(source, flags);
        if (!checked.ok) {
          problems.push({ path: `${path}.source`, message: checked.reason });
        } else {
          pattern =
            flags === undefined || flags === ''
              ? { kind: 'regex', source }
              : { kind: 'regex', source, flags };
        }
      }
    }
    for (const key of Object.keys(value)) {
      if (key !== 'kind' && key !== 'source' && key !== 'flags') {
        problems.push({ path: `${path}.${key}`, message: 'unknown field' });
      }
    }
  } else {
    problems.push({ path: `${path}.kind`, message: 'expected "exact" or "regex"' });
  }
  return problems.length > before ? undefined : pattern;
}

function readRuleMatch(
  value: unknown,
  problems: ValuePresentationRuleProblem[],
): ValuePresentationRuleMatch | undefined {
  if (!isPlainObject(value)) {
    problems.push({ path: 'match', message: 'expected an object' });
    return undefined;
  }
  const match: {
    connectionId?: string;
    database?: string;
    namespace?: string;
    table?: ValuePresentationNamePattern;
    column?: ValuePresentationNamePattern;
    databaseTypeName?: ValuePresentationNamePattern;
  } = {};
  let conditionCount = 0;
  if (value.connectionId !== undefined) {
    conditionCount += 1;
    const v = readIdentifierValue(value.connectionId, 'match.connectionId', problems);
    if (v !== undefined) match.connectionId = v;
  }
  if (value.database !== undefined) {
    conditionCount += 1;
    const v = readIdentifierValue(value.database, 'match.database', problems);
    if (v !== undefined) match.database = v;
  }
  if (value.namespace !== undefined) {
    conditionCount += 1;
    const v = readIdentifierValue(value.namespace, 'match.namespace', problems);
    if (v !== undefined) match.namespace = v;
  }
  if (value.table !== undefined) {
    conditionCount += 1;
    const v = readNamePattern(value.table, 'match.table', problems);
    if (v !== undefined) match.table = v;
  }
  if (value.column !== undefined) {
    conditionCount += 1;
    const v = readNamePattern(value.column, 'match.column', problems);
    if (v !== undefined) match.column = v;
  }
  if (value.databaseTypeName !== undefined) {
    conditionCount += 1;
    const v = readNamePattern(value.databaseTypeName, 'match.databaseTypeName', problems);
    if (v !== undefined) match.databaseTypeName = v;
  }
  for (const key of Object.keys(value)) {
    if (
      key !== 'connectionId' &&
      key !== 'database' &&
      key !== 'namespace' &&
      key !== 'table' &&
      key !== 'column' &&
      key !== 'databaseTypeName'
    ) {
      problems.push({ path: `match.${key}`, message: 'unknown field' });
    }
  }
  if (conditionCount === 0) {
    problems.push({
      path: 'match',
      message: 'at least one match condition is required',
    });
  }
  if (problems.length > 0) return undefined;
  return match;
}

/**
 * 完整结构验证（unknown 入参）。成功返回深冻结的规范规则（只含已识别
 * 字段）；失败返回全部路径化问题。名内空白、空条件、非法正则、非法
 * TimestampOptions 均在此拒绝——非法规则禁止保存（PRD §3.1）。
 */
export function validateValuePresentationRule(input: unknown): ValuePresentationRuleValidation {
  if (!isPlainObject(input)) {
    return { ok: false, problems: [{ path: '', message: 'expected an object' }] };
  }
  const problems: ValuePresentationRuleProblem[] = [];

  let name: string | undefined;
  if (typeof input.name !== 'string') {
    problems.push({ path: 'name', message: 'expected a string' });
  } else {
    const trimmed = input.name.trim();
    if (trimmed === '') {
      problems.push({ path: 'name', message: 'must not be empty' });
    } else if (/\s/.test(trimmed)) {
      problems.push({ path: 'name', message: 'must not contain whitespace' });
    } else if (trimmed.length > MAX_RULE_NAME_LENGTH) {
      problems.push({
        path: 'name',
        message: `must be at most ${MAX_RULE_NAME_LENGTH} characters`,
      });
    } else {
      name = trimmed;
    }
  }

  let enabled: boolean | undefined;
  if (typeof input.enabled !== 'boolean') {
    problems.push({ path: 'enabled', message: 'expected a boolean' });
  } else {
    enabled = input.enabled;
  }

  const match = readRuleMatch(input.match, problems);

  let options: TimestampOptions | undefined;
  const validatedOptions = validateTimestampOptions(input.options);
  if (!validatedOptions.ok) {
    for (const problem of validatedOptions.problems) {
      problems.push({ path: `options.${problem.path}`, message: problem.message });
    }
  } else {
    options = validatedOptions.options;
  }

  for (const key of Object.keys(input)) {
    if (key !== 'name' && key !== 'enabled' && key !== 'match' && key !== 'options') {
      problems.push({ path: key, message: 'unknown field' });
    }
  }

  if (
    problems.length > 0 ||
    name === undefined ||
    enabled === undefined ||
    match === undefined ||
    options === undefined
  ) {
    return { ok: false, problems };
  }

  const rule: ValuePresentationRule = {
    name,
    enabled,
    match,
    options,
  };
  Object.freeze(rule.match);
  Object.freeze(rule);
  return { ok: true, rule };
}

/**
 * 宽容解析规则数组（存储边界用）：非数组 → []；逐条验证，无效条目
 * 丢弃（原文保留在 store，不写回）；超过上限只保留前
 * MAX_VALUE_PRESENTATION_RULES 条有效规则。
 */
export function validateValuePresentationRules(
  input: unknown,
): ReadonlyArray<ValuePresentationRule> {
  if (!Array.isArray(input)) return [];
  const rules: ValuePresentationRule[] = [];
  for (const entry of input) {
    if (rules.length >= MAX_VALUE_PRESENTATION_RULES) break;
    const validated = validateValuePresentationRule(entry);
    if (validated.ok) rules.push(validated.rule);
  }
  return rules;
}

/**
 * 规则名唯一性检查（编辑器/面板层）：同一列表内重名（大小写敏感）
 * 拒绝。validateValuePresentationRule 不判重——单条规则本身无法
 * 知道列表。
 */
export function findDuplicateRuleName(
  rules: ReadonlyArray<Readonly<ValuePresentationRule>>,
  ignoreIndex?: number,
): string | undefined {
  const seen = new Set<string>();
  for (let i = 0; i < rules.length; i += 1) {
    if (i === ignoreIndex) continue;
    const name = rules[i]?.name;
    if (name === undefined) continue;
    if (seen.has(name)) return name;
    seen.add(name);
  }
  return undefined;
}

// ---------------------------------------------------------------------------
// 匹配：同一规则内条件为 AND；表级条件要求来源字段存在
// ---------------------------------------------------------------------------

/** 一条启用规则是否命中给定列上下文（禁用规则调用方先排除，见下）。 */
export function matchValuePresentationRule(
  rule: Readonly<ValuePresentationRule>,
  context: Readonly<ValuePresentationRuleContext>,
): boolean {
  const match = rule.match;
  if (match.connectionId !== undefined && context.connectionId !== match.connectionId) {
    return false;
  }
  if (match.database !== undefined && context.database !== match.database) {
    return false;
  }
  if (match.namespace !== undefined && context.namespace !== match.namespace) {
    return false;
  }
  if (
    match.table !== undefined &&
    (context.table === undefined || !matchNamePattern(match.table, context.table))
  ) {
    return false;
  }
  if (match.column !== undefined && !matchNamePattern(match.column, context.column)) {
    return false;
  }
  if (
    match.databaseTypeName !== undefined &&
    (context.databaseTypeName === undefined ||
      !matchNamePattern(match.databaseTypeName, context.databaseTypeName))
  ) {
    return false;
  }
  return true;
}

/**
 * 首条命中规则：按列表顺序，跳过禁用规则，返回第一条命中的规则；
 * 无命中返回 undefined。命中后冻结整列解释，调用方不再继续找
 * 低优先级规则（PRD §4）。
 */
export function findFirstMatchingRule(
  rules: ReadonlyArray<Readonly<ValuePresentationRule>>,
  context: Readonly<ValuePresentationRuleContext>,
): ValuePresentationRule | undefined {
  for (const rule of rules) {
    if (!rule.enabled) continue;
    if (matchValuePresentationRule(rule, context)) return rule;
  }
  return undefined;
}

/** 规则匹配条件的人类可读摘要（规则列表行展示用）。 */
export function describeRuleMatch(match: Readonly<ValuePresentationRuleMatch>): string {
  const parts: string[] = [];
  if (match.connectionId !== undefined) parts.push(`connection=${match.connectionId}`);
  if (match.database !== undefined) parts.push(`database=${match.database}`);
  if (match.namespace !== undefined) parts.push(`schema=${match.namespace}`);
  if (match.table !== undefined) parts.push(`table=${describeNamePattern(match.table)}`);
  if (match.column !== undefined) parts.push(`column=${describeNamePattern(match.column)}`);
  if (match.databaseTypeName !== undefined) {
    parts.push(`type=${describeNamePattern(match.databaseTypeName)}`);
  }
  return parts.join(', ');
}
