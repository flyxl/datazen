import { useEffect, useRef, useState } from 'react';
import { cn } from '../../lib/cn';

export interface EditableCellProps {
  value: unknown;
  type: string;
  onCommit: (value: unknown) => void;
  onCancel: () => void;
}

export function toEditString(v: unknown): string {
  if (v === null || v === undefined) return '';
  if (typeof v === 'object') {
    try {
      return JSON.stringify(v);
    } catch {
      return String(v);
    }
  }
  return String(v);
}

export function EditableCell({ value, type, onCommit, onCancel }: EditableCellProps) {
  const initial = useRef(toEditString(value));
  const [local, setLocal] = useState(() => toEditString(value));
  const done = useRef(false);

  useEffect(() => {
    const s = toEditString(value);
    initial.current = s;
    setLocal(s);
  }, [value]);

  function coerceAndCommit(raw: string) {
    if (done.current) return;
    done.current = true;

    if (raw === '' && (value === null || value === undefined)) {
      onCancel();
      return;
    }
    if (raw === initial.current) {
      onCancel();
      return;
    }
    if (raw === '') {
      onCommit(null);
      return;
    }
    if (type.includes('int') || type.includes('serial') || type.includes('bigint')) {
      onCommit(Number(raw));
      return;
    }
    if (type.includes('bool')) {
      onCommit(raw === 'true');
      return;
    }
    if (
      type.includes('float') ||
      type.includes('double') ||
      type.includes('numeric') ||
      type.includes('decimal') ||
      type.includes('real')
    ) {
      onCommit(Number(raw));
      return;
    }
    if (type.includes('json')) {
      try {
        onCommit(JSON.parse(raw));
      } catch {
        onCommit(raw);
      }
      return;
    }
    onCommit(raw);
  }

  function handleCancel() {
    if (done.current) return;
    done.current = true;
    onCancel();
  }

  return (
    <input
      autoFocus
      value={local}
      onChange={(e) => setLocal(e.target.value)}
      aria-label="Edit cell value"
      spellCheck={false}
      autoCapitalize="off"
      autoCorrect="off"
      className={cn(
        // Grid-cell editor: fill the cell edge-to-edge with a compact height
        // and minimal inset so it reads as part of the table, not a heavy
        // form input. Text alignment matches the surrounding cell content.
        'w-full min-w-0 h-7 bg-surface align-middle font-mono text-xs text-fg',
        'outline-none',
        'placeholder:text-fg-muted',
        'selection:bg-accent/30',
        'rounded-sm border border-accent shadow-none',
        'px-1.5',
      )}
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          coerceAndCommit(local);
        }
        if (e.key === 'Escape') {
          e.preventDefault();
          handleCancel();
        }
      }}
      onBlur={() => coerceAndCommit(local)}
    />
  );
}
