import { useMemo } from 'react';
import { marked } from 'marked';
import { parseMessageSegments, isSqlCodeBlock } from '../../lib/aiMessageBlocks';
import { sanitizeHtml } from '../../lib/htmlSanitizer';
import { cn } from '../../lib/cn';
import { AiCodeBlock } from './AiCodeBlock';

interface AiMessageContentProps {
  content: string;
  sqlDialect?: string;
  onInsertSql?: (sql: string) => void;
  onRunCode?: (code: string, language: string) => void;
  onNewQuery?: (code: string) => void;
  isStreaming?: boolean;
}

// Configure marked for GFM subset (no HTML passthrough)
marked.setOptions({
  gfm: true,
  breaks: true,
  pedantic: false,
});

/**
 * Custom renderer that replaces <code> blocks containing SQL with AiCodeBlock
 * and wraps regular code blocks in a styled container.
 */
function createCustomRenderer(isStreaming: boolean | undefined) {
  const renderer = new marked.Renderer();

  renderer.code = function ({ text, lang }: { text: string; lang?: string }) {
    const language = (lang ?? '').toLowerCase();
    const code = text.replace(/\n$/, '');

    if (isSqlCodeBlock(language, code)) {
      // Return a placeholder div; AiCodeBlock will be rendered by the parent
      return `<div data-ai-code="sql" data-language="${language}" data-streaming="${isStreaming ?? false}">${escapeHtmlAttr(code)}</div>`;
    }

    return `<div class="my-2 overflow-hidden rounded-md border border-edge bg-surface"><div class="flex items-center gap-1.5 border-b border-edge bg-surface-alt px-2 py-1"><span class="text-[10px] font-medium uppercase text-fg-muted">${escapeHtmlAttr(language || 'code')}</span></div><pre class="p-2 text-[11px] font-mono text-fg-secondary whitespace-pre-wrap overflow-x-auto"><code>${escapeHtml(code)}</code></pre></div>`;
  };

  return renderer;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function escapeHtmlAttr(text: string): string {
  return text.replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/**
 * Renders AI message content with GFM Markdown support.
 *
 * - Fenced code blocks (```lang ... ```) are extracted and rendered via AiCodeBlock.
 * - Remaining markdown is rendered via marked with GFM enabled.
 * - Output is sanitized to prevent XSS.
 */
export function AiMessageContent({
  content,
  sqlDialect,
  onInsertSql,
  onRunCode,
  onNewQuery,
  isStreaming,
}: AiMessageContentProps) {
  const segments = parseMessageSegments(content);

  // Render markdown for text segments, keeping code segments separate
  const renderedSegments = useMemo(() => {
    return segments.map((segment) => {
      if (segment.type === 'code') {
        return { ...segment, rendered: null };
      }

      const text = segment.content.trim();
      if (!text) return { ...segment, rendered: null };

      // If streaming and text doesn't end with a newline, it may be incomplete.
      // Render as markdown but don't let unclosed inline tags cause issues.
      const renderer = createCustomRenderer(isStreaming);

      const rawHtml = marked.parse(text, { renderer, async: false }) as string;
      const safeHtml = sanitizeHtml(rawHtml);

      return { ...segment, rendered: safeHtml };
    });
  }, [segments, isStreaming]);

  if (renderedSegments.length === 0) return null;

  // Single text segment — render markdown directly
  if (renderedSegments.length === 1 && renderedSegments[0].type === 'text') {
    const seg = renderedSegments[0];
    if (!seg.rendered) return null;
    return (
      <div
        className={cn(
          'ai-markdown prose-invert max-w-none text-xs',
          isStreaming && 'animate-pulse',
        )}
        dangerouslySetInnerHTML={{ __html: seg.rendered }}
      />
    );
  }

  return (
    <div className={cn(isStreaming && 'animate-pulse')}>
      {renderedSegments.map((segment, idx) => {
        if (segment.type === 'text') {
          if (!segment.rendered) return null;
          return (
            <div
              key={idx}
              className="ai-markdown prose-invert max-w-none text-xs"
              dangerouslySetInnerHTML={{ __html: segment.rendered }}
            />
          );
        }

        // Code segment
        return (
          <AiCodeBlock
            key={idx}
            language={segment.language}
            code={segment.code}
            sqlDialect={sqlDialect}
            onInsertSql={onInsertSql}
            onRunCode={onRunCode}
            onNewQuery={onNewQuery}
            isStreaming={isStreaming}
          />
        );
      })}
    </div>
  );
}
