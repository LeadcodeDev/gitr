//! Wires this app's light/dark theme pair: Gitr Light (this crate's own warm-neutral
//! theme) for light, Catppuccin Frappé for dark, loaded from vendored/authored JSON
//! (provenance and licensing in `themes/NOTICE.md`) rather than gpui-component's own
//! built-in palette.
//!
//! The two flavours come from different sources — Gitr Light is authored in this
//! directory, Frappé is Catppuccin's — so following the system appearance moves between
//! two independently designed palettes rather than between two halves of one. Dark stays
//! Catppuccin because only light was ever in scope for the change.

use std::rc::Rc;

use gpui::App;
use gpui_component::{Theme, ThemeConfig, ThemeRegistry};

const CATPPUCCIN_THEMES: &str = include_str!("../themes/catppuccin.json");
const GITR_LIGHT_THEME: &str = include_str!("../themes/gitr-light.json");

/// Exact name [`GITR_LIGHT_THEME`] gives its one theme.
pub(crate) const LIGHT_THEME_NAME: &str = "Gitr Light";

/// Exact name gpui-component's vendored file gives the dark flavour. Spelled without the
/// accent — matching this ASCII spelling, not the correct French "Frappé", is what makes
/// the lookup in [`install`] find the theme instead of silently falling through.
pub(crate) const DARK_THEME_NAME: &str = "Catppuccin Frappe";

/// Loads both vendored/authored theme sets and makes Gitr Light / Catppuccin Frappé this
/// app's light/dark pair, replacing gpui-component's own built-in palette for both modes.
///
/// Must run before [`crate::density::apply`]: [`Theme::change`] below re-derives every
/// legacy colour field from whichever config is active, and is the one call in this path
/// that could, for some future edit to either JSON file, reintroduce a font size or
/// radius override. Running density last makes that class of regression structurally
/// impossible rather than dependent on the JSON staying as it is today.
///
/// A parse failure or a missing flavour name is logged and left as gpui-component's
/// built-in theme: a wrong palette is tolerable, a startup panic is not.
pub fn install(cx: &mut App) {
    let registry = ThemeRegistry::global_mut(cx);
    if let Err(error) = registry.load_themes_from_str(GITR_LIGHT_THEME) {
        eprintln!(
            "gitr: the gitr-light theme failed to parse, keeping the built-in theme: {error:#}"
        );
        return;
    }
    if let Err(error) = registry.load_themes_from_str(CATPPUCCIN_THEMES) {
        eprintln!(
            "gitr: the vendored Catppuccin theme set failed to parse, keeping the built-in \
             theme: {error:#}"
        );
        return;
    }

    let Some((light, dark)) = resolve(cx) else {
        eprintln!(
            "gitr: theme set has no '{LIGHT_THEME_NAME}' or no '{DARK_THEME_NAME}', keeping \
             the built-in theme"
        );
        return;
    };

    let theme = Theme::global_mut(cx);
    let mode = theme.mode;
    theme.light_theme = light;
    theme.dark_theme = dark;

    Theme::change(mode, None, cx);
}

fn resolve(cx: &App) -> Option<(Rc<ThemeConfig>, Rc<ThemeConfig>)> {
    let themes = ThemeRegistry::global(cx).themes();
    Some((
        themes.get(LIGHT_THEME_NAME)?.clone(),
        themes.get(DARK_THEME_NAME)?.clone(),
    ))
}

/// Finds `flavour_name` in the vendored/authored JSON, the way [`install`] does through
/// the registry, but standalone — so tests can check either flavour without an `App`.
#[cfg(test)]
fn find_config(flavour_name: &str) -> Option<ThemeConfig> {
    let sources = [GITR_LIGHT_THEME, CATPPUCCIN_THEMES];
    sources.into_iter().find_map(|source| {
        let set: gpui_component::ThemeSet =
            serde_json::from_str(source).expect("theme JSON must parse");
        set.themes
            .into_iter()
            .find(|theme| theme.name == flavour_name)
    })
}

/// Resolves `flavour_name`'s colours by applying its config to a fresh [`Theme`] — so
/// [`crate::graph_palette`] and the history badge tests can check real hues without an
/// `App` or a window. Panics on a missing flavour: test-only, and a typo here should fail
/// the test loudly rather than silently check the wrong colours.
#[cfg(test)]
pub(crate) fn resolve_for_tests(flavour_name: &str) -> gpui_component::ThemeColor {
    let config = find_config(flavour_name)
        .unwrap_or_else(|| panic!("{flavour_name} missing from the vendored theme set"));

    let mut theme = Theme::default();
    theme.apply_config(&Rc::new(config));
    theme.colors
}

