//! Maps a commit graph's abstract lane colour onto this shell's theme.
//!
//! [`graph::LaneColor`] is an index, not a colour: the layout crate has no business
//! knowing what a theme looks like, and the same layout must render correctly in light
//! and dark mode. This is where that loop closes — workstream E draws the graph with
//! [`lane_color`].

use gpui::Hsla;
use gpui_component::ThemeColor;
use graph::{LaneColor, PALETTE_SIZE};

type Swatch = fn(&ThemeColor) -> Hsla;

/// Eight hues, alternating a full-strength and a light variant so two lanes that land
/// [`PALETTE_SIZE`] apart — the only way this mapping can repeat a colour — are never
/// adjacent in a real layout without many more concurrent lanes than a readable graph has.
///
/// The eighth entry reads `yellow_light`, not `magenta_light`. Under Catppuccin, a theme
/// sets `magenta_light` to the same hue as `magenta` at reduced alpha rather than a
/// distinct tint — under Frappé the two resolve to identical RGB, differing only in
/// opacity, which a 1-2px lane stroke cannot read as two colours. `yellow_light` has no
/// such override in either theme this crate applies (`theme_palette`), so it falls back
/// to gpui-component's own background-blended tint like `blue_light` does, keeping every
/// entry a genuinely distinct hue under both Catppuccin Latte and Frappé (see this
/// file's own tests below for the resolved values this was checked against).
const PALETTE: [Swatch; PALETTE_SIZE as usize] = [
    |theme| theme.blue,
    |theme| theme.green,
    |theme| theme.magenta,
    |theme| theme.yellow,
    |theme| theme.cyan,
    |theme| theme.red,
    |theme| theme.blue_light,
    |theme| theme.yellow_light,
];

/// The theme colour a renderer should paint lane `color` with.
///
/// Wraps modulo [`PALETTE_SIZE`], so it accepts any [`LaneColor`] a
/// [`graph::GraphLayout`] produces, however many lanes were actually laid out.
pub fn lane_color(color: LaneColor, theme: &ThemeColor) -> Hsla {
    PALETTE[color.0 as usize % PALETTE.len()](theme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_in_one_cycle_is_distinguishable_in_light_mode() {
        assert_all_distinct(&ThemeColor::light());
    }

    #[test]
    fn every_entry_in_one_cycle_is_distinguishable_in_dark_mode() {
        assert_all_distinct(&ThemeColor::dark());
    }

    #[test]
    fn every_entry_is_distinguishable_under_github_light() {
        assert_all_distinct(&crate::theme_palette::resolve_for_tests(
            crate::theme_palette::LIGHT_THEME_NAME,
        ));
    }

    #[test]
    fn every_entry_is_distinguishable_under_catppuccin_frappe() {
        assert_all_distinct(&crate::theme_palette::resolve_for_tests(
            crate::theme_palette::DARK_THEME_NAME,
        ));
    }

    /// Minimum Euclidean RGB distance (normalised `[0, 1]` per channel, so the maximum
    /// possible is `3.0_f32.sqrt()`) two lane colours must clear once composited over the
    /// background. Set below the closest pair either applied theme produces today — blue
    /// next to magenta, ~0.17 under Catppuccin Frappé, both being violet-leaning hues in
    /// Catppuccin's own palette — and above the collision this test was written to catch,
    /// where `magenta_light` collapsed onto `magenta` at ~0.0-0.07 (see the `PALETTE` doc
    /// comment).
    const MIN_DISTINGUISHABLE: f32 = 0.06;

    fn assert_all_distinct(theme: &ThemeColor) {
        let colors: Vec<Hsla> = (0..PALETTE_SIZE)
            .map(|index| lane_color(LaneColor(index), theme))
            .collect();

        for (i, a) in colors.iter().enumerate() {
            for (j, b) in colors.iter().enumerate().skip(i + 1) {
                let distance = crate::theme_palette::rendered_distance(theme.background, *a, *b);
                assert!(
                    distance > MIN_DISTINGUISHABLE,
                    "lane colors {i} and {j} read as the same colour once painted (distance {distance:.3})"
                );
            }
            let distance =
                crate::theme_palette::rendered_distance(theme.background, *a, theme.background);
            assert!(
                distance > MIN_DISTINGUISHABLE,
                "lane color {i} is indistinguishable from the background (distance {distance:.3})"
            );
        }
    }

    #[test]
    fn wraps_around_the_palette() {
        let theme = ThemeColor::light();
        assert_eq!(
            lane_color(LaneColor(0), &theme),
            lane_color(LaneColor(PALETTE_SIZE), &theme)
        );
        assert_eq!(
            lane_color(LaneColor(3), &theme),
            lane_color(LaneColor(PALETTE_SIZE + 3), &theme)
        );
    }
}
