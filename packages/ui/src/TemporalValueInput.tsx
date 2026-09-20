/**
 * Themed temporal value control (text field + in-app calendar/time popover).
 *
 * The native `datetime-local` widget renders with the OS theme at its own
 * size and format, so this control keeps the text field free-typable and
 * adds an in-app popover styled with the shared surface tokens — it follows
 * the app theme in both light and dark mode.
 *
 * Emitted values are canonical: `YYYY-MM-DD`, `HH:MM:SS`, or
 * `YYYY-MM-DDTHH:MM:SS` — the same shape database clients display.
 */
import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { cn } from './cn';
import { Input } from './Input';

export type TemporalPickerKind = 'date' | 'time' | 'datetime';

export interface TemporalValueInputProps {
  kind: TemporalPickerKind;
  value: string;
  onChange: (next: string) => void;
  placeholder?: string;
  /** Highlights the field; the dialog blocks OK on its own. */
  invalid?: boolean;
  /** IS NULL / IS NOT NULL operators have no value to pick. */
  disabled?: boolean;
  'data-testid'?: string;
}

const pad2 = (n: number | string): string => String(n).padStart(2, '0');

interface Parsed {
  y: number | null;
  mo: number | null;
  d: number | null;
  h: string;
  mi: string;
  s: string;
}

const VALUE_RE =
  /^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{1,2}):(\d{1,2})(?::(\d{1,2}))?)?|^(\d{1,2}):(\d{1,2})(?::(\d{1,2}))?/;

export function parseTemporal(value: string): Parsed {
  const m = VALUE_RE.exec(value ?? '');
  if (m && m[1]) {
    return {
      y: Number(m[1]),
      mo: Number(m[2]),
      d: Number(m[3]),
      h: m[4] ?? '',
      mi: m[5] ?? '',
      s: m[6] ?? '',
    };
  }
  if (m && m[7]) {
    return { y: null, mo: null, d: null, h: m[7], mi: m[8], s: m[9] ?? '' };
  }
  return { y: null, mo: null, d: null, h: '', mi: '', s: '' };
}

function datePart(p: Parsed): string {
  if (p.y === null || p.mo === null || p.d === null) return '';
  return `${p.y}-${pad2(p.mo)}-${pad2(p.d)}`;
}

function timePart(p: Parsed): string {
  if (!p.h && !p.mi && !p.s) return '';
  return `${pad2(p.h)}:${pad2(p.mi)}:${pad2(p.s)}`;
}

/** Compose the canonical string for `kind` from its current parts. */
function compose(kind: TemporalPickerKind, p: Parsed): string {
  const date = datePart(p);
  const time = timePart(p);
  if (kind === 'date') return date;
  if (kind === 'time') return time;
  if (!date && !time) return '';
  return `${date || todayDate()}T${time || '00:00:00'}`;
}

function todayDate(): string {
  const now = new Date();
  return `${now.getFullYear()}-${pad2(now.getMonth() + 1)}-${pad2(now.getDate())}`;
}

const WEEKDAY_BASE = new Date(2000, 0, 2); // a Sunday
const POPOVER_WIDTH = 236;

function weekdayLabels(): string[] {
  const fmt = new Intl.DateTimeFormat(undefined, { weekday: 'short' });
  return Array.from({ length: 7 }, (_, i) =>
    fmt.format(new Date(WEEKDAY_BASE.getFullYear(), 0, 2 + i)),
  );
}

