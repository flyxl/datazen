import { SettingHint } from './SettingHint';

export function SectionTitle({ children, hint }: { children: string; hint?: string }) {
  return (
    <h2 className="text-[13px] font-semibold uppercase tracking-wider text-fg-muted">
      {children}
      {hint && <SettingHint label={children} text={hint} />}
    </h2>
  );
}

export function SettingRow({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-start gap-4">
      <div className="flex min-h-9 w-32 shrink-0 items-center gap-1">
        <span className="min-w-0 text-sm leading-5 text-fg-secondary">{label}</span>
        {hint && <SettingHint label={label} text={hint} />}
      </div>
      <div className="min-w-0 flex-1">{children}</div>
    </div>
  );
}

export function ToggleRow({
  label,
  hint,
  checked,
  onChange,
  testId,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  /** Stable hook for tests; the accessible name is translated, so it cannot be one. */
  testId?: string;
}) {
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="text-sm text-fg-secondary">
        {label}
        {hint && <SettingHint label={label} text={hint} />}
      </div>
      <button
        type="button"
        role="switch"
        aria-label={label}
        aria-checked={checked}
        data-testid={testId}
        onClick={() => onChange(!checked)}
        className={`relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors ${
          checked ? 'bg-accent' : 'bg-edge'
        }`}
      >
        <span
          className={`inline-block h-3.5 w-3.5 rounded-full bg-white shadow transition-transform ${
            checked ? 'translate-x-[18px]' : 'translate-x-[3px]'
          }`}
        />
      </button>
    </div>
  );
}
