//! Port of `packages/frontend/src/windows/onboarding/onboardingLayout.ts`
//! (T287).
//!
//! The React file is the wizard shell's shared chrome metrics: four exported
//! Tailwind class constants consumed by `BrandSidebar.tsx` (aside padding,
//! footer height, chrome text) and `WizardFooter.tsx` (footer padding,
//! footer height, chrome text). Its one layout rule, stated in its own doc
//! comment: the brand sidebar's version row and the wizard's step-indicator
//! footer each render one line of small mono text, both rows must end on
//! the same bottom line, so they share one height and centre their content
//! inside it — never hard-code these values inside a single component.
//!
//! ## What maps where
//!
//! - Tailwind arbitrary-value classes become point constants (×0.75):
//!   `h-[56px]` → 42pt, `px-[48px]` → 36pt, `px-[34px]` → 25.5pt.
//! - `WIZARD_CHROME_TEXT_CLASS` (`text-[11.5px] font-mono text-fg-muted`)
//!   splits in two: the typographic half is [`chrome_text`] (8.625pt mono,
//!   weight 400), the colour half (`text-fg-muted`) is applied by the
//!   callers through `kit(dark).c.text_muted` (the established T015/T029
//!   mapping) — a layout module stays palette-free.
//! - This module absorbs the seam T021 left behind in `brand_sidebar.rs`:
//!   the sidebar's `ASIDE_PAD_X` / `FOOTER_HEIGHT` consts and its private
//!   `chrome_text` were the placeholder home for these values ("the seam
//!   T287 absorbs"); they now live here once, and `brand_sidebar.rs`
//!   re-exports them under their T021 names so the T021 audit's import
//!   path is untouched.
//! - Consumers: `brand_sidebar.rs` (T021) and the window shell
//!   (`onboarding_window.rs`, T072); `WizardFooter` (T247) and the wizard
//!   (T114) pick the constants up when they land.
//!
//! ## Keys
//!
//! None — the React file exports layout constants only and renders
//! nothing; its consumers document their own keys in their own modules.

use rune::TextStyle;

/// React `WIZARD_FOOTER_HEIGHT_CLASS` (`h-[56px]`, 56px → 42pt): the one
/// height the brand sidebar's version row and the wizard's step-indicator
/// footer share, so both lines of chrome text end on the same bottom line.
pub(crate) const WIZARD_FOOTER_HEIGHT: f32 = 42.0;

/// React `WIZARD_FOOTER_PX_CLASS` (`px-[48px]`, 48px → 36pt): the
/// right-hand footer's horizontal rhythm.
pub(crate) const WIZARD_FOOTER_PX: f32 = 36.0;

/// React `WIZARD_ASIDE_PX_CLASS` (`px-[34px]`, 34px → 25.5pt): the brand
/// sidebar's horizontal rhythm.
pub(crate) const WIZARD_ASIDE_PX: f32 = 25.5;

/// React `WIZARD_CHROME_TEXT_CLASS`'s typographic half
/// (`text-[11.5px] font-mono`, 11.5px → 8.625pt): the version label and
/// the step indicator's shared small mono text. The class's `text-fg-muted`
/// colour half is the caller's (`kit(dark).c.text_muted`).
pub(crate) fn chrome_text() -> TextStyle {
    TextStyle::new(8.625, 400.0).mono()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::Family;

    /// The Tailwind arbitrary pixel values ×0.75 — the port's px→pt ruling.
    #[test]
    fn the_arbitrary_pixel_classes_become_points() {
        assert_eq!(WIZARD_FOOTER_HEIGHT, 56.0 * 0.75); // h-[56px]
        assert_eq!(WIZARD_FOOTER_PX, 48.0 * 0.75); // px-[48px]
        assert_eq!(WIZARD_ASIDE_PX, 34.0 * 0.75); // px-[34px]
    }

    /// The file's one rule: both chrome rows share the height and centre
    /// their content inside it (`items-center`), so the mono chrome text —
    /// its default 1.25 line height included — must fit the shared height.
    #[test]
    fn the_chrome_text_fits_the_shared_footer_height() {
        let style = chrome_text();
        assert_eq!(style.size, 11.5 * 0.75); // text-[11.5px]
        assert_eq!(style.weight, 400.0); // font-normal
        assert_eq!(style.family, Family::Mono); // font-mono
        assert!(
            style.line_px() <= WIZARD_FOOTER_HEIGHT,
            "the chrome text ({}pt line) must fit the {}pt shared footer height",
            style.line_px(),
            WIZARD_FOOTER_HEIGHT
        );
    }

    /// The two horizontal rhythms stay distinct values, as in React: the
    /// sidebar's 34px column is the narrower band, the footer's 48px the
    /// wider one — pinning that the constants were not transposed.
    #[test]
    fn the_aside_rhythm_is_narrower_than_the_footer_band() {
        assert!(WIZARD_ASIDE_PX < WIZARD_FOOTER_PX);
        assert!(WIZARD_ASIDE_PX > 0.0 && WIZARD_FOOTER_PX > 0.0);
    }
}
