/**
 * A column the filter editor can filter on.
 *
 * `type` is the driver's type string for the column (`int4`, `varchar(64)`,
 * …). It decides which operators the column may be filtered with.
 */
export interface FilterColumn {
  name: string;
  /** The driver's type string; unknown types fall back to the text operators. */
  type?: string;
}
