//! Read-only value viewer: the decode (codec) × view matrix for Redis string
//! values (R8/R9). Operates on the binary-safe `ValueFrame.rawB64` so non-UTF-8
//! payloads survive the round trip. This component never mutates the stored
//! value — editing lives in `StringEditor`'s textarea path.

import { useCallback, useEffect, useRef, useState } from 'react';
import { Download, Copy } from 'lucide-react';
import { Button, cn } from '@datazen/ui';
import { useI18n } from '@datazen/ui';
import type { ValueFrame } from '../shared/types';
import { formatSize } from '../shared/formatSize';
import { invokeDecodeValue, redisCommandInvoke, type RedisInvokeFn } from '../shared/redisInvoke';
import {
  applyBrowserCodec,
  base64ToBytes,
  isBackendCodec,
  CODECS,
  type Codec,
} from './valueView/codecs';
import { renderView, VIEWS, type RenderedView, type ViewMode } from './valueView/views';
import { ValueViewOutput } from './valueView/render';

export interface ValueViewerProps {
  dbSessionId: string;
  frame: ValueFrame | null;
  invoke?: RedisInvokeFn;
  /**
   * 受控视图 / 解码选择（E-3）。缺省时组件自持状态（旧的独立预览用法不变）；
   * 宿主传入时，宿主要按当前 view 判定 I-5 只读态，所以这里只做受控转发。
   */
  view?: ViewMode;
  codec?: Codec;
  onViewChange?: (view: ViewMode) => void;
  onCodecChange?: (codec: Codec) => void;
  /**
   * false ⇒ 只渲染 Codec / View 两行"渲染预检"控件，把输出区让给宿主的常驻编辑区
   * （PRD §3.3：编辑区下方占满剩余高度，不允许再叠一份只读预览）。
   */
  showOutput?: boolean;
}

type Status = 'idle' | 'busy' | 'error';

