//! Line-background colours for the diff editor, and the [`TextDecoration`]s that paint
//! them onto [`format::DiffLineRanges`].
//!
//! The theme's syntax styles cannot express a background — [`gpui_component::ThemeStyle`]
//! carries only `color`, `font_style` and `font_weight` — so a per-line background has to
//! go through the editor's decorations collection instead (see `super::mod` for why).
//! [`LIGHT_ADDITION_BACKGROUND`] and [`LIGHT_DELETION_BACKGROUND`] are the exact values
//! given for the light theme; [`DARK_ADDITION_BACKGROUND`] and [`DARK_DELETION_BACKGROUND`]
//! are this module's own pair; see [`line_backgrounds`] for why dark needs one.

use gpui::{HighlightStyle, Hsla, rgb};
use gpui_component::ThemeMode;
use gpui_component::input::TextDecoration;

use super::format::DiffLineRanges;

const LIGHT_ADDITION_BACKGROUND: u32 = 0xdafbe1;
const LIGHT_DELETION_BACKGROUND: u32 = 0xffebe9;

/// Catppuccin Frappé's diff-addition syntax colour (`#a6d189`) sits at contrast ratio
/// 1.56:1 against [`LIGHT_ADDITION_BACKGROUND`] — pale-on-pale, unreadable. This is a
/// desaturated dark green chosen for at least 4.5:1 (WCAG AA) against that same syntax
/// colour instead, so additions stay legible once the theme switches to dark.
const DARK_ADDITION_BACKGROUND: u32 = 0x0f3d1f;

/// Mirrors [`DARK_ADDITION_BACKGROUND`]'s reasoning for Catppuccin Frappé's
/// diff-deletion syntax colour (`#e78284`, 2.31:1 against [`LIGHT_DELETION_BACKGROUND`]).
const DARK_DELETION_BACKGROUND: u32 = 0x3c0d13;

pub(super) struct LineBackgrounds {
    pub added: Hsla,
    pub deleted: Hsla,
}

pub(super) fn line_backgrounds(mode: ThemeMode) -> LineBackgrounds {
    let (added, deleted) = if mode.is_dark() {
        (DARK_ADDITION_BACKGROUND, DARK_DELETION_BACKGROUND)
    } else {
        (LIGHT_ADDITION_BACKGROUND, LIGHT_DELETION_BACKGROUND)
    };
    LineBackgrounds {
        added: rgb(added).into(),
        deleted: rgb(deleted).into(),
    }
}

pub(super) fn build_decorations(
    ranges: &DiffLineRanges,
    colors: &LineBackgrounds,
) -> Vec<TextDecoration> {
    let added_style = HighlightStyle {
        background_color: Some(colors.added),
        ..Default::default()
    };
    let deleted_style = HighlightStyle {
        background_color: Some(colors.deleted),
        ..Default::default()
    };

    ranges
        .additions
        .iter()
        .cloned()
        .map(|range| TextDecoration::new(range, added_style))
        .chain(
            ranges
                .deletions
                .iter()
                .cloned()
                .map(|range| TextDecoration::new(range, deleted_style)),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: gpui::Rgba, b: gpui::Rgba) -> bool {
        (a.r - b.r).abs() < 1e-3
            && (a.g - b.g).abs() < 1e-3
            && (a.b - b.b).abs() < 1e-3
            && (a.a - b.a).abs() < 1e-3
    }

    #[test]
    fn light_mode_uses_the_exact_given_hex_values() {
        let colors = line_backgrounds(ThemeMode::Light);
        assert!(approx_eq(
            colors.added.into(),
            rgb(LIGHT_ADDITION_BACKGROUND)
        ));
        assert!(approx_eq(
            colors.deleted.into(),
            rgb(LIGHT_DELETION_BACKGROUND)
        ));
    }

    #[test]
    fn dark_mode_does_not_reuse_the_light_pair() {
        let light = line_backgrounds(ThemeMode::Light);
        let dark = line_backgrounds(ThemeMode::Dark);
        assert!(!approx_eq(dark.added.into(), light.added.into()));
        assert!(!approx_eq(dark.deleted.into(), light.deleted.into()));
    }

    #[test]
    fn build_decorations_pairs_each_range_with_its_own_background() {
        let ranges = DiffLineRanges {
            additions: vec![0..3, 10..14],
            deletions: vec![5..8, 15..18],
        };
        let colors = line_backgrounds(ThemeMode::Light);

        let decorations = build_decorations(&ranges, &colors);

        assert_eq!(decorations.len(), 4);
        let additions: Vec<_> = decorations
            .iter()
            .filter(|d| d.style.background_color == Some(colors.added))
            .map(|d| d.range.clone())
            .collect();
        let deletions: Vec<_> = decorations
            .iter()
            .filter(|d| d.style.background_color == Some(colors.deleted))
            .map(|d| d.range.clone())
            .collect();
        assert_eq!(additions, ranges.additions);
        assert_eq!(deletions, ranges.deletions);
    }
}
