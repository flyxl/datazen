import { Select, type SelectOption } from '../../ui/Select';

export interface LinkSelectProps {
  /** i18n label without the angle brackets, e.g. "Click here to add fields". */
  label: string;
  options: SelectOption[];
  onPick: (value: string) => void;
  disabled?: boolean;
  testId: string;
}

/**
 * The Navicat `<Click here to add …>` affordance: a borderless select whose
 * trigger reads as a link and whose popup lists the candidates.
 *
 * Built on the shared `Select` rather than a bespoke popover so it inherits the
 * portal positioning, outside-click/Escape dismissal, type-to-filter and
 * keyboard navigation that component already has — and so E2E can drive it
 * through the same `role="option"` surface as every other picker.
 *
 * `value` is pinned to the empty string: this is an action, not a value holder,
 * so picking an option must not make the trigger look "filled in".
 */
export function LinkSelect({ label, options, onPick, disabled, testId }: LinkSelectProps) {
  return (
    <Select
      value=""
      options={options}
      onChange={onPick}
      placeholder={`<${label}>`}
      disabled={disabled || options.length === 0}
      fitContent
      searchable={options.length > 10}
      triggerDataAttrs={{ 'data-testid': testId }}
      className="h-6 border-0 bg-transparent px-0 text-[12px] text-fg-muted shadow-none hover:text-accent"
    />
  );
}
