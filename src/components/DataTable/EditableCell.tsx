import { useEffect, useRef, useState } from 'react';
import { Input } from '../ui/Input';

export interface EditableCellProps {
  value: unknown;
  type: string;
  onCommit: (value: unknown) => void;
  onCancel: () => void;
}

function toDisplayString(v: unknown): string {
  if (v === null || v === undefined) return '';
  return String(v);
}

export function EditableCell({ value, type, onCommit, onCancel }: EditableCellProps) {
  const initial = useRef(toDisplayString(value));
  const [local, setLocal] = useState(() => toDisplayString(value));
  const done = useRef(false);

  useEffect(() => {
    const s = toDisplayString(value);
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
    onCommit(raw);
  }

  function handleCancel() {
    if (done.current) return;
    done.current = true;
    onCancel();
  }

  return (
    <Input
      autoFocus
      value={local}
      onChange={(e) => setLocal(e.target.value)}
      className="h-8 font-mono text-xs"
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
