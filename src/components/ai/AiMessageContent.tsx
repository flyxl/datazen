import { parseMessageSegments } from '../../lib/aiMessageBlocks';
import { cn } from '../../lib/cn';
import { AiCodeBlock } from './AiCodeBlock';

interface AiMessageContentProps {
  content: string;
  sqlDialect?: string;
  onInsertSql?: (sql: string) => void;
  isStreaming?: boolean;
}

export function AiMessageContent({
  content,
  sqlDialect,
  onInsertSql,
  isStreaming,
}: AiMessageContentProps) {
  const segments = parseMessageSegments(content);

  if (segments.length === 0) return null;

  // Plain text without fences — keep simple pre-wrap rendering
  if (segments.length === 1 && segments[0].type === 'text') {
    return (
      <pre className={cn('whitespace-pre-wrap font-sans', isStreaming && 'animate-pulse')}>
        {segments[0].content}
      </pre>
    );
  }

  return (
    <div className={cn(isStreaming && 'animate-pulse')}>
      {segments.map((segment, idx) =>
        segment.type === 'text' ? (
          segment.content.trim() ? (
            <pre key={idx} className="whitespace-pre-wrap font-sans">
              {segment.content}
            </pre>
          ) : null
        ) : (
          <AiCodeBlock
            key={idx}
            language={segment.language}
            code={segment.code}
            sqlDialect={sqlDialect}
            onInsertSql={onInsertSql}
            isStreaming={isStreaming}
          />
        ),
      )}
    </div>
  );
}
