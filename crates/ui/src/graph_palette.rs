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
const PALETTE: [Swatch; PALETTE_SIZE as usize] = [
    |theme| theme.blue,
    |theme| theme.green,
    |theme| theme.magenta,
    |theme| theme.yellow,
    |theme| theme.cyan,
    |theme| theme.red,
    |theme| theme.blue_light,
    |theme| theme.magenta_light,
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

    fn assert_all_distinct(theme: &ThemeColor) {
        let colors: Vec<Hsla> = (0..PALETTE_SIZE)
            .map(|index| lane_color(LaneColor(index), theme))
            .collect();

        for (i, a) in colors.iter().enumerate() {
            for (j, b) in colors.iter().enumerate().skip(i + 1) {
                assert_ne!(a, b, "lane colors {i} and {j} must be distinguishable");
            }
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