export function ValueViewer({
  dbSessionId,
  frame,
  invoke = redisCommandInvoke,
  view: controlledView,
  codec: controlledCodec,
  onViewChange,
  onCodecChange,
  showOutput = true,
}: ValueViewerProps) {
  const { t } = useI18n();
  const [innerCodec, setInnerCodec] = useState<Codec>('none');
  const [innerView, setInnerView] = useState<ViewMode>('utf8');
  const [wrap, setWrap] = useState(true);
  const [status, setStatus] = useState<Status>('idle');
  const [errorText, setErrorText] = useState<string | null>(null);
  const [rendered, setRendered] = useState<RenderedView | null>(null);
  const seqRef = useRef(0);

  const codec = controlledCodec ?? innerCodec;
  const view = controlledView ?? innerView;
  const selectCodec = useCallback(
    (next: Codec) => {
      onCodecChange?.(next);
      if (controlledCodec === undefined) setInnerCodec(next);
    },
    [onCodecChange, controlledCodec],
  );
  const selectView = useCallback(
    (next: ViewMode) => {
      onViewChange?.(next);
      if (controlledView === undefined) setInnerView(next);
    },
    [onViewChange, controlledView],
  );

  const rawB64 = frame?.rawB64 ?? null;
  const memBytes = frame?.memBytes ?? null;

  const recompute = useCallback(async () => {
    if (!rawB64 || !showOutput) {
      setRendered(null);
      setStatus('idle');
      return;
    }
    const seq = ++seqRef.current;
    setStatus('busy');
    setErrorText(null);
    try {
      const bytes = base64ToBytes(rawB64);
      let decoded: Uint8Array;
      if (isBackendCodec(codec)) {
        const res = await invokeDecodeValue(dbSessionId, codec, rawB64, invoke);
        if (!res.ok) {
          // In-band decode failure (C-3): show the stable `reason` code as a
          // visible error — a failed decode must never render as `(empty)`.
          if (seq !== seqRef.current) return;
          setErrorText(res.reason ?? null);
          setStatus('error');
          setRendered(null);
          return;
        }
        decoded = new TextEncoder().encode(res.json ?? '');
      } else {
        decoded = await applyBrowserCodec(bytes, codec);
      }
      if (seq !== seqRef.current) return;
      setRendered(renderView(decoded, view));
      setStatus('idle');
    } catch (e) {
      if (seq !== seqRef.current) return;
      setErrorText(e instanceof Error ? e.message : String(e));
      setStatus('error');
      setRendered(null);
    }
  }, [rawB64, codec, view, dbSessionId, invoke, showOutput]);

  useEffect(() => {
    void recompute();
  }, [recompute]);

  // Switching to a byte view while a backend codec is active is fine; but when
  // leaving codec 'none' the base text view may be meaningless — keep utf8.
  const copyText = useCallback(() => {
    const text = rendered?.kind === 'text' ? rendered.text : (rawB64 ?? '');
    void navigator.clipboard?.writeText(text);
  }, [rendered, rawB64]);

  const downloadBytes = useCallback(() => {
    if (!rawB64) return;
    let bytes: Uint8Array;
    try {
      bytes = base64ToBytes(rawB64);
    } catch {
      return;
    }
    const copy = new ArrayBuffer(bytes.byteLength);
    new Uint8Array(copy).set(bytes);
    const url = URL.createObjectURL(new Blob([copy]));
    const a = document.createElement('a');
    a.href = url;
    a.download = frame?.key ? `${frame.key}.bin` : 'value.bin';
    a.click();
    URL.revokeObjectURL(url);
  }, [rawB64, frame?.key]);

  const noPayload = !frame || !rawB64;
  // 载荷缺席（只读态②的截断大 value 就是 `rawB64 === null`）只影响**输出区**：
  // Codec / View 两行必须留着，否则用户没法把视图切回文本档。

  return (
    <div
      className="space-y-2"
      data-testid="redis-value-viewer"
      data-show-output={showOutput ? 'true' : 'false'}
    >
      <div className="flex flex-wrap items-center gap-2 text-[11px]">
        <span className="text-fg-muted">{t('redis.codec.label')}</span>
        <div className="flex flex-wrap gap-1" data-testid="redis-codec-group" role="group">
          {CODECS.map((c) => (
            <button
              key={c}
              type="button"
              data-testid={`redis-codec-${c}`}
              className={cn(
                'rounded px-1.5 py-0.5 transition-colors',
                codec === c
                  ? 'bg-accent/15 text-accent'
                  : 'text-fg-secondary hover:bg-surface-raised',
              )}
              onClick={() => selectCodec(c)}
            >
              {t(`redis.codec.${c}` as 'redis.codec.none')}
            </button>
          ))}
        </div>
        {memBytes != null && (
          <span className="ml-auto rounded bg-surface-alt px-1.5 py-0.5 text-fg-muted">
            {formatSize(memBytes)}
          </span>
        )}
      </div>

      <div className="flex flex-wrap items-center gap-2 text-[11px]">
        <span className="text-fg-muted">{t('redis.view.label')}</span>
        <div className="flex flex-wrap gap-1" data-testid="redis-view-group" role="group">
          {VIEWS.map((v) => (
            <button
              key={v}
              type="button"
              data-testid={`redis-view-${v}`}
              className={cn(
                'rounded px-1.5 py-0.5 transition-colors',
                view === v
                  ? 'bg-accent/15 text-accent'
                  : 'text-fg-secondary hover:bg-surface-raised',
              )}
              onClick={() => selectView(v)}
            >
              {t(`redis.view.${v}` as 'redis.view.utf8')}
            </button>
          ))}
        </div>
      </div>

      {showOutput && noPayload && (
        <div
          className="rounded-md border border-edge bg-surface-alt px-2 py-3 text-center text-xs text-fg-muted"
          data-testid="redis-value-no-data"
        >
          {t('redis.view.noData')}
        </div>
      )}

      {showOutput && !noPayload && (
        <>
          <div className="flex flex-wrap items-center gap-2">
            <label className="flex cursor-pointer items-center gap-1.5 text-[11px] text-fg-secondary">
              <input
                type="checkbox"
                checked={wrap}
                onChange={(e) => setWrap(e.target.checked)}
                className="rounded border-edge"
                data-testid="redis-view-wrap"
              />
              {t('redis.view.wrap')}
            </label>
            <Button
              variant="secondary"
              className="h-6 gap-1 px-2 text-[11px]"
              onClick={copyText}
              data-testid="redis-view-copy"
            >
              <Copy className="h-3 w-3" />
              {t('redis.view.copy')}
            </Button>
            <Button
              variant="secondary"
              className="h-6 gap-1 px-2 text-[11px]"
              onClick={downloadBytes}
              data-testid="redis-view-download"
            >
              <Download className="h-3 w-3" />
              {t('redis.view.download')}
            </Button>
          </div>

          {status === 'error' && errorText && (
            <div className="rounded-md border border-danger/20 bg-danger/10 px-2 py-1.5 text-xs text-danger">
              {errorText}
            </div>
          )}

          <ValueViewOutput rendered={rendered} busy={status === 'busy'} wrap={wrap} />
        </>
      )}
    </div>
  );
}
