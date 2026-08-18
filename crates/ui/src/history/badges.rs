//! Reference badges shown inline before a commit's subject.
//!
//! A badge is coloured by [`BadgeKind`], not by the reference's identity, so every local
//! branch reads the same way and a remote branch or tag never gets mistaken for one.

use gpui::{Hsla, IntoElement, ParentElement as _};
use gpui_component::{Sizable as _, ThemeColor, tag::Tag};

use domain::Reference;

/// Which of the three badge colours a reference gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeKind {
    LocalBranch,
    RemoteBranch,
    Tag,
}

/// Classifies `reference` for badge colouring.
pub fn classify(reference: &Reference) -> BadgeKind {
    match reference {
        Reference::LocalBranch(_) => BadgeKind::LocalBranch,
        Reference::RemoteBranch { .. } => BadgeKind::RemoteBranch,
        Reference::Tag(_) => BadgeKind::Tag,
    }
}

/// The theme colour a badge of `kind` renders with.
pub fn badge_color(kind: BadgeKind, theme: &ThemeColor) -> Hsla {
    match kind {
        BadgeKind::LocalBranch => theme.blue,
        BadgeKind::RemoteBranch => theme.green,
        BadgeKind::Tag => theme.yellow,
    }
}

/// One reference badge, as it sits inline before a subject.
pub fn render_badge(reference: &Reference, theme: &ThemeColor) -> impl IntoElement {
    let color = badge_color(classify(reference), theme);

    Tag::custom(color.opacity(0.16), color, color.opacity(0.4))
        .rounded_full()
        .xsmall()
        .child(reference.short_name())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{BranchName, RemoteName, TagName};

    fn local() -> Reference {
        Reference::LocalBranch(BranchName::new("main").unwrap())
    }

    fn remote() -> Reference {
        Reference::RemoteBranch {
            remote: RemoteName::new("origin").unwrap(),
            branch: BranchName::new("main").unwrap(),
        }
    }

    fn tag() -> Reference {
        Reference::Tag(TagName::new("v1.0.0").unwrap())
    }

    #[test]
    fn classifies_each_reference_kind() {
        assert_eq!(classify(&local()), BadgeKind::LocalBranch);
        assert_eq!(classify(&remote()), BadgeKind::RemoteBranch);
        assert_eq!(classify(&tag()), BadgeKind::Tag);
    }

    #[test]
    fn every_kind_is_distinguishable_in_light_mode() {
        assert_all_distinct(&ThemeColor::light());
    }

    #[test]
    fn every_kind_is_distinguishable_in_dark_mode() {
        assert_all_distinct(&ThemeColor::dark());
    }

    #[test]
    fn every_kind_is_distinguishable_under_gitr_light() {
        assert_all_distinct(&crate::theme_palette::resolve_for_tests(
            crate::theme_palette::LIGHT_THEME_NAME,
        ));
    }

    #[test]
    fn every_kind_is_distinguishable_under_catppuccin_frappe() {
        assert_all_distinct(&crate::theme_palette::resolve_for_tests(
            crate::theme_palette::DARK_THEME_NAME,
        ));
    }

    /// Same threshold and reasoning as `graph_palette`'s: raw `Hsla` inequality does not
    /// catch a badge colour that composites to nearly the background or to another
    /// badge's colour, only compositing first does.
    const MIN_DISTINGUISHABLE: f32 = 0.06;

    fn assert_all_distinct(theme: &ThemeColor) {
        let colors = [
            badge_color(BadgeKind::LocalBranch, theme),
            badge_color(BadgeKind::RemoteBranch, theme),
            badge_color(BadgeKind::Tag, theme),
        ];
        for (i, a) in colors.iter().enumerate() {
            for (j, b) in colors.iter().enumerate().skip(i + 1) {
                let distance = crate::theme_palette::rendered_distance(theme.background, *a, *b);
                assert!(
                    distance > MIN_DISTINGUISHABLE,
                    "badge kinds {i} and {j} read as the same colour once painted (distance {distance:.3})"
                );
            }
        }
    }
}
