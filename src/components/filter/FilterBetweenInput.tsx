import { Input } from '../ui/Input';

/**
 * The value editor of a range operator (`between` / `notBetween`): two bounds
 * with an `AND` between them.
 *
 * **The bounds are never swapped.** A user who typed the bounds in the wrong
 * order gets a warning, not a silent correction: swapping them would turn a
 * typo into a query with a different (and plausible looking) result.
 */
export function FilterBetweenInput({
  value,
  onCommit,
  placeholder,
  connectorLabel,
  invertedLabel,
  autoFocus,
  disabled,
}: {
  /** The two bounds, lower bound first. */
  value: [string, string];
  onCommit: (bounds: [string, string]) => void;
  placeholder: string;
  /** The text between the two fields, e.g. `AND`. */
  connectorLabel: string;
  /** The warning shown when the lower bound is above the upper bound. */
  invertedLabel: string;
  autoFocus?: boolean;
  disabled?: boolean;
}) {
  const [lower, upper] = value;
  const inverted = isInverted(lower, upper);

  return (
    <div className="flex min-w-0 flex-nowrap items-center gap-1" data-testid="filter-between-input">
      <Input
        autoFocus={autoFocus}
        disabled={disabled}
        value={lower}
        onChange={(e) => onCommit([e.target.value, upper])}
        placeholder={placeholder}
        className="!h-7 w-20 shrink-0 !px-2 !text-xs"
        data-testid="filter-value"
      />
      <span className="shrink-0 text-[11px] text-fg-muted">{connectorLabel}</span>
      <Input
        disabled={disabled}
        value={upper}
        onChange={(e) => onCommit([lower, e.target.value])}
        placeholder={placeholder}
        className="!h-7 w-20 shrink-0 !px-2 !text-xs"
        data-testid="filter-value"
      />
      {inverted && (
        <span
          className="shrink-0 text-[11px] text-warning"
          title={invertedLabel}
          data-testid="filter-between-inverted"
        >
          {invertedLabel}
        </span>
      )}
    </div>
  );
}

/**
 * Whether the lower bound is above the upper bound.
 *
 * Only comparable values count: two numbers, or two strings the date parser
 * understands. Everything else is left alone — a warning that shows up for a
 * value pair that is not comparable is noise.
 */
export function isInverted(lower: string, upper: string): boolean {
  const low = lower.trim();
  const high = upper.trim();
  if (low === '' || high === '') return false;

  const lowNumber = Number(low);
  const highNumber = Number(high);
  if (Number.isFinite(lowNumber) && Number.isFinite(highNumber)) {
    return lowNumber > highNumber;
  }

  const lowTime = Date.parse(low);
  const highTime = Date.parse(high);
  if (Number.isFinite(lowTime) && Number.isFinite(highTime)) {
    return lowTime > highTime;
  }
  return false;
}
