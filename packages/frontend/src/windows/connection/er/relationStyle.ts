/**
 * How the ER canvas tells a declared relationship from an inferred one.
 *
 * The graph builder and the legend both read these, so the swatch in the legend
 * is literally the stroke drawn on the edge. A second copy in the legend could
 * drift from the canvas, and a legend that lies is worse than none.
 */

/** Stroke of a relationship a foreign key constraint declares. */
export const ER_DECLARED_COLOR = 'var(--c-accent, #3b82f6)';

/** Stroke of a relationship the engine inferred from structure and naming. */
export const ER_PREDICTED_COLOR = 'var(--c-warning, #e39a27)';

/**
 * Dash pattern for an inferred relationship.
 *
 * Dashed rather than only recoloured: a colour-only distinction disappears in a
 * monochrome export and for a colour-blind reader, while a dash survives both.
 */
export const ER_PREDICTED_DASH = '4 3';