/// Euclidean distance between `a` and `b` in normalised `[0, 1]` RGB space, after
/// compositing each over `background` the way gpui actually paints a translucent colour.
///
/// Plain `Hsla` inequality misses the collision this was written to catch: `magenta` and
/// `magenta_light` differed only in alpha under the vendored Catppuccin Frappé, so they
/// were unequal as raw `Hsla` values while resolving to the same painted colour (see
/// `graph_palette.rs`'s `PALETTE` doc comment). Compositing first exposes that; comparing
/// the raw structs does not.
#[cfg(test)]
pub(crate) fn rendered_distance(background: gpui::Hsla, a: gpui::Hsla, b: gpui::Hsla) -> f32 {
    let a = gpui::Rgba::from(background.blend(a));
    let b = gpui::Rgba::from(background.blend(b));
    ((a.r - b.r).powi(2) + (a.g - b.g).powi(2) + (a.b - b.b).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use gpui_component::ThemeSet;

    use super::*;

    #[test]
    fn catppuccin_json_parses() {
        let _: ThemeSet =
            serde_json::from_str(CATPPUCCIN_THEMES).expect("vendored Catppuccin JSON must parse");
    }

    #[test]
    fn gitr_light_json_parses() {
        let _: ThemeSet =
            serde_json::from_str(GITR_LIGHT_THEME).expect("authored Gitr Light JSON must parse");
    }

    #[test]
    fn both_followed_flavours_are_present_under_their_exact_names() {
        assert!(find_config(LIGHT_THEME_NAME).is_some());
        assert!(find_config(DARK_THEME_NAME).is_some());
    }

    #[test]
    fn density_tokens_survive_applying_either_flavour() {
        for name in [LIGHT_THEME_NAME, DARK_THEME_NAME] {
            let config = find_config(name)
                .unwrap_or_else(|| panic!("{name} missing from the vendored theme sets"));

            let mut theme = Theme {
                font_size: crate::density::FONT_SIZE,
                mono_font_size: crate::density::MONO_FONT_SIZE,
                radius: crate::density::RADIUS,
                ..Theme::default()
            };

            theme.apply_config(&Rc::new(config));

            assert_eq!(
                theme.font_size,
                crate::density::FONT_SIZE,
                "{name} overrode font_size"
            );
            assert_eq!(
                theme.mono_font_size,
                crate::density::MONO_FONT_SIZE,
                "{name} overrode mono_font_size"
            );
            assert_eq!(
                theme.radius,
                crate::density::RADIUS,
                "{name} overrode radius"
            );
        }
    }

    /// `tree-sitter-diff` paints additions and deletions through the generic `@string`
    /// and `@keyword` captures rather than dedicated `created`/`deleted` ones (see this
    /// module's doc comment on [`install`] and the task's own report for how that was
    /// established), so a legible diff depends on `highlight.syntax.string` and
    /// `highlight.syntax.keyword` resolving to a green and a red, not on the
    /// schema-complete but functionally inert top-level `created`/`deleted` keys this
    /// file also carries.
    #[test]
    fn diff_additions_and_deletions_are_distinguishable_syntax_colors() {
        let config = find_config(LIGHT_THEME_NAME)
            .unwrap_or_else(|| panic!("{LIGHT_THEME_NAME} missing from the theme sets"));
        let style = config
            .highlight
            .as_ref()
            .expect("Gitr Light must declare a highlight style");

        let string_style: gpui::HighlightStyle = style
            .syntax
            .string
            .expect("syntax.string must be set so diff additions render in colour")
            .into();
        let keyword_style: gpui::HighlightStyle = style
            .syntax
            .keyword
            .expect("syntax.keyword must be set so diff deletions render in colour")
            .into();

        let added = string_style.color.expect("syntax.string must set a color");
        let deleted = keyword_style
            .color
            .expect("syntax.keyword must set a color");

        let background = find_background(&config);
        let distance = rendered_distance(background, added, deleted);
        assert!(
            distance > 0.06,
            "diff additions and deletions read as the same colour once painted \
             (distance {distance:.3})"
        );

        let added_rgb = gpui::Rgba::from(added);
        let deleted_rgb = gpui::Rgba::from(deleted);
        assert!(
            added_rgb.g > added_rgb.r && added_rgb.g > added_rgb.b,
            "syntax.string should resolve to a green so additions read as additions"
        );
        assert!(
            deleted_rgb.r > deleted_rgb.g && deleted_rgb.r > deleted_rgb.b,
            "syntax.keyword should resolve to a red so deletions read as deletions"
        );
    }

    fn find_background(config: &ThemeConfig) -> gpui::Hsla {
        let mut theme = Theme::default();
        theme.apply_config(&Rc::new(config.clone()));
        theme.background
    }
}
