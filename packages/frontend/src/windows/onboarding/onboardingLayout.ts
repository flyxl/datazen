/**
 * Shared chrome metrics of the wizard shell.
 *
 * The brand sidebar's footer (app version) and the wizard's right-hand footer
 * (step indicator) each render one line of small mono text. Both rows must end
 * on the same bottom line, so they share one height and centre their content
 * inside it. Never hard-code these values inside a single component: changing
 * one without the other is what makes the two footers drift apart again.
 */
export const WIZARD_FOOTER_HEIGHT_CLASS = 'h-[56px]';

/** Horizontal rhythm of the right-hand footer. */
export const WIZARD_FOOTER_PX_CLASS = 'px-[48px]';

/** Horizontal rhythm of the brand sidebar. */
export const WIZARD_ASIDE_PX_CLASS = 'px-[34px]';

/** Typography shared by the version label and the step indicator. */
export const WIZARD_CHROME_TEXT_CLASS = 'text-[11.5px] font-mono text-fg-muted';