export function TemporalValueInput({
  kind,
  value,
  onChange,
  placeholder,
  invalid,
  disabled,
  'data-testid': testId = 'qb-temporal-input',
}: TemporalValueInputProps) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const parsed = parseTemporal(value);
  const today = new Date();
  const [viewYear, setViewYear] = useState(parsed.y ?? today.getFullYear());
  const [viewMonth, setViewMonth] = useState(parsed.mo ?? today.getMonth() + 1);

  // The popover is portalled to `document.body` with fixed coordinates: the
  // dialog body is a scroll container, and an in-flow absolute popover both
  // gets clipped by it and drags horizontal/vertical scrollbars along.
  const place = (): void => {
    const rect = rootRef.current?.getBoundingClientRect();
    if (!rect) return;
    const height = kind === 'time' ? 44 : kind === 'date' ? 250 : 292;
    let top = rect.bottom + 4;
    if (top + height > window.innerHeight - 8) {
      top = Math.max(8, rect.top - height - 4);
    }
    const left = Math.min(Math.max(8, rect.left), window.innerWidth - POPOVER_WIDTH - 8);
    setPos({ left, top });
  };

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (e: PointerEvent): void => {
      const target = e.target as Node;
      if (!rootRef.current?.contains(target) && !popoverRef.current?.contains(target)) {
        setOpen(false);
      }
    };
    const onReflow = (): void => setOpen(false);
    document.addEventListener('pointerdown', onPointerDown);
    window.addEventListener('resize', onReflow);
    window.addEventListener('scroll', onReflow, true);
    return () => {
      document.removeEventListener('pointerdown', onPointerDown);
      window.removeEventListener('resize', onReflow);
      window.removeEventListener('scroll', onReflow, true);
    };
  }, [open]);

  const pickDay = (day: number): void => {
    onChange(
      compose(kind, {
        ...parsed,
        y: viewYear,
        mo: viewMonth,
        d: day,
      }),
    );
    // Date-only columns are complete after one click; datetime stays open so
    // the time row can be adjusted.
    if (kind === 'date') setOpen(false);
  };

  const setTimeSegment = (part: 'h' | 'mi' | 's', raw: string): void => {
    const digits = raw.replace(/\D/g, '').slice(0, 2);
    // Editing the time before any date is picked still needs a date for the
    // datetime form — default it to today, keeping the typed segments.
    const base: Parsed = { ...parsed };
    if (kind === 'datetime' && base.y === null) {
      const t = parseTemporal(todayDate());
      base.y = t.y;
      base.mo = t.mo;
      base.d = t.d;
    }
    onChange(compose(kind, { ...base, [part]: digits }));
  };

  const openPopover = (): void => {
    if (open) {
      setOpen(false);
      return;
    }
    setViewYear(parsed.y ?? today.getFullYear());
    setViewMonth(parsed.mo ?? today.getMonth() + 1);
    place();
    setOpen(true);
  };

  return (
    <div ref={rootRef} className="relative w-full" data-testid={testId}>
      <Input
        value={value}
        placeholder={placeholder}
        disabled={disabled}
        aria-invalid={invalid || undefined}
        onChange={(e) => onChange(e.target.value)}
        className={cn(
          'h-8 w-full pr-8 text-xs font-mono',
          invalid && 'border-danger focus:border-danger',
        )}
        data-testid={`${testId}-text`}
      />
      <button
        type="button"
        aria-label="Open calendar"
        disabled={disabled}
        onClick={openPopover}
        data-testid={`${testId}-toggle`}
        className="absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-fg-muted hover:bg-surface-raised hover:text-fg"
      >
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
        >
          <rect x="3" y="4" width="18" height="18" rx="2" />
          <line x1="16" y1="2" x2="16" y2="6" />
          <line x1="8" y1="2" x2="8" y2="6" />
          <line x1="3" y1="10" x2="21" y2="10" />
        </svg>
      </button>

      {open &&
        pos &&
        createPortal(
          <div
            ref={popoverRef}
            style={{ position: 'fixed', left: pos.left, top: pos.top, width: POPOVER_WIDTH }}
            className="z-50 rounded-lg border border-edge bg-surface-alt p-2 shadow-xl"
            data-testid={`${testId}-popover`}
          >
            {kind !== 'time' && (
              <CalendarGrid
                year={viewYear}
                month={viewMonth}
                selected={parsed}
                onPrevMonth={() => shiftMonth(viewYear, viewMonth, -1, setViewYear, setViewMonth)}
                onNextMonth={() => shiftMonth(viewYear, viewMonth, 1, setViewYear, setViewMonth)}
                onPickDay={pickDay}
              />
            )}
            {kind !== 'date' && (
              <TimeRow kind={kind} parsed={parsed} onSegment={setTimeSegment} testId={testId} />
            )}
          </div>,
          document.body,
        )}
    </div>
  );
}

