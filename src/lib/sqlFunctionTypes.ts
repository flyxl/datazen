export interface FunctionParam {
  /** Parameter name shown in signature help (e.g. "expr", "separator"). */
  name: string;
  /** Brief type hint shown in signature help (e.g. "string", "any"). */
  type: string;
  /** If true, this parameter may be omitted. */
  optional?: boolean;
}

export interface FunctionEntry {
  /** Case-insensitive function name. */
  name: string;
  /** One-line description for detail popup. */
  description: string;
  /** Ordered parameter list. */
  params: readonly FunctionParam[];
  /** Dialect ids this function belongs to; undefined = all dialects. */
  dialects?: readonly string[];
  /** If true, the function returns a value (default true). */
  returnsValue?: boolean;
}
