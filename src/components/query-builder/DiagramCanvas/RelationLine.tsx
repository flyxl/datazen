import { useCallback } from 'react';
import type { RelationShape } from './fkGeometry';

export interface RelationLineProps {
  shape: RelationShape;
  /** True while this group is hovered or has its popover open. */
  active: boolean;
  onHoverChange: (groupId: string | null) => void;
  /**
   * Open the actions popover. Receives the raw **client** coordinates of the
   * click; the canvas converts them, because only it knows where its viewport
   * is and how far it is zoomed/panned.
   */
  onActivate: (groupId: string, origin: { clientX: number; clientY: number }) => void;
  /** True while a manual join drag is in progress (lines dim to reduce noise). */
  dimmed?: boolean;
}

/**
 * One relation, drawn as lines only — no glyphs, no labels.
 *
 * A group renders as a set of **orthogonal polylines**: one elbow for a plain
 * FK or manual join, and `stub → trunk → stub` for a composite FK (see
 * `fkGeometry`). Every segment carries its own state so a half-confirmed
 * composite group is visibly half-solid.
 *
 * No arrowheads: a connection is a relationship, not a direction. Terminals are
 * symmetric dots, and a chevron marks an end whose column is scrolled out of the
 * card's list (the line is clamped to the visible edge in that case).
 *
 * Interaction lives on a wide invisible copy of the path, because a 1.75px
 * stroke is far too thin to hit reliably.
 */
export function RelationLine({
  shape,
  active,
  onHoverChange,
  onActivate,
  dimmed = false,
}: RelationLineProps) {
  const handleClick = useCallback(
    (e: React.MouseEvent<SVGPathElement>) => {
      e.stopPropagation();
      onActivate(shape.groupId, { clientX: e.clientX, clientY: e.clientY });
    },
    [onActivate, shape.groupId],
  );

  return (
    <g
      className={[
        'qb-relation',
        `qb-relation--${shape.kind}`,
        active ? 'is-active' : '',
        dimmed ? 'is-dimmed' : '',
      ]
        .filter(Boolean)
        .join(' ')}
      data-testid={`qb-relation-${shape.groupId}`}
      data-relation-kind={shape.kind}
      data-relation-state={shape.state}
      data-relation-pairs={`${shape.confirmedCount}/${shape.pairCount}`}
      onPointerEnter={() => onHoverChange(shape.groupId)}
      onPointerLeave={() => onHoverChange(null)}
    >
      {shape.segments.map((segment, index) => (
        <path
          key={`seg-${index}`}
          className={`qb-relation-line qb-relation-line--${segment.state}`}
          data-part={segment.part}
          d={segment.d}
        />
      ))}

      {shape.dots.map((dot, index) => (
        <circle
          key={`dot-${index}`}
          className={
            `qb-relation-dot qb-relation-dot--${dot.state}` +
            // Clamped to the list edge because the column is scrolled out of
            // view: a hollow ring, so nothing on the canvas can be read as a
            // direction arrow.
            (dot.offscreen ? ' qb-relation-dot--offscreen' : '')
          }
          data-offscreen={dot.offscreen ?? undefined}
          cx={dot.x}
          cy={dot.y}
          r={3.2}
        />
      ))}

      {/* Wide invisible hit paths — one per drawn segment so the whole
          composite group (trunk included) is clickable. */}
      {shape.segments.map((segment, index) => (
        <path
          key={`hit-${index}`}
          className="qb-relation-hit"
          d={segment.d}
          onClick={handleClick}
          data-testid={`qb-relation-hit-${shape.groupId}`}
        />
      ))}
    </g>
  );
}