function shiftMonth(
  year: number,
  month: number,
  delta: number,
  setYear: (y: number) => void,
  setMonth: (m: number) => void,
): void {
  const next = month - 1 + delta;
  setYear(year + Math.floor(next / 12));
  setMonth((((next % 12) + 12) % 12) + 1);
}

function CalendarGrid({
  year,
  month,
  selected,
  onPrevMonth,
  onNextMonth,
  onPickDay,
}: {
  year: number;
  month: number;
  selected: Parsed;
  onPrevMonth: () => void;
  onNextMonth: () => void;
  onPickDay: (day: number) => void;
}) {
  const firstWeekday = new Date(year, month - 1, 1).getDay();
  const daysInMonth = new Date(year, month, 0).getDate();
  const title = new Intl.DateTimeFormat(undefined, {
    year: 'numeric',
    month: 'long',
  }).format(new Date(year, month - 1, 1));
  const now = new Date();

  return (
    <div>
      <div className="mb-1 flex items-center justify-between text-xs text-fg">
        <button
          type="button"
          aria-label="Previous month"
          onClick={onPrevMonth}
          data-testid="qb-cal-prev"
          className="rounded px-1.5 py-0.5 hover:bg-surface-raised"
        >
          ‹
        </button>
        <span className="font-semibold">{title}</span>
        <button
          type="button"
          aria-label="Next month"
          onClick={onNextMonth}
          data-testid="qb-cal-next"
          className="rounded px-1.5 py-0.5 hover:bg-surface-raised"
        >
          ›
        </button>
      </div>
      <div className="grid grid-cols-7 gap-0.5 text-center text-[10px] text-fg-muted">
        {weekdayLabels().map((w) => (
          <span key={w} className="py-0.5">
            {w}
          </span>
        ))}
      </div>
      <div className="grid grid-cols-7 gap-0.5">
        {Array.from({ length: firstWeekday }, (_, i) => (
          <span key={`pad-${i}`} />
        ))}
        {Array.from({ length: daysInMonth }, (_, i) => {
          const day = i + 1;
          const isSelected = selected.y === year && selected.mo === month && selected.d === day;
          const isToday =
            now.getFullYear() === year && now.getMonth() + 1 === month && now.getDate() === day;
          return (
            <button
              key={day}
              type="button"
              onClick={() => onPickDay(day)}
              data-testid={`qb-cal-day-${day}`}
              className={cn(
                'h-6 rounded text-[11px] text-fg hover:bg-surface-raised',
                isToday && 'border border-accent',
                isSelected && 'bg-accent text-on-accent hover:bg-accent',
              )}
            >
              {day}
            </button>
          );
        })}
      </div>
    </div>
  );
}

function TimeRow({
  kind,
  parsed,
  onSegment,
  testId,
}: {
  kind: TemporalPickerKind;
  parsed: Parsed;
  onSegment: (part: 'h' | 'mi' | 's', raw: string) => void;
  testId: string;
}) {
  const segment = (part: 'h' | 'mi' | 's', label: string) => (
    <input
      value={parsed[part]}
      inputMode="numeric"
      maxLength={2}
      placeholder={label}
      aria-label={label}
      onChange={(e) => onSegment(part, e.target.value)}
      data-testid={`${testId}-${part}`}
      className="h-6 w-8 rounded border border-edge bg-surface-inset px-1 text-center font-mono text-[11px] text-fg outline-none focus:border-accent"
    />
  );
  return (
    <div
      className={cn(
        'flex items-center justify-center gap-0.5 text-fg-muted',
        kind !== 'time' && 'mt-1 border-t border-edge pt-1',
      )}
    >
      {segment('h', 'HH')}
      <span>:</span>
      {segment('mi', 'MM')}
      <span>:</span>
      {segment('s', 'SS')}
    </div>
  );
}
