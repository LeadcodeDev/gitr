//! What the user asked the theme to do: pin light, pin dark, or keep following the OS.
//!
//! `gpui_component::ThemeMode` has exactly two variants, `Light` and `Dark` — there is no
//! `System`. [`ThemePreference`] is this application's own third state. `Workspace`'s
//! former `follow_system_theme` flag is now derived by calling [`Self::follows_system`]
//! on this enum wherever it is needed, rather than tracked as a second,
//! independently-settable flag that could disagree with it.

use gpui_component::{IconName, ThemeMode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    Light,
    Dark,
    #[default]
    System,
}

impl ThemePreference {
    /// The order the theme control's menu lists its three entries in.
    pub const ALL: [ThemePreference; 3] = [Self::Light, Self::Dark, Self::System];

    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::System => "System",
        }
    }

    /// The bundled Lucide set ships no monitor, laptop or display glyph, so there is no
    /// literal icon for "follow the system". `Cpu` stands in because it reads as the
    /// machine deciding, which is what the state means; the alternative in the set is a
    /// terminal window, and a terminal says nothing about appearance.
    pub fn icon(self) -> IconName {
        match self {
            Self::Light => IconName::Sun,
            Self::Dark => IconName::Moon,
            Self::System => IconName::Cpu,
        }
    }

    /// `true` only for [`Self::System`] — the one state in which the window must keep
    /// applying OS appearance changes as they arrive instead of holding a fixed mode.
    pub fn follows_system(self) -> bool {
        matches!(self, Self::System)
    }

    /// The [`ThemeMode`] this preference pins the window to, or `None` for
    /// [`Self::System`], which has no fixed mode of its own — the caller must read the
    /// window's current OS appearance instead.
    pub fn explicit_mode(self) -> Option<ThemeMode> {
        match self {
            Self::Light => Some(ThemeMode::Light),
            Self::Dark => Some(ThemeMode::Dark),
            Self::System => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preference_follows_the_system() {
        assert_eq!(ThemePreference::default(), ThemePreference::System);
    }

    #[test]
    fn only_system_follows_the_system() {
        assert!(ThemePreference::System.follows_system());
        assert!(!ThemePreference::Light.follows_system());
        assert!(!ThemePreference::Dark.follows_system());
    }

    #[test]
    fn light_and_dark_carry_their_own_fixed_mode_system_carries_none() {
        assert_eq!(
            ThemePreference::Light.explicit_mode(),
            Some(ThemeMode::Light)
        );
        assert_eq!(ThemePreference::Dark.explicit_mode(), Some(ThemeMode::Dark));
        assert_eq!(ThemePreference::System.explicit_mode(), None);
    }

    #[test]
    fn menu_order_is_light_dark_system() {
        assert_eq!(
            ThemePreference::ALL,
            [
                ThemePreference::Light,
                ThemePreference::Dark,
                ThemePreference::System
            ]
        );
    }

    #[test]
    fn round_trips_every_variant_through_json() {
        for preference in ThemePreference::ALL {
            let json = serde_json::to_string(&preference).expect("preference must serialise");
            let restored: ThemePreference =
                serde_json::from_str(&json).expect("preference must deserialise");
            assert_eq!(restored, preference);
        }
    }

    #[test]
    fn serialises_as_lower_snake_case() {
        assert_eq!(
            serde_json::to_string(&ThemePreference::System).unwrap(),
            "\"system\""
        );
        assert_eq!(
            serde_json::to_string(&ThemePreference::Light).unwrap(),
            "\"light\""
        );
        assert_eq!(
            serde_json::to_string(&ThemePreference::Dark).unwrap(),
            "\"dark\""
        );
    }
}
